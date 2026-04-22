#![cfg_attr(not(test), allow(dead_code))]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedAxiAnchor, PathDB};
use serde::{Deserialize, Serialize};

use crate::competency_questions::CompetencyCoverageWithTrustV1;
use crate::semantic_claim::{
    AgentTaskRefV1, CoverageEdgeV1, CoverageReportV1, ImplementationSurfaceKindV1,
    ImplementationSurfaceRefV1, RuntimeRuleCatalogV1, RuntimeRuleScopeV1,
};
use crate::trust_contract::TrustContractV1;

pub const INDUSTRIAL_HARNESS_CAMPAIGN_VERSION_V1: &str = "industrial_harness_campaign_v1";
pub const INDUSTRIAL_HARNESS_RUN_VERSION_V1: &str = "industrial_harness_run_v1";
pub const INDUSTRIAL_HARNESS_CQ_RESULTS_VERSION_V1: &str = "industrial_harness_cq_results_v1";
pub const INDUSTRIAL_HARNESS_COVERAGE_VERSION_V1: &str = "industrial_harness_coverage_v1";
pub const INDUSTRIAL_HARNESS_AGENT_REPORT_VERSION_V1: &str = "industrial_harness_agent_report_v1";
pub const INDUSTRIAL_HARNESS_DISTILL_VERSION_V1: &str = "industrial_harness_distill_v1";

pub const INDUSTRIAL_HARNESS_CACHE_DIR: &str = "_cache/industrial_harness";
pub const REGULATED_PRODUCTION_LINE_CAMPAIGN_ID: &str = "regulated_production_line_seed";
pub const REGULATED_PRODUCTION_LINE_MODULE_PATH: &str =
    "examples/industrial/RegulatedProductionLine.axi";
pub const REGULATED_PRODUCTION_LINE_CQ_PATH: &str =
    "examples/competency_questions/regulated_production_line_cq.json";
pub const REGULATED_PRODUCTION_LINE_MODULE_NAME: &str = "RegulatedProductionLine";
pub const REGULATED_PRODUCTION_LINE_SEED_NAME: &str = "RegulatedLineSeed";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialHarnessScenarioV1 {
    RegulatedProductionLineSeed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialHarnessRunStatusV1 {
    Materialized,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialHarnessCheckOutcomeV1 {
    Pass,
    Fail,
    NotRun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialHarnessCoverageStatusV1 {
    Covered,
    Partial,
    Gap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessSeedV1 {
    pub module_name: String,
    pub module_path: String,
    pub instance_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCampaignLayoutV1 {
    pub campaign_manifest_path: String,
    pub runs_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCampaignV1 {
    pub version: String,
    pub campaign_id: String,
    pub scenario: IndustrialHarnessScenarioV1,
    pub description: String,
    pub seed: IndustrialHarnessSeedV1,
    pub layout: IndustrialHarnessCampaignLayoutV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessRunArtifactPathsV1 {
    pub run: String,
    pub cq_results: String,
    pub coverage: String,
    pub agent_report: String,
    pub distill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessRunV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub scenario: IndustrialHarnessScenarioV1,
    pub created_at_unix_secs: u64,
    pub status: IndustrialHarnessRunStatusV1,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub artifacts: IndustrialHarnessRunArtifactPathsV1,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCqResultEntryV1 {
    pub cq_id: String,
    pub title: String,
    pub outcome: IndustrialHarnessCheckOutcomeV1,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCqResultsSummaryV1 {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub not_run: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCqResultsV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: IndustrialHarnessCqResultsSummaryV1,
    pub results: Vec<IndustrialHarnessCqResultEntryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCoverageSurfaceV1 {
    pub surface: String,
    pub status: IndustrialHarnessCoverageStatusV1,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCoverageSummaryV1 {
    pub total_surfaces: usize,
    pub covered_surfaces: usize,
    pub partial_surfaces: usize,
    pub gap_surfaces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessCoverageV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: IndustrialHarnessCoverageSummaryV1,
    pub surfaces: Vec<IndustrialHarnessCoverageSurfaceV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessFindingV1 {
    pub severity: String,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessAgentReportV1 {
    pub version: String,
    pub campaign_id: String,
    pub run_id: String,
    pub anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub summary: String,
    pub findings: Vec<IndustrialHarnessFindingV1>,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessDistillV1 {
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
pub struct IndustrialHarnessRunBundleV1 {
    pub run: IndustrialHarnessRunV1,
    pub cq_results: IndustrialHarnessCqResultsV1,
    pub coverage: IndustrialHarnessCoverageV1,
    pub agent_report: IndustrialHarnessAgentReportV1,
    pub distill: IndustrialHarnessDistillV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndustrialHarnessPersistedRunPathsV1 {
    pub campaign_manifest_path: PathBuf,
    pub run_path: PathBuf,
    pub cq_results_path: PathBuf,
    pub coverage_path: PathBuf,
    pub agent_report_path: PathBuf,
    pub distill_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessRunResponseV1 {
    pub version: String,
    pub cache_root: String,
    pub campaign_id: String,
    pub run_id: String,
    pub accepted_axi_anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    pub campaign_manifest_path: String,
    pub artifacts: IndustrialHarnessRunArtifactPathsV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessRunRequestV1 {
    pub cache_root: String,
    pub run_id: String,
    pub created_at_unix_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessInspectionVerificationV1 {
    pub total: usize,
    pub failed: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndustrialHarnessInspectionResultV1 {
    pub bundle: IndustrialHarnessRunBundleV1,
    pub verification: IndustrialHarnessInspectionVerificationV1,
}

pub fn sanitize_harness_path_component(raw: &str) -> String {
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

pub fn industrial_harness_root(cache_root: &Path) -> PathBuf {
    cache_root.join(INDUSTRIAL_HARNESS_CACHE_DIR)
}

pub fn industrial_harness_campaign_dir(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_harness_root(cache_root).join(sanitize_harness_path_component(campaign_id))
}

pub fn industrial_harness_runs_dir(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_harness_campaign_dir(cache_root, campaign_id).join("runs")
}

pub fn industrial_harness_run_dir(cache_root: &Path, campaign_id: &str, run_id: &str) -> PathBuf {
    industrial_harness_runs_dir(cache_root, campaign_id)
        .join(sanitize_harness_path_component(run_id))
}

pub fn industrial_harness_campaign_manifest_path(cache_root: &Path, campaign_id: &str) -> PathBuf {
    industrial_harness_campaign_dir(cache_root, campaign_id).join("campaign.json")
}

pub fn industrial_harness_run_manifest_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_harness_run_dir(cache_root, campaign_id, run_id).join("run.json")
}

pub fn industrial_harness_cq_results_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_harness_run_dir(cache_root, campaign_id, run_id).join("cq_results.json")
}

pub fn industrial_harness_coverage_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_harness_run_dir(cache_root, campaign_id, run_id).join("coverage.json")
}

pub fn industrial_harness_agent_report_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_harness_run_dir(cache_root, campaign_id, run_id).join("agent_report.json")
}

pub fn industrial_harness_distill_path(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
) -> PathBuf {
    industrial_harness_run_dir(cache_root, campaign_id, run_id).join("distill.json")
}

pub fn industrial_harness_artifact_paths(
    campaign_id: &str,
    run_id: &str,
) -> IndustrialHarnessRunArtifactPathsV1 {
    let campaign = sanitize_harness_path_component(campaign_id);
    let run = sanitize_harness_path_component(run_id);
    let base = format!("{INDUSTRIAL_HARNESS_CACHE_DIR}/{campaign}/runs/{run}");
    IndustrialHarnessRunArtifactPathsV1 {
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
) -> IndustrialHarnessCampaignV1 {
    let campaign_manifest_path = industrial_harness_campaign_manifest_path(cache_root, campaign_id)
        .to_string_lossy()
        .to_string();
    let runs_dir = industrial_harness_runs_dir(cache_root, campaign_id)
        .to_string_lossy()
        .to_string();
    IndustrialHarnessCampaignV1 {
        version: INDUSTRIAL_HARNESS_CAMPAIGN_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        scenario: IndustrialHarnessScenarioV1::RegulatedProductionLineSeed,
        description:
            "Industrial shadow-harness seed artifacts for the regulated production line example"
                .to_string(),
        seed: IndustrialHarnessSeedV1 {
            module_name: REGULATED_PRODUCTION_LINE_MODULE_NAME.to_string(),
            module_path: REGULATED_PRODUCTION_LINE_MODULE_PATH.to_string(),
            instance_name: REGULATED_PRODUCTION_LINE_SEED_NAME.to_string(),
        },
        layout: IndustrialHarnessCampaignLayoutV1 {
            campaign_manifest_path,
            runs_dir,
        },
    }
}

pub fn regulated_production_line_run_bundle(
    campaign_id: &str,
    run_id: &str,
    created_at_unix_secs: u64,
    anchor: AcceptedAxiAnchor,
    trust: TrustContractV1,
) -> IndustrialHarnessRunBundleV1 {
    let artifacts = industrial_harness_artifact_paths(campaign_id, run_id);
    let cq_results = vec![
        IndustrialHarnessCqResultEntryV1 {
            cq_id: "shipment_lineage_traceable".to_string(),
            title: "Shipment remains traceable to its released work order".to_string(),
            outcome: IndustrialHarnessCheckOutcomeV1::Pass,
            detail: "Seed artifact preserves the released shipment/work-order lineage required for a shadow harness replay.".to_string(),
        },
        IndustrialHarnessCqResultEntryV1 {
            cq_id: "certificate_release_link_complete".to_string(),
            title: "Supplier certificate linkage is fully stitched into release review".to_string(),
            outcome: IndustrialHarnessCheckOutcomeV1::Fail,
            detail: "Seed artifact intentionally leaves the certificate-to-release stitch as an explicit harness gap so regulated-line follow-up work has a deterministic placeholder.".to_string(),
        },
    ];
    let cq_summary = IndustrialHarnessCqResultsSummaryV1 {
        total: cq_results.len(),
        passed: cq_results
            .iter()
            .filter(|entry| entry.outcome == IndustrialHarnessCheckOutcomeV1::Pass)
            .count(),
        failed: cq_results
            .iter()
            .filter(|entry| entry.outcome == IndustrialHarnessCheckOutcomeV1::Fail)
            .count(),
        not_run: cq_results
            .iter()
            .filter(|entry| entry.outcome == IndustrialHarnessCheckOutcomeV1::NotRun)
            .count(),
    };

    let coverage_surfaces = vec![
        IndustrialHarnessCoverageSurfaceV1 {
            surface: "ontology.RegulatedLine.ShipmentFulfills".to_string(),
            status: IndustrialHarnessCoverageStatusV1::Covered,
            detail: "Seed artifacts carry shipment/order/work-order anchor lineage.".to_string(),
        },
        IndustrialHarnessCoverageSurfaceV1 {
            surface: "implementation.PLC_ChargeSequence".to_string(),
            status: IndustrialHarnessCoverageStatusV1::Covered,
            detail: "PLC routine remains visible in the regulated-line seed trace.".to_string(),
        },
        IndustrialHarnessCoverageSurfaceV1 {
            surface: "implementation.HMI_BlendOverview".to_string(),
            status: IndustrialHarnessCoverageStatusV1::Partial,
            detail: "UI surface is named but not yet paired with replay evidence in this slice.".to_string(),
        },
        IndustrialHarnessCoverageSurfaceV1 {
            surface: "quality.SupplierCertificate_to_ReleaseDecision".to_string(),
            status: IndustrialHarnessCoverageStatusV1::Gap,
            detail: "The shadow harness still lacks a typed stitched artifact for certificate-backed release review.".to_string(),
        },
    ];
    let coverage_summary = IndustrialHarnessCoverageSummaryV1 {
        total_surfaces: coverage_surfaces.len(),
        covered_surfaces: coverage_surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Covered)
            .count(),
        partial_surfaces: coverage_surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Partial)
            .count(),
        gap_surfaces: coverage_surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Gap)
            .count(),
    };

    let notes = vec![
        "Cache artifacts are read-only with respect to accepted ontology state.".to_string(),
        "This slice only materializes deterministic harness files under _cache/industrial_harness."
            .to_string(),
    ];

    IndustrialHarnessRunBundleV1 {
        run: IndustrialHarnessRunV1 {
            version: INDUSTRIAL_HARNESS_RUN_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            scenario: IndustrialHarnessScenarioV1::RegulatedProductionLineSeed,
            created_at_unix_secs,
            status: IndustrialHarnessRunStatusV1::Materialized,
            anchor: anchor.clone(),
            trust: trust.clone(),
            artifacts: artifacts.clone(),
            notes,
        },
        cq_results: IndustrialHarnessCqResultsV1 {
            version: INDUSTRIAL_HARNESS_CQ_RESULTS_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            anchor: anchor.clone(),
            trust: trust.clone(),
            summary: cq_summary,
            results: cq_results,
        },
        coverage: IndustrialHarnessCoverageV1 {
            version: INDUSTRIAL_HARNESS_COVERAGE_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            anchor: anchor.clone(),
            trust: trust.clone(),
            summary: coverage_summary,
            surfaces: coverage_surfaces,
        },
        agent_report: IndustrialHarnessAgentReportV1 {
            version: INDUSTRIAL_HARNESS_AGENT_REPORT_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            anchor: anchor.clone(),
            trust: trust.clone(),
            summary: "Seed harness confirms shipment lineage while preserving one explicit regulated release gap for follow-up hardening.".to_string(),
            findings: vec![
                IndustrialHarnessFindingV1 {
                    severity: "info".to_string(),
                    subject: "ShipmentFulfills".to_string(),
                    detail: "A typed anchor is attached to the run artifacts so replay outputs stay bound to one accepted snapshot/module pair.".to_string(),
                },
                IndustrialHarnessFindingV1 {
                    severity: "warning".to_string(),
                    subject: "SupplierCertificate".to_string(),
                    detail: "Certificate-backed release stitching is still a declared gap in the seed slice.".to_string(),
                },
            ],
            next_actions: vec![
                "Add real CQ execution outputs once the shadow-harness runner can query the accepted snapshot deterministically.".to_string(),
                "Attach a first typed release-review artifact that stitches supplier certificates into regulated release decisions.".to_string(),
            ],
        },
        distill: IndustrialHarnessDistillV1 {
            version: INDUSTRIAL_HARNESS_DISTILL_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            anchor,
            trust,
            summary: "Regulated-line seed artifacts now have a deterministic cache contract that carries accepted anchors and trust metadata.".to_string(),
            retained_insights: vec![
                "Shipment/order/work-order lineage is the first stable industrial shadow-harness seam.".to_string(),
                "Accepted snapshot and axi digest anchors are persisted directly on each run artifact.".to_string(),
            ],
            residual_risks: vec![
                "This slice materializes deterministic cache files only; it does not yet execute live replay or promotion workflows.".to_string(),
            ],
        },
    }
}

pub fn persist_industrial_harness_run_bundle(
    cache_root: &Path,
    campaign: &IndustrialHarnessCampaignV1,
    bundle: &IndustrialHarnessRunBundleV1,
) -> Result<IndustrialHarnessPersistedRunPathsV1> {
    let campaign_manifest_path =
        industrial_harness_campaign_manifest_path(cache_root, &campaign.campaign_id);
    let run_path = industrial_harness_run_manifest_path(
        cache_root,
        &bundle.run.campaign_id,
        &bundle.run.run_id,
    );
    let cq_results_path =
        industrial_harness_cq_results_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);
    let coverage_path =
        industrial_harness_coverage_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);
    let agent_report_path = industrial_harness_agent_report_path(
        cache_root,
        &bundle.run.campaign_id,
        &bundle.run.run_id,
    );
    let distill_path =
        industrial_harness_distill_path(cache_root, &bundle.run.campaign_id, &bundle.run.run_id);

    write_pretty_json(cache_root, &campaign_manifest_path, campaign)?;
    write_pretty_json(cache_root, &run_path, &bundle.run)?;
    write_pretty_json(cache_root, &cq_results_path, &bundle.cq_results)?;
    write_pretty_json(cache_root, &coverage_path, &bundle.coverage)?;
    write_pretty_json(cache_root, &agent_report_path, &bundle.agent_report)?;
    write_pretty_json(cache_root, &distill_path, &bundle.distill)?;

    Ok(IndustrialHarnessPersistedRunPathsV1 {
        campaign_manifest_path,
        run_path,
        cq_results_path,
        coverage_path,
        agent_report_path,
        distill_path,
    })
}

pub fn materialize_regulated_production_line_seed_harness(
    cache_root: &Path,
    run_id: &str,
    created_at_unix_secs: u64,
    anchor: AcceptedAxiAnchor,
    trust: TrustContractV1,
) -> Result<IndustrialHarnessPersistedRunPathsV1> {
    let campaign =
        regulated_production_line_campaign(cache_root, REGULATED_PRODUCTION_LINE_CAMPAIGN_ID);
    let bundle = regulated_production_line_run_bundle(
        REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
        run_id,
        created_at_unix_secs,
        anchor,
        trust,
    );
    persist_industrial_harness_run_bundle(cache_root, &campaign, &bundle)
}

#[allow(dead_code)]
pub fn materialize_regulated_production_line_seed_harness_now(
    cache_root: &Path,
    run_id: &str,
    anchor: AcceptedAxiAnchor,
    trust: TrustContractV1,
) -> Result<IndustrialHarnessPersistedRunPathsV1> {
    materialize_regulated_production_line_seed_harness(
        cache_root,
        run_id,
        now_unix_secs(),
        anchor,
        trust,
    )
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

fn regulated_production_line_cq_questions() -> Result<Vec<crate::world_model::CompetencyQuestionV1>>
{
    let path = repo_root().join(REGULATED_PRODUCTION_LINE_CQ_PATH);
    let text = fs::read_to_string(&path).with_context(|| {
        format!(
            "read regulated production line CQ file `{}`",
            path.display()
        )
    })?;
    serde_json::from_str(&text).with_context(|| {
        format!(
            "parse regulated production line CQ file `{}`",
            path.display()
        )
    })
}

fn regulated_production_line_surfaces() -> Vec<ImplementationSurfaceRefV1> {
    vec![
        ImplementationSurfaceRefV1 {
            surface_id: "workflow:released_order_lineage".to_string(),
            kind: ImplementationSurfaceKindV1::Workflow,
            label: "Released order lineage".to_string(),
            scopes: vec![
                RuntimeRuleScopeV1::relation("RegulatedLine", "WorkOrderForSalesOrder"),
                RuntimeRuleScopeV1::relation("RegulatedLine", "ShipmentFulfills"),
            ],
            code_refs: vec!["examples/industrial/RegulatedProductionLine.axi".to_string()],
            notes: vec![
                "Tracks sales-order to work-order to shipment lineage in the regulated seed."
                    .to_string(),
            ],
        },
        ImplementationSurfaceRefV1 {
            surface_id: "workflow:plc_charge_sequence".to_string(),
            kind: ImplementationSurfaceKindV1::Workflow,
            label: "PLC charge sequence".to_string(),
            scopes: vec![
                RuntimeRuleScopeV1::relation("RegulatedLine", "InspectionForWorkOrder"),
                RuntimeRuleScopeV1::relation("RegulatedLine", "LotHasCertificate"),
            ],
            code_refs: vec!["examples/industrial/RegulatedProductionLine.axi".to_string()],
            notes: vec![
                "Represents one automation/control seam that should stay aligned with release review."
                    .to_string(),
            ],
        },
        ImplementationSurfaceRefV1 {
            surface_id: "report:hmi_blend_overview".to_string(),
            kind: ImplementationSurfaceKindV1::Report,
            label: "HMI blend overview".to_string(),
            scopes: vec![RuntimeRuleScopeV1::relation(
                "RegulatedLine",
                "DeliveryCommitment",
            )],
            code_refs: vec!["examples/industrial/RegulatedProductionLine.axi".to_string()],
            notes: vec![
                "Represents the operator-facing planning/price/delivery reporting surface."
                    .to_string(),
            ],
        },
        ImplementationSurfaceRefV1 {
            surface_id: "doc_section:release_review_sop".to_string(),
            kind: ImplementationSurfaceKindV1::DocSection,
            label: "Release review SOP".to_string(),
            scopes: vec![RuntimeRuleScopeV1::relation(
                "RegulatedLine",
                "ShipmentFulfills",
            )],
            code_refs: vec!["examples/industrial/RegulatedProductionLine.axi".to_string()],
            notes: vec![
                "Represents the human review/governance surface for regulated shipment release."
                    .to_string(),
            ],
        },
    ]
}

fn coverage_edges_for_relation(
    catalog: &RuntimeRuleCatalogV1,
    schema: &str,
    relation: &str,
    surface_id: &str,
    status: crate::semantic_claim::CoverageStatusV1,
) -> Vec<CoverageEdgeV1> {
    catalog
        .relation_rules(schema, relation)
        .into_iter()
        .map(|rule| CoverageEdgeV1 {
            surface_id: surface_id.to_string(),
            rule_id: rule.rule_id.clone(),
            status,
            notes: Vec::new(),
        })
        .collect()
}

fn regulated_production_line_coverage_edges(meta: &MetaPlaneIndex) -> Vec<CoverageEdgeV1> {
    let catalog = crate::semantic_claim::runtime_rule_catalog(meta);
    let mut edges = Vec::new();
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "WorkOrderForSalesOrder",
        "workflow:released_order_lineage",
        crate::semantic_claim::CoverageStatusV1::Tested,
    ));
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "ShipmentFulfills",
        "workflow:released_order_lineage",
        crate::semantic_claim::CoverageStatusV1::Implemented,
    ));
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "InspectionForWorkOrder",
        "workflow:plc_charge_sequence",
        crate::semantic_claim::CoverageStatusV1::Implemented,
    ));
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "LotHasCertificate",
        "workflow:plc_charge_sequence",
        crate::semantic_claim::CoverageStatusV1::DocumentedOnly,
    ));
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "DeliveryCommitment",
        "report:hmi_blend_overview",
        crate::semantic_claim::CoverageStatusV1::DocumentedOnly,
    ));
    edges.extend(coverage_edges_for_relation(
        &catalog,
        "RegulatedLine",
        "ShipmentFulfills",
        "doc_section:release_review_sop",
        crate::semantic_claim::CoverageStatusV1::DocumentedOnly,
    ));
    edges
}

fn regulated_production_line_run_trust(anchor: &AcceptedAxiAnchor) -> TrustContractV1 {
    TrustContractV1 {
        trust_class: "runtime_guarded".to_string(),
        soundness: "accepted_anchor_scoped_shadow_harness_run".to_string(),
        coverage: "regulated_line_seed_runtime_reports".to_string(),
        scope: crate::trust_contract::TrustScopeV1 {
            anchor: format!("{}@{}", anchor.accepted_snapshot_id, anchor.axi_digest),
            context: "regulated_production_line_seed".to_string(),
        },
        reasons: vec![
            "runner reads an accepted snapshot through the existing read-only runtime loader"
                .to_string(),
            "artifacts are cache-only and do not mutate accepted ontology state".to_string(),
            "all outputs remain scoped to the accepted snapshot/module anchor used for the run"
                .to_string(),
        ],
        certifiable_disjuncts: None,
        execution_only_disjuncts: None,
        semantic_coverage: None,
        semantic_claims: Vec::new(),
        gaps: Vec::new(),
    }
}

fn lower_runtime_cq_results(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    coverage: CompetencyCoverageWithTrustV1,
    questions: &[crate::world_model::CompetencyQuestionV1],
) -> IndustrialHarnessCqResultsV1 {
    let results = coverage
        .questions
        .into_iter()
        .map(|result| {
            let title = questions
                .iter()
                .find(|q| q.name == result.name)
                .and_then(|q| q.question.clone())
                .unwrap_or_else(|| result.name.clone());
            IndustrialHarnessCqResultEntryV1 {
                cq_id: result.name,
                title,
                outcome: if result.satisfied {
                    IndustrialHarnessCheckOutcomeV1::Pass
                } else {
                    IndustrialHarnessCheckOutcomeV1::Fail
                },
                detail: format!(
                    "rows={} min_rows={} trust_class={} coverage={:?}",
                    result.rows, result.min_rows, result.trust.trust_class, result.trust.coverage
                ),
            }
        })
        .collect::<Vec<_>>();
    IndustrialHarnessCqResultsV1 {
        version: INDUSTRIAL_HARNESS_CQ_RESULTS_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: IndustrialHarnessCqResultsSummaryV1 {
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
    coverage: CoverageReportV1,
) -> IndustrialHarnessCoverageV1 {
    let surfaces = coverage
        .surface_reports
        .into_iter()
        .map(|surface| {
            let status = if surface.rules.is_empty() {
                IndustrialHarnessCoverageStatusV1::Gap
            } else if surface.missing_obligations.is_empty() {
                IndustrialHarnessCoverageStatusV1::Covered
            } else {
                IndustrialHarnessCoverageStatusV1::Partial
            };
            let detail = if surface.missing_obligations.is_empty() {
                format!(
                    "trust_class={:?} runtime_enforced={} review_only={}",
                    surface.trust_class, surface.runtime_enforced_rules, surface.review_only_rules
                )
            } else {
                format!(
                    "trust_class={:?} missing={}",
                    surface.trust_class,
                    surface.missing_obligations.join("; ")
                )
            };
            IndustrialHarnessCoverageSurfaceV1 {
                surface: surface.surface.label,
                status,
                detail,
            }
        })
        .collect::<Vec<_>>();
    let summary = IndustrialHarnessCoverageSummaryV1 {
        total_surfaces: surfaces.len(),
        covered_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Covered)
            .count(),
        partial_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Partial)
            .count(),
        gap_surfaces: surfaces
            .iter()
            .filter(|surface| surface.status == IndustrialHarnessCoverageStatusV1::Gap)
            .count(),
    };
    IndustrialHarnessCoverageV1 {
        version: INDUSTRIAL_HARNESS_COVERAGE_VERSION_V1.to_string(),
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
    report: crate::semantic_claim::AgentEngineeringReportV1,
) -> IndustrialHarnessAgentReportV1 {
    let mut findings = vec![IndustrialHarnessFindingV1 {
        severity: "info".to_string(),
        subject: "accepted_anchor".to_string(),
        detail: format!(
            "run remains bound to accepted snapshot {} and module digest {}",
            anchor.accepted_snapshot_id, anchor.axi_digest
        ),
    }];
    findings.extend(report.residual_unknowns.into_iter().map(|detail| {
        IndustrialHarnessFindingV1 {
            severity: "warning".to_string(),
            subject: "residual_unknown".to_string(),
            detail,
        }
    }));

    IndustrialHarnessAgentReportV1 {
        version: INDUSTRIAL_HARNESS_AGENT_REPORT_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: format!(
            "matched_scopes={} matched_rules={} trust_class={:?}",
            report.matched_scope_ids.len(),
            report.matched_rule_ids.len(),
            report.trust_class
        ),
        findings,
        next_actions: report.next_actions,
    }
}

fn lower_runtime_distill(
    campaign_id: &str,
    run_id: &str,
    anchor: &AcceptedAxiAnchor,
    trust: &TrustContractV1,
    cq_results: &IndustrialHarnessCqResultsV1,
    coverage: &IndustrialHarnessCoverageV1,
    agent_report: &IndustrialHarnessAgentReportV1,
) -> IndustrialHarnessDistillV1 {
    IndustrialHarnessDistillV1 {
        version: INDUSTRIAL_HARNESS_DISTILL_VERSION_V1.to_string(),
        campaign_id: campaign_id.to_string(),
        run_id: run_id.to_string(),
        anchor: anchor.clone(),
        trust: trust.clone(),
        summary: format!(
            "regulated-line shadow harness: {} / {} CQs satisfied; {} covered, {} partial, {} gap surfaces",
            cq_results.summary.passed,
            cq_results.summary.total,
            coverage.summary.covered_surfaces,
            coverage.summary.partial_surfaces,
            coverage.summary.gap_surfaces
        ),
        retained_insights: vec![
            "shipment/order/work-order lineage remains the primary industrial shadow-harness seam"
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
    meta: &MetaPlaneIndex,
    campaign_id: &str,
    run_id: &str,
    created_at_unix_secs: u64,
    anchor: AcceptedAxiAnchor,
) -> Result<IndustrialHarnessRunBundleV1> {
    let trust = regulated_production_line_run_trust(&anchor);
    let questions = regulated_production_line_cq_questions()?;
    let cq_coverage =
        crate::competency_questions::evaluate_competency_questions_with_trust(db, &questions)?;
    let surfaces = regulated_production_line_surfaces();
    let edges = regulated_production_line_coverage_edges(meta);
    let coverage = crate::semantic_claim::semantic_coverage_report(
        meta,
        Some(anchor.accepted_snapshot_id.clone()),
        "accepted",
        &surfaces,
        &edges,
    );
    let agent_report = crate::semantic_claim::agent_engineering_report(
        meta,
        Some(anchor.accepted_snapshot_id.clone()),
        "accepted",
        &AgentTaskRefV1 {
            task_id: "industrial_harness.regulated_line_seed".to_string(),
            label: "Regulated line shadow harness".to_string(),
            objective: Some(
                "check regulated line lineage, release review, and implementation surface coverage under one accepted anchor"
                    .to_string(),
            ),
            languages: vec!["ontology".to_string(), "plc".to_string(), "hmi".to_string(), "sop".to_string()],
            artifact_refs: vec![REGULATED_PRODUCTION_LINE_MODULE_PATH.to_string()],
            notes: Vec::new(),
        },
        &surfaces,
        &edges,
    );

    let cq_results = lower_runtime_cq_results(
        campaign_id,
        run_id,
        &anchor,
        &trust,
        cq_coverage,
        &questions,
    );
    let coverage = lower_runtime_coverage(campaign_id, run_id, &anchor, &trust, coverage);
    let agent_report =
        lower_runtime_agent_report(campaign_id, run_id, &anchor, &trust, agent_report);
    let distill = lower_runtime_distill(
        campaign_id,
        run_id,
        &anchor,
        &trust,
        &cq_results,
        &coverage,
        &agent_report,
    );

    let artifacts = industrial_harness_artifact_paths(campaign_id, run_id);
    Ok(IndustrialHarnessRunBundleV1 {
        run: IndustrialHarnessRunV1 {
            version: INDUSTRIAL_HARNESS_RUN_VERSION_V1.to_string(),
            campaign_id: campaign_id.to_string(),
            run_id: run_id.to_string(),
            scenario: IndustrialHarnessScenarioV1::RegulatedProductionLineSeed,
            created_at_unix_secs,
            status: IndustrialHarnessRunStatusV1::Materialized,
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

pub fn run_regulated_production_line_seed_harness(
    cache_root: &Path,
    accepted_snapshot_runtime: &crate::db_server::ReadOnlySemanticRuntime,
    run_id: &str,
    created_at_unix_secs: u64,
) -> Result<IndustrialHarnessRunResponseV1> {
    let anchor = accepted_snapshot_runtime
        .accepted_axi_anchor
        .clone()
        .ok_or_else(|| anyhow!("industrial harness requires a single accepted-module anchor"))?;
    let meta = accepted_snapshot_runtime
        .meta
        .as_ref()
        .ok_or_else(|| anyhow!("industrial harness requires meta-plane data"))?;
    let campaign =
        regulated_production_line_campaign(cache_root, REGULATED_PRODUCTION_LINE_CAMPAIGN_ID);
    let bundle = build_regulated_production_line_runtime_bundle(
        accepted_snapshot_runtime.db.as_ref(),
        meta,
        REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
        run_id,
        created_at_unix_secs,
        anchor.clone(),
    )?;
    let persisted = persist_industrial_harness_run_bundle(cache_root, &campaign, &bundle)?;
    Ok(IndustrialHarnessRunResponseV1 {
        version: "industrial_harness_run_response_v1".to_string(),
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

pub fn run_regulated_production_line_seed_harness_from_runtime_parts(
    cache_root: &Path,
    db: &PathDB,
    meta: &MetaPlaneIndex,
    anchor: AcceptedAxiAnchor,
    run_id: &str,
    created_at_unix_secs: u64,
) -> Result<IndustrialHarnessRunResponseV1> {
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
    let persisted = persist_industrial_harness_run_bundle(cache_root, &campaign, &bundle)?;
    Ok(IndustrialHarnessRunResponseV1 {
        version: "industrial_harness_run_response_v1".to_string(),
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

pub fn run_regulated_production_line_seed_harness_from_accepted_snapshot(
    cache_root: &Path,
    accepted_dir: &Path,
    snapshot: &str,
    run_id: &str,
    created_at_unix_secs: u64,
) -> Result<IndustrialHarnessRunResponseV1> {
    let runtime = crate::db_server::load_read_only_semantic_runtime(
        None,
        Some(accepted_dir),
        "accepted",
        snapshot,
    )?;
    run_regulated_production_line_seed_harness(cache_root, &runtime, run_id, created_at_unix_secs)
}

pub fn inspect_industrial_harness_run(
    cache_root: &Path,
    campaign_id: &str,
    run_id: &str,
    expected_anchor: Option<&AcceptedAxiAnchor>,
) -> Result<IndustrialHarnessInspectionResultV1> {
    let campaign: IndustrialHarnessCampaignV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_campaign_manifest_path(cache_root, campaign_id),
    )?)?;
    let run: IndustrialHarnessRunV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_run_manifest_path(cache_root, campaign_id, run_id),
    )?)?;
    let cq_results: IndustrialHarnessCqResultsV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_cq_results_path(cache_root, campaign_id, run_id),
    )?)?;
    let coverage: IndustrialHarnessCoverageV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_coverage_path(cache_root, campaign_id, run_id),
    )?)?;
    let agent_report: IndustrialHarnessAgentReportV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_agent_report_path(cache_root, campaign_id, run_id),
    )?)?;
    let distill: IndustrialHarnessDistillV1 = serde_json::from_str(&fs::read_to_string(
        industrial_harness_distill_path(cache_root, campaign_id, run_id),
    )?)?;

    let bundle = IndustrialHarnessRunBundleV1 {
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
        "all harness artifacts share the same campaign id".to_string(),
    );
    check(
        bundle.run.run_id == bundle.cq_results.run_id
            && bundle.run.run_id == bundle.coverage.run_id
            && bundle.run.run_id == bundle.agent_report.run_id
            && bundle.run.run_id == bundle.distill.run_id,
        "all harness artifacts share the same run id".to_string(),
    );
    check(
        bundle.run.anchor == bundle.cq_results.anchor
            && bundle.run.anchor == bundle.coverage.anchor
            && bundle.run.anchor == bundle.agent_report.anchor
            && bundle.run.anchor == bundle.distill.anchor,
        "all harness artifacts share the same accepted anchor".to_string(),
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

    Ok(IndustrialHarnessInspectionResultV1 {
        bundle,
        verification: IndustrialHarnessInspectionVerificationV1 {
            total: notes.len(),
            failed,
            notes,
        },
    })
}

fn write_pretty_json<T: Serialize>(cache_root: &Path, path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all_without_symlinks(cache_root, parent)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(anyhow!(
                    "refusing to overwrite symlinked harness artifact path `{}`",
                    path.display()
                ));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("stat harness artifact path `{}`", path.display()));
        }
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn create_dir_all_without_symlinks(cache_root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(cache_root).map_err(|_| {
        anyhow!(
            "industrial harness artifact path `{}` escapes cache root `{}`",
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
                "refusing to write industrial harness artifacts through symlinked cache root `{}`",
                current.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(anyhow!(
                "industrial harness cache root `{}` exists but is not a directory",
                current.display()
            ));
        }
    } else {
        fs::create_dir_all(&current).with_context(|| {
            format!(
                "create industrial harness cache root `{}`",
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
                        "refusing to write industrial harness artifacts through symlinked path component `{}`",
                        current.display()
                    ));
                }
                if !metadata.is_dir() {
                    return Err(anyhow!(
                        "industrial harness artifact parent `{}` exists but is not a directory",
                        current.display()
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!(
                        "create industrial harness artifact directory `{}`",
                        current.display()
                    )
                })?;
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("stat industrial harness path `{}`", current.display())
                });
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest};
    use serde_json::json;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-industrial-harness-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    fn sample_anchor() -> AcceptedAxiAnchor {
        AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::new("fnv1a64:regulated-line"),
        )
    }

    fn sample_trust() -> TrustContractV1 {
        TrustContractV1 {
            trust_class: "runtime_guarded".to_string(),
            soundness: "seed_cache_artifacts_are_anchor_scoped".to_string(),
            coverage: "regulated_line_seed_shadow_harness".to_string(),
            scope: crate::trust_contract::TrustScopeV1 {
                anchor: "accepted:regulated-line@fnv1a64:regulated-line".to_string(),
                context: "regulated_production_line_seed".to_string(),
            },
            reasons: vec![
                "cache-only seed artifacts carry accepted anchors".to_string(),
                "no accepted-plane mutation occurs in this helper".to_string(),
            ],
            certifiable_disjuncts: None,
            execution_only_disjuncts: None,
            semantic_coverage: None,
            semantic_claims: Vec::new(),
            gaps: Vec::new(),
        }
    }

    #[test]
    fn artifact_paths_are_deterministic_and_sanitized() {
        let paths = industrial_harness_artifact_paths(
            "regulated production/seed:v1",
            "run:2026-04-22T18:30:00Z",
        );
        assert_eq!(
            paths.run,
            "_cache/industrial_harness/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/run.json"
        );
        assert_eq!(
            paths.cq_results,
            "_cache/industrial_harness/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/cq_results.json"
        );
        assert_eq!(
            paths.coverage,
            "_cache/industrial_harness/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/coverage.json"
        );
        assert_eq!(
            paths.agent_report,
            "_cache/industrial_harness/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/agent_report.json"
        );
        assert_eq!(
            paths.distill,
            "_cache/industrial_harness/regulated_production_seed_v1/runs/run_2026-04-22T18_30_00Z/distill.json"
        );
    }

    #[test]
    fn run_bundle_json_shape_carries_anchor_and_trust() {
        let bundle = regulated_production_line_run_bundle(
            REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
            "seed-run-001",
            1_713_810_000,
            sample_anchor(),
            sample_trust(),
        );

        let run_json = serde_json::to_value(&bundle.run).expect("serialize run");
        assert_eq!(
            run_json,
            json!({
                "version": "industrial_harness_run_v1",
                "campaign_id": "regulated_production_line_seed",
                "run_id": "seed-run-001",
                "scenario": "regulated_production_line_seed",
                "created_at_unix_secs": 1713810000u64,
                "status": "materialized",
                "anchor": {
                    "accepted_snapshot_id": "accepted:regulated-line",
                    "axi_digest": "fnv1a64:regulated-line"
                },
                "trust": {
                    "trust_class": "runtime_guarded",
                    "soundness": "seed_cache_artifacts_are_anchor_scoped",
                    "coverage": "regulated_line_seed_shadow_harness",
                    "scope": {
                        "anchor": "accepted:regulated-line@fnv1a64:regulated-line",
                        "context": "regulated_production_line_seed"
                    },
                    "reasons": [
                        "cache-only seed artifacts carry accepted anchors",
                        "no accepted-plane mutation occurs in this helper"
                    ]
                },
                "artifacts": {
                    "run": "_cache/industrial_harness/regulated_production_line_seed/runs/seed-run-001/run.json",
                    "cq_results": "_cache/industrial_harness/regulated_production_line_seed/runs/seed-run-001/cq_results.json",
                    "coverage": "_cache/industrial_harness/regulated_production_line_seed/runs/seed-run-001/coverage.json",
                    "agent_report": "_cache/industrial_harness/regulated_production_line_seed/runs/seed-run-001/agent_report.json",
                    "distill": "_cache/industrial_harness/regulated_production_line_seed/runs/seed-run-001/distill.json"
                },
                "notes": [
                    "Cache artifacts are read-only with respect to accepted ontology state.",
                    "This slice only materializes deterministic harness files under _cache/industrial_harness."
                ]
            })
        );

        let cq_json = serde_json::to_value(&bundle.cq_results).expect("serialize cq results");
        assert_eq!(
            cq_json["summary"],
            json!({
                "total": 2,
                "passed": 1,
                "failed": 1,
                "not_run": 0
            })
        );
        assert_eq!(cq_json["results"][0]["outcome"], json!("pass"));
        assert_eq!(cq_json["results"][1]["outcome"], json!("fail"));
    }

    #[test]
    fn materialized_regulated_line_seed_writes_expected_cache_files() {
        let cache_root = temp_test_dir("regulated-line-seed");
        let persisted = materialize_regulated_production_line_seed_harness(
            &cache_root,
            "run/seed:42",
            4242,
            sample_anchor(),
            sample_trust(),
        )
        .expect("materialize harness");

        assert!(persisted.campaign_manifest_path.exists());
        assert!(persisted.run_path.exists());
        assert!(persisted.cq_results_path.exists());
        assert!(persisted.coverage_path.exists());
        assert!(persisted.agent_report_path.exists());
        assert!(persisted.distill_path.exists());

        let campaign: IndustrialHarnessCampaignV1 = serde_json::from_str(
            &fs::read_to_string(&persisted.campaign_manifest_path).expect("read campaign"),
        )
        .expect("deserialize campaign");
        assert_eq!(
            campaign.seed.module_path,
            REGULATED_PRODUCTION_LINE_MODULE_PATH
        );

        let run: IndustrialHarnessRunV1 =
            serde_json::from_str(&fs::read_to_string(&persisted.run_path).expect("read run"))
                .expect("deserialize run");
        assert_eq!(run.run_id, "run/seed:42");
        assert_eq!(run.anchor, sample_anchor());
        assert_eq!(run.trust, sample_trust());
        assert!(persisted.run_path.ends_with(
            "_cache/industrial_harness/regulated_production_line_seed/runs/run_seed_42/run.json"
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
                "_cache/industrial_harness/regulated_production_line_seed/runs/run-symlinked",
            );
            fs::create_dir_all(&artifact_dir).expect("create harness artifact dir");
            let redirect_target = cache_root.join("outside.json");
            symlink(&redirect_target, artifact_dir.join("run.json"))
                .expect("create symlinked run path");

            let err = materialize_regulated_production_line_seed_harness(
                &cache_root,
                "run-symlinked",
                4242,
                sample_anchor(),
                sample_trust(),
            )
            .expect_err("symlinked artifact path should be rejected");
            assert!(
                err.to_string()
                    .contains("refusing to overwrite symlinked harness artifact path"),
                "unexpected error: {err}"
            );

            fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        }
    }

    #[test]
    fn runtime_regulated_line_bundle_uses_real_reports() {
        let axi_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(REGULATED_PRODUCTION_LINE_MODULE_PATH);
        let db = crate::load_pathdb_for_cli(&axi_path).expect("load regulated line pathdb");
        let meta = MetaPlaneIndex::from_db(&db).expect("meta plane");
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::from_axi_text(
                &fs::read_to_string(&axi_path).expect("read regulated line axi"),
            ),
        );

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
        assert!(
            bundle.agent_report.summary.contains("matched_scopes="),
            "expected agent report summary to come from runtime report lowering"
        );
        assert!(
            bundle
                .distill
                .summary
                .contains("regulated-line shadow harness"),
            "expected distill summary to be derived from runtime results"
        );
    }
}
