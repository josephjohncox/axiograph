//! World model interface + guardrail costs (objective-driven / JEPA hooks).
//!
//! This module provides:
//! - a small plugin protocol (`axiograph_world_model_v1`),
//! - a stub backend (returns empty proposals),
//! - a command adapter (executes a local plugin),
//! - guardrail cost extraction from existing checks.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_ingest_docs::{ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1};
use axiograph_pathdb::checked_db::CheckedDb;
use axiograph_pathdb::PathDB;

pub const WORLD_MODEL_PROTOCOL_V1: &str = "axiograph_world_model_v1";

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn default_trace_id() -> String {
    format!("wm::{}", now_unix_secs())
}

// ---------------------------------------------------------------------------
// JEPA export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JepaExportFileV1 {
    pub version: String,
    pub axi_digest_v1: String,
    pub module_name: String,
    pub module_text: String,
    pub module: axiograph_dsl::schema_v1::SchemaV1Module,
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
}

pub fn build_jepa_export_from_axi_text(
    axi_text: &str,
    opts: &JepaExportOptions,
) -> Result<JepaExportFileV1> {
    if opts.mask_fields == 0 {
        return Err(anyhow!("--mask-fields must be > 0"));
    }

    let digest = axiograph_dsl::digest::axi_digest_v1(axi_text);
    let module = axiograph_dsl::axi_v1::parse_axi_v1(axi_text)?;

    let mut relations_by_schema: HashMap<String, HashSet<String>> = HashMap::new();
    for schema in &module.schemas {
        let entry = relations_by_schema
            .entry(schema.name.clone())
            .or_insert_with(HashSet::new);
        for rel in &schema.relations {
            entry.insert(rel.name.clone());
        }
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(opts.seed);
    let mut items: Vec<JepaExportItemV1> = Vec::new();

    for inst in &module.instances {
        if let Some(filter) = opts.instance_filter.as_ref() {
            if inst.name != *filter {
                continue;
            }
        }
        let Some(rel_set) = relations_by_schema.get(&inst.schema) else {
            continue;
        };

        for assign in &inst.assignments {
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
                    fields: fields
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
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
        module_name: module.module_name.clone(),
        module_text: axi_text.to_string(),
        module,
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
    let export: JepaExportFileV1 = serde_json::from_str(&text)?;
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

fn proposals_digest(file: &ProposalsFileV1) -> Result<String> {
    let bytes = serde_json::to_vec(file)
        .map_err(|e| anyhow!("failed to serialize proposals for digest: {e}"))?;
    Ok(axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes))
}

fn apply_proposals_to_db(db: &mut PathDB, proposals: &ProposalsFileV1) -> Result<()> {
    let digest = proposals_digest(proposals)?;
    let _summary =
        crate::proposals_import::import_proposals_file_into_pathdb(db, proposals, &digest)?;
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
        let (name, rest) = raw
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid task cost `{raw}` (expected name=value[:weight[:unit]])"))?;
        let mut parts = rest.split(':');
        let value_str = parts.next().unwrap_or("");
        if value_str.trim().is_empty() {
            return Err(anyhow!(
                "invalid task cost `{raw}` (expected name=value[:weight[:unit]])"
            ));
        }
        let value = value_str.trim().parse::<f64>().map_err(|_| {
            anyhow!("invalid task cost `{raw}` (value must be numeric)")
        })?;
        let weight = parts
            .next()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<f64>().map_err(|_| {
                    anyhow!("invalid task cost `{raw}` (weight must be numeric)")
                })
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
        let (name, query) = raw.split_once('=').ok_or_else(|| {
            anyhow!("invalid competency question `{raw}` (expected name=query)")
        })?;
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
    let quality = crate::quality::run_quality_checks(
        db,
        &PathBuf::from(input_label),
        profile,
        plane,
    )?;
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
    pub snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldModelInputV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_digest_v1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_module_text: Option<String>,
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
    pub trace_id: String,
    pub generated_at_unix_secs: u64,
    pub input: WorldModelInputV1,
    #[serde(default)]
    pub options: WorldModelOptionsV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldModelResponseV1 {
    pub protocol: String,
    pub trace_id: String,
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
    pub trace_id: String,
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
    pub trace_id: String,
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
            WorldModelBackend::Command { program, .. } => {
                format!("command({})", program.display())
            }
        };
        let model = self
            .model
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("default");
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
        }
    }

    pub fn backend_label(&self) -> String {
        match &self.backend {
            WorldModelBackend::Disabled => "disabled".to_string(),
            WorldModelBackend::Stub => "stub".to_string(),
            WorldModelBackend::Command { program, .. } => {
                format!("command:{}", program.display())
            }
        }
    }
}

fn empty_proposals(trace_id: &str) -> ProposalsFileV1 {
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
        .map_err(|e| anyhow!("failed to start world model plugin `{}`: {e}", program.display()))?;

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

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("world model plugin `{}` returned non-utf8 stdout: {e}", program.display()))?;
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

#[derive(Debug, Clone, Default)]
pub struct WorldModelProvenance {
    pub trace_id: String,
    pub backend: String,
    pub model: Option<String>,
    pub axi_digest_v1: Option<String>,
    pub guardrail_total_cost: Option<f64>,
    pub guardrail_profile: Option<String>,
    pub guardrail_plane: Option<String>,
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
        locator: provenance.trace_id.clone(),
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

fn apply_provenance_meta(meta: &mut ProposalMetaV1, provenance: &WorldModelProvenance) {
    meta.confidence = meta.confidence.clamp(0.0, 1.0);
    meta.metadata
        .entry("axiograph_world_model_trace_id".to_string())
        .or_insert_with(|| provenance.trace_id.clone());
    meta.metadata
        .entry("axiograph_world_model_backend".to_string())
        .or_insert_with(|| provenance.backend.clone());
    if let Some(model) = provenance.model.as_ref() {
        meta.metadata
            .entry("axiograph_world_model_model".to_string())
            .or_insert_with(|| model.clone());
    }
    if let Some(digest) = provenance.axi_digest_v1.as_ref() {
        meta.metadata
            .entry("axiograph_axi_digest_v1".to_string())
            .or_insert_with(|| digest.clone());
    }
    if let Some(cost) = provenance.guardrail_total_cost {
        meta.metadata
            .entry("axiograph_guardrail_total_cost".to_string())
            .or_insert_with(|| format!("{cost:.4}"));
    }
    if let Some(profile) = provenance.guardrail_profile.as_ref() {
        meta.metadata
            .entry("axiograph_guardrail_profile".to_string())
            .or_insert_with(|| profile.clone());
    }
    if let Some(plane) = provenance.guardrail_plane.as_ref() {
        meta.metadata
            .entry("axiograph_guardrail_plane".to_string())
            .or_insert_with(|| plane.clone());
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
    let task_cost_total: f64 = options
        .task_costs
        .iter()
        .map(|t| t.value * t.weight)
        .sum();
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
            Some(compute_competency_coverage(
                &planning_db,
                &options.competency_questions,
            )?)
        };

        let mut best: Option<(
            String,
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
            input
                .notes
                .push(format!("source=world_model_plan step={step} rollout={rollout}"));

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

            let provenance = WorldModelProvenance {
                trace_id: response.trace_id.clone(),
                backend: world_model.backend_label(),
                model: world_model.model.clone(),
                axi_digest_v1: base_input.axi_digest_v1.clone(),
                guardrail_total_cost: Some(guardrail_before.summary.total_cost),
                guardrail_profile: guardrail_profile_label,
                guardrail_plane: guardrail_plane_label,
            };

            let mut proposals =
                apply_world_model_provenance(response.proposals, &provenance);
            if options.max_new_proposals > 0 && proposals.proposals.len() > options.max_new_proposals {
                proposals.proposals.truncate(options.max_new_proposals);
            }

            let mut candidate = clone_db(&planning_db)?;
            apply_proposals_to_db(&mut candidate, &proposals)?;

            let guardrail_after =
                if options.include_guardrail && options.guardrail_profile != "off" {
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
                Some(compute_competency_coverage(
                    &candidate,
                    &options.competency_questions,
                )?)
            };
            let competency_cost = competency_after
                .as_ref()
                .map(|c| c.cost)
                .unwrap_or(0.0);

            let (validation_ok, validation_errors) = if options.validation_profile == "off" {
                (true, 0)
            } else {
                let validation = crate::proposals_validate::validate_proposals_v1(
                    &planning_db,
                    &proposals,
                    &options.validation_profile,
                    &options.validation_plane,
                )?;
                (
                    validation.ok,
                    validation.quality_delta.summary.error_count,
                )
            };

            let total_cost =
                guardrail_after.summary.total_cost + task_cost_total + competency_cost;
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
            guardrail_delta: guardrail_after.summary.total_cost - guardrail_before.summary.total_cost,
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

fn compute_competency_coverage(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
) -> Result<CompetencyCoverageSummaryV1> {
    if questions.is_empty() {
        return Ok(CompetencyCoverageSummaryV1::default());
    }

    let mut results: Vec<CompetencyQuestionResultV1> = Vec::new();
    let mut satisfied = 0usize;
    let mut total_cost = 0.0;

    for q in questions {
        let mut query = crate::axql::parse_axql_query(&q.query)?;
        if !q.contexts.is_empty() {
            let mut ctxs: Vec<crate::axql::AxqlContextSpec> = Vec::new();
            for raw in &q.contexts {
                if let Ok(id) = raw.parse::<u32>() {
                    ctxs.push(crate::axql::AxqlContextSpec::EntityId(id));
                } else {
                    ctxs.push(crate::axql::AxqlContextSpec::Name(raw.to_string()));
                }
            }
            query.contexts = ctxs;
        }
        let min_rows = if q.min_rows == 0 { 1 } else { q.min_rows };
        let limit = min_rows.min(1000);
        if query.limit == 0 || query.limit > limit {
            query.limit = limit;
        }

        let res = crate::axql::execute_axql_query(db, &query)?;
        let rows = res.rows.len();
        let ok = rows >= min_rows;
        if ok {
            satisfied += 1;
        }
        let weight = if q.weight <= 0.0 { 1.0 } else { q.weight };
        let cost = if ok { 0.0 } else { weight };
        total_cost += cost;

        results.push(CompetencyQuestionResultV1 {
            name: q.name.clone(),
            rows,
            min_rows,
            satisfied: ok,
            weight,
            cost,
        });
    }

    let total = questions.len();
    let coverage = if total == 0 {
        0.0
    } else {
        satisfied as f64 / total as f64
    };

    Ok(CompetencyCoverageSummaryV1 {
        total,
        satisfied,
        coverage,
        cost: total_cost,
        questions: results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        };
        let export = build_jepa_export_from_axi_text(axi, &opts).expect("export");
        assert!(!export.items.is_empty());
        for item in &export.items {
            assert_eq!(item.mask_fields.len(), 1);
        }
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
            trace_id: "trace".to_string(),
            backend: "stub".to_string(),
            model: Some("model".to_string()),
            axi_digest_v1: Some("digest".to_string()),
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
        assert!(meta.metadata.contains_key("axiograph_world_model_backend"));
        assert!(meta.metadata.contains_key("axiograph_world_model_model"));
        assert!(meta.metadata.contains_key("axiograph_axi_digest_v1"));
        assert!(meta.metadata.contains_key("axiograph_guardrail_total_cost"));
    }
}
