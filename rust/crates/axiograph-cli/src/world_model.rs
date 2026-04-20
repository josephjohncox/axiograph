//! World model interface + guardrail costs (objective-driven / JEPA hooks).
//!
//! This module provides:
//! - a small plugin protocol (`axiograph_world_model_v1`),
//! - a stub backend (returns empty proposals),
//! - a command adapter (executes a local plugin),
//! - guardrail cost extraction from existing checks.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_ingest_docs::{
    EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1,
};
use axiograph_pathdb::certificate::AxiWellTypedProofV1;
use axiograph_pathdb::checked_db::CheckedDb;
use axiograph_pathdb::{
    AcceptedSnapshotId, AxiDigest, PathDB, PathdbSnapshotId, ProposalDigest, WorldModelRunId,
};
use axiograph_pathdb::{Module, WellTypedModuleState};

pub const WORLD_MODEL_PROTOCOL_V1: &str = "axiograph_world_model_v1";

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn default_trace_id() -> WorldModelRunId {
    WorldModelRunId::new(format!("wm::{}", now_unix_secs()))
}

// ---------------------------------------------------------------------------
// JEPA export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JepaExportFileV1 {
    pub version: String,
    pub axi_digest_v1: AxiDigest,
    pub module_name: String,
    pub module_text: String,
    pub module: axiograph_dsl::schema_v1::SchemaV1Module,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_well_typed_proof_v1: Option<AxiWellTypedProofV1>,
    pub items: Vec<JepaExportItemV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JepaExportItemV1 {
    pub schema: String,
    pub instance: String,
    pub relation: String,
    pub fields: Vec<(String, String)>,
    pub mask_fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JepaExportOptions {
    pub instance_filter: Option<String>,
    pub max_items: usize,
    pub mask_fields: usize,
    pub seed: u64,
    /// Relation names to exclude from the export (useful to hide snapshot
    /// implementation details such as `interned_string` when falling back to
    /// `PathDBExportV1`).
    pub exclude_relations: Vec<String>,
}

pub fn build_jepa_export_from_axi_text(
    axi_text: &str,
    opts: &JepaExportOptions,
) -> Result<JepaExportFileV1> {
    if opts.mask_fields == 0 {
        return Err(anyhow!("--mask-fields must be > 0"));
    }

    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    build_jepa_export_from_well_typed_module(
        axi_text,
        canonical.module(),
        canonical.digest().clone(),
        opts,
    )
}

fn build_jepa_export_from_well_typed_module<S: WellTypedModuleState>(
    axi_text: &str,
    module: &Module<S>,
    digest: AxiDigest,
    opts: &JepaExportOptions,
) -> Result<JepaExportFileV1> {
    let typed_module = module.module();

    let mut relations_by_schema: HashMap<String, HashSet<String>> = HashMap::new();
    for schema in &typed_module.schemas {
        let entry = relations_by_schema
            .entry(schema.name.clone())
            .or_insert_with(HashSet::new);
        for rel in &schema.relations {
            entry.insert(rel.name.clone());
        }
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(opts.seed);
    let mut items: Vec<JepaExportItemV1> = Vec::new();
    let excluded: HashSet<&str> = opts.exclude_relations.iter().map(|s| s.as_str()).collect();

    for inst in &typed_module.instances {
        if let Some(filter) = opts.instance_filter.as_ref() {
            if inst.name != *filter {
                continue;
            }
        }
        let Some(rel_set) = relations_by_schema.get(&inst.schema) else {
            continue;
        };

        for assign in &inst.assignments {
            if excluded.contains(assign.name.as_str()) {
                continue;
            }
            if !rel_set.contains(&assign.name) {
                continue;
            }
            for item in &assign.value.items {
                let axiograph_dsl::schema_v1::SetItemV1::Tuple { fields } = item else {
                    continue;
                };
                if fields.is_empty() {
                    continue;
                }

                let mut mask: Vec<String> = Vec::new();
                let field_count = fields.len();
                let target_masks = opts.mask_fields.min(field_count);
                let mut used = HashSet::new();
                while mask.len() < target_masks {
                    let idx = rng.gen_range_usize(field_count);
                    if used.insert(idx) {
                        mask.push(fields[idx].0.clone());
                    }
                }

                let entry = JepaExportItemV1 {
                    schema: inst.schema.clone(),
                    instance: inst.name.clone(),
                    relation: assign.name.clone(),
                    fields: fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    mask_fields: mask,
                };

                items.push(entry);
                if opts.max_items > 0 && items.len() >= opts.max_items {
                    break;
                }
            }

            if opts.max_items > 0 && items.len() >= opts.max_items {
                break;
            }
        }

        if opts.max_items > 0 && items.len() >= opts.max_items {
            break;
        }
    }

    Ok(JepaExportFileV1 {
        version: "axi_jepa_export_v1".to_string(),
        axi_digest_v1: digest,
        module_name: typed_module.module_name.clone(),
        module_text: axi_text.to_string(),
        module: typed_module.clone(),
        axi_well_typed_proof_v1: Some(module.proof().clone()),
        items,
    })
}

pub fn write_jepa_export(
    input: &Path,
    out: &Path,
    opts: &JepaExportOptions,
) -> Result<JepaExportFileV1> {
    let text = std::fs::read_to_string(input)?;
    let export = build_jepa_export_from_axi_text(&text, opts)?;
    let json = serde_json::to_string_pretty(&export)?;
    std::fs::write(out, json)?;
    Ok(export)
}

#[allow(dead_code)]
pub fn read_jepa_export(path: &Path) -> Result<JepaExportFileV1> {
    let text = std::fs::read_to_string(path)?;
    let mut export: JepaExportFileV1 = serde_json::from_str(&text)?;
    if export.axi_well_typed_proof_v1.is_none() {
        let validated = axiograph_pathdb::validate_axi_v1_module(export.module.clone())?;
        let (_, proof) = validated.into_parts();
        export.axi_well_typed_proof_v1 = Some(proof);
    }
    Ok(export)
}

// ---------------------------------------------------------------------------
// Guardrail costs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCostReportV1 {
    pub version: String,
    pub generated_at_unix_secs: u64,
    pub input: String,
    pub profile: String,
    pub plane: String,
    pub summary: GuardrailCostSummaryV1,
    pub terms: Vec<GuardrailCostTermV1>,
    pub quality: GuardrailQualitySummaryV1,
    pub checked: GuardrailCheckedSummaryV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCostSummaryV1 {
    pub total_cost: f64,
    pub term_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCostTermV1 {
    pub name: String,
    pub value: f64,
    pub weight: f64,
    pub cost: f64,
    pub unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardrailQualitySummaryV1 {
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardrailCheckedSummaryV1 {
    pub axi_facts_checked: usize,
    pub axi_fact_errors: usize,
    pub rewrite_rules_checked: usize,
    pub rewrite_rule_errors: usize,
    pub context_checked_facts: usize,
    pub context_checked_edges: usize,
    pub context_errors: usize,
    pub modal_checked_edges: usize,
    pub modal_errors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailCostWeightsV1 {
    pub quality_error: f64,
    pub quality_warning: f64,
    pub quality_info: f64,
    pub axi_fact_error: f64,
    pub rewrite_rule_error: f64,
    pub context_error: f64,
    pub modal_error: f64,
}

impl GuardrailCostWeightsV1 {
    pub fn defaults() -> Self {
        Self {
            quality_error: 10.0,
            quality_warning: 2.0,
            quality_info: 0.5,
            axi_fact_error: 10.0,
            rewrite_rule_error: 8.0,
            context_error: 5.0,
            modal_error: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardrailPlanSummaryV1 {
    pub total_cost: f64,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub axi_fact_errors: usize,
    pub rewrite_rule_errors: usize,
    pub context_errors: usize,
    pub modal_errors: usize,
}

fn guardrail_plan_summary(report: &GuardrailCostReportV1) -> GuardrailPlanSummaryV1 {
    GuardrailPlanSummaryV1 {
        total_cost: report.summary.total_cost,
        error_count: report.quality.error_count,
        warning_count: report.quality.warning_count,
        info_count: report.quality.info_count,
        axi_fact_errors: report.checked.axi_fact_errors,
        rewrite_rule_errors: report.checked.rewrite_rule_errors,
        context_errors: report.checked.context_errors,
        modal_errors: report.checked.modal_errors,
    }
}

fn empty_guardrail_report(input: &str, profile: &str, plane: &str) -> GuardrailCostReportV1 {
    GuardrailCostReportV1 {
        version: "guardrail_costs_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input.to_string(),
        profile: profile.to_string(),
        plane: plane.to_string(),
        summary: GuardrailCostSummaryV1 {
            total_cost: 0.0,
            term_count: 0,
        },
        terms: Vec::new(),
        quality: GuardrailQualitySummaryV1::default(),
        checked: GuardrailCheckedSummaryV1::default(),
    }
}

fn proposals_digest(file: &ProposalsFileV1) -> Result<ProposalDigest> {
    let bytes = serde_json::to_vec(file)
        .map_err(|e| anyhow!("failed to serialize proposals for digest: {e}"))?;
    Ok(ProposalDigest::new(
        axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes),
    ))
}

fn apply_proposals_to_db(db: &mut PathDB, proposals: &ProposalsFileV1) -> Result<()> {
    let digest = proposals_digest(proposals)?;
    let _summary =
        crate::proposals_import::import_proposals_file_into_pathdb(db, proposals, digest.as_str())?;
    Ok(())
}

fn clone_db(db: &PathDB) -> Result<PathDB> {
    let bytes = db.to_bytes()?;
    Ok(PathDB::from_bytes(&bytes)?)
}

pub fn parse_guardrail_weights(pairs: &[String]) -> Result<GuardrailCostWeightsV1> {
    let mut weights = GuardrailCostWeightsV1::defaults();
    for raw in pairs {
        let (key, value) = raw
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid guardrail weight `{raw}` (expected key=value)"))?;
        let v = value.trim().parse::<f64>().map_err(|_| {
            anyhow!("invalid guardrail weight `{raw}` (expected key=value with numeric value)")
        })?;
        match key.trim() {
            "quality_error" => weights.quality_error = v,
            "quality_warning" => weights.quality_warning = v,
            "quality_info" => weights.quality_info = v,
            "axi_fact_error" => weights.axi_fact_error = v,
            "rewrite_rule_error" => weights.rewrite_rule_error = v,
            "context_error" => weights.context_error = v,
            "modal_error" => weights.modal_error = v,
            other => {
                return Err(anyhow!(
                    "unknown guardrail weight `{other}` (expected one of: quality_error, quality_warning, quality_info, axi_fact_error, rewrite_rule_error, context_error, modal_error)"
                ))
            }
        }
    }
    Ok(weights)
}

pub fn parse_task_costs(items: &[String]) -> Result<Vec<WorldModelTaskCostV1>> {
    let mut out: Vec<WorldModelTaskCostV1> = Vec::new();
    for raw in items {
        let (name, rest) = raw.split_once('=').ok_or_else(|| {
            anyhow!("invalid task cost `{raw}` (expected name=value[:weight[:unit]])")
        })?;
        let mut parts = rest.split(':');
        let value_str = parts.next().unwrap_or("");
        if value_str.trim().is_empty() {
            return Err(anyhow!(
                "invalid task cost `{raw}` (expected name=value[:weight[:unit]])"
            ));
        }
        let value = value_str
            .trim()
            .parse::<f64>()
            .map_err(|_| anyhow!("invalid task cost `{raw}` (value must be numeric)"))?;
        let weight = parts
            .next()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<f64>()
                    .map_err(|_| anyhow!("invalid task cost `{raw}` (weight must be numeric)"))
            })
            .transpose()?
            .unwrap_or(1.0);
        let unit = parts
            .next()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("count")
            .to_string();
        if parts.next().is_some() {
            return Err(anyhow!(
                "invalid task cost `{raw}` (expected name=value[:weight[:unit]])"
            ));
        }
        out.push(WorldModelTaskCostV1 {
            name: name.trim().to_string(),
            value,
            weight,
            unit,
            notes: None,
        });
    }
    Ok(out)
}

pub fn parse_competency_questions(items: &[String]) -> Result<Vec<CompetencyQuestionV1>> {
    let mut out: Vec<CompetencyQuestionV1> = Vec::new();
    for raw in items {
        let (name, query) = raw
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid competency question `{raw}` (expected name=query)"))?;
        let name = name.trim();
        let query = query.trim();
        if name.is_empty() || query.is_empty() {
            return Err(anyhow!(
                "invalid competency question `{raw}` (expected name=query)"
            ));
        }
        out.push(CompetencyQuestionV1 {
            name: name.to_string(),
            question: None,
            query: query.to_string(),
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        });
    }
    Ok(out)
}

pub fn load_competency_questions(path: &Path) -> Result<Vec<CompetencyQuestionV1>> {
    let text = std::fs::read_to_string(path)?;
    let questions: Vec<CompetencyQuestionV1> = serde_json::from_str(&text)?;
    Ok(questions)
}

pub fn compute_guardrail_costs(
    db: &PathDB,
    input_label: &str,
    profile: &str,
    plane: &str,
    weights: &GuardrailCostWeightsV1,
) -> Result<GuardrailCostReportV1> {
    let quality =
        crate::quality::run_quality_checks(db, &PathBuf::from(input_label), profile, plane)?;
    let checked = CheckedDb::check(db)?;

    let quality_summary = GuardrailQualitySummaryV1 {
        error_count: quality.summary.error_count,
        warning_count: quality.summary.warning_count,
        info_count: quality.summary.info_count,
    };

    let checked_summary = GuardrailCheckedSummaryV1 {
        axi_facts_checked: checked.axi_fact_typecheck.checked_facts,
        axi_fact_errors: checked.axi_fact_typecheck.errors.len(),
        rewrite_rules_checked: checked.rewrite_rule_typecheck.checked_rules,
        rewrite_rule_errors: checked.rewrite_rule_typecheck.errors.len(),
        context_checked_facts: checked.context_invariants.checked_facts,
        context_checked_edges: checked.context_invariants.checked_scope_edges,
        context_errors: checked.context_invariants.errors.len(),
        modal_checked_edges: checked.modal_invariants.checked_edges,
        modal_errors: checked.modal_invariants.errors.len(),
    };

    let mut terms: Vec<GuardrailCostTermV1> = Vec::new();
    let mut push_term = |name: &str, value: f64, weight: f64, notes: Option<String>| {
        terms.push(GuardrailCostTermV1 {
            name: name.to_string(),
            value,
            weight,
            cost: value * weight,
            unit: "count".to_string(),
            notes,
        });
    };

    push_term(
        "quality_error",
        quality_summary.error_count as f64,
        weights.quality_error,
        None,
    );
    push_term(
        "quality_warning",
        quality_summary.warning_count as f64,
        weights.quality_warning,
        None,
    );
    push_term(
        "quality_info",
        quality_summary.info_count as f64,
        weights.quality_info,
        None,
    );
    push_term(
        "axi_fact_error",
        checked_summary.axi_fact_errors as f64,
        weights.axi_fact_error,
        None,
    );
    push_term(
        "rewrite_rule_error",
        checked_summary.rewrite_rule_errors as f64,
        weights.rewrite_rule_error,
        None,
    );
    push_term(
        "context_error",
        checked_summary.context_errors as f64,
        weights.context_error,
        None,
    );
    push_term(
        "modal_error",
        checked_summary.modal_errors as f64,
        weights.modal_error,
        None,
    );

    let total_cost = terms.iter().map(|t| t.cost).sum::<f64>();
    let summary = GuardrailCostSummaryV1 {
        total_cost,
        term_count: terms.len(),
    };

    Ok(GuardrailCostReportV1 {
        version: "guardrail_costs_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input_label.to_string(),
        profile: profile.to_string(),
        plane: plane.to_string(),
        summary,
        terms,
        quality: quality_summary,
        checked: checked_summary,
    })
}

// ---------------------------------------------------------------------------
// World model plugin protocol
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelSnapshotRefV1 {
    pub kind: String, // "axpd" | "store"
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<PathdbSnapshotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelInputV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_digest_v1: Option<AxiDigest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_module_text: Option<String>,
    /// Describes the provenance of `axi_module_text` when provided.
    ///
    /// Expected values:
    /// - `canonical_module_export` (preferred)
    /// - `pathdb_export_fallback` (debug-only; includes PathDBExportV1 internals)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_input_kind: Option<String>,
    /// If the snapshot contains multiple canonical modules, records which module
    /// was selected (or requested) for export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_input_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<JepaExportFileV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<WorldModelSnapshotRefV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail: Option<GuardrailCostReportV1>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelObjectiveV1 {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelTaskCostV1 {
    pub name: String,
    pub value: f64,
    pub weight: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyQuestionV1 {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub query: String,
    #[serde(default)]
    pub min_rows: usize,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub contexts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyQuestionResultV1 {
    pub name: String,
    pub rows: usize,
    pub min_rows: usize,
    pub satisfied: bool,
    pub weight: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyCoverageSummaryV1 {
    pub total: usize,
    pub satisfied: usize,
    pub coverage: f64,
    pub cost: f64,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionResultV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelOptionsV1 {
    #[serde(default)]
    pub max_new_proposals: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub objectives: Vec<WorldModelObjectiveV1>,
    #[serde(default)]
    pub task_costs: Vec<WorldModelTaskCostV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizon_steps: Option<usize>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldModelRequestV1 {
    pub protocol: String,
    pub trace_id: WorldModelRunId,
    pub generated_at_unix_secs: u64,
    pub input: WorldModelInputV1,
    #[serde(default)]
    pub options: WorldModelOptionsV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldModelResponseV1 {
    pub protocol: String,
    pub trace_id: WorldModelRunId,
    pub generated_at_unix_secs: u64,
    pub proposals: ProposalsFileV1,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorldModelPlanOptionsV1 {
    pub horizon_steps: usize,
    pub rollouts: usize,
    pub max_new_proposals: usize,
    pub seed: Option<u64>,
    pub goals: Vec<String>,
    pub task_costs: Vec<WorldModelTaskCostV1>,
    pub competency_questions: Vec<CompetencyQuestionV1>,
    pub guardrail_profile: String,
    pub guardrail_plane: String,
    pub guardrail_weights: GuardrailCostWeightsV1,
    pub include_guardrail: bool,
    pub validation_profile: String,
    pub validation_plane: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldModelPlanStepV1 {
    pub step: usize,
    pub trace_id: WorldModelRunId,
    pub proposals: ProposalsFileV1,
    pub guardrail_before: GuardrailPlanSummaryV1,
    pub guardrail_after: GuardrailPlanSummaryV1,
    pub guardrail_delta: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competency_before: Option<CompetencyCoverageSummaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competency_after: Option<CompetencyCoverageSummaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competency_delta: Option<f64>,
    pub competency_cost: f64,
    pub task_cost_total: f64,
    pub total_cost: f64,
    pub validation_ok: bool,
    pub validation_errors: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldModelPlanReportV1 {
    pub version: String,
    pub trace_id: WorldModelRunId,
    pub generated_at_unix_secs: u64,
    pub horizon_steps: usize,
    pub rollouts: usize,
    pub max_new_proposals: usize,
    pub guardrail_profile: String,
    pub guardrail_plane: String,
    pub guardrail_weights: GuardrailCostWeightsV1,
    pub task_costs: Vec<WorldModelTaskCostV1>,
    pub task_cost_total: f64,
    #[serde(default)]
    pub competency_questions: Vec<CompetencyQuestionV1>,
    pub steps: Vec<WorldModelPlanStepV1>,
}

#[derive(Debug, Clone)]
pub enum WorldModelBackend {
    Disabled,
    Stub,
    Command { program: PathBuf, args: Vec<String> },
    Http { url: String },
}

impl Default for WorldModelBackend {
    fn default() -> Self {
        WorldModelBackend::Disabled
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorldModelState {
    pub backend: WorldModelBackend,
    pub model: Option<String>,
}

impl WorldModelState {
    pub fn status_line(&self) -> String {
        let backend = match &self.backend {
            WorldModelBackend::Disabled => "disabled".to_string(),
            WorldModelBackend::Stub => "stub".to_string(),
            WorldModelBackend::Command { program, args } => {
                if args.iter().any(|s| s == "world-model-plugin-llm") {
                    "llm".to_string()
                } else {
                    format!("command({})", program.display())
                }
            }
            WorldModelBackend::Http { url } => format!("http({url})"),
        };
        let model = self.model.as_ref().map(|s| s.as_str()).unwrap_or("default");
        format!("world_model: backend={backend} model={model}")
    }

    pub fn propose(&self, req: &WorldModelRequestV1) -> Result<WorldModelResponseV1> {
        match &self.backend {
            WorldModelBackend::Disabled => Err(anyhow!(
                "world model backend is disabled (configure --world-model-plugin or use stub)"
            )),
            WorldModelBackend::Stub => Ok(WorldModelResponseV1 {
                protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
                trace_id: req.trace_id.clone(),
                generated_at_unix_secs: now_unix_secs(),
                proposals: empty_proposals(&req.trace_id),
                notes: vec!["stub backend (no proposals)".to_string()],
                error: None,
            }),
            WorldModelBackend::Command { program, args } => {
                let response = run_world_model_plugin(program, args, req)?;
                Ok(response)
            }
            WorldModelBackend::Http { url } => run_world_model_http(url, req),
        }
    }

    pub fn backend_label(&self) -> String {
        match &self.backend {
            WorldModelBackend::Disabled => "disabled".to_string(),
            WorldModelBackend::Stub => "stub".to_string(),
            WorldModelBackend::Command { program, args } => {
                if args.iter().any(|s| s == "world-model-plugin-llm") {
                    "llm".to_string()
                } else {
                    format!("command:{}", program.display())
                }
            }
            WorldModelBackend::Http { url } => format!("http:{url}"),
        }
    }
}

#[cfg(feature = "world-model-http")]
fn run_world_model_http(url: &str, req: &WorldModelRequestV1) -> Result<WorldModelResponseV1> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| anyhow!("failed to build http client: {e}"))?;
    let resp = client
        .post(url)
        .json(req)
        .send()
        .map_err(|e| anyhow!("world model http backend failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!(
            "world model http backend returned {status}: {text}"
        ));
    }
    let parsed = resp
        .json()
        .map_err(|e| anyhow!("world model http backend returned invalid JSON: {e}"))?;
    Ok(parsed)
}

#[cfg(not(feature = "world-model-http"))]
fn run_world_model_http(_url: &str, _req: &WorldModelRequestV1) -> Result<WorldModelResponseV1> {
    Err(anyhow!(
        "world model http backend is unavailable (enable feature `world-model-http`)"
    ))
}

fn empty_proposals(trace_id: impl AsRef<str>) -> ProposalsFileV1 {
    let trace_id = trace_id.as_ref();
    ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at: now_unix_secs().to_string(),
        source: ProposalSourceV1 {
            source_type: "world_model".to_string(),
            locator: trace_id.to_string(),
        },
        schema_hint: None,
        proposals: Vec::new(),
    }
}

fn run_world_model_plugin(
    program: &Path,
    args: &[String],
    req: &WorldModelRequestV1,
) -> Result<WorldModelResponseV1> {
    let payload = serde_json::to_vec(req)?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            anyhow!(
                "failed to start world model plugin `{}`: {e}",
                program.display()
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(&payload)
            .map_err(|e| anyhow!("failed to write stdin for world model plugin: {e}"))?;
    } else {
        return Err(anyhow!("failed to open stdin for world model plugin"));
    }

    let output = child
        .wait_with_output()
        .map_err(|e| anyhow!("world model plugin `{}` failed: {e}", program.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "world model plugin `{}` failed (exit={:?}): {}",
            program.display(),
            output.status.code(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| {
        anyhow!(
            "world model plugin `{}` returned non-utf8 stdout: {e}",
            program.display()
        )
    })?;
    let response: WorldModelResponseV1 = serde_json::from_str(&stdout).map_err(|e| {
        let preview: String = stdout.chars().take(400).collect();
        anyhow!(
            "world model plugin `{}` returned invalid JSON: {e}; stdout starts with: {preview:?}",
            program.display()
        )
    })?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Provenance helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WorldModelProvenance {
    pub trace_id: WorldModelRunId,
    pub run_id: WorldModelRunId,
    pub backend: String,
    pub model: Option<String>,
    pub axi_digest_v1: Option<AxiDigest>,
    pub pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: Option<ProposalDigest>,
    pub guardrail_total_cost: Option<f64>,
    pub guardrail_profile: Option<String>,
    pub guardrail_plane: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub struct WorldModelProposalLineage {
    pub trace_id: Option<WorldModelRunId>,
    pub run_id: Option<WorldModelRunId>,
    pub backend: Option<String>,
    pub model: Option<String>,
    pub axi_digest_v1: Option<AxiDigest>,
    pub pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: Option<ProposalDigest>,
    pub guardrail_total_cost: Option<f64>,
    pub guardrail_profile: Option<String>,
    pub guardrail_plane: Option<String>,
}

pub fn build_world_model_provenance(
    response: &WorldModelResponseV1,
    backend: String,
    model: Option<String>,
    axi_digest_v1: Option<AxiDigest>,
    pathdb_snapshot_id: Option<PathdbSnapshotId>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    guardrail_total_cost: Option<f64>,
    guardrail_profile: Option<String>,
    guardrail_plane: Option<String>,
) -> Result<WorldModelProvenance> {
    Ok(WorldModelProvenance {
        trace_id: response.trace_id.clone(),
        run_id: response.trace_id.clone(),
        backend,
        model,
        axi_digest_v1,
        pathdb_snapshot_id,
        accepted_snapshot_id,
        proposals_digest: Some(proposals_digest(&response.proposals)?),
        guardrail_total_cost,
        guardrail_profile,
        guardrail_plane,
    })
}

pub fn build_world_model_run_record(
    provenance: &WorldModelProvenance,
    proposals: &ProposalsFileV1,
    committed_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    committed_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    notes: Vec<String>,
) -> Result<crate::accepted_plane::WorldModelRunRecordV1> {
    let proposals_digest = provenance
        .proposals_digest
        .clone()
        .unwrap_or(proposals_digest(proposals)?);
    let status = if committed_pathdb_snapshot_id.is_some() {
        crate::accepted_plane::WorldModelRunStatusV1::CommittedToPathdb
    } else {
        crate::accepted_plane::WorldModelRunStatusV1::Previewed
    };

    Ok(crate::accepted_plane::WorldModelRunRecordV1 {
        version: "world_model_run_record_v1".to_string(),
        run_id: provenance.run_id.clone(),
        trace_id: provenance.trace_id.clone(),
        created_at_unix_secs: now_unix_secs(),
        status,
        backend: provenance.backend.clone(),
        model: provenance.model.clone(),
        axi_digest_v1: provenance.axi_digest_v1.clone(),
        input_pathdb_snapshot_id: provenance.pathdb_snapshot_id.clone(),
        input_accepted_snapshot_id: provenance.accepted_snapshot_id.clone(),
        proposals_digest,
        proposal_count: proposals.proposals.len(),
        committed_pathdb_snapshot_id,
        committed_accepted_snapshot_id,
        guardrail_total_cost: provenance.guardrail_total_cost,
        guardrail_profile: provenance.guardrail_profile.clone(),
        guardrail_plane: provenance.guardrail_plane.clone(),
        notes,
    })
}

pub fn apply_world_model_provenance(
    mut proposals: ProposalsFileV1,
    provenance: &WorldModelProvenance,
) -> ProposalsFileV1 {
    if proposals.generated_at.trim().is_empty() {
        proposals.generated_at = now_unix_secs().to_string();
    }
    proposals.source = ProposalSourceV1 {
        source_type: "world_model".to_string(),
        locator: provenance.trace_id.to_string(),
    };

    for p in &mut proposals.proposals {
        let meta = match p {
            ProposalV1::Entity { meta, .. } => meta,
            ProposalV1::Relation { meta, .. } => meta,
        };
        apply_provenance_meta(meta, provenance);
    }

    proposals
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn extract_world_model_proposal_lineage(meta: &ProposalMetaV1) -> WorldModelProposalLineage {
    fn optional_id<T>(meta: &ProposalMetaV1, key: &str) -> Option<T>
    where
        T: From<String>,
    {
        meta.metadata.get(key).cloned().map(T::from)
    }

    WorldModelProposalLineage {
        trace_id: optional_id(meta, "axiograph_world_model_trace_id"),
        run_id: optional_id(meta, "axiograph_world_model_run_id"),
        backend: meta.metadata.get("axiograph_world_model_backend").cloned(),
        model: meta.metadata.get("axiograph_world_model_model").cloned(),
        axi_digest_v1: optional_id(meta, "axiograph_axi_digest_v1"),
        pathdb_snapshot_id: optional_id(meta, "axiograph_pathdb_snapshot_id"),
        accepted_snapshot_id: optional_id(meta, "axiograph_accepted_snapshot_id"),
        proposals_digest: optional_id(meta, "axiograph_proposals_digest"),
        guardrail_total_cost: meta
            .metadata
            .get("axiograph_guardrail_total_cost")
            .and_then(|s| s.parse::<f64>().ok()),
        guardrail_profile: meta.metadata.get("axiograph_guardrail_profile").cloned(),
        guardrail_plane: meta.metadata.get("axiograph_guardrail_plane").cloned(),
    }
}

fn apply_provenance_meta(meta: &mut ProposalMetaV1, provenance: &WorldModelProvenance) {
    fn set_reserved(meta: &mut ProposalMetaV1, key: &str, value: impl Into<String>) {
        meta.metadata.insert(key.to_string(), value.into());
    }

    fn set_optional_reserved(meta: &mut ProposalMetaV1, key: &str, value: Option<String>) {
        match value {
            Some(value) => {
                meta.metadata.insert(key.to_string(), value);
            }
            None => {
                meta.metadata.remove(key);
            }
        }
    }

    meta.confidence = meta.confidence.clamp(0.0, 1.0);
    set_reserved(
        meta,
        "axiograph_world_model_trace_id",
        provenance.trace_id.to_string(),
    );
    set_reserved(
        meta,
        "axiograph_world_model_run_id",
        provenance.run_id.to_string(),
    );
    set_reserved(
        meta,
        "axiograph_world_model_backend",
        provenance.backend.clone(),
    );
    set_optional_reserved(
        meta,
        "axiograph_world_model_model",
        provenance.model.clone(),
    );
    set_optional_reserved(
        meta,
        "axiograph_axi_digest_v1",
        provenance.axi_digest_v1.as_ref().map(ToString::to_string),
    );
    set_optional_reserved(
        meta,
        "axiograph_pathdb_snapshot_id",
        provenance
            .pathdb_snapshot_id
            .as_ref()
            .map(ToString::to_string),
    );
    set_optional_reserved(
        meta,
        "axiograph_accepted_snapshot_id",
        provenance
            .accepted_snapshot_id
            .as_ref()
            .map(ToString::to_string),
    );
    set_optional_reserved(
        meta,
        "axiograph_proposals_digest",
        provenance
            .proposals_digest
            .as_ref()
            .map(ToString::to_string),
    );
    set_optional_reserved(
        meta,
        "axiograph_guardrail_total_cost",
        provenance
            .guardrail_total_cost
            .map(|cost| format!("{cost:.4}")),
    );
    set_optional_reserved(
        meta,
        "axiograph_guardrail_profile",
        provenance.guardrail_profile.clone(),
    );
    set_optional_reserved(
        meta,
        "axiograph_guardrail_plane",
        provenance.guardrail_plane.clone(),
    );
}

pub(crate) fn world_model_llm_prompt(req: &WorldModelRequestV1) -> (String, Value) {
    let trace_id = if req.trace_id.as_str().trim().is_empty() {
        default_trace_id()
    } else {
        req.trace_id.clone()
    };
    let opts = &req.options;
    let input = &req.input;

    let export_summary = input.export.as_ref().map(|export| {
        let sample = export
            .items
            .iter()
            .take(3)
            .map(|it| {
                json!({
                    "schema": it.schema,
                    "instance": it.instance,
                    "relation": it.relation,
                    "fields": it.fields.iter().take(4).cloned().collect::<Vec<_>>(),
                    "mask_fields": it.mask_fields.iter().take(4).cloned().collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        json!({
            "module_name": export.module_name,
            "axi_digest_v1": export.axi_digest_v1,
            "items": export.items.len(),
            "sample": sample,
        })
    });

    let summary = json!({
        "trace_id": trace_id,
        "generated_at": req.generated_at_unix_secs,
        "goals": opts.goals,
        "objectives": opts.objectives,
        "task_costs": opts.task_costs,
        "max_new_proposals": opts.max_new_proposals,
        "notes": opts.notes,
        "axi_digest_v1": input.axi_digest_v1,
        "axi_input_kind": input.axi_input_kind,
        "axi_input_module": input.axi_input_module,
        "export_summary": export_summary,
        "export_path": input.export_path,
    });

    let prompt = [
        "You are a world-model assistant for Axiograph.",
        "Return ONLY JSON (no markdown) that conforms to:",
        "ProposalsFileV1 = {",
        "  \"version\": 1,",
        "  \"generated_at\": \"<unix-secs as string>\",",
        "  \"source\": {\"source_type\": \"world_model\", \"locator\": \"<trace_id>\"},",
        "  \"schema_hint\": null,",
        "  \"proposals\": [ ProposalV1 (entity or relation) ]",
        "}",
        "ProposalV1 entity:",
        "{ \"kind\":\"Entity\", \"proposal_id\":\"...\", \"confidence\":0.0-1.0, \"evidence\":[], \"public_rationale\":\"...\", \"metadata\":{}, \"schema_hint\":null,",
        "  \"entity_id\":\"...\", \"entity_type\":\"...\", \"name\":\"...\", \"attributes\":{}, \"description\":null }",
        "ProposalV1 relation:",
        "{ \"kind\":\"Relation\", \"proposal_id\":\"...\", \"confidence\":0.0-1.0, \"evidence\":[], \"public_rationale\":\"...\", \"metadata\":{}, \"schema_hint\":null,",
        "  \"relation_id\":\"...\", \"rel_type\":\"...\", \"source\":\"...\", \"target\":\"...\", \"attributes\":{} }",
        "Rules:",
        "- Propose at most max_new_proposals items.",
        "- Use stable ids (e.g. wm::<trace_id>::n).",
        "- Keep confidence between 0.55 and 0.9.",
        "- Use only info grounded in export_summary + goals.",
        "- Do NOT infer relationships from mere co-occurrence of string tokens/ids (especially intern tables or other low-level snapshot artifacts). If you cannot cite a specific typed item/field in export_summary that supports the proposal, do not propose it.",
        "- If axi_input_kind is `pathdb_export_fallback`, treat the input as debug-only (it may contain implementation details). Prefer returning fewer proposals with stronger grounding.",
    ]
    .join("\n");

    (prompt, summary)
}

pub(crate) fn normalize_world_model_proposals_value(
    trace_id: impl AsRef<str>,
    value: Value,
) -> ProposalsFileV1 {
    let trace_id = trace_id.as_ref();
    let now = now_unix_secs().to_string();
    let obj = value.as_object().cloned().unwrap_or_default();
    let generated_at = obj
        .get("generated_at")
        .and_then(|v| v.as_str())
        .unwrap_or(&now)
        .to_string();
    let schema_hint = obj
        .get("schema_hint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let proposals_v = obj.get("proposals").and_then(|v| v.as_array()).cloned();
    let proposals_v = proposals_v.unwrap_or_default();

    let mut proposals: Vec<ProposalV1> = Vec::new();
    for (idx, item) in proposals_v.iter().enumerate() {
        let Some(p) = item.as_object() else { continue };
        let kind_raw = p
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("Relation")
            .to_ascii_lowercase();
        let kind = if kind_raw == "entity" {
            "Entity"
        } else {
            "Relation"
        };
        let base_id = format!("wm::{trace_id}::{idx}");
        let proposal_id = p
            .get("proposal_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&base_id)
            .to_string();

        let confidence = p
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7)
            .clamp(0.0, 1.0);
        let public_rationale = p
            .get("public_rationale")
            .and_then(|v| v.as_str())
            .unwrap_or("world model proposal")
            .to_string();

        let metadata = p
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), value_to_string(v)))
                    .collect::<HashMap<String, String>>()
            })
            .unwrap_or_default();

        let schema_hint = p
            .get("schema_hint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let evidence = p
            .get("evidence")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<EvidencePointer>>(v).ok())
            .unwrap_or_default();

        let meta = ProposalMetaV1 {
            proposal_id: proposal_id.clone(),
            confidence,
            evidence,
            public_rationale,
            metadata,
            schema_hint,
        };

        if kind == "Entity" {
            let entity_id = p
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("{base_id}:entity"))
                .to_string();
            let entity_type = p
                .get("entity_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Entity")
                .to_string();
            let name = p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&entity_id)
                .to_string();
            let attributes = p
                .get("attributes")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), value_to_string(v)))
                        .collect::<HashMap<String, String>>()
                })
                .unwrap_or_default();
            let description = p
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            proposals.push(ProposalV1::Entity {
                meta,
                entity_id,
                entity_type,
                name,
                attributes,
                description,
            });
        } else {
            let relation_id = p
                .get("relation_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("{base_id}:rel"))
                .to_string();
            let rel_type = p
                .get("rel_type")
                .and_then(|v| v.as_str())
                .unwrap_or("related_to")
                .to_string();
            let source = p
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let target = p
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let attributes = p
                .get("attributes")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), value_to_string(v)))
                        .collect::<HashMap<String, String>>()
                })
                .unwrap_or_default();
            proposals.push(ProposalV1::Relation {
                meta,
                relation_id,
                rel_type,
                source,
                target,
                attributes,
            });
        }
    }

    ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: ProposalSourceV1 {
            source_type: "world_model".to_string(),
            locator: trace_id.to_string(),
        },
        schema_hint,
        proposals,
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn make_world_model_request(
    input: WorldModelInputV1,
    options: WorldModelOptionsV1,
) -> WorldModelRequestV1 {
    WorldModelRequestV1 {
        protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
        trace_id: default_trace_id(),
        generated_at_unix_secs: now_unix_secs(),
        input,
        options,
    }
}

pub fn run_world_model_plan(
    db: &PathDB,
    world_model: &WorldModelState,
    base_input: &WorldModelInputV1,
    options: &WorldModelPlanOptionsV1,
) -> Result<WorldModelPlanReportV1> {
    if options.horizon_steps == 0 {
        return Err(anyhow!("world model plan: horizon_steps must be > 0"));
    }
    if options.rollouts == 0 {
        return Err(anyhow!("world model plan: rollouts must be > 0"));
    }

    let mut planning_db = clone_db(db)?;
    let mut steps: Vec<WorldModelPlanStepV1> = Vec::new();
    let task_cost_total: f64 = options.task_costs.iter().map(|t| t.value * t.weight).sum();
    let plan_trace = default_trace_id();

    for step in 0..options.horizon_steps {
        let guardrail_before = if options.include_guardrail && options.guardrail_profile != "off" {
            compute_guardrail_costs(
                &planning_db,
                &format!("{plan_trace}:step{step}"),
                &options.guardrail_profile,
                &options.guardrail_plane,
                &options.guardrail_weights,
            )?
        } else {
            empty_guardrail_report(
                &format!("{plan_trace}:step{step}"),
                &options.guardrail_profile,
                &options.guardrail_plane,
            )
        };
        let competency_before = if options.competency_questions.is_empty() {
            None
        } else {
            Some(crate::competency_questions::evaluate_competency_questions(
                &planning_db,
                &options.competency_questions,
            )?)
        };

        let mut best: Option<(
            WorldModelRunId,
            ProposalsFileV1,
            GuardrailCostReportV1,
            Option<CompetencyCoverageSummaryV1>,
            bool,
            usize,
            f64,
            Vec<String>,
        )> = None;

        for rollout in 0..options.rollouts {
            let mut input = base_input.clone();
            if options.include_guardrail && options.guardrail_profile != "off" {
                input.guardrail = Some(guardrail_before.clone());
            }
            input.notes.push(format!(
                "source=world_model_plan step={step} rollout={rollout}"
            ));

            let mut wm_opts = WorldModelOptionsV1::default();
            wm_opts.max_new_proposals = options.max_new_proposals;
            wm_opts.seed = options
                .seed
                .map(|s| s.wrapping_add((step as u64) * 1_000 + rollout as u64));
            wm_opts.goals = options.goals.clone();
            wm_opts.task_costs = options.task_costs.clone();
            wm_opts.horizon_steps = Some(options.horizon_steps);

            let req = make_world_model_request(input, wm_opts);
            let mut response = world_model.propose(&req)?;
            if let Some(err) = response.error.take() {
                return Err(anyhow!("world model error: {err}"));
            }

            let guardrail_profile_label = if options.guardrail_profile == "off" {
                None
            } else {
                Some(options.guardrail_profile.clone())
            };
            let guardrail_plane_label = if options.guardrail_profile == "off" {
                None
            } else {
                Some(options.guardrail_plane.clone())
            };

            let provenance = build_world_model_provenance(
                &response,
                world_model.backend_label(),
                world_model.model.clone(),
                base_input.axi_digest_v1.clone(),
                base_input
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.snapshot_id.clone()),
                base_input
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.accepted_snapshot_id.clone()),
                Some(guardrail_before.summary.total_cost),
                guardrail_profile_label,
                guardrail_plane_label,
            )?;

            let mut proposals = apply_world_model_provenance(response.proposals, &provenance);
            if options.max_new_proposals > 0
                && proposals.proposals.len() > options.max_new_proposals
            {
                proposals.proposals.truncate(options.max_new_proposals);
            }

            let mut candidate = clone_db(&planning_db)?;
            apply_proposals_to_db(&mut candidate, &proposals)?;

            let guardrail_after = if options.include_guardrail && options.guardrail_profile != "off"
            {
                compute_guardrail_costs(
                    &candidate,
                    &format!("{plan_trace}:step{step}:rollout{rollout}"),
                    &options.guardrail_profile,
                    &options.guardrail_plane,
                    &options.guardrail_weights,
                )?
            } else {
                empty_guardrail_report(
                    &format!("{plan_trace}:step{step}:rollout{rollout}"),
                    &options.guardrail_profile,
                    &options.guardrail_plane,
                )
            };
            let competency_after = if options.competency_questions.is_empty() {
                None
            } else {
                Some(crate::competency_questions::evaluate_competency_questions(
                    &candidate,
                    &options.competency_questions,
                )?)
            };
            let competency_cost = competency_after.as_ref().map(|c| c.cost).unwrap_or(0.0);

            let (validation_ok, validation_errors) = if options.validation_profile == "off" {
                (true, 0)
            } else {
                let validation = crate::proposals_validate::validate_proposals_v1(
                    &planning_db,
                    &proposals,
                    &options.validation_profile,
                    &options.validation_plane,
                )?;
                (validation.ok, validation.quality_delta.summary.error_count)
            };

            let total_cost = guardrail_after.summary.total_cost + task_cost_total + competency_cost;
            let candidate_tuple = (
                response.trace_id.clone(),
                proposals,
                guardrail_after,
                competency_after,
                validation_ok,
                validation_errors,
                total_cost,
                response.notes.clone(),
            );

            let better = match best.as_ref() {
                None => true,
                Some((_, _, _, _, _, _, best_cost, _)) => total_cost < *best_cost,
            };
            if better {
                best = Some(candidate_tuple);
            }
        }

        let (
            trace_id,
            proposals,
            guardrail_after,
            competency_after,
            validation_ok,
            validation_errors,
            total_cost,
            notes,
        ) = best.ok_or_else(|| anyhow!("world model plan: no rollout produced proposals"))?;

        apply_proposals_to_db(&mut planning_db, &proposals)?;

        let competency_cost = competency_after.as_ref().map(|c| c.cost).unwrap_or(0.0);
        let competency_delta = match (competency_before.as_ref(), competency_after.as_ref()) {
            (Some(before), Some(after)) => Some(after.coverage - before.coverage),
            _ => None,
        };
        let step_report = WorldModelPlanStepV1 {
            step,
            trace_id,
            proposals,
            guardrail_before: guardrail_plan_summary(&guardrail_before),
            guardrail_after: guardrail_plan_summary(&guardrail_after),
            guardrail_delta: guardrail_after.summary.total_cost
                - guardrail_before.summary.total_cost,
            competency_before,
            competency_after,
            competency_delta,
            competency_cost,
            task_cost_total,
            total_cost,
            validation_ok,
            validation_errors,
            notes,
        };
        steps.push(step_report);
    }

    Ok(WorldModelPlanReportV1 {
        version: "world_model_plan_v1".to_string(),
        trace_id: plan_trace,
        generated_at_unix_secs: now_unix_secs(),
        horizon_steps: options.horizon_steps,
        rollouts: options.rollouts,
        max_new_proposals: options.max_new_proposals,
        guardrail_profile: options.guardrail_profile.clone(),
        guardrail_plane: options.guardrail_plane.clone(),
        guardrail_weights: options.guardrail_weights.clone(),
        task_costs: options.task_costs.clone(),
        task_cost_total,
        competency_questions: options.competency_questions.clone(),
        steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("axiograph_{label}_{pid}_{nanos}.json"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create temp dir");
        }
        path
    }

    #[test]
    fn jepa_export_masks_fields() {
        let axi = r#"
module M
schema S:
  object A
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;
        let opts = JepaExportOptions {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: Vec::new(),
        };
        let export = build_jepa_export_from_axi_text(axi, &opts).expect("export");
        assert!(!export.items.is_empty());
        for item in &export.items {
            assert_eq!(item.mask_fields.len(), 1);
        }
        let proof = export
            .axi_well_typed_proof_v1
            .as_ref()
            .expect("JEPA export should carry a well-typed proof");
        assert_eq!(proof.module_name, "M");
        assert_eq!(proof.schema_count, 1);
        assert_eq!(proof.instance_count, 1);
    }

    #[test]
    fn jepa_export_can_exclude_relations() {
        let axi = r#"
module M
schema S:
  object A
  relation interned_string(id: A, text: A)
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  interned_string = {(id=x, text=y)}
  R = {(from=x, to=y)}
"#;
        let opts = JepaExportOptions {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: vec!["interned_string".to_string()],
        };
        let export = build_jepa_export_from_axi_text(axi, &opts).expect("export");
        assert!(!export.items.is_empty());
        for item in &export.items {
            assert_ne!(item.relation, "interned_string");
        }
    }

    #[test]
    fn read_jepa_export_backfills_missing_well_typed_proof() {
        let axi = r#"
module Legacy
schema S:
  object A
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;
        let opts = JepaExportOptions {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: Vec::new(),
        };
        let export = build_jepa_export_from_axi_text(axi, &opts).expect("export");
        let mut json = serde_json::to_value(&export).expect("serialize export");
        let removed = json
            .as_object_mut()
            .expect("export object")
            .remove("axi_well_typed_proof_v1");
        assert!(removed.is_some(), "expected serialized proof field");

        let path = unique_temp_file("legacy_jepa_export");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json).expect("serialize legacy export"),
        )
        .expect("write legacy export");

        let restored = read_jepa_export(&path).expect("read legacy export");
        let _ = fs::remove_file(&path);

        let proof = restored
            .axi_well_typed_proof_v1
            .as_ref()
            .expect("legacy export should be backfilled with a proof");
        assert_eq!(proof.module_name, "Legacy");
        assert_eq!(proof.schema_count, 1);
        assert_eq!(proof.instance_count, 1);
    }

    #[test]
    fn jepa_export_rejects_pathdb_export_snapshot_inputs() {
        let canonical = r#"
module Demo
schema S:
  object A
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, canonical)
            .expect("import canonical module");
        db.build_indexes();
        let snapshot_export = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)
            .expect("export pathdb snapshot");

        let opts = JepaExportOptions {
            instance_filter: None,
            max_items: 10,
            mask_fields: 1,
            seed: 1,
            exclude_relations: Vec::new(),
        };
        let err = build_jepa_export_from_axi_text(&snapshot_export, &opts)
            .expect_err("JEPA export should reject PathDBExportV1 snapshots");
        assert!(err
            .to_string()
            .contains("expected a canonical .axi module, but input is a PathDBExportV1 snapshot"));
    }

    #[test]
    fn guardrail_costs_sum_terms() {
        let db = PathDB::new();
        let report = compute_guardrail_costs(
            &db,
            "test",
            "fast",
            "both",
            &GuardrailCostWeightsV1::defaults(),
        )
        .expect("guardrail");
        let sum: f64 = report.terms.iter().map(|t| t.cost).sum();
        assert!((sum - report.summary.total_cost).abs() < 1e-9);
        assert_eq!(report.summary.term_count, report.terms.len());
    }

    #[test]
    fn provenance_applies_metadata_and_clamps_confidence() {
        let mut proposals = ProposalsFileV1 {
            version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
            generated_at: "".to_string(),
            source: ProposalSourceV1 {
                source_type: "test".to_string(),
                locator: "unit".to_string(),
            },
            schema_hint: None,
            proposals: vec![ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "e1".to_string(),
                    confidence: 1.5,
                    evidence: Vec::new(),
                    public_rationale: "r".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: None,
                },
                entity_id: "e1".to_string(),
                entity_type: "Thing".to_string(),
                name: "thing".to_string(),
                attributes: HashMap::new(),
                description: None,
            }],
        };

        let prov = WorldModelProvenance {
            trace_id: WorldModelRunId::new("wm::trace"),
            run_id: WorldModelRunId::new("wm::run"),
            backend: "stub".to_string(),
            model: Some("model".to_string()),
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:digest")),
            pathdb_snapshot_id: Some(PathdbSnapshotId::new("pathdb:snap")),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:snap")),
            proposals_digest: Some(ProposalDigest::new("fnv1a64:proposals")),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
        };

        proposals = apply_world_model_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        assert!(meta.confidence <= 1.0);
        assert!(meta.metadata.contains_key("axiograph_world_model_trace_id"));
        assert_eq!(
            meta.metadata
                .get("axiograph_world_model_run_id")
                .map(String::as_str),
            Some("wm::run")
        );
        assert!(meta.metadata.contains_key("axiograph_world_model_backend"));
        assert!(meta.metadata.contains_key("axiograph_world_model_model"));
        assert!(meta.metadata.contains_key("axiograph_axi_digest_v1"));
        assert_eq!(
            meta.metadata
                .get("axiograph_pathdb_snapshot_id")
                .map(String::as_str),
            Some("pathdb:snap")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_accepted_snapshot_id")
                .map(String::as_str),
            Some("accepted:snap")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_proposals_digest")
                .map(String::as_str),
            Some("fnv1a64:proposals")
        );
        assert!(meta.metadata.contains_key("axiograph_guardrail_total_cost"));
    }

    #[test]
    fn provenance_overwrites_reserved_lineage_keys_and_clears_absent_optionals() {
        let mut proposals = ProposalsFileV1 {
            version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
            generated_at: "".to_string(),
            source: ProposalSourceV1 {
                source_type: "test".to_string(),
                locator: "unit".to_string(),
            },
            schema_hint: None,
            proposals: vec![ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "e1".to_string(),
                    confidence: 0.5,
                    evidence: Vec::new(),
                    public_rationale: "r".to_string(),
                    metadata: HashMap::from([
                        (
                            "axiograph_world_model_trace_id".to_string(),
                            "spoofed-trace".to_string(),
                        ),
                        (
                            "axiograph_world_model_run_id".to_string(),
                            "spoofed-run".to_string(),
                        ),
                        (
                            "axiograph_axi_digest_v1".to_string(),
                            "fnv1a64:spoofed".to_string(),
                        ),
                        (
                            "axiograph_pathdb_snapshot_id".to_string(),
                            "pathdb:spoofed".to_string(),
                        ),
                        (
                            "axiograph_accepted_snapshot_id".to_string(),
                            "accepted:spoofed".to_string(),
                        ),
                        (
                            "axiograph_world_model_model".to_string(),
                            "spoofed-model".to_string(),
                        ),
                    ]),
                    schema_hint: None,
                },
                entity_id: "e1".to_string(),
                entity_type: "Thing".to_string(),
                name: "thing".to_string(),
                attributes: HashMap::new(),
                description: None,
            }],
        };

        let prov = WorldModelProvenance {
            trace_id: WorldModelRunId::new("wm::authoritative"),
            run_id: WorldModelRunId::new("wm::authoritative-run"),
            backend: "stub".to_string(),
            model: None,
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:real")),
            pathdb_snapshot_id: Some(PathdbSnapshotId::new("pathdb:7")),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:8")),
            proposals_digest: None,
            guardrail_total_cost: None,
            guardrail_profile: None,
            guardrail_plane: None,
        };

        proposals = apply_world_model_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        assert_eq!(
            meta.metadata
                .get("axiograph_world_model_trace_id")
                .map(String::as_str),
            Some("wm::authoritative")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_world_model_run_id")
                .map(String::as_str),
            Some("wm::authoritative-run")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_axi_digest_v1")
                .map(String::as_str),
            Some("fnv1a64:real")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_pathdb_snapshot_id")
                .map(String::as_str),
            Some("pathdb:7")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_accepted_snapshot_id")
                .map(String::as_str),
            Some("accepted:8")
        );
        assert!(
            !meta.metadata.contains_key("axiograph_world_model_model"),
            "runtime-owned optional keys should be cleared when provenance omits them"
        );
    }

    #[test]
    fn extracted_world_model_lineage_rehydrates_typed_ids_from_metadata() {
        let mut proposals = ProposalsFileV1 {
            version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
            generated_at: "".to_string(),
            source: ProposalSourceV1 {
                source_type: "test".to_string(),
                locator: "unit".to_string(),
            },
            schema_hint: None,
            proposals: vec![ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "e1".to_string(),
                    confidence: 0.9,
                    evidence: Vec::new(),
                    public_rationale: "r".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: None,
                },
                entity_id: "e1".to_string(),
                entity_type: "Thing".to_string(),
                name: "thing".to_string(),
                attributes: HashMap::new(),
                description: None,
            }],
        };

        let prov = WorldModelProvenance {
            trace_id: WorldModelRunId::new("wm::typed"),
            run_id: WorldModelRunId::new("wm::typed-run"),
            backend: "plugin".to_string(),
            model: Some("deterministic".to_string()),
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:0123456789abcdef")),
            pathdb_snapshot_id: Some(PathdbSnapshotId::new("pathdb:55")),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:21")),
            proposals_digest: Some(ProposalDigest::new("fnv1a64:proposals-typed")),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
        };

        proposals = apply_world_model_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        let lineage = extract_world_model_proposal_lineage(meta);

        assert_eq!(
            lineage.trace_id.as_ref().map(|id| id.as_str()),
            Some("wm::typed")
        );
        assert_eq!(
            lineage.run_id.as_ref().map(|id| id.as_str()),
            Some("wm::typed-run")
        );
        assert_eq!(lineage.backend.as_deref(), Some("plugin"));
        assert_eq!(lineage.model.as_deref(), Some("deterministic"));
        assert_eq!(
            lineage.axi_digest_v1.as_ref().map(|id| id.as_str()),
            Some("fnv1a64:0123456789abcdef")
        );
        assert_eq!(
            lineage.pathdb_snapshot_id.as_ref().map(|id| id.as_str()),
            Some("pathdb:55")
        );
        assert_eq!(
            lineage.accepted_snapshot_id.as_ref().map(|id| id.as_str()),
            Some("accepted:21")
        );
        assert_eq!(
            lineage.proposals_digest.as_ref().map(|id| id.as_str()),
            Some("fnv1a64:proposals-typed")
        );
        assert_eq!(lineage.guardrail_total_cost, Some(1.25));
        assert_eq!(lineage.guardrail_profile.as_deref(), Some("fast"));
        assert_eq!(lineage.guardrail_plane.as_deref(), Some("both"));
    }

    #[test]
    fn world_model_request_round_trips_typed_ids_as_string_json() {
        let req = WorldModelRequestV1 {
            protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
            trace_id: WorldModelRunId::new("wm::123"),
            generated_at_unix_secs: 123,
            input: WorldModelInputV1 {
                axi_digest_v1: Some(AxiDigest::new("fnv1a64:abc")),
                axi_module_text: Some("module Demo\n".to_string()),
                axi_input_kind: Some("canonical_module_export".to_string()),
                axi_input_module: Some("Demo".to_string()),
                export: None,
                export_path: None,
                snapshot: Some(WorldModelSnapshotRefV1 {
                    kind: "store".to_string(),
                    path: "/tmp/store".to_string(),
                    snapshot_id: Some(PathdbSnapshotId::new("pathdb:42")),
                    accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:9")),
                }),
                guardrail: None,
                notes: vec!["typed".to_string()],
            },
            options: WorldModelOptionsV1::default(),
        };

        let json = serde_json::to_value(&req).expect("serialize request");
        assert_eq!(json["trace_id"], "wm::123");
        assert_eq!(json["input"]["axi_digest_v1"], "fnv1a64:abc");
        assert_eq!(json["input"]["snapshot"]["snapshot_id"], "pathdb:42");
        assert_eq!(
            json["input"]["snapshot"]["accepted_snapshot_id"],
            "accepted:9"
        );

        let round_trip: WorldModelRequestV1 =
            serde_json::from_value(json).expect("deserialize request");
        assert_eq!(round_trip.trace_id.as_str(), "wm::123");
        assert_eq!(
            round_trip
                .input
                .axi_digest_v1
                .as_ref()
                .map(AxiDigest::as_str),
            Some("fnv1a64:abc")
        );
        let snapshot = round_trip.input.snapshot.expect("snapshot");
        assert_eq!(
            snapshot.snapshot_id.as_ref().map(PathdbSnapshotId::as_str),
            Some("pathdb:42")
        );
        assert_eq!(
            snapshot
                .accepted_snapshot_id
                .as_ref()
                .map(AcceptedSnapshotId::as_str),
            Some("accepted:9")
        );
    }

    #[test]
    fn world_model_response_and_plan_report_round_trip_typed_ids_as_strings() {
        let response = WorldModelResponseV1 {
            protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
            trace_id: WorldModelRunId::new("wm::response"),
            generated_at_unix_secs: 55,
            proposals: ProposalsFileV1 {
                version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
                generated_at: "now".to_string(),
                source: ProposalSourceV1 {
                    source_type: "test".to_string(),
                    locator: "unit".to_string(),
                },
                schema_hint: None,
                proposals: Vec::new(),
            },
            notes: vec!["ok".to_string()],
            error: None,
        };
        let response_json = serde_json::to_value(&response).expect("serialize response");
        assert_eq!(response_json["trace_id"], "wm::response");
        let response_round_trip: WorldModelResponseV1 =
            serde_json::from_value(response_json).expect("deserialize response");
        assert_eq!(response_round_trip.trace_id.as_str(), "wm::response");

        let plan = WorldModelPlanReportV1 {
            version: "wm_plan_v1".to_string(),
            trace_id: WorldModelRunId::new("wm::plan"),
            generated_at_unix_secs: 77,
            horizon_steps: 1,
            rollouts: 2,
            max_new_proposals: 3,
            guardrail_profile: "fast".to_string(),
            guardrail_plane: "both".to_string(),
            guardrail_weights: GuardrailCostWeightsV1::defaults(),
            task_costs: vec![WorldModelTaskCostV1 {
                name: "completion".to_string(),
                value: 1.0,
                weight: 2.0,
                unit: "count".to_string(),
                notes: Some("unit".to_string()),
            }],
            task_cost_total: 2.0,
            competency_questions: Vec::new(),
            steps: vec![WorldModelPlanStepV1 {
                step: 0,
                trace_id: WorldModelRunId::new("wm::plan"),
                proposals: ProposalsFileV1 {
                    version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
                    generated_at: "now".to_string(),
                    source: ProposalSourceV1 {
                        source_type: "test".to_string(),
                        locator: "unit".to_string(),
                    },
                    schema_hint: None,
                    proposals: Vec::new(),
                },
                guardrail_before: GuardrailPlanSummaryV1::default(),
                guardrail_after: GuardrailPlanSummaryV1::default(),
                guardrail_delta: 0.0,
                competency_before: Some(CompetencyCoverageSummaryV1::default()),
                competency_after: Some(CompetencyCoverageSummaryV1::default()),
                competency_delta: Some(0.0),
                competency_cost: 0.0,
                task_cost_total: 2.0,
                total_cost: 2.0,
                validation_ok: true,
                validation_errors: 0,
                notes: vec!["step".to_string()],
            }],
        };
        let plan_json = serde_json::to_value(&plan).expect("serialize plan");
        assert_eq!(plan_json["trace_id"], "wm::plan");
        assert_eq!(plan_json["steps"][0]["trace_id"], "wm::plan");
        let plan_round_trip: WorldModelPlanReportV1 =
            serde_json::from_value(plan_json).expect("deserialize plan");
        assert_eq!(plan_round_trip.trace_id.as_str(), "wm::plan");
        assert_eq!(plan_round_trip.steps[0].trace_id.as_str(), "wm::plan");
    }

    #[test]
    fn build_world_model_provenance_computes_typed_proposal_digest_and_keeps_run_id_distinct() {
        let base_response = WorldModelResponseV1 {
            protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
            trace_id: WorldModelRunId::new("wm::digest"),
            generated_at_unix_secs: 77,
            proposals: ProposalsFileV1 {
                version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
                generated_at: "now".to_string(),
                source: ProposalSourceV1 {
                    source_type: "test".to_string(),
                    locator: "unit".to_string(),
                },
                schema_hint: None,
                proposals: vec![ProposalV1::Entity {
                    meta: ProposalMetaV1 {
                        proposal_id: "e1".to_string(),
                        confidence: 0.7,
                        evidence: Vec::new(),
                        public_rationale: "r".to_string(),
                        metadata: HashMap::new(),
                        schema_hint: None,
                    },
                    entity_id: "e1".to_string(),
                    entity_type: "Thing".to_string(),
                    name: "thing".to_string(),
                    attributes: HashMap::new(),
                    description: None,
                }],
            },
            notes: Vec::new(),
            error: None,
        };

        let provenance = build_world_model_provenance(
            &base_response,
            "stub".to_string(),
            Some("model".to_string()),
            Some(AxiDigest::new("fnv1a64:base")),
            Some(PathdbSnapshotId::new("pathdb:1")),
            Some(AcceptedSnapshotId::new("accepted:1")),
            Some(1.25),
            Some("fast".to_string()),
            Some("both".to_string()),
        )
        .expect("build provenance");

        assert_eq!(provenance.trace_id.as_str(), "wm::digest");
        assert_eq!(provenance.run_id.as_str(), "wm::digest");
        let digest = provenance
            .proposals_digest
            .as_ref()
            .expect("proposal-set digest should be present");
        assert!(digest.as_str().starts_with("fnv1a64:"));

        let mut changed = base_response.clone();
        if let ProposalV1::Entity { name, .. } = &mut changed.proposals.proposals[0] {
            *name = "thing-2".to_string();
        }
        let changed_provenance = build_world_model_provenance(
            &changed,
            "stub".to_string(),
            Some("model".to_string()),
            Some(AxiDigest::new("fnv1a64:base")),
            Some(PathdbSnapshotId::new("pathdb:1")),
            Some(AcceptedSnapshotId::new("accepted:1")),
            Some(1.25),
            Some("fast".to_string()),
            Some("both".to_string()),
        )
        .expect("build changed provenance");

        assert_ne!(
            provenance.proposals_digest, changed_provenance.proposals_digest,
            "proposal-set digest should change when the emitted proposals change"
        );
    }

    #[test]
    fn build_world_model_run_record_carries_typed_anchor_lineage_and_commit_state() {
        let response = WorldModelResponseV1 {
            protocol: WORLD_MODEL_PROTOCOL_V1.to_string(),
            trace_id: WorldModelRunId::new("wm::record"),
            generated_at_unix_secs: 77,
            proposals: ProposalsFileV1 {
                version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
                generated_at: "now".to_string(),
                source: ProposalSourceV1 {
                    source_type: "test".to_string(),
                    locator: "unit".to_string(),
                },
                schema_hint: None,
                proposals: vec![ProposalV1::Entity {
                    meta: ProposalMetaV1 {
                        proposal_id: "e1".to_string(),
                        confidence: 0.7,
                        evidence: Vec::new(),
                        public_rationale: "r".to_string(),
                        metadata: HashMap::new(),
                        schema_hint: None,
                    },
                    entity_id: "e1".to_string(),
                    entity_type: "Thing".to_string(),
                    name: "thing".to_string(),
                    attributes: HashMap::new(),
                    description: None,
                }],
            },
            notes: vec!["unit".to_string()],
            error: None,
        };
        let provenance = build_world_model_provenance(
            &response,
            "plugin".to_string(),
            Some("deterministic".to_string()),
            Some(AxiDigest::new("fnv1a64:axi")),
            Some(PathdbSnapshotId::new("pathdb:before")),
            Some(AcceptedSnapshotId::new("accepted:before")),
            Some(0.5),
            Some("fast".to_string()),
            Some("both".to_string()),
        )
        .expect("build provenance");

        let record = build_world_model_run_record(
            &provenance,
            &response.proposals,
            Some(PathdbSnapshotId::new("pathdb:after")),
            Some(AcceptedSnapshotId::new("accepted:after")),
            vec!["persisted".to_string()],
        )
        .expect("build run record");

        assert_eq!(record.run_id.as_str(), "wm::record");
        assert_eq!(record.trace_id.as_str(), "wm::record");
        assert_eq!(
            record.status,
            crate::accepted_plane::WorldModelRunStatusV1::CommittedToPathdb
        );
        assert_eq!(record.backend, "plugin");
        assert_eq!(record.model.as_deref(), Some("deterministic"));
        assert_eq!(
            record
                .input_pathdb_snapshot_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("pathdb:before")
        );
        assert_eq!(
            record
                .committed_pathdb_snapshot_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("pathdb:after")
        );
        assert_eq!(record.proposal_count, 1);
        assert!(record.proposals_digest.as_str().starts_with("fnv1a64:"));
    }
}
