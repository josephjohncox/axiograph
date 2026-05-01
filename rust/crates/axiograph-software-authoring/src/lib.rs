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

const COMPETENCY_QUESTION_BUNDLE_VERSION_V1: &str = "competency_question_bundle_v1";
const AUTHORING_COMPETENCY_QUESTIONS_REPORT_VERSION_V1: &str =
    "authoring_competency_questions_report_v1";
const LSP_CMD_CODEGEN_PLAN: &str = "axiograph.authoring.codegenPlan";
const LSP_CMD_COMPETENCY_QUESTIONS: &str = "axiograph.authoring.competencyQuestions";
const LSP_CMD_COVERAGE_QUERY: &str = "axiograph.authoring.coverageQuery";
const LSP_CMD_DEFINITION_QUERY: &str = "axiograph.authoring.definitionQuery";
const LSP_CMD_LSP_CAPABILITIES: &str = "axiograph.authoring.lspCapabilities";
const LSP_CMD_OVERLAY_CHECK: &str = "axiograph.authoring.overlayCheck";
const LSP_CMD_SOFTWARE_COVERAGE: &str = "axiograph.authoring.softwareCoverage";

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
    /// Load/lower question-first `.cq` text and optionally validate refs against canonical `.axi`.
    CompetencyQuestions(CompetencyQuestionsCliArgs),
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

#[derive(Args, Debug, Clone)]
struct CompetencyQuestionsCliArgs {
    /// Optional canonical .axi module used to validate referenced types/relations.
    #[arg(long)]
    axi: Option<PathBuf>,

    /// Question-first `.cq` file.
    #[arg(long)]
    cq: PathBuf,

    /// Emit the report as JSON.
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
    pub authoring_flow: axiograph_tooling_overlays::AuthoringFlowReportV1,
    pub policy: ContinuousCheckPolicySummary,
    pub repo_root: String,
    pub typed_refs: BehaviorReportTypedRefs,
    pub codegen: CodegenCoverageSummary,
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
pub struct ContinuousCheckPolicySummary {
    pub strict_coverage: bool,
    pub require_code_refs: bool,
    pub require_runtime_theory: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct BehaviorReportTypedRefs {
    pub anchors: Vec<String>,
    pub matched_scope_ids: Vec<String>,
    pub matched_rule_ids: Vec<String>,
    pub surface_ids: Vec<String>,
    pub residual_obligations: Vec<String>,
    pub uncovered_rule_ids: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct CodegenCoverageSummary {
    pub preview_count: usize,
    pub required_languages: Vec<String>,
    pub present_languages: Vec<String>,
    pub missing_languages: Vec<String>,
    pub language_statuses: Vec<CodegenLanguageStatus>,
}

#[derive(Debug, Serialize)]
pub struct CodegenLanguageStatus {
    pub language: String,
    pub present: bool,
    pub file_hints: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct RuntimeTheoryPresence {
    pub present: bool,
    pub module_digest: Option<String>,
    pub closure_tiers: Vec<String>,
    pub checked_obligations: Option<u64>,
    pub review_only_obligations: Option<u64>,
    pub ontology_closed: Option<bool>,
    pub complete: Option<bool>,
    pub blocking_judgments: Option<u64>,
    pub residual_obligations: Option<u64>,
    pub blocked_obligations: Option<u64>,
    pub blocking_errors: Option<u64>,
    pub completeness_claim: Option<String>,
    pub ontology_closure_claim: Option<String>,
    pub residual_obligation_ids: Vec<String>,
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
        Commands::CompetencyQuestions(args) => run_competency_questions(args),
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
        println!(
            "  next: run `axiograph authoring materialize-skeletons --behavior-report <behavior_report.json> --out-dir <review_dir>` when the behavior report is reviewed"
        );
    }
    Ok(())
}

fn run_competency_questions(args: CompetencyQuestionsCliArgs) -> Result<()> {
    let cq_text = fs::read_to_string(&args.cq)
        .with_context(|| format!("read competency questions `{}`", args.cq.display()))?;
    let axi_text = args
        .axi
        .as_ref()
        .map(|path| {
            fs::read_to_string(path)
                .with_context(|| format!("read canonical .axi `{}`", path.display()))
        })
        .transpose()?;
    let report =
        build_authoring_competency_questions_report_from_text(axi_text.as_deref(), &cq_text)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human_competency_questions_report(&report);
    }
    Ok(())
}

fn run_continuous_check(args: ContinuousCheckArgs) -> Result<()> {
    let report = read_json(&args.behavior_report)?;

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
    let report = behavior_case_authoring_report_from_value(report)?;
    let previews = collect_codegen_previews(&report);
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

fn print_human_competency_questions_report(report: &AuthoringCompetencyQuestionsReportV1) {
    println!(
        "competency questions: mode={} total={} executable={} unresolved={}",
        report.coverage_mode,
        report.total_questions,
        report.executable_questions,
        report.unresolved_questions
    );
    if !report.matched_refs.is_empty() {
        println!("  matched refs:");
        for reference in &report.matched_refs {
            println!(
                "    {} {} for {}",
                reference.ref_kind, reference.ref_id, reference.question
            );
        }
    }
    if !report.missing_refs.is_empty() {
        println!("  missing refs:");
        for reference in &report.missing_refs {
            println!(
                "    {} {} for {}",
                reference.ref_kind, reference.ref_id, reference.question
            );
        }
    }
    for note in &report.notes {
        println!("  note: {note}");
    }
    for action in &report.next_actions {
        println!("  next: {action}");
    }
}

pub fn build_continuous_software_coverage_report(
    report: &Value,
    options: &ContinuousCheckOptions,
) -> Result<ContinuousSoftwareCoverageReportV1> {
    let report = behavior_case_authoring_report_from_value(report)?;
    let repo_root = options
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| options.repo_root.clone());
    let required_codegen_languages = normalize_languages(&options.require_codegen)
        .into_iter()
        .collect::<Vec<_>>();
    let codegen_previews = collect_codegen_previews(&report);
    let present_codegen_languages = codegen_previews
        .iter()
        .map(|preview| preview.language.clone())
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

    let code_refs = behavior_report_code_refs(&report);
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

    let coverage = coverage_summary(&report);
    let competency = competency_summary(&report);
    let runtime_theory = runtime_theory_presence(&report);
    let typed_refs = behavior_report_typed_refs(&report, &coverage);
    let codegen = codegen_coverage_summary(
        &codegen_previews,
        &required_codegen_languages,
        &present_codegen_languages,
        &missing_codegen_languages,
    );

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
    if options.strict_coverage && runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        failures.push(format!(
            "{} runtime-theory obligation(s) remain residual",
            runtime_theory.residual_obligations.unwrap_or(0)
        ));
    } else if runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        warnings.push(format!(
            "{} runtime-theory obligation(s) remain residual",
            runtime_theory.residual_obligations.unwrap_or(0)
        ));
    }
    if runtime_theory
        .completeness_claim
        .as_deref()
        .is_some_and(|claim| !claim.starts_with("claimed_under_"))
    {
        warnings.push(
            "runtime theory sidecar does not claim completeness for all obligations".to_string(),
        );
    }
    if runtime_theory
        .ontology_closure_claim
        .as_deref()
        .is_some_and(|claim| !claim.starts_with("claimed_under_"))
    {
        warnings.push(
            "runtime theory sidecar does not claim ontology closure for all obligations"
                .to_string(),
        );
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
    if runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        next_actions.push(
            "resolve residual runtime-theory obligations or keep the continuous gate advisory"
                .to_string(),
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
    let case_id = report
        .behavior_case
        .case_id
        .clone()
        .or_else(|| report.receipt.case_id.clone());
    let coverage_mode = if options.strict_coverage {
        axiograph_tooling_overlays::CoverageModeV1::Enforced
    } else {
        axiograph_tooling_overlays::CoverageModeV1::Advisory
    };
    let authoring_flow = axiograph_tooling_overlays::build_authoring_flow_report_v1(
        axiograph_tooling_overlays::AuthoringFlowSourceV1::ContinuousCheck,
        case_id.clone(),
        axiograph_tooling_overlays::authoring_coverage_profile_summary_v1(
            coverage_mode,
            options.strict_coverage,
            options.require_code_refs,
            options.require_runtime_theory,
            options.strict_coverage,
            required_codegen_languages.clone(),
        ),
        axiograph_tooling_overlays::AuthoringCoverageSummaryV1 {
            total_rules: coverage.total_rules,
            covered_rules: coverage.covered_rules,
            tested_rules: coverage.tested_rules,
            implemented_rules: coverage.implemented_rules,
            drifted_rules: coverage.drifted_rules,
            missing_obligations: coverage.missing_obligations.clone(),
            uncovered_rule_ids: coverage.uncovered_rule_ids.clone(),
            code_refs_total: code_refs.len(),
            missing_code_refs: missing_code_refs.clone(),
            required_codegen_languages: required_codegen_languages.clone(),
            present_codegen_languages: present_codegen_languages.clone(),
            missing_codegen_languages: missing_codegen_languages.clone(),
            runtime_theory_present: runtime_theory.present,
            runtime_theory_residual_obligations: runtime_theory.residual_obligations,
            runtime_theory_blocking_obligations: runtime_theory.blocking_judgments,
        },
        pass,
        failures.clone(),
        warnings.clone(),
        next_actions.clone(),
    );

    Ok(ContinuousSoftwareCoverageReportV1 {
        version: "continuous_software_coverage_report_v1",
        case_id,
        status,
        pass,
        authoring_flow,
        policy: ContinuousCheckPolicySummary {
            strict_coverage: options.strict_coverage,
            require_code_refs: options.require_code_refs,
            require_runtime_theory: options.require_runtime_theory,
        },
        repo_root: repo_root.display().to_string(),
        typed_refs,
        codegen,
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

pub fn build_continuous_software_coverage_report_from_json_str(
    report_json: &str,
    options: &ContinuousCheckOptions,
) -> Result<ContinuousSoftwareCoverageReportV1> {
    let report: Value = serde_json::from_str(report_json)
        .context("parse behavior_case_report_v1 JSON wire payload")?;
    build_continuous_software_coverage_report(&report, options)
}

fn print_human_continuous_report(report: &ContinuousSoftwareCoverageReportV1) {
    println!(
        "continuous software coverage: {:?} case={}",
        report.status,
        report.case_id.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "  authoring flow: profile={:?} source={:?}",
        report.authoring_flow.profile.profile, report.authoring_flow.source
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
        "  runtime_theory: present={} ontology_closed={:?} complete={:?} residual={:?} blocking={:?}",
        report.runtime_theory.present,
        report.runtime_theory.ontology_closed,
        report.runtime_theory.complete,
        report.runtime_theory.residual_obligations,
        report.runtime_theory.blocking_judgments
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
        println!(
            "  diagnostics: .axi parse, .cq authoring, overlay refs, runtime-theory sidecars, behavior-case schema, coverage policy"
        );
        println!(
            "  code actions: definition query, competency questions, coverage query, codegen plan, continuous coverage"
        );
        println!(
            "  commands: {LSP_CMD_CODEGEN_PLAN}, {LSP_CMD_COMPETENCY_QUESTIONS}, {LSP_CMD_COVERAGE_QUERY}, {LSP_CMD_SOFTWARE_COVERAGE}"
        );
        println!("  next: configure the host to launch `axiograph authoring lsp` over stdio");
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
        println!(
            "  next: use MCP for read-only agent tools, LSP for editor diagnostics/actions, and CLI for file materialization"
        );
    }
    Ok(())
}

pub fn software_authoring_tool_specs_v1() -> Value {
    json!({
        "version": "axiograph_software_authoring_tool_specs_v1",
        "report_contracts": {
            "authoring_flow": "authoring_flow_report_v1",
            "continuous_software_coverage": "continuous_software_coverage_report_v1",
            "profiles": ["advisory", "strict", "ci"],
            "profile_semantics": {
                "advisory": "reports gaps and next actions without treating missing refs or residual obligations as a CI contract",
                "strict": "fails closed on explicit strict/enforced coverage requirements",
                "ci": "strict coverage plus required code refs, runtime-theory sidecars, and unresolved-obligation failure"
            }
        },
        "tools": [
            {
                "name": "axiograph.authoring.continuous_check",
                "description": "Check a behavior_case_report_v1 against typed refs, semantic coverage, competency, codegen language coverage, code-ref, and runtime-theory sidecar expectations.",
                "mutation": "read_only",
                "output_reports": ["continuous_software_coverage_report_v1", "authoring_flow_report_v1"]
            },
            {
                "name": "axiograph.authoring.materialize_skeletons",
                "description": "Materialize generated test skeleton previews from a behavior_case_report_v1 into a review directory.",
                "mutation": "writes_files"
            },
            {
                "name": "axiograph.authoring.codegen_plan",
                "description": "Return codegen file hints, language coverage, mapped surface refs, and caveats from a typed tooling overlay.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.coverage_query",
                "description": "Run an exploratory coverage query over canonical .axi plus an optional tooling overlay.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.competency_questions",
                "description": "Load/lower question-first `.cq` text and validate referenced types/relations against canonical .axi without requiring raw query authoring.",
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
                "description": "Return editor/LSP capability metadata for read-only overlay, weak-query, runtime-theory, codegen, and coverage reports.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.integration_manifest",
                "description": "Return host launch metadata for LSP, rmcp MCP, and background authoring processes.",
                "mutation": "read_only"
            },
            {
                "name": "axiograph.authoring.mcp",
                "description": "Read-only stdio MCP server exposing Axiograph authoring, typed coverage, competency-question checks, authoring-flow profiles, codegen planning, runtime-theory sidecar, and definition-query tools.",
                "mutation": "read_only"
            }
        ]
    })
}

pub fn software_authoring_integration_manifest_v1() -> Value {
    json!({
        "version": "axiograph_software_authoring_integration_manifest_v1",
        "report_contracts": {
            "authoring_flow": {
                "version": "authoring_flow_report_v1",
                "embedded_in": [
                    "continuous_software_coverage_report_v1.authoring_flow"
                ],
                "profiles": ["advisory", "strict", "ci"]
            }
        },
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
                { "language": "axiograph-cq", "pattern": "**/*.cq" },
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
        "report_contracts": {
            "authoring_flow": "authoring_flow_report_v1",
            "profiles": ["advisory", "strict", "ci"],
            "continuous_coverage_field": "authoring_flow"
        },
        "document_selector": [
            { "language": "axiograph", "pattern": "**/*.axi" },
            { "language": "axiograph-cq", "pattern": "**/*.cq" },
            { "language": "json", "pattern": "**/*tooling_overlay*.json" },
            { "language": "json", "pattern": "**/*behavior_case*.json" }
        ],
        "diagnostics": [
            "canonical_axi_parse",
            "runtime_theory_check",
            "competency_question_authoring",
            "runtime_theory_sidecar_presence",
            "overlay_ref_resolution",
            "behavior_case_schema",
            "coverage_policy"
        ],
        "code_actions": [
            LSP_CMD_DEFINITION_QUERY,
            LSP_CMD_COMPETENCY_QUESTIONS,
            LSP_CMD_COVERAGE_QUERY,
            LSP_CMD_CODEGEN_PLAN,
            LSP_CMD_OVERLAY_CHECK,
            LSP_CMD_SOFTWARE_COVERAGE
        ],
        "commands": [
            LSP_CMD_OVERLAY_CHECK,
            LSP_CMD_SOFTWARE_COVERAGE,
            LSP_CMD_CODEGEN_PLAN,
            LSP_CMD_COVERAGE_QUERY,
            LSP_CMD_DEFINITION_QUERY,
            LSP_CMD_COMPETENCY_QUESTIONS,
            LSP_CMD_LSP_CAPABILITIES
        ],
        "non_claims": [
            "LSP/editor diagnostics are runtime authoring feedback, not Lean certification.",
            "Editor code actions are review artifacts and must not mutate accepted ontology state directly.",
            "Continuous coverage commands report typed gaps and next actions; generated files still require explicit CLI materialization."
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
    overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default)]
    overlay_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct AxiOverlayMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default)]
    overlay_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct DefinitionQueryMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default)]
    definition_query: Option<axiograph_tooling_overlays::DefinitionQueryV1>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct CoverageQueryMcpArgs {
    axi_text: String,
    #[serde(default)]
    overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default)]
    coverage_query: Option<axiograph_tooling_overlays::CoverageQueryV1>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct CompetencyQuestionsMcpArgs {
    #[serde(default)]
    axi_text: Option<String>,
    #[serde(default)]
    cq_text: Option<String>,
    #[serde(default)]
    questions: Vec<AuthoringCompetencyQuestionV1>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct SoftwareCoverageMcpArgs {
    behavior_report: Value,
    #[serde(default)]
    overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default)]
    overlay_text: Option<String>,
    #[serde(default = "default_repo_root")]
    repo_root: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AuthoringMcpPayload {
    payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct AuthoringCompetencyQuestionHintsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub about: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub given: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expect: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuthoringCompetencyQuestionV1 {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring: Option<AuthoringCompetencyQuestionHintsV1>,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_min_rows")]
    pub min_rows: usize,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<String>,
}

impl Default for AuthoringCompetencyQuestionV1 {
    fn default() -> Self {
        Self {
            name: String::new(),
            question: None,
            authoring: None,
            query: String::new(),
            min_rows: default_min_rows(),
            weight: default_weight(),
            contexts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthoringCompetencyQuestionRefV1 {
    pub question: String,
    pub ref_kind: String,
    pub ref_id: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AuthoringCompetencyQuestionsReportV1 {
    pub version: String,
    pub coverage_mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<AuthoringCompetencyQuestionV1>,
    pub total_questions: usize,
    pub executable_questions: usize,
    pub unresolved_questions: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_refs: Vec<AuthoringCompetencyQuestionRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_refs: Vec<AuthoringCompetencyQuestionRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<String>,
}

fn default_min_rows() -> usize {
    1
}

fn default_weight() -> f64 {
    1.0
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
        description = "Return generated skeleton file hints, language coverage, and mapped surface refs from a typed tooling overlay."
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
        name = "axiograph_authoring_competency_questions",
        description = "Load/lower question-first `.cq` text and validate referenced types/relations against canonical .axi text without executing queries."
    )]
    fn competency_questions(
        &self,
        Parameters(args): Parameters<CompetencyQuestionsMcpArgs>,
    ) -> std::result::Result<Json<AuthoringMcpPayload>, String> {
        let arguments = tool_arguments_value(args)?;
        call_authoring_mcp_tool(json!({
            "name": "axiograph_authoring_competency_questions",
            "arguments": arguments
        }))
        .map(authoring_mcp_payload)
        .map_err(|err| err.to_string())
    }

    #[tool(
        name = "axiograph_authoring_software_coverage",
        description = "Evaluate a behavior-case report against a tooling overlay, repository root, runtime-theory sidecar expectations, and embedded authoring-flow profile."
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
            "description": "Return generated skeleton file hints, language coverage, and mapped surface refs from a typed tooling overlay.",
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
            "name": "axiograph_authoring_competency_questions",
            "title": "Axiograph Competency Questions",
            "description": "Load/lower question-first `.cq` text and validate referenced types/relations against canonical .axi text without executing queries.",
            "inputSchema": authoring_competency_questions_input_schema()
        },
        {
            "name": "axiograph_authoring_software_coverage",
            "title": "Axiograph Software Coverage",
            "description": "Evaluate a behavior-case report against a tooling overlay, repository root, runtime-theory sidecar expectations, and embedded authoring-flow profile.",
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
            let query = query_arg(&arguments, "definition_query", "definition query")?;
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
            let query = query_arg(&arguments, "coverage_query", "coverage query")?;
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
        "axiograph_authoring_competency_questions" => {
            let args: CompetencyQuestionsMcpArgs = serde_json::from_value(arguments)
                .context("parse competency question tool arguments")?;
            Ok(serde_json::to_value(
                build_authoring_competency_questions_report(args)?,
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
            let behavior_report =
                axiograph_tooling_overlays::behavior_case_coverage_view_from_value(
                    &behavior_report,
                )?;
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
    json!({
        "type": "object",
        "required": ["axi_text", query_key],
        "properties": {
            "axi_text": { "type": "string" },
            "overlay": overlay_schema,
            "overlay_text": { "type": "string" },
            "definition_query": axiograph_tooling_overlays::definition_query_schema(),
            "coverage_query": axiograph_tooling_overlays::coverage_query_schema()
        },
        "additionalProperties": false
    })
}

fn authoring_competency_questions_input_schema() -> Value {
    let question_schema =
        serde_json::to_value(schemars::schema_for!(AuthoringCompetencyQuestionV1))
            .expect("competency question schema should serialize");
    json!({
        "type": "object",
        "properties": {
            "axi_text": {
                "type": "string",
                "description": "Optional canonical .axi text used to validate referenced schema object and relation names."
            },
            "cq_text": {
                "type": "string",
                "description": "Question-first .cq text using version, question, ask, about, given, and expect records."
            },
            "questions": {
                "type": "array",
                "items": question_schema,
                "default": []
            }
        },
        "anyOf": [
            { "required": ["cq_text"] },
            { "required": ["questions"] }
        ],
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
                "command": LSP_CMD_DEFINITION_QUERY
            }
        },
        {
            "title": "Axiograph: run exploratory coverage query",
            "kind": "quickfix",
            "command": {
                "title": "Run coverage query",
                "command": LSP_CMD_COVERAGE_QUERY
            }
        },
        {
            "title": "Axiograph: check competency questions",
            "kind": "quickfix",
            "command": {
                "title": "Check competency questions",
                "command": LSP_CMD_COMPETENCY_QUESTIONS
            }
        },
        {
            "title": "Axiograph: plan generated tests",
            "kind": "source",
            "command": {
                "title": "Plan codegen",
                "command": LSP_CMD_CODEGEN_PLAN
            }
        },
        {
            "title": "Axiograph: check overlay refs",
            "kind": "source",
            "command": {
                "title": "Check overlay",
                "command": LSP_CMD_OVERLAY_CHECK
            }
        },
        {
            "title": "Axiograph: check software coverage",
            "kind": "source",
            "command": {
                "title": "Check software coverage",
                "command": LSP_CMD_SOFTWARE_COVERAGE
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
        LSP_CMD_CODEGEN_PLAN => {
            let overlay = overlay_arg(&arg)?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::codegen_plan_report(&overlay),
            )?)
        }
        LSP_CMD_OVERLAY_CHECK => {
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
        LSP_CMD_DEFINITION_QUERY => {
            let axi_text = arg
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("definitionQuery requires arguments[0].axi_text"))?;
            let query = query_arg(&arg, "definition_query", "definitionQuery")?;
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
        LSP_CMD_COVERAGE_QUERY => {
            let axi_text = arg
                .get("axi_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("coverageQuery requires arguments[0].axi_text"))?;
            let query = query_arg(&arg, "coverage_query", "coverageQuery")?;
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
        LSP_CMD_COMPETENCY_QUESTIONS => {
            let args: CompetencyQuestionsMcpArgs = serde_json::from_value(arg)
                .context("parse competencyQuestions arguments")?;
            Ok(serde_json::to_value(
                build_authoring_competency_questions_report(args)?,
            )?)
        }
        LSP_CMD_SOFTWARE_COVERAGE => {
            let behavior_report = arg
                .get("behavior_report")
                .cloned()
                .ok_or_else(|| anyhow!("softwareCoverage requires arguments[0].behavior_report"))?;
            let overlay = overlay_arg(&arg)?;
            let repo_root = arg.get("repo_root").and_then(Value::as_str).unwrap_or(".");
            let behavior_report =
                axiograph_tooling_overlays::behavior_case_coverage_view_from_value(
                    &behavior_report,
                )?;
            Ok(serde_json::to_value(
                axiograph_tooling_overlays::continuous_coverage_report_from_behavior_report(
                    &behavior_report,
                    &overlay,
                    Path::new(repo_root),
                ),
            )?)
        }
        LSP_CMD_LSP_CAPABILITIES => Ok(software_authoring_lsp_capabilities_v1()),
        other => Err(anyhow!("unsupported authoring LSP command `{other}`")),
    }
}

fn query_arg(value: &Value, primary_key: &str, label: &str) -> Result<Value> {
    value
        .get(primary_key)
        .cloned()
        .ok_or_else(|| anyhow!("{label} requires `{primary_key}`"))
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

fn build_authoring_competency_questions_report(
    args: CompetencyQuestionsMcpArgs,
) -> Result<AuthoringCompetencyQuestionsReportV1> {
    let mut questions = Vec::new();
    if let Some(cq_text) = args.cq_text.as_deref() {
        questions.extend(parse_authoring_competency_question_text(cq_text)?);
    }
    questions.extend(args.questions);
    if questions.is_empty() {
        bail!("competency question tool requires `cq_text` or at least one `questions` item");
    }
    for question in &mut questions {
        lower_authoring_competency_question(question);
        validate_authoring_competency_question(question)?;
    }

    let kernel = args
        .axi_text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(axiograph_tooling_overlays::compile_kernel_from_axi_text)
        .transpose()?;

    let mut matched_refs = BTreeSet::new();
    let mut missing_refs = BTreeSet::new();
    for question in &questions {
        for reference in refs_for_authoring_competency_question(question) {
            match kernel.as_ref() {
                Some(kernel) if authoring_competency_ref_exists(kernel, &reference) => {
                    matched_refs.insert(reference);
                }
                Some(_) => {
                    missing_refs.insert(reference);
                }
                None => {}
            }
        }
    }

    let executable_questions = questions
        .iter()
        .filter(|question| !question.query.trim().is_empty())
        .count();
    let unresolved_questions = questions.len().saturating_sub(executable_questions);
    let mut notes = Vec::new();
    if kernel.is_none() {
        notes.push(
            "No canonical .axi text was provided, so referenced schema objects and relations were not validated."
                .to_string(),
        );
    }
    if unresolved_questions > 0 {
        notes.push(format!(
            "{unresolved_questions} question(s) remain authoring obligations because they do not lower to an executable typed query."
        ));
    }
    let mut next_actions = Vec::new();
    if !missing_refs.is_empty() {
        next_actions.push(
            "Fix missing referenced schema objects/relations or add typed ontology/refinement handles before strict coverage."
                .to_string(),
        );
    }
    if unresolved_questions > 0 {
        next_actions.push(
            "Add an executable `expect: exists Schema.Rel(...)` or `expect: instance of Schema.Type` line, or keep the question as an explicit residual authoring obligation."
                .to_string(),
        );
    }
    if missing_refs.is_empty() && unresolved_questions == 0 {
        next_actions.push(
            "Evaluate these CQs with `semantic_competency_questions` or the CLI behavior-case/CQ runner against a loaded runtime."
                .to_string(),
        );
    }

    let coverage_mode = if kernel.is_some() && missing_refs.is_empty() && unresolved_questions == 0
    {
        "authoring_checked"
    } else {
        "advisory"
    };

    Ok(AuthoringCompetencyQuestionsReportV1 {
        version: AUTHORING_COMPETENCY_QUESTIONS_REPORT_VERSION_V1.to_string(),
        coverage_mode: coverage_mode.to_string(),
        total_questions: questions.len(),
        executable_questions,
        unresolved_questions,
        questions,
        matched_refs: matched_refs.into_iter().collect(),
        missing_refs: missing_refs.into_iter().collect(),
        next_actions,
        notes,
        non_claims: vec![
            "This authoring report validates CQ syntax, lowering, and optional ontology ref presence only; it does not execute queries."
                .to_string(),
            "CQ satisfaction, promotion gates, and correctness claims require the semantic runtime, accepted anchors, and configured trust/CQ policies."
                .to_string(),
        ],
    })
}

pub fn build_authoring_competency_questions_report_from_text(
    axi_text: Option<&str>,
    cq_text: &str,
) -> Result<AuthoringCompetencyQuestionsReportV1> {
    build_authoring_competency_questions_report(CompetencyQuestionsMcpArgs {
        axi_text: axi_text.map(str::to_string),
        cq_text: Some(cq_text.to_string()),
        questions: Vec::new(),
    })
}

fn parse_authoring_competency_question_text(
    text: &str,
) -> Result<Vec<AuthoringCompetencyQuestionV1>> {
    let mut questions = Vec::new();
    let mut current: Option<AuthoringCompetencyQuestionV1> = None;
    let mut saw_version = false;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(version) = line.strip_prefix("version ") {
            let version = version.trim();
            if version != COMPETENCY_QUESTION_BUNDLE_VERSION_V1 {
                bail!(
                    "unsupported competency question text version `{version}` at line {line_no} (expected `{COMPETENCY_QUESTION_BUNDLE_VERSION_V1}`)"
                );
            }
            saw_version = true;
            continue;
        }
        if let Some(name) = line
            .strip_prefix("question ")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            if let Some(mut question) = current.take() {
                lower_authoring_competency_question(&mut question);
                validate_authoring_competency_question(&question)?;
                questions.push(question);
            }
            let name = name.trim();
            if name.is_empty() {
                bail!("empty competency question name at line {line_no}");
            }
            current = Some(AuthoringCompetencyQuestionV1 {
                name: name.to_string(),
                ..AuthoringCompetencyQuestionV1::default()
            });
            continue;
        }

        let Some(question) = current.as_mut() else {
            bail!("competency question field before `question <name>:` at line {line_no}");
        };
        let (key, value) = line.split_once(':').ok_or_else(|| {
            anyhow!("invalid competency question field at line {line_no} (expected `key: value`)")
        })?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "ask" | "asks" | "question" => {
                question.question = Some(value.to_string());
                question
                    .authoring
                    .get_or_insert_with(Default::default)
                    .ask = Some(value.to_string());
            }
            "about" => question
                .authoring
                .get_or_insert_with(Default::default)
                .about
                .push(value.to_string()),
            "given" => question
                .authoring
                .get_or_insert_with(Default::default)
                .given
                .push(value.to_string()),
            "expect" | "expects" => question
                .authoring
                .get_or_insert_with(Default::default)
                .expect
                .push(value.to_string()),
            "note" | "notes" => question
                .authoring
                .get_or_insert_with(Default::default)
                .notes
                .push(value.to_string()),
            "axql" => question.query = value.to_string(),
            "min_rows" => {
                question.min_rows = value
                    .parse::<usize>()
                    .map_err(|err| anyhow!("invalid min_rows at line {line_no}: {err}"))?;
            }
            "weight" => {
                question.weight = value
                    .parse::<f64>()
                    .map_err(|err| anyhow!("invalid weight at line {line_no}: {err}"))?;
            }
            "context" => question.contexts.push(value.to_string()),
            "contexts" => {
                question.contexts.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|ctx| !ctx.is_empty())
                        .map(str::to_string),
                );
            }
            other => bail!("unsupported competency question field `{other}` at line {line_no}"),
        }
    }

    if let Some(mut question) = current.take() {
        lower_authoring_competency_question(&mut question);
        validate_authoring_competency_question(&question)?;
        questions.push(question);
    }
    if !saw_version {
        bail!("competency question text requires `version {COMPETENCY_QUESTION_BUNDLE_VERSION_V1}`");
    }
    if questions.is_empty() {
        bail!("competency question text contains no questions");
    }
    Ok(questions)
}

fn validate_authoring_competency_question(question: &AuthoringCompetencyQuestionV1) -> Result<()> {
    if question.name.trim().is_empty() {
        bail!("competency question has an empty name");
    }
    let has_authoring = question.authoring.as_ref().is_some_and(|hints| {
        hints.ask.is_some()
            || !hints.about.is_empty()
            || !hints.given.is_empty()
            || !hints.expect.is_empty()
            || !hints.notes.is_empty()
    });
    if question.query.trim().is_empty() && question.question.is_none() && !has_authoring {
        bail!(
            "competency question `{}` requires `ask: ...`, `expect: ...`, or `axql: ...`",
            question.name
        );
    }
    if question.min_rows == 0 {
        bail!(
            "competency question `{}` requires min_rows > 0",
            question.name
        );
    }
    if !question.weight.is_finite() || question.weight <= 0.0 {
        bail!(
            "competency question `{}` requires a finite positive weight",
            question.name
        );
    }
    Ok(())
}

fn lower_authoring_competency_question(question: &mut AuthoringCompetencyQuestionV1) {
    if !question.query.trim().is_empty() {
        return;
    }
    if let Some(query) = lower_authoring_competency_question_query(question) {
        question.query = query;
    }
}

fn lower_authoring_competency_question_query(
    question: &AuthoringCompetencyQuestionV1,
) -> Option<String> {
    let hints = question.authoring.as_ref()?;
    let limit = question.min_rows.max(1);
    let relation_exprs = hints
        .expect
        .iter()
        .filter_map(|expect| relation_expression_from_expect(expect))
        .collect::<Vec<_>>();
    if relation_exprs.len() == 1 {
        return Some(format!(
            "select ?f where ?f = {} limit {limit}",
            relation_exprs[0]
        ));
    }
    if relation_exprs.len() > 1 {
        let atoms = relation_exprs
            .iter()
            .enumerate()
            .map(|(idx, expr)| format!("?f{idx} = {expr}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!("select ?f0 where {atoms} limit {limit}"));
    }

    for expect in &hints.expect {
        if let Some(type_ref) = type_ref_from_expect(expect) {
            return Some(format!("select ?x where ?x is {type_ref} limit {limit}"));
        }
    }

    let relation = hints
        .about
        .iter()
        .map(|value| value.trim())
        .find(|value| looks_like_qualified_relation_ref(value))?;
    let fields = hints
        .given
        .iter()
        .map(|value| value.trim())
        .filter(|value| value.contains('='))
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return None;
    }
    Some(format!(
        "select ?f where ?f = {relation}({}) limit {limit}",
        fields.join(", ")
    ))
}

fn refs_for_authoring_competency_question(
    question: &AuthoringCompetencyQuestionV1,
) -> Vec<AuthoringCompetencyQuestionRefV1> {
    let Some(hints) = question.authoring.as_ref() else {
        return Vec::new();
    };
    let mut refs = BTreeSet::new();
    for expect in &hints.expect {
        if let Some(relation_expr) = relation_expression_from_expect(expect) {
            if let Some(ref_id) = qualified_ref_head(&relation_expr) {
                refs.insert(AuthoringCompetencyQuestionRefV1 {
                    question: question.name.clone(),
                    ref_kind: "relation".to_string(),
                    ref_id,
                });
            }
        }
        if let Some(type_ref) = type_ref_from_expect(expect) {
            refs.insert(AuthoringCompetencyQuestionRefV1 {
                question: question.name.clone(),
                ref_kind: "object".to_string(),
                ref_id: type_ref,
            });
        }
    }
    for about in &hints.about {
        if looks_like_qualified_relation_ref(about) {
            if let Some(ref_id) = qualified_ref_head(about) {
                refs.insert(AuthoringCompetencyQuestionRefV1 {
                    question: question.name.clone(),
                    ref_kind: "relation".to_string(),
                    ref_id,
                });
            }
        }
    }
    refs.into_iter().collect()
}

fn authoring_competency_ref_exists(
    kernel: &axiograph_pathdb::kernel_ir::KernelModuleIr,
    reference: &AuthoringCompetencyQuestionRefV1,
) -> bool {
    let Some((schema_ref, local_ref)) = reference.ref_id.split_once('.') else {
        return false;
    };
    kernel.schemas.iter().any(|schema| {
        schema_ref_matches(schema.schema_id.as_str(), schema_ref)
            && match reference.ref_kind.as_str() {
                "object" => schema.object_types.contains(local_ref),
                "relation" => schema.relations.contains_key(local_ref),
                _ => false,
            }
    })
}

fn schema_ref_matches(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual
            .rsplit_once(':')
            .map(|(_, local)| local == expected)
            .unwrap_or(false)
}

fn relation_expression_from_expect(expect: &str) -> Option<String> {
    let value = expect.trim();
    let value = value
        .strip_prefix("exists ")
        .or_else(|| value.strip_prefix("fact "))
        .or_else(|| value.strip_prefix("relation "))
        .unwrap_or(value)
        .trim();
    if value.contains('(') && value.contains(')') && looks_like_qualified_relation_ref(value) {
        Some(value.to_string())
    } else {
        None
    }
}

fn type_ref_from_expect(expect: &str) -> Option<String> {
    let value = expect.trim();
    let value = value
        .strip_prefix("instance of ")
        .or_else(|| value.strip_prefix("type "))
        .or_else(|| value.strip_prefix("object "))
        .or_else(|| value.strip_prefix("exists "))
        .unwrap_or(value)
        .trim();
    if !value.contains('(') && looks_like_qualified_ref(value) {
        Some(value.to_string())
    } else {
        None
    }
}

fn qualified_ref_head(value: &str) -> Option<String> {
    let head = value.split_once('(').map(|(head, _)| head).unwrap_or(value);
    if looks_like_qualified_ref(head) {
        Some(head.trim().to_string())
    } else {
        None
    }
}

fn looks_like_qualified_relation_ref(value: &str) -> bool {
    let head = value.split_once('(').map(|(head, _)| head).unwrap_or(value);
    looks_like_qualified_ref(head)
}

fn looks_like_qualified_ref(value: &str) -> bool {
    let Some((schema, local)) = value.trim().split_once('.') else {
        return false;
    };
    !schema.trim().is_empty()
        && !local.trim().is_empty()
        && schema.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && local.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
    } else if uri.ends_with(".cq")
        || text
            .trim_start()
            .starts_with("version competency_question_bundle_v1")
    {
        diagnostics_for_cq_document(text)
    } else if uri.ends_with(".json") || text.trim_start().starts_with('{') {
        diagnostics_for_json_document(text)
    } else {
        Vec::new()
    }
}

fn diagnostics_for_cq_document(text: &str) -> Vec<Diagnostic> {
    match parse_authoring_competency_question_text(text) {
        Ok(questions) => {
            let unresolved = questions
                .iter()
                .filter(|question| question.query.trim().is_empty())
                .collect::<Vec<_>>();
            unresolved
                .into_iter()
                .map(|question| {
                    diagnostic(
                        DiagnosticSeverity::WARNING,
                        "axiograph.cq.authoring",
                        &format!(
                            "competency question `{}` is valid authoring intent but does not yet lower to an executable typed query",
                            question.name
                        ),
                        line_for_cq_question(text, &question.name).unwrap_or(0),
                        0,
                    )
                })
                .collect()
        }
        Err(err) => vec![diagnostic(
            DiagnosticSeverity::ERROR,
            "axiograph.cq",
            &err.to_string(),
            0,
            0,
        )],
    }
}

fn line_for_cq_question(text: &str, question_name: &str) -> Option<usize> {
    let expected = format!("question {question_name}:");
    text.lines()
        .position(|line| line.trim() == expected)
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
                "axiograph.behavior_case.embedded_tooling",
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
                "axiograph.behavior_case.embedded_tooling",
                "BehaviorCaseV1 must not embed implementation_surfaces; use ToolingOverlayBundleV1",
                0,
                0,
            ));
        }
        if value.pointer("/behavior_case/coverage_edges").is_some() {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::ERROR,
                "axiograph.behavior_case.embedded_tooling",
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

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseAuthoringReportV1 {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    behavior_case: BehaviorCaseAuthoringCaseViewV1,
    #[serde(default)]
    receipt: BehaviorCaseAuthoringReceiptViewV1,
    #[serde(default)]
    context_report: BehaviorCaseAuthoringContextReportViewV1,
    #[serde(default)]
    runtime_theory_check: Option<axiograph_tooling_overlays::RuntimeTheorySidecarSummaryV1>,
    #[serde(default)]
    codegen_previews: Vec<CodegenPreview>,
    #[serde(default)]
    code_refs: Vec<String>,
    #[serde(default)]
    anchors: Vec<String>,
    #[serde(default)]
    residual_unknowns: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseAuthoringCaseViewV1 {
    #[serde(default)]
    case_id: Option<String>,
    #[serde(default)]
    anchors: Vec<String>,
    #[serde(default)]
    context: Option<BehaviorCaseContextHintsViewV1>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseContextHintsViewV1 {
    #[serde(default)]
    surfaces: Vec<BehaviorCaseSurfaceHintViewV1>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseSurfaceHintViewV1 {
    #[serde(default)]
    code_refs: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseAuthoringReceiptViewV1 {
    #[serde(default)]
    case_id: Option<String>,
    #[serde(default)]
    anchors: Vec<String>,
    #[serde(default)]
    matched_scope_ids: Vec<String>,
    #[serde(default)]
    matched_rule_ids: Vec<String>,
    #[serde(default)]
    surface_ids: Vec<String>,
    #[serde(default)]
    residual_obligations: Vec<String>,
    #[serde(default)]
    code_refs: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseAuthoringContextReportViewV1 {
    #[serde(default)]
    coverage: BehaviorCaseAuthoringCoverageViewV1,
    #[serde(default)]
    competency_coverage: Option<CompetencyCoverageViewV1>,
    #[serde(default)]
    runtime_theory_check: Option<axiograph_tooling_overlays::RuntimeTheorySidecarSummaryV1>,
    #[serde(default)]
    anchors: Vec<String>,
    #[serde(default)]
    matched_scope_ids: Vec<String>,
    #[serde(default)]
    matched_rule_ids: Vec<String>,
    #[serde(default)]
    surface_ids: Vec<String>,
    #[serde(default)]
    residual_unknowns: Vec<String>,
    #[serde(default)]
    code_refs: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BehaviorCaseAuthoringCoverageViewV1 {
    #[serde(default)]
    total_rules: u64,
    #[serde(default)]
    runtime_enforced_rules: u64,
    #[serde(default)]
    covered_rules: u64,
    #[serde(default)]
    tested_rules: u64,
    #[serde(default)]
    implemented_rules: u64,
    #[serde(default)]
    ontology_only_rules: u64,
    #[serde(default)]
    drifted_rules: u64,
    #[serde(default)]
    missing_obligations: Vec<String>,
    #[serde(default)]
    uncovered_rule_ids: Vec<String>,
    #[serde(default)]
    next_actions: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CompetencyCoverageViewV1 {
    #[serde(default)]
    total: u64,
    #[serde(default)]
    satisfied: u64,
    #[serde(default)]
    coverage: f64,
    #[serde(default)]
    questions: Vec<CompetencyQuestionViewV1>,
}

#[derive(Debug, Deserialize, Default)]
struct CompetencyQuestionViewV1 {
    #[serde(default)]
    name: String,
    #[serde(default)]
    satisfied: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CodegenPreview {
    #[serde(default)]
    language: String,
    #[serde(default)]
    file_hint: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

fn behavior_case_authoring_report_from_value(
    report: &Value,
) -> Result<BehaviorCaseAuthoringReportV1> {
    let report: BehaviorCaseAuthoringReportV1 = serde_json::from_value(report.clone())
        .context("parse behavior report as BehaviorCaseAuthoringReportV1")?;
    match report.version.as_deref() {
        Some("behavior_case_report_v1") => Ok(report),
        other => Err(anyhow!(
            "expected behavior_case_report_v1, got {:?}",
            other.unwrap_or("<missing>")
        )),
    }
}

fn collect_codegen_previews(report: &BehaviorCaseAuthoringReportV1) -> Vec<CodegenPreview> {
    report
        .codegen_previews
        .iter()
        .filter_map(|preview| {
            let language = preview.language.trim().to_lowercase();
            if language.is_empty() {
                return None;
            }
            Some(CodegenPreview {
                language,
                file_hint: preview.file_hint.clone(),
                content: preview.content.clone(),
            })
        })
        .collect()
}

fn coverage_summary(report: &BehaviorCaseAuthoringReportV1) -> CoverageSummary {
    let coverage = &report.context_report.coverage;
    let mut summary = CoverageSummary {
        total_rules: coverage.total_rules,
        runtime_enforced_rules: coverage.runtime_enforced_rules,
        covered_rules: coverage.covered_rules,
        tested_rules: coverage.tested_rules,
        implemented_rules: coverage.implemented_rules,
        ontology_only_rules: coverage.ontology_only_rules,
        drifted_rules: coverage.drifted_rules,
        missing_obligations: coverage.missing_obligations.clone(),
        uncovered_rule_ids: coverage.uncovered_rule_ids.clone(),
        next_actions: coverage.next_actions.clone(),
    };
    if summary.total_rules == 0 {
        summary.total_rules = report.receipt.matched_rule_ids.len() as u64;
    }
    summary
}

fn competency_summary(report: &BehaviorCaseAuthoringReportV1) -> CompetencySummary {
    let Some(coverage) = report.context_report.competency_coverage.as_ref() else {
        return CompetencySummary::default();
    };
    let unsatisfied_questions = coverage
        .questions
        .iter()
        .filter(|question| !question.satisfied)
        .map(|question| question.name.clone())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();

    CompetencySummary {
        total: coverage.total,
        satisfied: coverage.satisfied,
        coverage: coverage.coverage,
        unsatisfied_questions,
    }
}

fn behavior_report_typed_refs(
    report: &BehaviorCaseAuthoringReportV1,
    coverage: &CoverageSummary,
) -> BehaviorReportTypedRefs {
    let mut residual_obligations = coverage
        .missing_obligations
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    residual_obligations.extend(report.receipt.residual_obligations.iter().cloned());
    residual_obligations.extend(report.context_report.residual_unknowns.iter().cloned());
    residual_obligations.extend(report.residual_unknowns.iter().cloned());

    let anchors = report
        .anchors
        .iter()
        .chain(report.behavior_case.anchors.iter())
        .chain(report.context_report.anchors.iter())
        .chain(report.receipt.anchors.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let matched_scope_ids = report
        .context_report
        .matched_scope_ids
        .iter()
        .chain(report.receipt.matched_scope_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let matched_rule_ids = report
        .context_report
        .matched_rule_ids
        .iter()
        .chain(report.receipt.matched_rule_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let surface_ids = report
        .context_report
        .surface_ids
        .iter()
        .chain(report.receipt.surface_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    BehaviorReportTypedRefs {
        anchors,
        matched_scope_ids,
        matched_rule_ids,
        surface_ids,
        residual_obligations: residual_obligations.into_iter().collect(),
        uncovered_rule_ids: coverage.uncovered_rule_ids.clone(),
    }
}

fn behavior_report_code_refs(report: &BehaviorCaseAuthoringReportV1) -> Vec<String> {
    let behavior_case_context_refs = report
        .behavior_case
        .context
        .as_ref()
        .into_iter()
        .flat_map(|context| context.surfaces.iter())
        .flat_map(|surface| surface.code_refs.iter());
    report
        .code_refs
        .iter()
        .chain(behavior_case_context_refs)
        .chain(report.context_report.code_refs.iter())
        .chain(report.receipt.code_refs.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn codegen_coverage_summary(
    previews: &[CodegenPreview],
    required_languages: &[String],
    present_languages: &[String],
    missing_languages: &[String],
) -> CodegenCoverageSummary {
    let languages = required_languages
        .iter()
        .chain(present_languages.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let present = present_languages.iter().cloned().collect::<BTreeSet<_>>();
    let language_statuses = languages
        .into_iter()
        .map(|language| {
            let file_hints = previews
                .iter()
                .filter(|preview| preview.language == language)
                .filter_map(|preview| preview.file_hint.clone())
                .collect::<Vec<_>>();
            CodegenLanguageStatus {
                present: present.contains(&language),
                language,
                file_hints,
            }
        })
        .collect::<Vec<_>>();

    CodegenCoverageSummary {
        preview_count: previews.len(),
        required_languages: required_languages.to_vec(),
        present_languages: present_languages.to_vec(),
        missing_languages: missing_languages.to_vec(),
        language_statuses,
    }
}

fn runtime_theory_presence(report: &BehaviorCaseAuthoringReportV1) -> RuntimeTheoryPresence {
    let Some(mut theory) = report
        .runtime_theory_check
        .clone()
        .or_else(|| report.context_report.runtime_theory_check.clone())
    else {
        return RuntimeTheoryPresence::default();
    };
    theory.present = true;
    let completeness_claim = theory.completeness_claim.clone();
    let ontology_closure_claim = theory.ontology_closure_claim.clone();
    let blocking_judgments =
        Some(theory.blocked_obligations.unwrap_or(0) + theory.blocking_errors.unwrap_or(0));
    RuntimeTheoryPresence {
        present: true,
        module_digest: theory.module_digest,
        closure_tiers: theory.closure_tiers,
        checked_obligations: theory.checked_obligations,
        review_only_obligations: theory.review_only_obligations,
        ontology_closed: ontology_closure_claim
            .as_deref()
            .map(|claim| claim.starts_with("claimed_under_")),
        complete: completeness_claim
            .as_deref()
            .map(|claim| claim.starts_with("claimed_under_")),
        blocking_judgments,
        residual_obligations: theory.residual_obligations,
        blocked_obligations: theory.blocked_obligations,
        blocking_errors: theory.blocking_errors,
        completeness_claim,
        ontology_closure_claim,
        residual_obligation_ids: theory.residual_obligation_ids,
        notes: theory.notes,
    }
}

fn normalize_languages(languages: &[String]) -> BTreeSet<String> {
    languages
        .iter()
        .map(|language| language.trim().to_lowercase())
        .filter(|language| !language.is_empty())
        .collect()
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

    fn sample_authoring_axi_text() -> &'static str {
        r#"
module OrderFulfillmentDomain

schema OrderFulfillment:
  object Order
  object Payment
  object Shipment
  relation OrderHasPayment(order: Order, payment: Payment)
  relation ShipmentFulfillsOrder(shipment: Shipment, order: Order)

theory OrderFulfillmentRules on OrderFulfillment:
  constraint key OrderHasPayment(order, payment)

instance Seed of OrderFulfillment:
  Order = {Order_1}
  Payment = {Payment_1}
  Shipment = {Shipment_1}
  OrderHasPayment = {(order=Order_1, payment=Payment_1)}
  ShipmentFulfillsOrder = {(shipment=Shipment_1, order=Order_1)}
"#
    }

    fn sample_authoring_cq_text() -> &'static str {
        r#"
version competency_question_bundle_v1

question payment_link:
  ask: does each accepted order have a payment?
  expect: exists OrderFulfillment.OrderHasPayment(order=Order_1, payment=Payment_1)
  min_rows: 1

question shipment_link:
  ask: does each shipment fulfill an order?
  expect: exists OrderFulfillment.ShipmentFulfillsOrder(shipment=Shipment_1, order=Order_1)
"#
    }

    fn sample_overlay(value: Value) -> axiograph_tooling_overlays::ToolingOverlayBundleV1 {
        serde_json::from_value(value).expect("sample overlay should parse")
    }

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
        assert_eq!(
            coverage.authoring_flow.version,
            axiograph_tooling_overlays::AUTHORING_FLOW_REPORT_VERSION_V1
        );
        assert_eq!(
            coverage.authoring_flow.profile.profile,
            axiograph_tooling_overlays::AuthoringCoverageProfileV1::Advisory
        );
        assert_eq!(coverage.authoring_flow.coverage.code_refs_total, 1);
        assert_eq!(coverage.missing_codegen_languages, Vec::<String>::new());
        assert_eq!(coverage.codegen.preview_count, 4);
        assert!(coverage
            .codegen
            .language_statuses
            .iter()
            .any(|status| status.language == "go" && status.present));
        assert_eq!(
            coverage.typed_refs.residual_obligations,
            vec!["map one ontology-only rule".to_string()]
        );
        assert_eq!(coverage.missing_code_refs.len(), 1);
        assert!(coverage
            .warnings
            .iter()
            .any(|warning| warning.contains("semantic coverage is incomplete")));
    }

    #[test]
    fn authoring_competency_questions_lower_and_validate_refs() {
        let report = build_authoring_competency_questions_report(CompetencyQuestionsMcpArgs {
            axi_text: Some(sample_authoring_axi_text().to_string()),
            cq_text: Some(sample_authoring_cq_text().to_string()),
            questions: Vec::new(),
        })
        .expect("competency report");

        assert_eq!(
            report.version,
            AUTHORING_COMPETENCY_QUESTIONS_REPORT_VERSION_V1
        );
        assert_eq!(report.coverage_mode, "authoring_checked");
        assert_eq!(report.total_questions, 2);
        assert_eq!(report.executable_questions, 2);
        assert_eq!(report.unresolved_questions, 0);
        assert!(report.questions.iter().all(|question| question
            .query
            .starts_with("select ?f where ?f = OrderFulfillment.")));
        assert!(report
            .matched_refs
            .iter()
            .any(|reference| reference.ref_id == "OrderFulfillment.OrderHasPayment"));
        assert!(report.missing_refs.is_empty());
    }

    #[test]
    fn authoring_competency_questions_keep_unlowered_questions_advisory() {
        let report = build_authoring_competency_questions_report(CompetencyQuestionsMcpArgs {
            axi_text: Some(sample_authoring_axi_text().to_string()),
            cq_text: Some(
                r#"
version competency_question_bundle_v1

question policy_gap:
  ask: what operational policy must govern exceptional fulfillment?
  note: needs ontology refinement before executable CQ lowering
"#
                .to_string(),
            ),
            questions: Vec::new(),
        })
        .expect("competency report");

        assert_eq!(report.coverage_mode, "advisory");
        assert_eq!(report.executable_questions, 0);
        assert_eq!(report.unresolved_questions, 1);
        assert!(report
            .next_actions
            .iter()
            .any(|action| action.contains("Add an executable `expect")));
    }

    #[test]
    fn authoring_competency_questions_report_missing_refs() {
        let report = build_authoring_competency_questions_report(CompetencyQuestionsMcpArgs {
            axi_text: Some(sample_authoring_axi_text().to_string()),
            cq_text: Some(
                r#"
version competency_question_bundle_v1

question unknown_relation:
  ask: does a missing relation lower but fail ref validation?
  expect: exists OrderFulfillment.DoesNotExist(order=Order_1)
"#
                .to_string(),
            ),
            questions: Vec::new(),
        })
        .expect("competency report");

        assert_eq!(report.coverage_mode, "advisory");
        assert_eq!(report.executable_questions, 1);
        assert_eq!(report.missing_refs.len(), 1);
        assert_eq!(report.missing_refs[0].ref_id, "OrderFulfillment.DoesNotExist");
    }

    #[test]
    fn lsp_competency_questions_accepts_question_first_text() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "id": "competency-questions",
                "method": "workspace/executeCommand",
                "params": {
                    "command": "axiograph.authoring.competencyQuestions",
                    "arguments": [
                        {
                            "axi_text": sample_authoring_axi_text(),
                            "cq_text": sample_authoring_cq_text()
                        }
                    ]
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        assert!(
            responses[0].get("error").is_none(),
            "unexpected LSP error: {}",
            responses[0]
        );
        assert_eq!(
            responses[0]["result"]["version"],
            json!(AUTHORING_COMPETENCY_QUESTIONS_REPORT_VERSION_V1)
        );
        assert_eq!(responses[0]["result"]["coverage_mode"], json!("authoring_checked"));
        assert_eq!(responses[0]["result"]["executable_questions"], json!(2));
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
        assert_eq!(
            coverage.authoring_flow.coverage.missing_codegen_languages,
            vec!["go".to_string()]
        );
    }

    #[test]
    fn continuous_check_uses_runtime_theory_sidecar_for_strict_gates() {
        let report = json!({
            "version": "behavior_case_report_v1",
            "behavior_case": {"case_id": "case"},
            "context_report": {
                "coverage": {"total_rules": 1, "covered_rules": 1, "drifted_rules": 0},
                "competency_coverage": {"total": 0, "satisfied": 0, "coverage": 1.0}
            },
            "runtime_theory_check": {
                "version": "runtime_theory_check_summary_v1",
                "module_digest": "fnv1a64:test",
                "checked_obligations": 1,
                "review_only_obligations": 0,
                "residual_obligations": 1,
                "blocked_obligations": 0,
                "blocking_errors": 0,
                "closure_tiers": ["finite_fragment"],
                "completeness_claim": "not_claimed_for_all_obligations",
                "ontology_closure_claim": "not_claimed_for_all_obligations",
                "residual_obligation_ids": ["runtime/theory/residual"]
            },
            "codegen_previews": [
                {"language": "rust", "file_hint": "tests/case.rs", "content": "#[test] fn case() {}\n"}
            ]
        });
        let args = ContinuousCheckOptions {
            repo_root: PathBuf::from("."),
            require_codegen: vec!["rust".into()],
            strict_coverage: true,
            require_code_refs: false,
            require_runtime_theory: true,
        };

        let coverage =
            build_continuous_software_coverage_report(&report, &args).expect("build report");
        assert!(!coverage.pass);
        assert_eq!(
            coverage.authoring_flow.profile.profile,
            axiograph_tooling_overlays::AuthoringCoverageProfileV1::Strict
        );
        assert_eq!(
            coverage.runtime_theory.module_digest.as_deref(),
            Some("fnv1a64:test")
        );
        assert_eq!(
            coverage.runtime_theory.residual_obligation_ids,
            vec!["runtime/theory/residual".to_string()]
        );
        assert!(coverage
            .failures
            .iter()
            .any(|failure| failure.contains("runtime-theory obligation")));
    }

    #[test]
    fn continuous_check_marks_ci_profile_when_all_fail_closed_switches_are_set() {
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
            require_codegen: vec!["rust".into()],
            strict_coverage: true,
            require_code_refs: true,
            require_runtime_theory: true,
        };

        let coverage =
            build_continuous_software_coverage_report(&report, &args).expect("build report");
        assert_eq!(
            coverage.authoring_flow.profile.profile,
            axiograph_tooling_overlays::AuthoringCoverageProfileV1::Ci
        );
        assert!(coverage.authoring_flow.profile.ci_ready);
        assert!(!coverage.pass);
        assert!(coverage
            .failures
            .iter()
            .any(|failure| failure.contains("runtime theory-check summary")));
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
        assert!(
            responses[0]["result"]["capabilities"]["executeCommandProvider"]["commands"]
                .as_array()
                .expect("commands")
                .contains(&json!("axiograph.authoring.competencyQuestions"))
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
                assert!(result["capabilities"]["executeCommandProvider"]["commands"]
                    .as_array()
                    .expect("commands")
                    .contains(&json!("axiograph.authoring.competencyQuestions")));
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
                                "case_id": "embedded_tooling",
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
    fn lsp_embedded_behavior_case_fields_emit_tooling_overlay_diagnostics() {
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
                                "case_id": "embedded_tooling",
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
            diagnostic.source.as_deref() == Some("axiograph.behavior_case.embedded_tooling")
                && diagnostic.severity == Some(lsp_types::DiagnosticSeverity::ERROR)
        }));
    }

    #[test]
    fn lsp_cq_document_reports_unresolved_authoring_intent() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": "file:///tmp/order_flow.cq",
                        "text": r#"
version competency_question_bundle_v1

question policy_gap:
  ask: what exceptional fulfillment policy applies?
  note: needs ontology refinement
"#
                    }
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        let params: lsp_types::PublishDiagnosticsParams =
            serde_json::from_value(responses[0]["params"].clone()).expect("typed params");
        assert_eq!(params.uri.as_str(), "file:///tmp/order_flow.cq");
        assert_eq!(params.diagnostics.len(), 1);
        assert_eq!(
            params.diagnostics[0].source.as_deref(),
            Some("axiograph.cq.authoring")
        );
        assert_eq!(
            params.diagnostics[0].severity,
            Some(lsp_types::DiagnosticSeverity::WARNING)
        );
        assert_eq!(params.diagnostics[0].range.start.line, 3);
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
    fn lsp_definition_query_accepts_schema_named_query_key() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "id": "definition-query",
                "method": "workspace/executeCommand",
                "params": {
                    "command": "axiograph.authoring.definitionQuery",
                    "arguments": [
                        {
                            "axi_text": sample_authoring_axi_text(),
                            "definition_query": {
                                "prompt": "define the shipment fulfills order business rule",
                                "include_queries": true,
                                "max_matches": 3
                            }
                        }
                    ]
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        assert!(
            responses[0].get("error").is_none(),
            "unexpected LSP error: {}",
            responses[0]
        );
        assert_eq!(
            responses[0]["result"]["version"],
            json!("definition_query_report_v1")
        );
        assert_eq!(
            responses[0]["result"]["coverage_mode"],
            json!("definition_query")
        );
    }

    #[test]
    fn lsp_coverage_query_accepts_schema_named_query_key() {
        let mut state = AuthoringLspStateV1::default();
        let responses = handle_lsp_message_v1(
            &mut state,
            json!({
                "jsonrpc": "2.0",
                "id": "coverage-query",
                "method": "workspace/executeCommand",
                "params": {
                    "command": "axiograph.authoring.coverageQuery",
                    "arguments": [
                        {
                            "axi_text": sample_authoring_axi_text(),
                            "coverage_query": {
                                "terms": ["payment"],
                                "max_matches": 3
                            }
                        }
                    ]
                }
            }),
        );

        assert_eq!(responses.len(), 1);
        assert!(
            responses[0].get("error").is_none(),
            "unexpected LSP error: {}",
            responses[0]
        );
        assert_eq!(
            responses[0]["result"]["version"],
            json!("coverage_query_report_v1")
        );
        assert_eq!(
            responses[0]["result"]["coverage_mode"],
            json!("advisory")
        );
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
        assert_eq!(
            manifest["report_contracts"]["authoring_flow"]["version"],
            json!("authoring_flow_report_v1")
        );
    }

    #[test]
    fn metadata_surfaces_publish_authoring_flow_profiles() {
        let specs = software_authoring_tool_specs_v1();
        assert_eq!(
            specs["report_contracts"]["profiles"],
            json!(["advisory", "strict", "ci"])
        );
        let continuous_tool = specs["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == json!("axiograph.authoring.continuous_check"))
            .expect("continuous check tool");
        assert!(continuous_tool["output_reports"]
            .as_array()
            .expect("output reports")
            .contains(&json!("authoring_flow_report_v1")));

        let lsp = software_authoring_lsp_capabilities_v1();
        assert_eq!(
            lsp["report_contracts"]["continuous_coverage_field"],
            json!("authoring_flow")
        );
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
        let competency_tool =
            <AuthoringRmcpServer as rmcp::handler::server::ServerHandler>::get_tool(
                &server,
                "axiograph_authoring_competency_questions",
            )
            .expect("competency question tool");
        assert_eq!(
            competency_tool.name.as_ref(),
            "axiograph_authoring_competency_questions"
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
                overlay: Some(sample_overlay(json!({
                    "version": "tooling_overlay_bundle_v1",
                    "codegen_plan": {
                        "languages": ["go", "python"],
                        "test_name": "shipment_release"
                    }
                }))),
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
    fn rmcp_competency_questions_tool_returns_structured_content() {
        let Json(payload) = AuthoringRmcpServer
            .competency_questions(Parameters(CompetencyQuestionsMcpArgs {
                axi_text: Some(sample_authoring_axi_text().to_string()),
                cq_text: Some(sample_authoring_cq_text().to_string()),
                questions: Vec::new(),
            }))
            .expect("competency questions");
        let payload = payload.payload;

        assert_eq!(
            payload["version"],
            json!(AUTHORING_COMPETENCY_QUESTIONS_REPORT_VERSION_V1)
        );
        assert_eq!(payload["coverage_mode"], json!("authoring_checked"));
        assert_eq!(payload["executable_questions"], json!(2));

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
            overlay: None,
            overlay_text: Some(
                json!({
                    "version": "tooling_overlay_bundle_v0",
                    "codegen_plan": {
                        "languages": ["go"],
                        "test_name": "shipment_release"
                    }
                })
                .to_string(),
            ),
        })) {
            Ok(_) => panic!("expected unknown overlay version to be rejected"),
            Err(err) => err,
        };

        assert!(
            err.contains("overlay") || err.contains("version"),
            "unexpected overlay rejection error: {err}"
        );
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
            .any(|tool| tool["name"] == json!("axiograph_authoring_competency_questions")));
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
