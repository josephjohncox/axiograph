use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "axiograph-software-authoring")]
#[command(
    about = "Software coverage, tooling-overlay codegen, and explicit skeleton materialization for Axiograph"
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
    /// Emit software-coverage, overlay-codegen, and workspace-service metadata.
    ToolSpecs {
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
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
    pub runtime_theory: Option<axiograph_tooling_overlays::RuntimeTheoryCheckSummaryV1>,
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
    }
}

fn run_codegen_plan(overlay_path: &Path, json_output: bool) -> Result<()> {
    let text = read_text_bounded(overlay_path, "tooling overlay")?;
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
        axiograph_security::write_file_atomic_bounded(
            &path,
            content,
            8 * 1024 * 1024,
            "software authoring output",
        )
        .with_context(|| format!("write `{}`", path.display()))?;
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
    match runtime_theory.as_ref() {
        None if options.require_runtime_theory => failures
            .push("behavior report does not include a runtime theory-check summary".to_string()),
        None => warnings
            .push("behavior report has no embedded runtime theory-check summary".to_string()),
        Some(summary) => failures.extend(summary.gate_blockers()),
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
    if runtime_theory.is_none() {
        next_actions.push(
            "generate the behavior report from canonical `.axi` so it carries RuntimeTheoryCheckSummaryV1"
                .to_string(),
        );
    } else if runtime_theory
        .as_ref()
        .is_some_and(|summary| !summary.gate_blockers().is_empty())
    {
        next_actions.push(
            "resolve every review-only, residual, or blocked runtime-theory obligation".to_string(),
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
            runtime_theory_present: runtime_theory.is_some(),
            runtime_theory_residual_obligations: runtime_theory
                .as_ref()
                .map(|summary| summary.residual_obligations as u64),
            runtime_theory_blocking_obligations: runtime_theory.as_ref().map(|summary| {
                summary
                    .review_only_obligations
                    .saturating_add(summary.residual_obligations)
                    .saturating_add(summary.blocked_obligations)
                    .saturating_add(summary.blocking_errors) as u64
            }),
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
    let report: Value = axiograph_security::parse_json_bounded(
        report_json.as_bytes(),
        8 * 1024 * 1024,
        "behavior_case_report_v1",
    )?;
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
    if let Some(summary) = report.runtime_theory.as_ref() {
        println!(
            "  runtime_theory: present=true fragments=[{}] checked={} review_only={} residual={} blocked={}",
            summary.scope.fragments.join(", "),
            summary.checked_obligations,
            summary.review_only_obligations,
            summary.residual_obligations,
            summary.blocked_obligations.saturating_add(summary.blocking_errors)
        );
    } else {
        println!("  runtime_theory: present=false");
    }
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
                "name": "axiograph.authoring.workspace",
                "description": "The unified typed ontology-authoring service is owned by the main axiograph CLI; this standalone crate only retains software coverage, overlay codegen planning, and explicit skeleton materialization.",
                "mutation": "read_only",
                "entrypoint": "axiograph authoring workspace --workspace <root> --request <request.json>"
            }
        ]
    })
}

fn read_text_bounded(path: &Path, label: &str) -> Result<String> {
    const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
    axiograph_security::read_utf8_file_bounded(path, MAX_INPUT_BYTES, label)
}

fn read_json(path: &Path) -> Result<Value> {
    let text = read_text_bounded(path, "software authoring JSON")?;
    axiograph_security::parse_json_bounded(
        text.as_bytes(),
        8 * 1024 * 1024,
        "software authoring JSON",
    )
    .with_context(|| format!("parse `{}` as JSON", path.display()))
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
    runtime_theory_check: Option<axiograph_tooling_overlays::RuntimeTheoryCheckSummaryV1>,
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
    runtime_theory_check: Option<axiograph_tooling_overlays::RuntimeTheoryCheckSummaryV1>,
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

fn runtime_theory_presence(
    report: &BehaviorCaseAuthoringReportV1,
) -> Option<axiograph_tooling_overlays::RuntimeTheoryCheckSummaryV1> {
    report
        .runtime_theory_check
        .clone()
        .or_else(|| report.context_report.runtime_theory_check.clone())
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
    use serde_json::json;

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
                "report_version": "runtime_theory_check_report_v1",
                "module_digest": "axi:revision:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "theory_count": 1,
                "checked_obligations": 1,
                "review_only_obligations": 1,
                "residual_obligations": 1,
                "blocked_obligations": 0,
                "excluded_by_evidence": 0,
                "blocking_errors": 0,
                "scope": {"fragments": ["finite_fragment"]},
                "admissibility_trace": {},
                "transport_summary": {},
                "residual_obligation_ids": ["runtime/theory/residual"],
                "non_claims": [{
                    "code": "closure_engine_not_implemented",
                    "message": "admissibility scan only"
                }]
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
        let runtime = coverage.runtime_theory.as_ref().expect("runtime summary");
        assert_eq!(
            runtime.module_digest,
            "axi:revision:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            runtime.residual_obligation_ids,
            vec!["runtime/theory/residual".to_string()]
        );
        assert_eq!(runtime.scope.fragments, vec!["finite_fragment"]);
        assert!(runtime
            .non_claims
            .iter()
            .any(|claim| claim.code == "closure_engine_not_implemented"));
        assert!(coverage
            .failures
            .iter()
            .any(|failure| failure.contains("review-only")));
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
}
