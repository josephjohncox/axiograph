use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request};
use lsp_types::{
    CodeActionProviderCapability, Diagnostic, DiagnosticSeverity, ExecuteCommandOptions,
    InitializeResult, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities as LspServerCapabilities, ServerInfo as LspServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "axiograph-software-authoring")]
#[command(
    about = "Software authoring, continuous semantic coverage, codegen, and editor integration tools for Axiograph"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Check a BehaviorCaseReportV1 as a continuous software-coverage gate.
    ContinuousCheck(ContinuousCheckArgs),
    /// Materialize generated test skeleton previews from a BehaviorCaseReportV1.
    MaterializeSkeletons(MaterializeSkeletonsArgs),
    /// Return codegen skeleton file hints from a typed tooling overlay.
    CodegenPlan {
        /// Input JSON file containing `ToolingOverlayBundleV1`.
        #[arg(long)]
        overlay: PathBuf,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Emit read-only plugin/tool metadata for agent and editor integrations.
    ToolSpecs {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Emit minimal LSP capability metadata for editor integrations.
    LspCapabilities {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Emit host launch metadata for lsp-server LSP and rmcp MCP integrations.
    IntegrationManifest {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run the lsp-server/lsp-types stdio LSP authoring server.
    Lsp,
    /// Run the read-only stdio MCP authoring server.
    Mcp,
}

#[derive(Args, Debug, Clone)]
struct ContinuousCheckArgs {
    /// Path to a behavior_case_report_v1 JSON file.
    #[arg(long)]
    behavior_report: PathBuf,

    /// Repository root used to resolve code_refs from the behavior report.
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// Comma-separated list of generated languages required in codegen_previews.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "rust,typescript,python,go"
    )]
    require_codegen: Vec<String>,

    /// Fail when ontology rules are not mapped to implementation/test coverage.
    #[arg(long)]
    strict_coverage: bool,

    /// Fail when code_refs do not exist on disk.
    #[arg(long)]
    require_code_refs: bool,

    /// Fail unless the report carries a runtime theory-check summary.
    #[arg(long)]
    require_runtime_theory: bool,

    /// Emit the report as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct MaterializeSkeletonsArgs {
    /// Path to a behavior_case_report_v1 JSON file.
    #[arg(long)]
    behavior_report: PathBuf,

    /// Output directory for generated skeletons. file_hint paths are rooted here.
    #[arg(long)]
    out_dir: PathBuf,

    /// Optional comma-separated language filter.
    #[arg(long, value_delimiter = ',')]
    language: Vec<String>,

    /// Overwrite existing generated files.
    #[arg(long)]
    overwrite: bool,

    /// Emit the materialization report as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone)]
pub struct ContinuousCheckOptions {
    pub repo_root: PathBuf,
    pub require_codegen: Vec<String>,
    pub strict_coverage: bool,
    pub require_code_refs: bool,
    pub require_runtime_theory: bool,
}

#[derive(Debug, Clone)]
pub struct MaterializeSkeletonsOptions {
    pub out_dir: PathBuf,
    pub language: Vec<String>,
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct ContinuousSoftwareCoverageReportV1 {
    pub version: &'static str,
    pub case_id: Option<String>,
    pub status: CoverageGateStatus,
    pub pass: bool,
    pub repo_root: String,
    pub required_codegen_languages: Vec<String>,
    pub present_codegen_languages: Vec<String>,
    pub missing_codegen_languages: Vec<String>,
    pub code_refs_total: usize,
    pub existing_code_refs: Vec<String>,
    pub missing_code_refs: Vec<String>,
    pub coverage: CoverageSummary,
    pub competency: CompetencySummary,
    pub runtime_theory: RuntimeTheoryPresence,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageGateStatus {
    Passed,
    PassedWithWarnings,
    Failed,
}

#[derive(Debug, Default, Serialize)]
pub struct CoverageSummary {
    pub total_rules: u64,
    pub runtime_enforced_rules: u64,
    pub covered_rules: u64,
    pub tested_rules: u64,
    pub implemented_rules: u64,
    pub ontology_only_rules: u64,
    pub drifted_rules: u64,
    pub missing_obligations: Vec<String>,
    pub uncovered_rule_ids: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct CompetencySummary {
    pub total: u64,
    pub satisfied: u64,
    pub coverage: f64,
    pub unsatisfied_questions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeTheoryPresence {
    pub present: bool,
    pub ontology_closed: Option<bool>,
    pub complete: Option<bool>,
    pub blocking_judgments: Option<u64>,
    pub residual_obligations: Option<u64>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CodegenMaterializationReportV1 {
    pub version: &'static str,
    pub source_report: String,
    pub out_dir: String,
    pub written_files: Vec<MaterializedSkeletonV1>,
    pub skipped_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MaterializedSkeletonV1 {
    pub language: String,
    pub path: String,
    pub bytes: usize,
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::ContinuousCheck(args) => run_continuous_check(args),
        Commands::MaterializeSkeletons(args) => run_materialize_skeletons(args),
        Commands::CodegenPlan { overlay, json } => run_codegen_plan(&overlay, json),
        Commands::ToolSpecs { json } => print_tool_specs(json),
        Commands::LspCapabilities { json } => print_lsp_capabilities(json),
        Commands::IntegrationManifest { json } => print_integration_manifest(json),
        Commands::Lsp => run_lsp_stdio(),
        Commands::Mcp => run_mcp_stdio(),
    }
}

fn run_codegen_plan(overlay_path: &Path, json_output: bool) -> Result<()> {
    let text = fs::read_to_string(overlay_path)
        .with_context(|| format!("read `{}`", overlay_path.display()))?;
    let overlay = axiograph_tooling_overlays::parse_overlay_bundle(&text)?;
    let report = axiograph_tooling_overlays::codegen_plan_report(&overlay);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("codegen plan: mode={:?}", report.coverage_mode);
        for hint in &report.file_hints {
            println!("  {hint}");
        }
        for caveat in &report.caveats {
            println!("  caveat: {caveat}");
        }
    }
    Ok(())
}

fn run_continuous_check(args: ContinuousCheckArgs) -> Result<()> {
    let report = read_json(&args.behavior_report)?;
    validate_behavior_report(&report)?;

    let coverage_report = build_continuous_software_coverage_report(
        &report,
        &ContinuousCheckOptions {
            repo_root: args.repo_root,
            require_codegen: args.require_codegen,
            strict_coverage: args.strict_coverage,
            require_code_refs: args.require_code_refs,
            require_runtime_theory: args.require_runtime_theory,
        },
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&coverage_report)?);
    } else {
        print_human_continuous_report(&coverage_report);
    }

    if coverage_report.pass {
        Ok(())
    } else {
        bail!("continuous software coverage check failed")
    }
}

fn run_materialize_skeletons(args: MaterializeSkeletonsArgs) -> Result<()> {
    let report = read_json(&args.behavior_report)?;
    validate_behavior_report(&report)?;
    let materialization = materialize_skeletons_from_report(
        &report,
        &args.behavior_report,
        &MaterializeSkeletonsOptions {
            out_dir: args.out_dir,
            language: args.language,
            overwrite: args.overwrite,
        },
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&materialization)?);
    } else {
        println!(
            "materialized {} skeleton(s), skipped {}",
            materialization.written_files.len(),
            materialization.skipped_files.len()
        );
        for file in &materialization.written_files {
            println!("  {} {}", file.language, file.path);
        }
        for skipped in &materialization.skipped_files {
            println!("  skipped: {skipped}");
        }
    }
    Ok(())
}

pub fn materialize_skeletons_from_report(
    report: &Value,
    source_report: &Path,
    options: &MaterializeSkeletonsOptions,
) -> Result<CodegenMaterializationReportV1> {
    validate_behavior_report(report)?;
    let previews = collect_codegen_previews(report);
    let filter = normalize_languages(&options.language);
    fs::create_dir_all(&options.out_dir).with_context(|| {
        format!(
            "create materialized skeleton output dir `{}`",
            options.out_dir.display()
        )
    })?;

    let mut written_files = Vec::new();
    let mut skipped_files = Vec::new();
    for preview in previews {
        if !filter.is_empty() && !filter.contains(&preview.language) {
            continue;
        }
        let Some(file_hint) = preview.file_hint.as_deref() else {
            skipped_files.push(format!("{}: missing file_hint", preview.language));
            continue;
        };
        let Some(content) = preview.content.as_deref() else {
            skipped_files.push(format!("{}:{file_hint}: missing content", preview.language));
            continue;
        };
        let path = safe_join(&options.out_dir, file_hint)?;
        if path.exists() && !options.overwrite {
            skipped_files.push(format!(
                "{}: exists; pass --overwrite to replace",
                path.display()
            ));
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir `{}`", parent.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("write `{}`", path.display()))?;
        written_files.push(MaterializedSkeletonV1 {
            language: preview.language,
            path: path.display().to_string(),
            bytes: content.len(),
        });
    }

    let materialization = CodegenMaterializationReportV1 {
        version: "codegen_materialization_report_v1",
        source_report: source_report.display().to_string(),
        out_dir: options.out_dir.display().to_string(),
        written_files,
        skipped_files,
    };

    Ok(materialization)
}

pub fn build_continuous_software_coverage_report(
    report: &Value,
    options: &ContinuousCheckOptions,
) -> Result<ContinuousSoftwareCoverageReportV1> {
    validate_behavior_report(report)?;
    let repo_root = options
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| options.repo_root.clone());
    let required_codegen_languages = normalize_languages(&options.require_codegen)
        .into_iter()
        .collect::<Vec<_>>();
    let present_codegen_languages = collect_codegen_previews(report)
        .into_iter()
        .map(|preview| preview.language)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let present_set = present_codegen_languages
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_codegen_languages = required_codegen_languages
        .iter()
        .filter(|language| !present_set.contains(*language))
        .cloned()
        .collect::<Vec<_>>();

    let code_refs = collect_string_arrays(report, "code_refs")
        .into_iter()
        .collect::<Vec<_>>();
    let mut existing_code_refs = Vec::new();
    let mut missing_code_refs = Vec::new();
    for code_ref in &code_refs {
        if safe_join(&repo_root, code_ref)
            .map(|path| path.exists())
            .unwrap_or(false)
        {
            existing_code_refs.push(code_ref.clone());
        } else {
            missing_code_refs.push(code_ref.clone());
        }
    }

    let coverage = coverage_summary(report);
    let competency = competency_summary(report);
    let runtime_theory = runtime_theory_presence(report);

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    if !missing_codegen_languages.is_empty() {
        failures.push(format!(
            "missing required codegen previews: {}",
            missing_codegen_languages.join(", ")
        ));
    }
    if coverage.drifted_rules > 0 {
        failures.push(format!(
            "{} ontology rule(s) are drifted from implementation surfaces",
            coverage.drifted_rules
        ));
    }
    if competency.satisfied < competency.total {
        failures.push(format!(
            "only {}/{} competency question(s) are satisfied",
            competency.satisfied, competency.total
        ));
    }
    if options.strict_coverage && coverage.covered_rules < coverage.total_rules {
        failures.push(format!(
            "semantic coverage is incomplete: {}/{} rule(s) covered",
            coverage.covered_rules, coverage.total_rules
        ));
    } else if coverage.covered_rules < coverage.total_rules {
        warnings.push(format!(
            "semantic coverage is incomplete: {}/{} rule(s) covered; pass --strict-coverage to fail",
            coverage.covered_rules, coverage.total_rules
        ));
    }
    if options.strict_coverage && !coverage.missing_obligations.is_empty() {
        failures.push(format!(
            "{} coverage obligation(s) remain unresolved",
            coverage.missing_obligations.len()
        ));
    } else if !coverage.missing_obligations.is_empty() {
        warnings.push(format!(
            "{} coverage obligation(s) remain unresolved",
            coverage.missing_obligations.len()
        ));
    }
    if options.require_code_refs && !missing_code_refs.is_empty() {
        failures.push(format!(
            "{} code_ref path(s) are missing on disk",
            missing_code_refs.len()
        ));
    } else if !missing_code_refs.is_empty() {
        warnings.push(format!(
            "{} code_ref path(s) are target surfaces, not existing files; pass --require-code-refs to fail",
            missing_code_refs.len()
        ));
    }
    if options.require_runtime_theory && !runtime_theory.present {
        failures
            .push("behavior report does not include a runtime theory-check summary".to_string());
    } else if !runtime_theory.present {
        warnings.push(
            "behavior report has no embedded runtime theory-check summary; run `axiograph check theory` beside this gate"
                .to_string(),
        );
    }
    if runtime_theory.ontology_closed == Some(false) {
        failures.push("runtime theory closure is present but ontology_closed=false".to_string());
    }
    if runtime_theory.complete == Some(false) {
        failures.push("runtime theory completeness is present but complete=false".to_string());
    }
    if runtime_theory.blocking_judgments.unwrap_or(0) > 0 {
        failures.push(format!(
            "{} blocking runtime-theory judgment(s) are present",
            runtime_theory.blocking_judgments.unwrap_or(0)
        ));
    }

    let mut next_actions = coverage.next_actions.clone();
    if !missing_codegen_languages.is_empty() {
        next_actions.push(format!(
            "add behavior-case codegen previews for {}",
            missing_codegen_languages.join(", ")
        ));
    }
    if !missing_code_refs.is_empty() {
        next_actions.push(
            "materialize generated skeletons or map code_refs to real implementation/test files"
                .to_string(),
        );
    }
    if !runtime_theory.present {
        next_actions.push(
            "attach the RuntimeTheoryCheckReportV1 summary to this behavior-case gate".to_string(),
        );
    }
    dedup_strings(&mut next_actions);

    let status = if failures.is_empty() && warnings.is_empty() {
        CoverageGateStatus::Passed
    } else if failures.is_empty() {
        CoverageGateStatus::PassedWithWarnings
    } else {
        CoverageGateStatus::Failed
    };
    let pass = failures.is_empty();

    Ok(ContinuousSoftwareCoverageReportV1 {
        version: "continuous_software_coverage_report_v1",
        case_id: path_string(report, &["behavior_case", "case_id"]),
        status,
        pass,
        repo_root: repo_root.display().to_string(),
        required_codegen_languages,
        present_codegen_languages,
        missing_codegen_languages,
        code_refs_total: code_refs.len(),
        existing_code_refs,
        missing_code_refs,
        coverage,
        competency,
        runtime_theory,
        failures,
        warnings,
        next_actions,
    })
}

fn print_human_continuous_report(report: &ContinuousSoftwareCoverageReportV1) {
    println!(
        "continuous software coverage: {:?} case={}",
        report.status,
        report.case_id.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "  codegen: present=[{}] missing=[{}]",
        report.present_codegen_languages.join(", "),
        report.missing_codegen_languages.join(", ")
    );
    println!(
        "  semantic coverage: covered={}/{} tested={} drifted={} missing_obligations={}",
        report.coverage.covered_rules,
        report.coverage.total_rules,
        report.coverage.tested_rules,
        report.coverage.drifted_rules,
        report.coverage.missing_obligations.len()
    );
    println!(
        "  competency: satisfied={}/{}",
        report.competency.satisfied, report.competency.total
    );
    println!(
        "  code_refs: existing={} missing={}",
        report.existing_code_refs.len(),
        report.missing_code_refs.len()
    );
    println!(
        "  runtime_theory: present={} ontology_closed={:?} complete={:?}",
        report.runtime_theory.present,
        report.runtime_theory.ontology_closed,
        report.runtime_theory.complete
    );
    for failure in &report.failures {
        println!("  failure: {failure}");
    }
    for warning in &report.warnings {
        println!("  warning: {warning}");
    }
    for action in report.next_actions.iter().take(12) {
        println!("  next: {action}");
    }
}

fn print_tool_specs(json_output: bool) -> Result<()> {
    let specs = software_authoring_tool_specs_v1();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&specs)?);
    } else {
        println!("software authoring tools:");
        for tool in specs["tools"].as_array().into_iter().flatten() {
            println!(
                "  {} - {}",
                tool["name"].as_str().unwrap_or("<unknown>"),
                tool["description"].as_str().unwrap_or("")
            );
        }
    }
    Ok(())
}

fn print_lsp_capabilities(json_output: bool) -> Result<()> {
    let capabilities = software_authoring_lsp_capabilities_v1();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&capabilities)?);
    } else {
        println!("software authoring LSP capabilities:");
        println!("  diagnostics: overlay refs, behavior-case schema, coverage policy");
        println!("  code actions: definition query, coverage query, codegen plan");
        println!(
            "  commands: axiograph.authoring.codegenPlan, axiograph.authoring.coverageQuery, axiograph.authoring.softwareCoverage"
        );
    }
    Ok(())
}

fn print_integration_manifest(json_output: bool) -> Result<()> {
    let manifest = software_authoring_integration_manifest_v1();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
    } else {
        println!("software authoring integration manifest:");
        println!("  lsp: axiograph authoring lsp");
        println!("  mcp: axiograph authoring mcp");
        println!("  transport: stdio, host-managed background process");
    }
    Ok(())
}

pub fn software_authoring_tool_specs_v1() -> Value {
    json!({
        "version": "axiograph_software_authoring_tool_specs_v1",
        "tools": [
            {
                "name": "axiograph.authoring.continuous_check",
                "description": "Check a behavior_case_report_v1 against semantic coverage, competency, codegen, code-ref, and runtime-theory expectations.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.materialize_skeletons",
                "description": "Materialize generated test skeleton previews from a behavior_case_report_v1 into a review directory.",
                "mutation": "writes_files"
            },
            {
                "name": "axiograph.authoring.codegen_plan",
                "description": "Return codegen file hints and caveats from a typed tooling overlay.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.coverage_query",
                "description": "Run an exploratory coverage query over canonical .axi plus an optional tooling overlay.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.overlay_check",
                "description": "Validate typed software-authoring overlay refs against canonical .axi / KernelModuleIr.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.definition_query",
                "description": "Weak authoring query for defining a process, function, business rule, or implementation surface.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.lsp",
                "description": "lsp-server/lsp-types stdio LSP editor loop for authoring diagnostics and read-only command execution.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.lsp_capabilities",
                "description": "Return editor/LSP capability metadata for integrating Axiograph authoring tools.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.integration_manifest",
                "description": "Return host launch metadata for LSP, rmcp MCP, and background authoring processes.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.mcp",
                "description": "Read-only stdio MCP server exposing Axiograph authoring, coverage, codegen planning, and definition-query tools.",
                "mutation": "read_only"
            }
        ]
    })
}

pub fn software_authoring_integration_manifest_v1() -> Value {
    json!({
        "version": "axiograph_software_authoring_integration_manifest_v1",
        "process_model": {
            "lsp": "host_managed_background_process",
            "mcp": "host_managed_background_process",
            "shutdown": "send LSP shutdown/exit or MCP exit notification, close stdin, or let the host terminate the process",
            "mutation_policy": "mcp_tools_are_read_only; lsp_commands_are_read_only; file_materialization_is_cli_only"
        },
        "lsp": {
            "transport": "stdio",
            "command": "axiograph",
            "args": ["authoring", "lsp"],
            "standalone_command": "axiograph-software-authoring",
            "standalone_args": ["lsp"],
            "implementation": {
                "transport_crate": "lsp-server",
                "transport_version": "0.7.9",
                "types_crate": "lsp-types",
                "types_version": "0.97"
            },
            "language_id": "axiograph",
            "document_selector": [
                { "language": "axiograph", "pattern": "**/*.axi" },
                { "language": "json", "pattern": "**/*tooling_overlay*.json" },
                { "language": "json", "pattern": "**/*behavior_case*.json" }
            ]
        },
        "mcp": {
            "transport": "stdio",
            "command": "axiograph",
            "args": ["authoring", "mcp"],
            "standalone_command": "axiograph-software-authoring",
            "standalone_args": ["mcp"],
            "implementation": {
                "crate": "rmcp",
                "version": "1.5.0",
                "features": ["server", "transport-io", "macros", "schemars"]
            },
            "capabilities": { "tools": {} },
            "tools": software_authoring_mcp_tools_v1()
        },
        "host_notes": [
            "Cursor, Codex, Claude Code, and similar hosts should launch the MCP command as a local stdio server and keep it alive for the session.",
            "Editor clients should launch the LSP command as a standard stdio language server.",
            "Generated files must be materialized through explicit CLI commands, not MCP tool calls."
        ],
        "non_claims": [
            "MCP tool results are authoring guidance and runtime checks, not Lean certification.",
            "LSP diagnostics are local feedback and do not mutate accepted ontology state."
        ]
    })
}

pub fn software_authoring_lsp_capabilities_v1() -> Value {
    json!({
        "version": "axiograph_software_authoring_lsp_capabilities_v1",
        "language_id": "axiograph",
        "document_selector": [
            { "language": "axiograph", "pattern": "**/*.axi" },
            { "language": "json", "pattern": "**/*tooling_overlay*.json" },
            { "language": "json", "pattern": "**/*behavior_case*.json" }
        ],
        "diagnostics": [
            "canonical_axi_parse",
            "runtime_theory_check",
            "overlay_ref_resolution",
            "behavior_case_schema",
            "coverage_policy"
        ],
        "code_actions": [
            "axiograph.authoring.definitionQuery",
            "axiograph.authoring.coverageQuery",
            "axiograph.authoring.codegenPlan",
            "axiograph.authoring.overlayCheck",
            "axiograph.authoring.softwareCoverage"
        ],
        "commands": [
            "axiograph.authoring.overlayCheck",
            "axiograph.authoring.softwareCoverage",
            "axiograph.authoring.codegenPlan",
            "axiograph.authoring.coverageQuery",
            "axiograph.authoring.definitionQuery",
            "axiograph.authoring.lspCapabilities"
        ],
        "non_claims": [
            "LSP/editor diagnostics are runtime authoring feedback, not Lean certification.",
            "Editor code actions are review artifacts and must not mutate accepted ontology state directly."
        ]
    })
}

pub fn run_lsp_stdio() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    run_lsp_connection(connection)?;
    io_threads
        .join()
        .context("join Axiograph authoring LSP stdio threads")?;
    Ok(())
}

pub fn run_mcp_stdio() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime for rmcp stdio server")?;
    runtime.block_on(run_rmcp_mcp_stdio())
}

async fn run_rmcp_mcp_stdio() -> Result<()> {
    let service = AuthoringRmcpServer
        .serve(rmcp::transport::stdio())
        .await
        .context("serve Axiograph authoring MCP server with rmcp stdio transport")?;
    service
        .waiting()
        .await
        .context("wait for Axiograph authoring MCP server shutdown")?;
    Ok(())
}

#[derive(Clone)]
struct AuthoringRmcpServer;

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct EmptyMcpArgs {}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct OverlayMcpArgs {
    #[serde(default)]
    overlay: Option<Value>,
    #[serde(default)]
    overlay_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct AxiOverlayMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<Value>,
    #[serde(default)]
    overlay_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct DefinitionQueryMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<Value>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default)]
    definition_query: Option<Value>,
    #[serde(default)]
    query: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct CoverageQueryMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<Value>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default)]
    coverage_query: Option<Value>,
    #[serde(default)]
    query: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct SoftwareCoverageMcpArgs {
    behavior_report: Value,
    #[serde(default)]
    overlay: Option<Value>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default = "default_repo_root")]
    repo_root: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AuthoringMcpPayload {
    payload: Value,
}

fn default_repo_root() -> String {
    ".".to_string()
}

fn authoring_mcp_payload(value: Value) -> Json<AuthoringMcpPayload> {
    Json(AuthoringMcpPayload { payload: value })
}

fn tool_arguments_value<T: Serialize>(args: T) -> std::result::Result<Value, String> {
    serde_json::to_value(args).map_err(|err| err.to_string())
}

#[tool_router]
impl AuthoringRmcpServer {
    #[tool(
        name = "axiograph_authoring_lsp_capabilities",
        description = "Return editor/LSP capability metadata for Axiograph authoring."
    )]
    fn lsp_capabilities(
        &self,
        Parameters(_args): Parameters<EmptyMcpArgs>,
    ) -> Json<AuthoringMcpPayload> {
        authoring_mcp_payload(software_authoring_lsp_capabilities_v1())
    }

    #[tool(
        name = "axiograph_authoring_integration_manifest",
        description = "Return host launch metadata for lsp-server LSP, rmcp MCP, and background authoring processes."
    )]
    fn integration_manifest(
        &self,
        Parameters(_args): Parameters<EmptyMcpArgs>,
    ) -> Json<AuthoringMcpPayload> {
        authoring_mcp_payload(software_authoring_integration_manifest_v1())
    }

    #[tool(
        name = "axiograph_authoring_codegen_plan",
        description = "Return generated skeleton file hints from a typed tooling overlay."
    )]
    fn codegen_plan(
        &self,
        Parameters(args): Parameters<OverlayMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_codegen_plan",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }

    #[tool(
        name = "axiograph_authoring_overlay_check",
        description = "Validate typed overlay refs against canonical .axi text."
    )]
    fn overlay_check(
        &self,
        Parameters(args): Parameters<AxiOverlayMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_overlay_check",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }

    #[tool(
        name = "axiograph_authoring_definition_query",
        description = "Run a weak definition query for a process, function, business rule, relation, or surface."
    )]
    fn definition_query(
        &self,
        Parameters(args): Parameters<DefinitionQueryMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_definition_query",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }

    #[tool(
        name = "axiograph_authoring_coverage_query",
        description = "Run an exploratory coverage query over canonical .axi text and optional overlay data."
    )]
    fn coverage_query(
        &self,
        Parameters(args): Parameters<CoverageQueryMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_coverage_query",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }

    #[tool(
        name = "axiograph_authoring_software_coverage",
        description = "Evaluate a behavior-case report against a tooling overlay and repository root."
    )]
    fn software_coverage(
        &self,
        Parameters(args): Parameters<SoftwareCoverageMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_software_coverage",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }
}

#[tool_handler]
impl rmcp::handler::server::ServerHandler for AuthoringRmcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("axiograph-software-authoring", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub fn software_authoring_mcp_tools_v1() -> Value {
    json!([
        {
            "name": "axiograph_authoring_lsp_capabilities",
            "title": "Axiograph LSP Capabilities",
            "description": "Return editor/LSP capability metadata for Axiograph authoring.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "axiograph_authoring_integration_manifest",
            "title": "Axiograph Integration Manifest",
            "description": "Return host launch metadata for lsp-server LSP, rmcp MCP, and background authoring processes.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "axiograph_authoring_codegen_plan",
            "title": "Axiograph Codegen Plan",
            "description": "Return generated skeleton file hints from a typed tooling overlay.",
            "inputSchema": authoring_overlay_input_schema()
        },
        {
            "name": "axiograph_authoring_overlay_check",
            "title": "Axiograph Overlay Check",
            "description": "Validate typed overlay refs against canonical .axi text.",
            "inputSchema": authoring_axi_overlay_input_schema()
        },
        {
            "name": "axiograph_authoring_definition_query",
            "title": "Axiograph Definition Query",
            "description": "Run a weak definition query for a process, function, business rule, relation, or surface.",
            "inputSchema": authoring_axi_query_input_schema("definition_query")
        },
        {
            "name": "axiograph_authoring_coverage_query",
            "title": "Axiograph Coverage Query",
            "description": "Run an exploratory coverage query over canonical .axi text and optional overlay data.",
            "inputSchema": authoring_axi_query_input_schema("coverage_query")
        },
        {
            "name": "axiograph_authoring_software_coverage",
            "title": "Axiograph Software Coverage",
            "description": "Evaluate a behavior-case report against a tooling overlay and repository root.",
            "inputSchema": authoring_software_coverage_input_schema()
        }
    ])
}

fn call_authoring_mcp_tool(params: Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("tools/call requires params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "axiograph_authoring_lsp_capabilities" => Ok(software_authoring_lsp_capabilities_v1()),
        "axiograph_authoring_integration_manifest" => {
            Ok(software_authoring_integration_manifest_v1())
        }
        "axiograph_authoring_codegen_plan" => {
            let overlay = overlay_arg(&arguments)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::codegen_plan_report(&overlay),
            )?)
        }
        "axiograph_authoring_overlay_check" => {
            let axi_text = arguments
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("overlay check requires `axi_text`"))?;
            let overlay = overlay_arg(&arguments)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::validate_overlay_bundle(&kernel, &overlay),
            )?)
        }
        "axiograph_authoring_definition_query" => {
            let axi_text = arguments
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("definition query requires `axi_text`"))?;
            let query = arguments
                .get("definition_query")
                .or_else(|| arguments.get("query"))
                .cloned()
                .ok_or_else(|| {
                    anyhow!("definition query requires `definition_query` or `query`")
                })?;
            let query: axiograph_tooling_overlays::DefinitionQueryV1 =
                serde_json::from_value(query)?;
            let overlay = optional_overlay_arg(&arguments)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::definition_query_report(
                    &kernel,
                    overlay.as_ref(),
                    &query,
                ),
            )?)
        }
        "axiograph_authoring_coverage_query" => {
            let axi_text = arguments
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("coverage query requires `axi_text`"))?;
            let query = arguments
                .get("coverage_query")
                .or_else(|| arguments.get("query"))
                .cloned()
                .ok_or_else(|| anyhow!("coverage query requires `coverage_query` or `query`"))?;
            let query: axiograph_tooling_overlays::CoverageQueryV1 = serde_json::from_value(query)?;
            let overlay = optional_overlay_arg(&arguments)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::coverage_query_report(
                    &kernel,
                    overlay.as_ref(),
                    &query,
                ),
            )?)
        }
        "axiograph_authoring_software_coverage" => {
            let behavior_report = arguments
                .get("behavior_report")
                .cloned()
                .ok_or_else(|| anyhow!("software coverage requires `behavior_report`"))?;
            let overlay = overlay_arg(&arguments)?;
            let repo_root = arguments
                .get("repo_root")
                .and_then(Value::as_str)
                .unwrap_or(".");
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::continuous_coverage_report_from_behavior_report(
                    &behavior_report,
                    &overlay,
                    Path::new(repo_root),
                ),
            )?)
        }
        other => Err(anyhow!("unsupported authoring MCP tool `{other}`")),
    }
}

fn authoring_overlay_input_schema() -> Value {
    let overlay_schema = axiograph_tooling_overlays::tooling_overlay_bundle_schema();
    json!({
        "type": "object",
        "oneOf": [
            { "required": ["overlay"] },
            { "required": ["overlay_text"] }
        ],
        "properties": {
            "overlay": overlay_schema,
            "overlay_text": { "type": "string" }
        },
        "additionalProperties": false
    })
}

fn authoring_axi_overlay_input_schema() -> Value {
    let overlay_schema = axiograph_tooling_overlays::tooling_overlay_bundle_schema();
    json!({
        "type": "object",
        "required": ["axi_text"],
        "properties": {
            "axi_text": { "type": "string" },
            "overlay": overlay_schema,
            "overlay_text": { "type": "string" }
        },
        "oneOf": [
            { "required": ["axi_text", "overlay"] },
            { "required": ["axi_text", "overlay_text"] }
        ],
        "additionalProperties": false
    })
}

fn authoring_axi_query_input_schema(query_key: &str) -> Value {
    let overlay_schema = axiograph_tooling_overlays::tooling_overlay_bundle_schema();
    let query_schema = match query_key {
        "definition_query" => axiograph_tooling_overlays::definition_query_schema(),
        "coverage_query" => axiograph_tooling_overlays::coverage_query_schema(),
        _ => json!({ "type": "object" }),
    };
    json!({
        "type": "object",
        "required": ["axi_text", query_key],
        "properties": {
            "axi_text": { "type": "string" },
            "overlay": overlay_schema,
            "overlay_text": { "type": "string" },
            "definition_query": axiograph_tooling_overlays::definition_query_schema(),
            "coverage_query": axiograph_tooling_overlays::coverage_query_schema(),
            "query": query_schema
        },
        "additionalProperties": false
    })
}

fn authoring_software_coverage_input_schema() -> Value {
    let overlay_schema = axiograph_tooling_overlays::tooling_overlay_bundle_schema();
    json!({
        "type": "object",
        "required": ["behavior_report"],
        "oneOf": [
            { "required": ["behavior_report", "overlay"] },
            { "required": ["behavior_report", "overlay_text"] }
        ],
        "properties": {
            "behavior_report": { "type": "object" },
            "overlay": overlay_schema,
            "overlay_text": { "type": "string" },
            "repo_root": { "type": "string", "default": "." }
        },
        "additionalProperties": false
    })
}

fn run_lsp_connection(connection: Connection) -> Result<()> {
    let (initialize_id, _initialize_params) = connection
        .initialize_start()
        .context("receive LSP initialize request")?;
    connection
        .initialize_finish(
            initialize_id,
            serde_json::to_value(lsp_initialize_result())?,
        )
        .context("finish LSP initialization")?;

    let mut state = AuthoringLspStateV1::default();
    for message in &connection.receiver {
        let messages = match message {
            Message::Request(request) => {
                if connection
                    .handle_shutdown(&request)
                    .context("handle LSP shutdown")?
                {
                    break;
                }
                handle_lsp_request_message_v1(&mut state, request)?
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    break;
                }
                handle_lsp_notification_message_v1(&mut state, notification)?
            }
            Message::Response(_) => Vec::new(),
        };
        for message in messages {
            connection
                .sender
                .send(message)
                .context("send LSP response/notification")?;
        }
        if state.exit_requested {
            break;
        }
    }
    Ok(())
}

fn handle_lsp_request_message_v1(
    state: &mut AuthoringLspStateV1,
    request: Request,
) -> Result<Vec<Message>> {
    let request = json!({
        "id": request.id,
        "method": request.method,
        "params": request.params,
    });
    lsp_json_messages_to_typed(handle_lsp_message_v1(state, request))
}

fn handle_lsp_notification_message_v1(
    state: &mut AuthoringLspStateV1,
    notification: Notification,
) -> Result<Vec<Message>> {
    let notification = json!({
        "method": notification.method,
        "params": notification.params,
    });
    lsp_json_messages_to_typed(handle_lsp_message_v1(state, notification))
}

fn lsp_json_messages_to_typed(messages: Vec<Value>) -> Result<Vec<Message>> {
    messages
        .into_iter()
        .map(|message| serde_json::from_value(message).context("convert LSP JSON to typed message"))
        .collect()
}

#[derive(Debug, Default)]
pub struct AuthoringLspStateV1 {
    documents: BTreeMap<String, String>,
    shutdown_requested: bool,
    exit_requested: bool,
}

pub fn handle_lsp_message_v1(state: &mut AuthoringLspStateV1, message: Value) -> Vec<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    match method {
        "initialize" => id
            .map(|id| {
                lsp_response(
                    id,
                    json!({
                        "capabilities": lsp_server_capabilities(),
                        "serverInfo": {
                            "name": "axiograph-software-authoring",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                )
            })
            .into_iter()
            .collect(),
        "initialized" => Vec::new(),
        "shutdown" => {
            state.shutdown_requested = true;
            id.map(|id| lsp_response(id, Value::Null))
                .into_iter()
                .collect()
        }
        "exit" => {
            state.exit_requested = true;
            Vec::new()
        }
        "textDocument/didOpen" => {
            let uri = params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = params
                .pointer("/textDocument/text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !uri.is_empty() {
                state.documents.insert(uri.clone(), text.clone());
                vec![publish_diagnostics(
                    &uri,
                    diagnostics_for_document(&uri, &text),
                )]
            } else {
                Vec::new()
            }
        }
        "textDocument/didChange" => {
            let uri = params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = params
                .get("contentChanges")
                .and_then(Value::as_array)
                .and_then(|changes| changes.last())
                .and_then(|change| change.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !uri.is_empty() {
                state.documents.insert(uri.clone(), text.clone());
                vec![publish_diagnostics(
                    &uri,
                    diagnostics_for_document(&uri, &text),
                )]
            } else {
                Vec::new()
            }
        }
        "textDocument/codeAction" => id
            .map(|id| lsp_response(id, authoring_code_actions()))
            .into_iter()
            .collect(),
        "workspace/executeCommand" => id
            .map(|id| match execute_lsp_authoring_command(params) {
                Ok(result) => lsp_response(id, result),
                Err(err) => {
                    lsp_error_response(id, ErrorCode::RequestFailed as i32, err.to_string())
                }
            })
            .into_iter()
            .collect(),
        _ if id.is_some() => vec![lsp_error_response(
            id.unwrap(),
            ErrorCode::MethodNotFound as i32,
            format!("unsupported LSP method `{method}`"),
        )],
        _ => Vec::new(),
    }
}

fn lsp_initialize_result() -> InitializeResult {
    let caps = software_authoring_lsp_capabilities_v1();
    let commands = caps["commands"]
        .as_array()
        .map(|commands| {
            commands
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    InitializeResult {
        capabilities: LspServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            ..LspServerCapabilities::default()
        },
        server_info: Some(LspServerInfo {
            name: "axiograph-software-authoring".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}

fn lsp_server_capabilities() -> Value {
    serde_json::to_value(lsp_initialize_result().capabilities)
        .unwrap_or_else(|_| json!({ "textDocumentSync": 1 }))
}

fn authoring_code_actions() -> Value {
    json!([
        {
            "title": "Axiograph: run weak definition query",
            "kind": "quickfix",
            "command": {
                "title": "Run definition query",
                "command": "axiograph.authoring.definitionQuery"
            }
        },
        {
            "title": "Axiograph: run exploratory coverage query",
            "kind": "quickfix",
            "command": {
                "title": "Run coverage query",
                "command": "axiograph.authoring.coverageQuery"
            }
        },
        {
            "title": "Axiograph: plan generated tests",
            "kind": "source",
            "command": {
                "title": "Plan codegen",
                "command": "axiograph.authoring.codegenPlan"
            }
        },
        {
            "title": "Axiograph: check overlay refs",
            "kind": "source",
            "command": {
                "title": "Check overlay",
                "command": "axiograph.authoring.overlayCheck"
            }
        },
        {
            "title": "Axiograph: check software coverage",
            "kind": "source",
            "command": {
                "title": "Check software coverage",
                "command": "axiograph.authoring.softwareCoverage"
            }
        }
    ])
}

fn execute_lsp_authoring_command(params: Value) -> Result<Value> {
    let command = params
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("workspace/executeCommand requires params.command"))?;
    let arg = params
        .get("arguments")
        .and_then(Value::as_array)
        .and_then(|args| args.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    match command {
        "axiograph.authoring.codegenPlan" => {
            let overlay = overlay_arg(&arg)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::codegen_plan_report(&overlay),
            )?)
        }
        "axiograph.authoring.overlayCheck" => {
            let axi_text = arg
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("overlayCheck requires arguments[0].axi_text"))?;
            let overlay = overlay_arg(&arg)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::validate_overlay_bundle(&kernel, &overlay),
            )?)
        }
        "axiograph.authoring.definitionQuery" => {
            let axi_text = arg
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("definitionQuery requires arguments[0].axi_text"))?;
            let query = arg
                .get("query")
                .cloned()
                .ok_or_else(|| anyhow!("definitionQuery requires arguments[0].query"))?;
            let query: axiograph_tooling_overlays::DefinitionQueryV1 =
                serde_json::from_value(query)?;
            let overlay = optional_overlay_arg(&arg)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::definition_query_report(
                    &kernel,
                    overlay.as_ref(),
                    &query,
                ),
            )?)
        }
        "axiograph.authoring.coverageQuery" => {
            let axi_text = arg
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("coverageQuery requires arguments[0].axi_text"))?;
            let query = arg
                .get("query")
                .cloned()
                .ok_or_else(|| anyhow!("coverageQuery requires arguments[0].query"))?;
            let query: axiograph_tooling_overlays::CoverageQueryV1 = serde_json::from_value(query)?;
            let overlay = optional_overlay_arg(&arg)?;
            let kernel = axiograph_tooling_overlays::compile_kernel_from_axi_text(axi_text)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::coverage_query_report(
                    &kernel,
                    overlay.as_ref(),
                    &query,
                ),
            )?)
        }
        "axiograph.authoring.softwareCoverage" => {
            let behavior_report = arg
                .get("behavior_report")
                .cloned()
                .ok_or_else(|| anyhow!("softwareCoverage requires arguments[0].behavior_report"))?;
            let overlay = overlay_arg(&arg)?;
            let repo_root = arg.get("repo_root").and_then(Value::as_str).unwrap_or(".");
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::continuous_coverage_report_from_behavior_report(
                    &behavior_report,
                    &overlay,
                    Path::new(repo_root),
                ),
            )?)
        }
        "axiograph.authoring.lspCapabilities" => Ok(software_authoring_lsp_capabilities_v1()),
        other => Err(anyhow!("unsupported authoring LSP command `{other}`")),
    }
}

fn overlay_arg(value: &Value) -> Result<axiograph_tooling_overlays::ToolingOverlayBundleV1> {
    optional_overlay_arg(value)?
        .ok_or_else(|| anyhow!("command requires `overlay` or `overlay_text`"))
}

fn optional_overlay_arg(
    value: &Value,
) -> Result<Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>> {
    if let Some(overlay_text) = value.get("overlay_text").and_then(Value::as_str) {
        return axiograph_tooling_overlays::parse_overlay_bundle(overlay_text).map(Some);
    }
    if let Some(overlay) = value.get("overlay").cloned() {
        let bundle: axiograph_tooling_overlays::ToolingOverlayBundleV1 =
            serde_json::from_value(overlay)?;
        if bundle.version != axiograph_tooling_overlays::TOOLING_OVERLAY_BUNDLE_VERSION_V1 {
            return Err(anyhow!(
                "expected overlay version `{}`, got `{}`",
                axiograph_tooling_overlays::TOOLING_OVERLAY_BUNDLE_VERSION_V1,
                bundle.version
            ));
        }
        return Ok(Some(bundle));
    }
    Ok(None)
}

fn diagnostics_for_document(uri: &str, text: &str) -> Vec<Diagnostic> {
    if uri.ends_with(".axi") || text.trim_start().starts_with("module ") {
        match axiograph_tooling_overlays::compile_kernel_from_axi_text(text) {
            Ok(_) => Vec::new(),
            Err(err) => vec![diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.axi",
                &err.to_string(),
                0,
                0,
            )],
        }
    } else if uri.ends_with(".json") || text.trim_start().starts_with('{') {
        diagnostics_for_json_document(text)
    } else {
        Vec::new()
    }
}

fn diagnostics_for_json_document(text: &str) -> Vec<Diagnostic> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(err) => {
            return vec![diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.json",
                &format!("invalid JSON: {err}"),
                err.line().saturating_sub(1),
                err.column().saturating_sub(1),
            )]
        }
    };
    let mut diagnostics = Vec::new();
    if value.get("behavior_case").is_some() {
        if value.pointer("/behavior_case/context").is_some() {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.behavior_case.stale_tooling",
                "BehaviorCaseV1 is domain-only; move context/tooling fields into a tooling overlay",
                0,
                0,
            ));
        }
        if value
            .pointer("/behavior_case/implementation_surfaces")
            .is_some()
        {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.behavior_case.stale_tooling",
                "BehaviorCaseV1 must not embed implementation_surfaces; use ToolingOverlayBundleV1",
                0,
                0,
            ));
        }
        if value.pointer("/behavior_case/coverage_edges").is_some() {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.behavior_case.stale_tooling",
                "BehaviorCaseV1 must not embed coverage_edges; use ToolingOverlayBundleV1",
                0,
                0,
            ));
        }
    }
    if value.get("version").and_then(Value::as_str) == Some("tooling_overlay_bundle_v1") {
        if let Err(err) = serde_json::from_value::<axiograph_tooling_overlays::ToolingOverlayBundleV1>(
            value.clone(),
        ) {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.overlay.schema",
                &format!("invalid ToolingOverlayBundleV1: {err}"),
                0,
                0,
            ));
        }
    }
    diagnostics
}

fn publish_diagnostics(uri: &str, diagnostics: Vec<Diagnostic>) -> Value {
    let params = publish_diagnostics_params(uri, diagnostics);
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": params
    })
}

fn publish_diagnostics_params(uri: &str, diagnostics: Vec<Diagnostic>) -> Value {
    let Ok(uri) = uri.parse::<Uri>() else {
        return json!({
            "uri": uri,
            "diagnostics": diagnostics
        });
    };
    serde_json::to_value(PublishDiagnosticsParams::new(uri, diagnostics, None))
        .expect("serialize lsp_types PublishDiagnosticsParams")
}

fn diagnostic(
    severity: DiagnosticSeverity,
    source: &str,
    message: &str,
    line: usize,
    character: usize,
) -> Diagnostic {
    let line = line.min(u32::MAX as usize) as u32;
    let character = character.min(u32::MAX as usize) as u32;
    Diagnostic {
        range: Range::new(
            Position::new(line, character),
            Position::new(line, character.saturating_add(1)),
        ),
        severity: Some(severity),
        source: Some(source.to_string()),
        message: message.to_string(),
        ..Diagnostic::default()
    }
}

fn lsp_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn lsp_error_response(id: Value, code: i32, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read `{}`", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse `{}` as JSON", path.display()))
}

fn validate_behavior_report(report: &Value) -> Result<()> {
    match path_string(report, &["version"]).as_deref() {
        Some("behavior_case_report_v1") => Ok(()),
        other => Err(anyhow!(
            "expected behavior_case_report_v1, got {:?}",
            other.unwrap_or("<missing>")
        )),
    }
}

#[derive(Debug)]
struct CodegenPreview {
    language: String,
    file_hint: Option<String>,
    content: Option<String>,
}

fn collect_codegen_previews(report: &Value) -> Vec<CodegenPreview> {
    report
        .get("codegen_previews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|preview| {
            let language = preview.get("language")?.as_str()?.trim().to_lowercase();
            Some(CodegenPreview {
                language,
                file_hint: preview
                    .get("file_hint")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: preview
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn coverage_summary(report: &Value) -> CoverageSummary {
    let mut summary = CoverageSummary {
        total_rules: path_u64(report, &["context_report", "coverage", "total_rules"]),
        runtime_enforced_rules: path_u64(
            report,
            &["context_report", "coverage", "runtime_enforced_rules"],
        ),
        covered_rules: path_u64(report, &["context_report", "coverage", "covered_rules"]),
        tested_rules: path_u64(report, &["context_report", "coverage", "tested_rules"]),
        implemented_rules: path_u64(report, &["context_report", "coverage", "implemented_rules"]),
        ontology_only_rules: path_u64(
            report,
            &["context_report", "coverage", "ontology_only_rules"],
        ),
        drifted_rules: path_u64(report, &["context_report", "coverage", "drifted_rules"]),
        missing_obligations: path_string_array(
            report,
            &["context_report", "coverage", "missing_obligations"],
        ),
        uncovered_rule_ids: path_string_array(
            report,
            &["context_report", "coverage", "uncovered_rule_ids"],
        ),
        next_actions: path_string_array(report, &["context_report", "coverage", "next_actions"]),
    };
    if summary.total_rules == 0 {
        summary.total_rules = path_u64(report, &["case_receipt", "matched_rule_ids"]);
    }
    summary
}

fn competency_summary(report: &Value) -> CompetencySummary {
    let questions = report
        .pointer("/context_report/competency_coverage/questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unsatisfied_questions = questions
        .iter()
        .filter(|question| question.get("satisfied").and_then(Value::as_bool) != Some(true))
        .filter_map(|question| {
            question
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();

    CompetencySummary {
        total: path_u64(report, &["context_report", "competency_coverage", "total"]),
        satisfied: path_u64(
            report,
            &["context_report", "competency_coverage", "satisfied"],
        ),
        coverage: path_f64(
            report,
            &["context_report", "competency_coverage", "coverage"],
        ),
        unsatisfied_questions,
    }
}

fn runtime_theory_presence(report: &Value) -> RuntimeTheoryPresence {
    let Some(theory) = first_object_by_key(report, &["runtime_theory_check", "runtime_theory"])
    else {
        return RuntimeTheoryPresence {
            present: false,
            ontology_closed: None,
            complete: None,
            blocking_judgments: None,
            residual_obligations: None,
            notes: Vec::new(),
        };
    };
    RuntimeTheoryPresence {
        present: true,
        ontology_closed: find_bool(theory, &["ontology_closed", "closed"]),
        complete: find_bool(theory, &["complete", "complete_under_assumptions"]),
        blocking_judgments: find_u64(theory, &["blocking_judgments", "blocking_count"]),
        residual_obligations: find_u64(theory, &["residual_obligations", "residual_count"]),
        notes: collect_string_arrays(theory, "notes")
            .into_iter()
            .collect::<Vec<_>>(),
    }
}

fn path_string(value: &Value, path: &[&str]) -> Option<String> {
    get_path(value, path)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn path_u64(value: &Value, path: &[&str]) -> u64 {
    get_path(value, path)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_array().map(|values| values.len() as u64))
        })
        .unwrap_or(0)
}

fn path_f64(value: &Value, path: &[&str]) -> f64 {
    get_path(value, path).and_then(Value::as_f64).unwrap_or(0.0)
}

fn path_string_array(value: &Value, path: &[&str]) -> Vec<String> {
    get_path(value, path)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn normalize_languages(languages: &[String]) -> BTreeSet<String> {
    languages
        .iter()
        .map(|language| language.trim().to_lowercase())
        .filter(|language| !language.is_empty())
        .collect()
}

fn collect_string_arrays(value: &Value, key: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    collect_string_arrays_inner(value, key, &mut found);
    found
}

fn collect_string_arrays_inner(value: &Value, key: &str, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(values)) = map.get(key) {
                for value in values {
                    if let Some(text) = value.as_str() {
                        found.insert(text.to_string());
                    }
                }
            }
            for nested in map.values() {
                collect_string_arrays_inner(nested, key, found);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_string_arrays_inner(nested, key, found);
            }
        }
        _ => {}
    }
}

fn first_object_by_key<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(candidate @ Value::Object(_)) = map.get(*key) {
                    return Some(candidate);
                }
            }
            for nested in map.values() {
                if let Some(found) = first_object_by_key(nested, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(values) => values
            .iter()
            .find_map(|nested| first_object_by_key(nested, keys)),
        _ => None,
    }
}

fn find_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(bool_value) = map.get(*key).and_then(Value::as_bool) {
                    return Some(bool_value);
                }
            }
            map.values().find_map(|nested| find_bool(nested, keys))
        }
        Value::Array(values) => values.iter().find_map(|nested| find_bool(nested, keys)),
        _ => None,
    }
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(number) = map.get(*key).and_then(Value::as_u64) {
                    return Some(number);
                }
            }
            map.values().find_map(|nested| find_u64(nested, keys))
        }
        Value::Array(values) => values.iter().find_map(|nested| find_u64(nested, keys)),
        _ => None,
    }
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() {
        bail!("refusing absolute generated path `{relative}`");
    }
    let mut out = base.to_path_buf();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("refusing unsafe generated path `{relative}`");
            }
        }
    }
    Ok(out)
}

fn dedup_strings(strings: &mut Vec<String>) {
    let mut counts: BTreeMap<String, ()> = BTreeMap::new();
    strings.retain(|item| counts.insert(item.clone(), ()).is_none());
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_server::{Message, Notification, Request};
    use rmcp::handler::server::tool::IntoCallToolResult;
    use serde_json::json;
    use std::{thread, time::Duration};

    #[test]
    fn continuous_check_warns_on_unmaterialized_code_refs_but_checks_codegen() {
        let report = json!({
            "version": "behavior_case_report_v1",
            "behavior_case": {
                "case_id": "software_authoring.reserve_credit",
                "context": {
                    "surfaces": [
                        {"code_refs": ["services/orders/reserve_credit.go"]}
                    ]
                }
            },
            "context_report": {
                "coverage": {
                    "total_rules": 2,
                    "covered_rules": 1,
                    "tested_rules": 1,
                    "implemented_rules": 0,
                    "ontology_only_rules": 1,
                    "drifted_rules": 0,
                    "missing_obligations": ["map one ontology-only rule"],
                    "next_actions": ["add coverage edge"]
                },
                "competency_coverage": {
                    "total": 1,
                    "satisfied": 1,
                    "coverage": 1.0,
                    "questions": [
                        {"name": "cq", "satisfied": true}
                    ]
                }
            },
            "codegen_previews": [
                {"language": "go", "file_hint": "tests/case_test.go", "content": "package tests\n"},
                {"language": "python", "file_hint": "tests/test_case.py", "content": "def test_case(): pass\n"},
                {"language": "rust", "file_hint": "tests/case.rs", "content": "#[test] fn case() {}\n"},
                {"language": "typescript", "file_hint": "tests/case.spec.ts", "content": "it('case', () => {})\n"}
            ]
        });
        let args = ContinuousCheckOptions {
            repo_root: PathBuf::from("/definitely/not/a/real/repo"),
            require_codegen: vec![
                "go".into(),
                "python".into(),
                "rust".into(),
                "typescript".into(),
            ],
            strict_coverage: false,
            require_code_refs: false,
            require_runtime_theory: false,
        };

        let coverage =
            build_continuous_software_coverage_report(&report, &args).expect("build report");
        assert!(coverage.pass);
        assert_eq!(coverage.status, CoverageGateStatus::PassedWithWarnings);
        assert_eq!(coverage.missing_codegen_languages, Vec::<String>::new());
        assert_eq!(coverage.missing_code_refs.len(), 1);
        assert!(coverage
            .warnings
            .iter()
            .any(|warning| warning.contains("semantic coverage is incomplete")));
    }

    #[test]
    fn continuous_check_fails_on_missing_required_codegen() {
        let report = json!({
            "version": "behavior_case_report_v1",
            "behavior_case": {"case_id": "case"},
            "context_report": {
                "coverage": {"total_rules": 1, "covered_rules": 1, "drifted_rules": 0},
                "competency_coverage": {"total": 0, "satisfied": 0, "coverage": 1.0}
            },
            "codegen_previews": [
                {"language": "rust", "file_hint": "tests/case.rs", "content": "#[test] fn case() {}\n"}
            ]
        });
        let args = ContinuousCheckOptions {
            repo_root: PathBuf::from("."),
            require_codegen: vec!["rust".into(), "go".into()],
            strict_coverage: false,
            require_code_refs: false,
            require_runtime_theory: false,
        };

        let coverage =
            build_continuous_software_coverage_report(&report, &args).expect("build report");
        assert!(!coverage.pass);
        assert_eq!(coverage.status, CoverageGateStatus::Failed);
        assert_eq!(coverage.missing_codegen_languages, vec!["go"]);
    }

    #[test]
    fn lsp_initialize_returns_authoring_capabilities() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {}
            }),
        );

        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses[0]["result"]["serverInfo"]["name"],
            json!("axiograph-software-authoring")
        );
        assert!(
            responses[0]["result"]["capabilities"]["executeCommandProvider"]["commands"]
                .as_array()
                .expect("commands")
                .contains(&json!("axiograph.authoring.coverageQuery"))
        );
    }

    #[test]
    fn lsp_server_connection_uses_lsp_server_transport_and_publishes_diagnostics() {
        let (server, client) = Connection::memory();
        let handle = thread::spawn(move || run_lsp_connection(server));

        client
            .sender
            .send(Message::Request(Request::new(
                1.into(),
                "initialize".to_string(),
                json!({ "capabilities": {} }),
            )))
            .expect("send initialize");

        let initialize_response = client
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("receive initialize response");
        match initialize_response {
            Message::Response(response) => {
                assert!(response.error.is_none());
                let result = response.result.expect("initialize result");
                assert_eq!(
                    result["serverInfo"]["name"],
                    json!("axiograph-software-authoring")
                );
                assert!(result["capabilities"]["executeCommandProvider"]["commands"]
                    .as_array()
                    .expect("commands")
                    .contains(&json!("axiograph.authoring.coverageQuery")));
            }
            other => panic!("expected initialize response, got {other:?}"),
        }

        client
            .sender
            .send(Message::Notification(Notification::new(
                "initialized".to_string(),
                Value::Null,
            )))
            .expect("send initialized");
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didOpen".to_string(),
                json!({
                    "textDocument": {
                        "uri": "file:///tmp/behavior_case.json",
                        "languageId": "json",
                        "version": 1,
                        "text": serde_json::to_string(&json!({
                            "version": "behavior_case_v1",
                            "behavior_case": {
                                "case_id": "stale",
                                "context": {},
                                "implementation_surfaces": [],
                                "coverage_edges": []
                            }
                        })).expect("json")
                    }
                }),
            )))
            .expect("send didOpen");

        let diagnostic_message = client
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("receive diagnostics");
        match diagnostic_message {
            Message::Notification(notification) => {
                assert_eq!(notification.method, "textDocument/publishDiagnostics");
                let diagnostics = notification.params["diagnostics"]
                    .as_array()
                    .expect("diagnostics");
                assert_eq!(diagnostics.len(), 3);
            }
            other => panic!("expected diagnostic notification, got {other:?}"),
        }

        client
            .sender
            .send(Message::Request(Request::new(
                2.into(),
                "shutdown".to_string(),
                Value::Null,
            )))
            .expect("send shutdown");
        let shutdown_response = client
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("receive shutdown response");
        assert!(matches!(shutdown_response, Message::Response(_)));
        client
            .sender
            .send(Message::Notification(Notification::new(
                "exit".to_string(),
                Value::Null,
            )))
            .expect("send exit");

        handle.join().expect("join LSP thread").expect("run LSP");
    }

    #[test]
    fn lsp_stale_behavior_case_fields_emit_tooling_overlay_diagnostics() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": "file:///tmp/behavior_case.json",
                        "text": serde_json::to_string(&json!({
                            "version": "behavior_case_v1",
                            "behavior_case": {
                                "case_id": "stale",
                                "context": {},
                                "implementation_surfaces": [],
                                "coverage_edges": []
                            }
                        })).expect("json")
                    }
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        let params: lsp_types::PublishDiagnosticsParams =
            serde_json::from_value(responses[0]["params"].clone()).expect("typed params");
        assert_eq!(params.uri.as_str(), "file:///tmp/behavior_case.json");
        assert_eq!(params.diagnostics.len(), 3);
        assert!(params.diagnostics.iter().all(|diagnostic| {
            diagnostic.source.as_deref() == Some("axiograph.behavior_case.stale_tooling")
                && diagnostic.severity == Some(lsp_types::DiagnosticSeverity::ERROR)
        }));
    }

    #[test]
    fn lsp_codegen_plan_command_returns_file_hints() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "id": "codegen",
                "method": "workspace/executeCommand",
                "params": {
                    "command": "axiograph.authoring.codegenPlan",
                    "arguments": [
                        {
                            "overlay": {
                                "version": "tooling_overlay_bundle_v1",
                                "codegen_plan": {
                                    "languages": ["rust", "typescript"],
                                    "test_name": "reserve_credit"
                                }
                            }
                        }
                    ]
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses[0]["result"]["version"],
            json!("codegen_plan_report_v1")
        );
        assert!(responses[0]["result"]["file_hints"]
            .as_array()
            .expect("file hints")
            .iter()
            .any(|hint| hint.as_str().unwrap_or("").ends_with("reserve_credit.rs")));
    }

    #[test]
    fn integration_manifest_names_production_protocol_crates() {
        let manifest = software_authoring_integration_manifest_v1();

        assert!(manifest.get("mcp_protocol_versions").is_none());
        assert_eq!(
            manifest["lsp"]["implementation"]["transport_crate"],
            json!("lsp-server")
        );
        assert_eq!(
            manifest["lsp"]["implementation"]["types_crate"],
            json!("lsp-types")
        );
        assert_eq!(manifest["mcp"]["implementation"]["crate"], json!("rmcp"));
    }

    #[test]
    fn rmcp_server_info_and_tool_router_expose_read_only_authoring_tools() {
        let server = AuthoringRmcpServer;
        let info = <AuthoringRmcpServer as rmcp::handler::server::ServerHandler>::get_info(&server);

        assert_eq!(info.server_info.name, "axiograph-software-authoring");
        assert!(info.capabilities.tools.is_some());

        let definition_tool =
            <AuthoringRmcpServer as rmcp::handler::server::ServerHandler>::get_tool(
                &server,
                "axiograph_authoring_definition_query",
            )
            .expect("definition query tool");
        assert_eq!(
            definition_tool.name.as_ref(),
            "axiograph_authoring_definition_query"
        );
        assert!(
            <AuthoringRmcpServer as rmcp::handler::server::ServerHandler>::get_tool(
                &server,
                "axiograph_authoring_materialize_skeletons",
            )
            .is_none()
        );
    }

    #[test]
    fn rmcp_codegen_plan_tool_returns_structured_content() {
        let Json(payload) = AuthoringRmcpServer
            .codegen_plan(Parameters(OverlayMcpArgs {
                overlay: Some(json!({
                    "version": "tooling_overlay_bundle_v1",
                    "codegen_plan": {
                        "languages": ["go", "python"],
                        "test_name": "shipment_release"
                    }
                })),
                overlay_text: None,
            }))
            .expect("codegen plan");
        let payload = payload.payload;

        assert_eq!(payload["version"], json!("codegen_plan_report_v1"));
        assert!(payload["file_hints"]
            .as_array()
            .expect("file hints")
            .iter()
            .any(|hint| hint
                .as_str()
                .unwrap_or("")
                .ends_with("shipment_release_test.go")));

        let result = authoring_mcp_payload(payload.clone())
            .into_call_tool_result()
            .expect("rmcp call result");
        assert_eq!(
            result.structured_content.as_ref(),
            Some(&json!({ "payload": payload }))
        );
        assert_eq!(result.is_error, Some(false));
    }

    #[test]
    fn rmcp_codegen_plan_tool_rejects_unknown_overlay_versions() {
        let err = match AuthoringRmcpServer.codegen_plan(Parameters(OverlayMcpArgs {
            overlay: Some(json!({
                "version": "tooling_overlay_bundle_v0",
                "codegen_plan": {
                    "languages": ["go"],
                    "test_name": "shipment_release"
                }
            })),
            overlay_text: None,
        })) {
            Ok(_) => panic!("expected stale overlay version to be rejected"),
            Err(err) => err,
        };

        assert!(err.contains("expected overlay version"));
    }

    #[test]
    fn rmcp_published_tool_specs_exclude_file_materializers() {
        let tools = software_authoring_mcp_tools_v1();
        let tools = tools.as_array().expect("tools");

        assert!(tools
            .iter()
            .any(|tool| tool["name"] == json!("axiograph_authoring_definition_query")));
        assert!(tools
            .iter()
            .all(|tool| tool["name"] != json!("axiograph_authoring_materialize_skeletons")));
        assert!(tools.iter().all(|tool| {
            tool["name"]
                .as_str()
                .map(|name| !name.contains("materialize"))
                .unwrap_or(false)
        }));
    }
}
