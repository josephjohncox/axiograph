//! Industrial engineering example runtime.
//!
//! This crate is intentionally outside `axiograph-cli`: it demonstrates how a
//! domain package can consume canonical `.axi`, import it through the shared
//! runtime substrate, and produce anchored teaching artifacts without becoming
//! part of the ontology kernel.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest, PathDB};
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const INDUSTRIAL_EXAMPLE_CAMPAIGN_VERSION_V1: &str = "regulated_production_line_campaign_v1";
pub const INDUSTRIAL_EXAMPLE_RUN_VERSION_V1: &str = "regulated_production_line_run_v1";
pub const INDUSTRIAL_EXAMPLE_CQ_RESULTS_VERSION_V1: &str =
    "regulated_production_line_cq_results_v1";
pub const INDUSTRIAL_EXAMPLE_COVERAGE_VERSION_V1: &str = "regulated_production_line_coverage_v1";
pub const INDUSTRIAL_EXAMPLE_AGENT_REPORT_VERSION_V1: &str =
    "regulated_production_line_agent_report_v1";
pub const INDUSTRIAL_EXAMPLE_DISTILL_VERSION_V1: &str = "regulated_production_line_distill_v1";
pub const COMPETENCY_QUESTION_BUNDLE_VERSION_V1: &str = "competency_question_bundle_v1";

pub const INDUSTRIAL_EXAMPLE_CACHE_DIR: &str = "_cache/regulated_production_line";
pub const REGULATED_PRODUCTION_LINE_CAMPAIGN_ID: &str = "regulated_production_line_seed";
pub const REGULATED_PRODUCTION_LINE_MODULE_PATH: &str =
    "examples/industrial/RegulatedProductionLine.axi";
pub const REGULATED_PRODUCTION_LINE_CQ_PATH: &str =
    "examples/competency_questions/regulated_production_line.cq";
pub const REGULATED_PRODUCTION_LINE_MODULE_NAME: &str = "RegulatedProductionLine";
pub const REGULATED_PRODUCTION_LINE_SEED_NAME: &str = "RegulatedLineSeed";

fn read_text_bounded(path: impl AsRef<Path>) -> Result<String> {
    const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
    axiograph_security::read_utf8_file_bounded(
        path.as_ref(),
        MAX_INPUT_BYTES,
        "industrial example input",
    )
}

fn read_json_bounded<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
    let text = read_text_bounded(path)?;
    axiograph_security::parse_json_bounded(
        text.as_bytes(),
        MAX_INPUT_BYTES,
        "industrial example JSON",
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustScopeV1 {
    pub anchor: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustContractV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: TrustScopeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetencyQuestionV1 {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub query: String,
    #[serde(default = "default_min_rows")]
    pub min_rows: usize,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetencyQuestionBundleV1 {
    pub version: String,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

fn default_min_rows() -> usize {
    1
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompetencyQuestionTrustV1 {
    pub trust_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetencyQuestionEvaluationV1 {
    pub name: String,
    pub rows: usize,
    pub min_rows: usize,
    pub satisfied: bool,
    pub weight: f64,
    pub cost: f64,
    pub trust: CompetencyQuestionTrustV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CompetencyCoverageWithTrustV1 {
    pub total: usize,
    pub satisfied: usize,
    pub coverage: f64,
    pub cost: f64,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionEvaluationV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceCoverageHintV1 {
    Tested,
    Implemented,
    DocumentedOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndustrialSurfaceV1 {
    surface_id: &'static str,
    label: &'static str,
    relation_names: &'static [&'static str],
    hint: SurfaceCoverageHintV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialExampleScenarioV1 {
    RegulatedProductionLineSeed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialExampleRunStatusV1 {
    Materialized,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialExampleCheckOutcomeV1 {
    Pass,
    Fail,
    NotRun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialExampleCoverageStatusV1 {
    Covered,
    Partial,
    Gap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleSeedV1 {
    pub module_name: String,
    pub module_path: String,
    pub instance_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCampaignLayoutV1 {
    pub campaign_manifest_path: String,
    pub runs_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCampaignV1 {
    pub version: String,
    pub campaign_id: String,
    pub scenario: IndustrialExampleScenarioV1,
    pub description: String,
    pub seed: IndustrialExampleSeedV1,
    pub layout: IndustrialExampleCampaignLayoutV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleRunArtifactPathsV1 {
    pub run: String,
    pub cq_results: String,
    pub coverage: String,
    pub agent_report: String,
    pub distill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleRunV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub scenario: IndustrialExampleScenarioV1,
    pub created_at_unix_secs: u64,
    pub status: IndustrialExampleRunStatusV1,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub artifacts: IndustrialExampleRunArtifactPathsV1,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCqResultEntryV1 {
    pub cq_id: String,
    pub title: String,
    pub outcome: IndustrialExampleCheckOutcomeV1,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCqResultsSummaryV1 {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub not_run: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCqResultsV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: IndustrialExampleCqResultsSummaryV1,
    pub results: Vec<IndustrialExampleCqResultEntryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCoverageSurfaceV1 {
    pub surface: String,
    pub status: IndustrialExampleCoverageStatusV1,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCoverageSummaryV1 {
    pub total_surfaces: usize,
    pub covered_surfaces: usize,
    pub partial_surfaces: usize,
    pub gap_surfaces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleCoverageV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: IndustrialExampleCoverageSummaryV1,
    pub surfaces: Vec<IndustrialExampleCoverageSurfaceV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleFindingV1 {
    pub severity: String,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleAgentReportV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: String,
    pub findings: Vec<IndustrialExampleFindingV1>,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleDistillV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: String,
    #[serde(default)]
    pub retained_insights: Vec<String>,
    #[serde(default)]
    pub residual_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleRunBundleV1 {
    pub run: IndustrialExampleRunV1,
    pub cq_results: IndustrialExampleCqResultsV1,
    pub coverage: IndustrialExampleCoverageV1,
    pub agent_report: IndustrialExampleAgentReportV1,
    pub distill: IndustrialExampleDistillV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndustrialExamplePersistedRunPathsV1 {
    pub campaign_manifest_path: PathBuf,
    pub run_path: PathBuf,
    pub cq_results_path: PathBuf,
    pub coverage_path: PathBuf,
    pub agent_report_path: PathBuf,
    pub distill_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleRunResponseV1 {
    pub version: String,
    pub cache_root: String,
    pub campaign_id: String,
    pub run_id: String,
    pub accepted_axi_anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub campaign_manifest_path: String,
    pub artifacts: IndustrialExampleRunArtifactPathsV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleRunRequestV1 {
    pub cache_root: String,
    pub run_id: String,
    pub created_at_unix_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleInspectionVerificationV1 {
    pub total: usize,
    pub failed: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialExampleInspectionResultV1 {
    pub bundle: IndustrialExampleRunBundleV1,
    pub verification: IndustrialExampleInspectionVerificationV1,
}

pub fn sanitize_example_path_component(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

pub fn industrial_example_root(cache_root: &Path) -> PathBuf {
    cache_root.join(INDUSTRIAL_EXAMPLE_CACHE_DIR)
}

pub fn industrial_example_campaign_dir(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_example_root(cache_root).join(sanitize_example_path_component(campaign_id))
}

pub fn industrial_example_runs_dir(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_example_campaign_dir(cache_root, campaign_id).join("runs")
}

pub fn industrial_example_run_dir(cache_root: &Path, campaign_id: &str, run_id: &str) -> PathBuf {
    industrial_example_runs_dir(cache_root, campaign_id)
        .join(sanitize_example_path_component(run_id))
}

pub fn industrial_example_campaign_manifest_path(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_example_campaign_dir(cache_root, campaign_id).join("campaign.json")
}

pub fn industrial_example_run_manifest_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_example_run_dir(cache_root, campaign_id, run_id).join("run.json")
}

pub fn industrial_example_cq_results_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_example_run_dir(cache_root, campaign_id, run_id).join("cq_results.json")
}

pub fn industrial_example_coverage_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_example_run_dir(cache_root, campaign_id, run_id).join("coverage.json")
}

pub fn industrial_example_agent_report_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_example_run_dir(cache_root, campaign_id, run_id).join("agent_report.json")
}

pub fn industrial_example_distill_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_example_run_dir(cache_root, campaign_id, run_id).join("distill.json")
}

pub fn industrial_example_persisted_run_paths(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> IndustrialExamplePersistedRunPathsV1 {
    IndustrialExamplePersistedRunPathsV1 {
        campaign_manifest_path: industrial_example_campaign_manifest_path(cache_root, campaign_id),
        run_path: industrial_example_run_manifest_path(cache_root, campaign_id, run_id),
        cq_results_path: industrial_example_cq_results_path(cache_root, campaign_id, run_id),
        coverage_path: industrial_example_coverage_path(cache_root, campaign_id, run_id),
        agent_report_path: industrial_example_agent_report_path(cache_root, campaign_id, run_id),
        distill_path: industrial_example_distill_path(cache_root, campaign_id, run_id),
    }
}

pub fn industrial_example_artifact_paths(
    campaign_id: &str,
    run_id: &str,
) -> IndustrialExampleRunArtifactPathsV1 {
    let campaign = sanitize_example_path_component(campaign_id);
    let run = sanitize_example_path_component(run_id);
    let base = format!("{INDUSTRIAL_EXAMPLE_CACHE_DIR}/{campaign}/runs/{run}");
    IndustrialExampleRunArtifactPathsV1 {
        run: format!("{base}/run.json"),
        cq_results: format!("{base}/cq_results.json"),
        coverage: format!("{base}/coverage.json"),
        agent_report: format!("{base}/agent_report.json"),
        distill: format!("{base}/distill.json"),
    }
}

pub fn regulated_production_line_campaign(
    cache_root: &Path,
    campaign_id: &str,
) -> IndustrialExampleCampaignV1 {
    let campaign_manifest_path = industrial_example_campaign_manifest_path(cache_root, campaign_id)
        .to_string_lossy()
        .to_string();
    let runs_dir = industrial_example_runs_dir(cache_root, campaign_id)
        .to_string_lossy()
        .to_string();
    IndustrialExampleCampaignV1 {
        version: INDUSTRIAL_EXAMPLE_CAMPAIGN_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        scenario: IndustrialExampleScenarioV1::RegulatedProductionLineSeed,
        description: "Regulated production line seed artifacts".to_string(),
        seed: IndustrialExampleSeedV1 {
            module_name: REGULATED_PRODUCTION_LINE_MODULE_NAME.to_string(),
            module_path: REGULATED_PRODUCTION_LINE_MODULE_PATH.to_string(),
            instance_name: REGULATED_PRODUCTION_LINE_SEED_NAME.to_string(),
        },
        layout: IndustrialExampleCampaignLayoutV1 {
            campaign_manifest_path,
            runs_dir,
        },
    }
}

pub fn persist_industrial_example_run_bundle(
    cache_root: &Path,
    campaign: &IndustrialExampleCampaignV1,
    bundle: &IndustrialExampleRunBundleV1,
) -> Result<IndustrialExamplePersistedRunPathsV1> {
    let campaign_manifest_path =
        industrial_example_campaign_manifest_path(cache_root, &campaign.campaign_id);
    let run_path = industrial_example_run_manifest_path(
        cache_root,
        &bundle.run.campaign_id,
        &bundle.run.run_id,
    );
    let cq_results_path =
        industrial_example_cq_results_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);
    let coverage_path =
        industrial_example_coverage_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);
    let agent_report_path = industrial_example_agent_report_path(
        cache_root,
        &bundle.run.campaign_id,
        &bundle.run.run_id,
    );
    let distill_path =
        industrial_example_distill_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);

    write_pretty_json(cache_root, &campaign_manifest_path, campaign)?;
    write_pretty_json(cache_root, &run_path, &bundle.run)?;
    write_pretty_json(cache_root, &cq_results_path, &bundle.cq_results)?;
    write_pretty_json(cache_root, &coverage_path, &bundle.coverage)?;
    write_pretty_json(cache_root, &agent_report_path, &bundle.agent_report)?;
    write_pretty_json(cache_root, &distill_path, &bundle.distill)?;

    Ok(IndustrialExamplePersistedRunPathsV1 {
        campaign_manifest_path,
        run_path,
        cq_results_path,
        coverage_path,
        agent_report_path,
        distill_path,
    })
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

fn regulated_production_line_cq_questions() -> Result<Vec<CompetencyQuestionV1>> {
    let path = repo_root().join(REGULATED_PRODUCTION_LINE_CQ_PATH);
    let text = read_text_bounded(&path).with_context(|| {
        format!(
            "read regulated production line CQ file `{}`",
            path.display()
        )
    })?;
    parse_competency_question_text(&text).with_context(|| {
        format!(
            "parse regulated production line CQ file `{}`",
            path.display()
        )
    })
}

fn parse_competency_question_text(text: &str) -> Result<Vec<CompetencyQuestionV1>> {
    let mut version_seen = false;
    let mut questions = Vec::new();
    let mut current: Option<CompetencyQuestionV1> = None;

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_no = line_idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(version) = line.strip_prefix("version ").map(str::trim) {
            if version != COMPETENCY_QUESTION_BUNDLE_VERSION_V1 {
                return Err(anyhow!(
                    "line {line_no}: unsupported competency question bundle version `{version}` (expected `{COMPETENCY_QUESTION_BUNDLE_VERSION_V1})"
                ));
            }
            version_seen = true;
            continue;
        }

        if let Some(name) = line
            .strip_prefix("question ")
            .and_then(|rest| rest.strip_suffix(':'))
            .map(str::trim)
        {
            if name.is_empty() {
                return Err(anyhow!("line {line_no}: question name must be non-empty"));
            }
            if let Some(question) = current.take() {
                questions.push(finalize_competency_question(question)?);
            }
            current = Some(CompetencyQuestionV1 {
                name: name.to_string(),
                question: None,
                query: String::new(),
                min_rows: default_min_rows(),
                weight: default_weight(),
                contexts: Vec::new(),
            });
            continue;
        }

        let Some(question) = current.as_mut() else {
            return Err(anyhow!(
                "line {line_no}: expected `question <name>:` before question fields"
            ));
        };

        if let Some(ask) = line.strip_prefix("ask:").map(str::trim) {
            question.question = Some(ask.to_string()).filter(|ask| !ask.is_empty());
        } else if let Some(expect) = line.strip_prefix("expect:").map(str::trim) {
            if expect.is_empty() {
                return Err(anyhow!("line {line_no}: expect clause must be non-empty"));
            }
            if !question.query.is_empty() {
                question.query.push('\n');
            }
            question.query.push_str(expect);
        } else if let Some(min_rows) = line.strip_prefix("min_rows:").map(str::trim) {
            question.min_rows = min_rows
                .parse::<usize>()
                .with_context(|| format!("line {line_no}: invalid min_rows `{min_rows}`"))?;
        } else if let Some(weight) = line.strip_prefix("weight:").map(str::trim) {
            question.weight = weight
                .parse::<f64>()
                .with_context(|| format!("line {line_no}: invalid weight `{weight}`"))?;
        } else if let Some(context) = line.strip_prefix("context:").map(str::trim) {
            if !context.is_empty() {
                question.contexts.push(context.to_string());
            }
        } else {
            return Err(anyhow!("line {line_no}: unsupported CQ field `{line}`"));
        }
    }

    if let Some(question) = current.take() {
        questions.push(finalize_competency_question(question)?);
    }
    if !version_seen {
        return Err(anyhow!(
            "missing `version {COMPETENCY_QUESTION_BUNDLE_VERSION_V1}` header"
        ));
    }
    if questions.is_empty() {
        return Err(anyhow!(
            "competency question file must define at least one question"
        ));
    }
    Ok(questions)
}

fn finalize_competency_question(question: CompetencyQuestionV1) -> Result<CompetencyQuestionV1> {
    if question.query.trim().is_empty() {
        return Err(anyhow!(
            "question `{}` must include at least one `expect:` clause",
            question.name
        ));
    }
    Ok(question)
}

fn regulated_production_line_surfaces() -> Vec<IndustrialSurfaceV1> {
    vec![
        IndustrialSurfaceV1 {
            surface_id: "workflow:released_order_lineage",
            label: "Released order lineage",
            relation_names: &["WorkOrderForSalesOrder", "ShipmentFulfills"],
            hint: SurfaceCoverageHintV1::Tested,
        },
        IndustrialSurfaceV1 {
            surface_id: "workflow:plc_charge_sequence",
            label: "PLC charge sequence",
            relation_names: &["InspectionForWorkOrder", "LotHasCertificate"],
            hint: SurfaceCoverageHintV1::Implemented,
        },
        IndustrialSurfaceV1 {
            surface_id: "report:hmi_blend_overview",
            label: "HMI blend overview",
            relation_names: &["DeliveryCommitment"],
            hint: SurfaceCoverageHintV1::DocumentedOnly,
        },
        IndustrialSurfaceV1 {
            surface_id: "doc_section:release_review_sop",
            label: "Release review SOP",
            relation_names: &["ShipmentFulfills"],
            hint: SurfaceCoverageHintV1::DocumentedOnly,
        },
    ]
}

fn regulated_production_line_run_trust(anchor: &AcceptedAxiAnchor) -> TrustContractV1 {
    TrustContractV1 {
        trust_class: "runtime_guarded".to_string(),
        soundness: "accepted_anchor_scoped_example_run".to_string(),
        coverage: "regulated_line_seed_runtime_reports".to_string(),
        scope: TrustScopeV1 {
            anchor: format!("{}@{}", anchor.accepted_snapshot_id, anchor.axi_digest),
            context: "regulated_production_line_seed".to_string(),
        },
        reasons: vec![
            "runner imports canonical .axi through the shared PathDB runtime importer".to_string(),
            "artifacts are cache-only and do not mutate accepted ontology state".to_string(),
            "all outputs remain scoped to the accepted snapshot/module anchor used for the run"
                .to_string(),
        ],
        gaps: Vec::new(),
    }
}

fn relation_refs_in_query(query: &str) -> Vec<(String, String)> {
    static RELATION_REF_RE: OnceLock<std::result::Result<Regex, regex::Error>> = OnceLock::new();
    let Ok(regex) = RELATION_REF_RE
        .get_or_init(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)\("))
    else {
        return Vec::new();
    };
    let mut refs = regex
        .captures_iter(query)
        .filter_map(|capture| {
            Some((
                capture.get(1)?.as_str().to_string(),
                capture.get(2)?.as_str().to_string(),
            ))
        })
        .collect::<Vec<_>>();
    refs.sort();
    refs.dedup();
    refs
}

fn fact_count_for_relation(db: &PathDB, schema: &str, relation: &str) -> usize {
    db.fact_nodes_by_axi_schema_relation(schema, relation).len() as usize
}

fn evaluate_competency_questions_with_trust(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
) -> CompetencyCoverageWithTrustV1 {
    if questions.is_empty() {
        return CompetencyCoverageWithTrustV1::default();
    }

    let mut satisfied = 0usize;
    let mut total_cost = 0.0;
    let mut results = Vec::new();

    for question in questions {
        let refs = relation_refs_in_query(&question.query);
        let min_rows = question.min_rows.max(1);
        let weight = if question.weight <= 0.0 {
            1.0
        } else {
            question.weight
        };
        let rows = if refs.is_empty() {
            0
        } else {
            refs.iter()
                .map(|(schema, relation)| fact_count_for_relation(db, schema, relation))
                .min()
                .unwrap_or(0)
        };
        let ok = rows >= min_rows;
        if ok {
            satisfied += 1;
        }
        let cost = if ok { 0.0 } else { weight };
        total_cost += cost;
        results.push(CompetencyQuestionEvaluationV1 {
            name: question.name.clone(),
            rows,
            min_rows,
            satisfied: ok,
            weight,
            cost,
            trust: CompetencyQuestionTrustV1 {
                trust_class: "runtime_guarded".to_string(),
                coverage: Some("relation_presence_over_imported_axi_instance".to_string()),
                reasons: vec![
                    "example runtime evaluates canonical .axi relation presence through PathDB fact metadata".to_string(),
                    "this is a pedagogical scenario check, not a full AxQL completeness claim"
                        .to_string(),
                ],
            },
        });
    }

    let total = questions.len();
    CompetencyCoverageWithTrustV1 {
        total,
        satisfied,
        coverage: satisfied as f64 / total as f64,
        cost: total_cost,
        questions: results,
    }
}

fn lower_runtime_cq_results(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    coverage: CompetencyCoverageWithTrustV1,
    questions: &[CompetencyQuestionV1],
) -> IndustrialExampleCqResultsV1 {
    let results = coverage
        .questions
        .into_iter()
        .map(|result| {
            let title = questions
                .iter()
                .find(|q| q.name == result.name)
                .and_then(|q| q.question.clone())
                .unwrap_or_else(|| result.name.clone());
            IndustrialExampleCqResultEntryV1 {
                cq_id: result.name,
                title,
                outcome: if result.satisfied {
                    IndustrialExampleCheckOutcomeV1::Pass
                } else {
                    IndustrialExampleCheckOutcomeV1::Fail
                },
                detail: format!(
                    "rows={} min_rows={} trust_class={} coverage={:?}",
                    result.rows, result.min_rows, result.trust.trust_class, result.trust.coverage
                ),
            }
        })
        .collect::<Vec<_>>();
    IndustrialExampleCqResultsV1 {
        version: INDUSTRIAL_EXAMPLE_CQ_RESULTS_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: IndustrialExampleCqResultsSummaryV1 {
            total: coverage.total,
            passed: coverage.satisfied,
            failed: coverage.total.saturating_sub(coverage.satisfied),
            not_run: 0,
        },
        results,
    }
}

fn lower_runtime_coverage(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    db: &PathDB,
    surfaces: Vec<IndustrialSurfaceV1>,
) -> IndustrialExampleCoverageV1 {
    let surfaces = surfaces
        .into_iter()
        .map(|surface| {
            let present = surface
                .relation_names
                .iter()
                .filter(|relation| fact_count_for_relation(db, "RegulatedLine", relation) > 0)
                .count();
            let status = if present == 0 {
                IndustrialExampleCoverageStatusV1::Gap
            } else if present == surface.relation_names.len() {
                IndustrialExampleCoverageStatusV1::Covered
            } else {
                IndustrialExampleCoverageStatusV1::Partial
            };
            let hint = match surface.hint {
                SurfaceCoverageHintV1::Tested => "tested",
                SurfaceCoverageHintV1::Implemented => "implemented",
                SurfaceCoverageHintV1::DocumentedOnly => "documented_only",
            };
            let detail = format!(
                "surface_id={} hint={} present_relations={} required_relations={}",
                surface.surface_id,
                hint,
                present,
                surface.relation_names.len()
            );
            IndustrialExampleCoverageSurfaceV1 {
                surface: surface.label.to_string(),
                status,
                detail,
            }
        })
        .collect::<Vec<_>>();
    let summary = IndustrialExampleCoverageSummaryV1 {
        total_surfaces: surfaces.len(),
        covered_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialExampleCoverageStatusV1::Covered)
            .count(),
        partial_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialExampleCoverageStatusV1::Partial)
            .count(),
        gap_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialExampleCoverageStatusV1::Gap)
            .count(),
    };
    IndustrialExampleCoverageV1 {
        version: INDUSTRIAL_EXAMPLE_COVERAGE_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary,
        surfaces,
    }
}

fn lower_runtime_agent_report(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    cq_results: &IndustrialExampleCqResultsV1,
    coverage: &IndustrialExampleCoverageV1,
) -> IndustrialExampleAgentReportV1 {
    let mut findings = vec![IndustrialExampleFindingV1 {
        severity: "info".to_string(),
        subject: "accepted_anchor".to_string(),
        detail: format!(
            "run remains bound to accepted snapshot {} and module digest {}",
            anchor.accepted_snapshot_id, anchor.axi_digest
        ),
    }];

    if cq_results.summary.failed > 0 {
        findings.push(IndustrialExampleFindingV1 {
            severity: "warning".to_string(),
            subject: "competency_questions".to_string(),
            detail: format!(
                "{} competency question(s) are not satisfied by the imported .axi instance",
                cq_results.summary.failed
            ),
        });
    }
    if coverage.summary.gap_surfaces > 0 || coverage.summary.partial_surfaces > 0 {
        findings.push(IndustrialExampleFindingV1 {
            severity: "warning".to_string(),
            subject: "implementation_surface_coverage".to_string(),
            detail: format!(
                "{} partial and {} gap surface(s) remain in the example coverage map",
                coverage.summary.partial_surfaces, coverage.summary.gap_surfaces
            ),
        });
    }
    let next_actions = vec![
        "open the reviewable .axi module and inspect relation objects, roles, contexts, and instance facts".to_string(),
        "add a new competency question when a domain behavior is missing from the model".to_string(),
        "promote successful example deltas through the core semantic VCS rather than mutating this cache".to_string(),
    ];

    IndustrialExampleAgentReportV1 {
        version: INDUSTRIAL_EXAMPLE_AGENT_REPORT_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: format!(
            "cq_passed={} cq_total={} covered_surfaces={} partial_surfaces={} gap_surfaces={}",
            cq_results.summary.passed,
            cq_results.summary.total,
            coverage.summary.covered_surfaces,
            coverage.summary.partial_surfaces,
            coverage.summary.gap_surfaces
        ),
        findings,
        next_actions,
    }
}

fn lower_runtime_distill(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    cq_results: &IndustrialExampleCqResultsV1,
    coverage: &IndustrialExampleCoverageV1,
    agent_report: &IndustrialExampleAgentReportV1,
) -> IndustrialExampleDistillV1 {
    IndustrialExampleDistillV1 {
        version: INDUSTRIAL_EXAMPLE_DISTILL_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: format!(
            "regulated-line example runtime: {} / {} CQs satisfied; {} covered, {} partial, {} gap surfaces",
            cq_results.summary.passed,
            cq_results.summary.total,
            coverage.summary.covered_surfaces,
            coverage.summary.partial_surfaces,
            coverage.summary.gap_surfaces
        ),
        retained_insights: vec![
            "shipment/order/work-order lineage remains the primary regulated production line seam"
                .to_string(),
            format!(
                "agent report emitted {} next action(s) under one accepted anchor",
                agent_report.next_actions.len()
            ),
        ],
        residual_risks: agent_report.findings.iter().map(|finding| finding.detail.clone()).collect(),
    }
}

pub fn build_regulated_production_line_runtime_bundle(
    db: &PathDB,
    _meta: &MetaPlaneIndex,
    campaign_id: &str,
    run_id: &str,
    created_at_unix_secs: u64,
    anchor: AcceptedAxiAnchor,
) -> Result<IndustrialExampleRunBundleV1> {
    let trust = regulated_production_line_run_trust(&anchor);
    let questions = regulated_production_line_cq_questions()?;
    let cq_coverage = evaluate_competency_questions_with_trust(db, &questions);
    let surfaces = regulated_production_line_surfaces();

    let cq_results = lower_runtime_cq_results(
        campaign_id,
        run_id,
        &anchor,
        &trust,
        cq_coverage,
        &questions,
    );
    let coverage = lower_runtime_coverage(campaign_id, run_id, &anchor, &trust, db, surfaces);
    let agent_report =
        lower_runtime_agent_report(campaign_id, run_id, &anchor, &trust, &cq_results, &coverage);
    let distill = lower_runtime_distill(
        campaign_id,
        run_id,
        &anchor,
        &trust,
        &cq_results,
        &coverage,
        &agent_report,
    );

    let artifacts = industrial_example_artifact_paths(campaign_id, run_id);
    Ok(IndustrialExampleRunBundleV1 {
        run: IndustrialExampleRunV1 {
            version: INDUSTRIAL_EXAMPLE_RUN_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            scenario: IndustrialExampleScenarioV1::RegulatedProductionLineSeed,
            created_at_unix_secs,
            status: IndustrialExampleRunStatusV1::Materialized,
            anchor: anchor.clone(),
            trust: trust.clone(),
            artifacts,
            notes: vec![
                "Cache artifacts are read-only with respect to accepted ontology state.".to_string(),
                "Runtime CQ/coverage/agent-report outputs are anchored to one accepted snapshot/module pair.".to_string(),
            ],
        },
        cq_results,
        coverage,
        agent_report,
        distill,
    })
}

pub fn run_regulated_production_line_seed_example_from_runtime_parts(
    cache_root: &Path,
    db: &PathDB,
    meta: &MetaPlaneIndex,
    anchor: AcceptedAxiAnchor,
    run_id: &str,
    created_at_unix_secs: u64,
) -> Result<IndustrialExampleRunResponseV1> {
    let campaign =
        regulated_production_line_campaign(cache_root, REGULATED_PRODUCTION_LINE_CAMPAIGN_ID);
    let bundle = build_regulated_production_line_runtime_bundle(
        db,
        meta,
        REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
        run_id,
        created_at_unix_secs,
        anchor.clone(),
    )?;
    let persisted = persist_industrial_example_run_bundle(cache_root, &campaign, &bundle)?;
    Ok(IndustrialExampleRunResponseV1 {
        version: "regulated_production_line_run_response_v1".to_string(),
        cache_root: cache_root.to_string_lossy().to_string(),
        campaign_id: bundle.run.campaign_id.clone(),
        run_id: bundle.run.run_id.clone(),
        accepted_axi_anchor: anchor,
        trust: bundle.run.trust.clone(),
        campaign_manifest_path: persisted
            .campaign_manifest_path
            .to_string_lossy()
            .to_string(),
        artifacts: bundle.run.artifacts,
    })
}

pub fn run_regulated_production_line_seed_example_from_axi_module(
    cache_root: &Path,
    axi_path: &Path,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    run_id: &str,
    created_at_unix_secs: u64,
) -> Result<IndustrialExampleRunResponseV1> {
    let axi_text = read_text_bounded(axi_path)
        .with_context(|| format!("read industrial .axi module `{}`", axi_path.display()))?;
    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &axi_text)
        .with_context(|| format!("import industrial .axi module `{}`", axi_path.display()))?;
    db.build_indexes();
    let meta = MetaPlaneIndex::from_db(&db)?;
    let digest = AxiDigest::from_axi_text(&axi_text);
    let snapshot_id =
        accepted_snapshot_id.unwrap_or_else(|| AcceptedSnapshotId::new(format!("axi:{digest}")));
    let anchor = AcceptedAxiAnchor::new(snapshot_id, digest);
    run_regulated_production_line_seed_example_from_runtime_parts(
        cache_root,
        &db,
        &meta,
        anchor,
        run_id,
        created_at_unix_secs,
    )
}

pub fn inspect_industrial_example_run(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
    expected_anchor: Option<&AcceptedAxiAnchor>,
) -> Result<IndustrialExampleInspectionResultV1> {
    let campaign: IndustrialExampleCampaignV1 = read_json_bounded(
        industrial_example_campaign_manifest_path(cache_root, campaign_id),
    )?;
    let run: IndustrialExampleRunV1 = read_json_bounded(industrial_example_run_manifest_path(
        cache_root,
        campaign_id,
        run_id,
    ))?;
    let cq_results: IndustrialExampleCqResultsV1 = read_json_bounded(
        industrial_example_cq_results_path(cache_root, campaign_id, run_id),
    )?;
    let coverage: IndustrialExampleCoverageV1 = read_json_bounded(
        industrial_example_coverage_path(cache_root, campaign_id, run_id),
    )?;
    let agent_report: IndustrialExampleAgentReportV1 = read_json_bounded(
        industrial_example_agent_report_path(cache_root, campaign_id, run_id),
    )?;
    let distill: IndustrialExampleDistillV1 = read_json_bounded(industrial_example_distill_path(
        cache_root,
        campaign_id,
        run_id,
    ))?;

    let bundle = IndustrialExampleRunBundleV1 {
        run,
        cq_results,
        coverage,
        agent_report,
        distill,
    };

    let mut notes = Vec::new();
    let mut failed = 0usize;
    let mut check = |ok: bool, note: String| {
        if !ok {
            failed += 1;
        }
        notes.push(note);
    };

    check(
        campaign.campaign_id == bundle.run.campaign_id,
        format!(
            "campaign id {} run manifest campaign {}",
            campaign.campaign_id, bundle.run.campaign_id
        ),
    );
    check(
        bundle.run.campaign_id == bundle.cq_results.campaign_id
            && bundle.run.campaign_id == bundle.coverage.campaign_id
            && bundle.run.campaign_id == bundle.agent_report.campaign_id
            && bundle.run.campaign_id == bundle.distill.campaign_id,
        "all example artifacts share the same campaign id".to_string(),
    );
    check(
        bundle.run.run_id == bundle.cq_results.run_id
            && bundle.run.run_id == bundle.coverage.run_id
            && bundle.run.run_id == bundle.agent_report.run_id
            && bundle.run.run_id == bundle.distill.run_id,
        "all example artifacts share the same run id".to_string(),
    );
    check(
        bundle.run.anchor == bundle.cq_results.anchor
            && bundle.run.anchor == bundle.coverage.anchor
            && bundle.run.anchor == bundle.agent_report.anchor
            && bundle.run.anchor == bundle.distill.anchor,
        "all example artifacts share the same accepted anchor".to_string(),
    );
    if let Some(expected_anchor) = expected_anchor {
        check(
            &bundle.run.anchor == expected_anchor,
            format!(
                "run anchor matches expected anchor {}",
                expected_anchor.accepted_snapshot_id
            ),
        );
    }

    Ok(IndustrialExampleInspectionResultV1 {
        bundle,
        verification: IndustrialExampleInspectionVerificationV1 {
            total: notes.len(),
            failed,
            notes,
        },
    })
}

fn render_value_label<T>(value: &T) -> String
where
    T: Serialize + std::fmt::Debug,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{value:?}"))
}

pub fn render_industrial_example_inspection(
    cache_root: &Path,
    inspection: &IndustrialExampleInspectionResultV1,
) -> String {
    let bundle = &inspection.bundle;
    let paths = industrial_example_persisted_run_paths(
        cache_root,
        &bundle.run.campaign_id,
        &bundle.run.run_id,
    );

    format!(
        "status\n  campaign: {}\n  run: {}\n  scenario: {}\n  created_at_unix_secs: {}\n  status: {}\n  accepted anchor: {} @ {}\n  verification: total={} failed={}\nreport\n  cq: total={} passed={} failed={} not_run={}\n  coverage: total_surfaces={} covered={} partial={} gap={}\n  agent: {}\n  agent findings: {}\n  next actions: {}\n  distill: {}\n  residual risks: {}\nreadback\n  campaign manifest: {}\n  run artifact: {}\n  cq results: {}\n  coverage: {}\n  agent report: {}\n  distill: {}",
        bundle.run.campaign_id,
        bundle.run.run_id,
        render_value_label(&bundle.run.scenario),
        bundle.run.created_at_unix_secs,
        render_value_label(&bundle.run.status),
        bundle.run.anchor.accepted_snapshot_id,
        bundle.run.anchor.axi_digest,
        inspection.verification.total,
        inspection.verification.failed,
        bundle.cq_results.summary.total,
        bundle.cq_results.summary.passed,
        bundle.cq_results.summary.failed,
        bundle.cq_results.summary.not_run,
        bundle.coverage.summary.total_surfaces,
        bundle.coverage.summary.covered_surfaces,
        bundle.coverage.summary.partial_surfaces,
        bundle.coverage.summary.gap_surfaces,
        bundle.agent_report.summary,
        bundle.agent_report.findings.len(),
        bundle.agent_report.next_actions.len(),
        bundle.distill.summary,
        bundle.distill.residual_risks.len(),
        paths.campaign_manifest_path.display(),
        paths.run_path.display(),
        paths.cq_results_path.display(),
        paths.coverage_path.display(),
        paths.agent_report_path.display(),
        paths.distill_path.display(),
    )
}

fn write_pretty_json<T: Serialize>(cache_root: &Path, path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all_without_symlinks(cache_root, parent)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(anyhow!(
                    "refusing to overwrite symlinked example artifact path `{}`",
                    path.display()
                ));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("stat example artifact path `{}`", path.display()));
        }
    }
    axiograph_security::write_file_atomic_bounded(
        path,
        serde_json::to_string_pretty(value)?,
        8 * 1024 * 1024,
        "industrial example artifact",
    )?;
    Ok(())
}

fn create_dir_all_without_symlinks(cache_root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(cache_root).map_err(|_| {
        anyhow!(
            "regulated production line artifact path `{}` escapes cache root `{}`",
            path.display(),
            cache_root.display()
        )
    })?;
    let mut current = cache_root.to_path_buf();
    if current.exists() {
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("stat cache root `{}`", current.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(anyhow!(
                "refusing to write regulated production line artifacts through symlinked cache root `{}`",
                current.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(anyhow!(
                "regulated production line cache root `{}` exists but is not a directory",
                current.display()
            ));
        }
    } else {
        fs::create_dir_all(&current).with_context(|| {
            format!(
                "create regulated production line cache root `{}`",
                current.display()
            )
        })?;
    }
    for component in relative.components() {
        if matches!(component, std::path::Component::CurDir) {
            continue;
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(anyhow!(
                        "refusing to write regulated production line artifacts through symlinked path component `{}`",
                        current.display()
                    ));
                }
                if !metadata.is_dir() {
                    return Err(anyhow!(
                        "regulated production line artifact parent `{}` exists but is not a directory",
                        current.display()
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!(
                        "create regulated production line artifact directory `{}`",
                        current.display()
                    )
                })?;
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "stat regulated production line path `{}`",
                        current.display()
                    )
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-industrial-example-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    fn regulated_line_runtime_parts() -> (PathDB, MetaPlaneIndex, AcceptedAxiAnchor) {
        let axi_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(REGULATED_PRODUCTION_LINE_MODULE_PATH);
        let axi_text = read_text_bounded(&axi_path).expect("read regulated line axi");
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &axi_text)
            .expect("import regulated line pathdb");
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db).expect("meta plane");
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::from_axi_text(&axi_text),
        );
        (db, meta, anchor)
    }

    fn regulated_line_runtime_bundle(run_id: &str) -> IndustrialExampleRunBundleV1 {
        let (db, meta, anchor) = regulated_line_runtime_parts();
        build_regulated_production_line_runtime_bundle(
            &db,
            &meta,
            REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
            run_id,
            1_713_810_000,
            anchor,
        )
        .expect("build runtime regulated line bundle")
    }

    #[test]
    fn artifact_paths_are_deterministic_and_sanitized() {
        let paths = industrial_example_artifact_paths(
            "regulated production/seed:v1",
            "run:2026-04-22T18:30:00Z",
        );
        assert_eq!(
            paths.run,
            "_cache/regulated_production_line/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/run.json"
        );
        assert_eq!(
            paths.cq_results,
            "_cache/regulated_production_line/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/cq_results.json"
        );
        assert_eq!(
            paths.coverage,
            "_cache/regulated_production_line/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/coverage.json"
        );
        assert_eq!(
            paths.agent_report,
            "_cache/regulated_production_line/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/agent_report.json"
        );
        assert_eq!(
            paths.distill,
            "_cache/regulated_production_line/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/distill.json"
        );
    }

    #[test]
    fn run_bundle_json_shape_carries_anchor_and_trust() {
        let bundle = regulated_line_runtime_bundle("seed-run-001");

        assert_eq!(bundle.run.version, INDUSTRIAL_EXAMPLE_RUN_VERSION_V1);
        assert_eq!(
            bundle.run.campaign_id,
            REGULATED_PRODUCTION_LINE_CAMPAIGN_ID
        );
        assert_eq!(bundle.run.run_id, "seed-run-001");
        assert_eq!(
            bundle.run.scenario,
            IndustrialExampleScenarioV1::RegulatedProductionLineSeed
        );
        assert_eq!(
            bundle.run.status,
            IndustrialExampleRunStatusV1::Materialized
        );
        assert_eq!(bundle.run.trust.trust_class, "runtime_guarded");
        assert_eq!(
            bundle.run.trust.soundness,
            "accepted_anchor_scoped_example_run"
        );
        assert_eq!(
            bundle.run.artifacts.run,
            "_cache/regulated_production_line/regulated_production_line_seed/runs/seed-run-001/run.json"
        );

        assert_eq!(bundle.cq_results.summary.total, 6);
        assert!(bundle.cq_results.summary.passed >= 1);
    }

    #[test]
    fn materialized_regulated_line_seed_writes_expected_cache_files() {
        let cache_root = temp_test_dir("regulated-line-seed");
        let campaign =
            regulated_production_line_campaign(&cache_root, REGULATED_PRODUCTION_LINE_CAMPAIGN_ID);
        let bundle = regulated_line_runtime_bundle("run/seed:42");
        let persisted = persist_industrial_example_run_bundle(&cache_root, &campaign, &bundle)
            .expect("materialize example");

        assert!(persisted.campaign_manifest_path.exists());
        assert!(persisted.run_path.exists());
        assert!(persisted.cq_results_path.exists());
        assert!(persisted.coverage_path.exists());
        assert!(persisted.agent_report_path.exists());
        assert!(persisted.distill_path.exists());

        let campaign: IndustrialExampleCampaignV1 = serde_json::from_str(
            &read_text_bounded(&persisted.campaign_manifest_path).expect("read campaign"),
        )
        .expect("deserialize campaign");
        assert_eq!(
            campaign.seed.module_path,
            REGULATED_PRODUCTION_LINE_MODULE_PATH
        );

        let run: IndustrialExampleRunV1 =
            serde_json::from_str(&read_text_bounded(&persisted.run_path).expect("read run"))
                .expect("deserialize run");
        assert_eq!(run.run_id, "run/seed:42");
        assert_eq!(run.anchor, bundle.run.anchor);
        assert_eq!(run.trust, bundle.run.trust);
        assert!(persisted.run_path.ends_with(
            "_cache/regulated_production_line/regulated_production_line_seed/runs/run_seed_42/run.json"
        ));

        fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
    }

    #[test]
    fn materialization_rejects_symlinked_artifact_paths() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let cache_root = temp_test_dir("regulated-line-symlink-guard");
            let artifact_dir = cache_root.join(
                "_cache/regulated_production_line/regulated_production_line_seed/runs/run-symlinked",
            );
            fs::create_dir_all(&artifact_dir).expect("create example artifact dir");
            let redirect_target = cache_root.join("outside.json");
            symlink(&redirect_target, artifact_dir.join("run.json"))
                .expect("create symlinked run path");

            let campaign = regulated_production_line_campaign(
                &cache_root,
                REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
            );
            let bundle = regulated_line_runtime_bundle("run-symlinked");
            let err = persist_industrial_example_run_bundle(&cache_root, &campaign, &bundle)
                .expect_err("symlinked artifact path should be rejected");
            assert!(
                err.to_string()
                    .contains("refusing to overwrite symlinked example artifact path"),
                "unexpected error: {err}"
            );

            fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        }
    }

    #[test]
    fn runtime_regulated_line_bundle_uses_real_reports() {
        let (db, meta, anchor) = regulated_line_runtime_parts();

        let bundle = build_regulated_production_line_runtime_bundle(
            &db,
            &meta,
            REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
            "runtime-seed-001",
            1_713_810_000,
            anchor.clone(),
        )
        .expect("build runtime regulated line bundle");

        assert_eq!(bundle.run.anchor, anchor);
        assert_eq!(bundle.cq_results.summary.total, 6);
        assert!(!bundle.coverage.surfaces.is_empty());
        assert!(bundle.agent_report.summary.contains("cq_passed="));
        assert!(
            bundle
                .distill
                .summary
                .contains("regulated-line example runtime"),
            "expected distill summary to be derived from runtime results"
        );
    }
}
