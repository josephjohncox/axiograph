//! Predictive proposal adapter interface + guardrail costs (objective-driven / training export hooks).
//!
//! This module provides:
//! - a small plugin protocol (`axiograph_predictive_proposal_v1`),
//! - a stub backend (returns empty proposals),
//! - a command adapter (executes a local plugin),
//! - guardrail cost extraction from existing checks.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
#[cfg(any(
    feature = "llm-ollama",
    feature = "llm-openai",
    feature = "llm-anthropic"
))]
use serde_json::json;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axiograph_cli::proposal_adapter_boundary::{
    parse_predictive_proposal_response_bounded, validate_predictive_proposal_response_shape,
};
pub use axiograph_cli::proposal_adapter_boundary::{
    PredictiveProposalResponseV1, PREDICTIVE_PROPOSAL_PROTOCOL_V1,
};
use axiograph_ingest_docs::{
    EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1,
};
use axiograph_kernel::MaterializationIdV2;
use axiograph_pathdb::certificate::AxiWellTypedProofV1;
use axiograph_pathdb::checked_db::CheckedDb;
use axiograph_pathdb::{
    AcceptedSnapshotId, AxiDigest, PathDB, ProposalAdapterRunId, ProposalDigest,
};
use axiograph_pathdb::{Module, WellTypedModuleState};

pub const COMPETENCY_QUESTION_BUNDLE_VERSION_V1: &str = "competency_question_bundle_v1";
const MAX_COMPETENCY_QUESTIONS: usize = 10_000;
pub(crate) const MAX_PROPOSAL_ROLLOUT_HORIZON: usize = 16;
pub(crate) const MAX_PROPOSAL_ROLLOUTS: usize = 16;
const MAX_PROPOSAL_ROLLOUT_ATTEMPTS: usize = 64;
const MAX_PROPOSAL_PLAN_GOALS: usize = 256;
const MAX_PROPOSAL_PLAN_TASK_COSTS: usize = 1_024;
const MAX_PROPOSAL_PLAN_NEW_PROPOSALS: usize = 10_000;
const MAX_PROPOSAL_PLAN_TEXT_BYTES: usize = 1024 * 1024;
const MAX_PROPOSAL_COST_COMPONENT: f64 = 1_000_000.0;

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn default_trace_id() -> ProposalAdapterRunId {
    ProposalAdapterRunId::new(format!("proposal::{}", now_unix_secs()))
}

// ---------------------------------------------------------------------------
// masked-tuple training export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskedTupleTrainingExportV1 {
    pub version: String,
    pub revision_digest_v2: AxiDigest,
    pub module_name: String,
    pub module_text: String,
    pub module: axiograph_dsl::schema_v1::SchemaV1Module,
    pub axi_well_typed_proof_v1: AxiWellTypedProofV1,
    pub items: Vec<MaskedTupleTrainingExportItemV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskedTupleTrainingExportItemV1 {
    pub schema: String,
    pub instance: String,
    pub relation: String,
    pub fields: Vec<(String, String)>,
    pub mask_fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MaskedTupleTrainingExportOptionsV1 {
    pub instance_filter: Option<String>,
    pub max_items: usize,
    pub mask_fields: usize,
    pub seed: u64,
    /// Relation names to exclude from the derived training export when a caller
    /// wants to suppress specific canonical relations.
    pub exclude_relations: Vec<String>,
}

pub fn build_training_export_from_axi_text(
    axi_text: &str,
    opts: &MaskedTupleTrainingExportOptionsV1,
) -> Result<MaskedTupleTrainingExportV1> {
    if opts.mask_fields == 0 {
        return Err(anyhow!("--mask-fields must be > 0"));
    }

    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    build_training_export_from_well_typed_module(
        axi_text,
        canonical.module(),
        canonical.digest().clone(),
        opts,
    )
}

fn build_training_export_from_well_typed_module<S: WellTypedModuleState>(
    axi_text: &str,
    module: &Module<S>,
    digest: AxiDigest,
    opts: &MaskedTupleTrainingExportOptionsV1,
) -> Result<MaskedTupleTrainingExportV1> {
    let typed_module = module.module();

    let mut relations_by_schema: HashMap<String, HashSet<String>> = HashMap::new();
    for schema in &typed_module.schemas {
        let entry = relations_by_schema.entry(schema.name.clone()).or_default();
        for rel in &schema.relations {
            entry.insert(rel.name.clone());
        }
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(opts.seed);
    let mut items: Vec<MaskedTupleTrainingExportItemV1> = Vec::new();
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
                let axiograph_dsl::schema_v1::SetItemV1::Tuple { fields, .. } = item else {
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

                let entry = MaskedTupleTrainingExportItemV1 {
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

    Ok(MaskedTupleTrainingExportV1 {
        version: "axi_training_export_v1".to_string(),
        revision_digest_v2: digest,
        module_name: typed_module.module_name.clone(),
        module_text: axi_text.to_string(),
        module: typed_module.clone(),
        axi_well_typed_proof_v1: module.proof().clone(),
        items,
    })
}

pub fn write_training_export(
    input: &Path,
    out: &Path,
    opts: &MaskedTupleTrainingExportOptionsV1,
) -> Result<MaskedTupleTrainingExportV1> {
    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let export = build_training_export_from_axi_text(&text, opts)?;
    let json = serde_json::to_string_pretty(&export)?;
    crate::security::write_output_bounded(out, json, "CLI output")?;
    Ok(export)
}

#[allow(dead_code)]
pub fn read_training_export(path: &Path) -> Result<MaskedTupleTrainingExportV1> {
    let text = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let export: MaskedTupleTrainingExportV1 = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "masked tuple training export",
    )?;
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
        axiograph_kernel::object_blob_digest_v2(&bytes),
    ))
}

fn apply_proposals_to_db(db: &mut PathDB, proposals: &ProposalsFileV1) -> Result<()> {
    let digest = proposals_digest(proposals)?;
    let _summary =
        crate::proposals_import::import_proposals_file_into_pathdb(db, proposals, digest.as_str())?;
    Ok(())
}

fn clone_db(db: &PathDB) -> Result<PathDB> {
    db.detached_clone()
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

pub fn parse_task_costs(items: &[String]) -> Result<Vec<ProposalTaskCostV1>> {
    let mut out: Vec<ProposalTaskCostV1> = Vec::new();
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
        out.push(ProposalTaskCostV1 {
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
            authoring: None,
            query: query.to_string(),
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        });
    }
    Ok(out)
}

pub fn load_competency_questions(path: &Path) -> Result<Vec<CompetencyQuestionV1>> {
    let text = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("json") {
        return parse_competency_question_text(&text);
    }
    let bundle: CompetencyQuestionBundleV1 = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "competency question bundle",
    )?;
    if bundle.questions.len() > MAX_COMPETENCY_QUESTIONS {
        return Err(anyhow!(
            "competency question count exceeds {MAX_COMPETENCY_QUESTIONS}"
        ));
    }
    if bundle.version != COMPETENCY_QUESTION_BUNDLE_VERSION_V1 {
        return Err(anyhow!(
            "unsupported competency question bundle version `{}` (expected `{}`)",
            bundle.version,
            COMPETENCY_QUESTION_BUNDLE_VERSION_V1
        ));
    }
    Ok(bundle.questions)
}

pub fn parse_competency_question_text(text: &str) -> Result<Vec<CompetencyQuestionV1>> {
    let mut questions = Vec::new();
    let mut current: Option<CompetencyQuestionV1> = None;
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
                return Err(anyhow!(
                    "unsupported competency question text version `{version}` at line {line_no} (expected `{COMPETENCY_QUESTION_BUNDLE_VERSION_V1}`)"
                ));
            }
            saw_version = true;
            continue;
        }
        if let Some(name) = line
            .strip_prefix("question ")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            if let Some(mut question) = current.take() {
                lower_competency_question_text_record(&mut question);
                validate_competency_question_text_record(&question)?;
                if questions.len() >= MAX_COMPETENCY_QUESTIONS {
                    return Err(anyhow!(
                        "competency question count exceeds {MAX_COMPETENCY_QUESTIONS}"
                    ));
                }
                questions.push(question);
            }
            let name = name.trim();
            if name.is_empty() {
                return Err(anyhow!("empty competency question name at line {line_no}"));
            }
            current = Some(CompetencyQuestionV1 {
                name: name.to_string(),
                question: None,
                authoring: None,
                query: String::new(),
                min_rows: 1,
                weight: 1.0,
                contexts: Vec::new(),
            });
            continue;
        }

        let Some(question) = current.as_mut() else {
            return Err(anyhow!(
                "competency question field before `question <name>:` at line {line_no}"
            ));
        };
        let (key, value) = line.split_once(':').ok_or_else(|| {
            anyhow!("invalid competency question field at line {line_no} (expected `key: value`)")
        })?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "ask" | "asks" | "question" => {
                question.question = Some(value.to_string());
                question.authoring.get_or_insert_with(Default::default).ask =
                    Some(value.to_string());
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
            other => {
                return Err(anyhow!(
                    "unsupported competency question field `{other}` at line {line_no}"
                ));
            }
        }
    }

    if let Some(mut question) = current.take() {
        lower_competency_question_text_record(&mut question);
        validate_competency_question_text_record(&question)?;
        if questions.len() >= MAX_COMPETENCY_QUESTIONS {
            return Err(anyhow!(
                "competency question count exceeds {MAX_COMPETENCY_QUESTIONS}"
            ));
        }
        questions.push(question);
    }
    if !saw_version {
        return Err(anyhow!(
            "competency question text requires `version {COMPETENCY_QUESTION_BUNDLE_VERSION_V1}`"
        ));
    }
    if questions.is_empty() {
        return Err(anyhow!("competency question text contains no questions"));
    }
    Ok(questions)
}

fn validate_competency_question_text_record(question: &CompetencyQuestionV1) -> Result<()> {
    if question.name.trim().is_empty() {
        return Err(anyhow!("competency question has an empty name"));
    }
    let has_authoring = question.authoring.as_ref().is_some_and(|hints| {
        !hints.about.is_empty()
            || !hints.given.is_empty()
            || !hints.expect.is_empty()
            || hints.ask.is_some()
            || !hints.notes.is_empty()
    });
    if question.query.trim().is_empty() && question.question.is_none() && !has_authoring {
        return Err(anyhow!(
            "competency question `{}` requires `ask: ...`, `expect: ...`, or `axql: ...`",
            question.name
        ));
    }
    if question.contexts.len() > 1_024 {
        return Err(anyhow!(
            "competency question `{}` context count exceeds 1024",
            question.name
        ));
    }
    if question.min_rows == 0 {
        return Err(anyhow!(
            "competency question `{}` requires min_rows > 0",
            question.name
        ));
    }
    if !question.weight.is_finite() || question.weight <= 0.0 {
        return Err(anyhow!(
            "competency question `{}` requires a finite positive weight",
            question.name
        ));
    }
    Ok(())
}

fn lower_competency_question_text_record(question: &mut CompetencyQuestionV1) {
    if !question.query.trim().is_empty() {
        return;
    }
    if let Some(query) = lower_competency_question_authoring(question) {
        question.query = query;
    }
}

fn lower_competency_question_authoring(question: &CompetencyQuestionV1) -> Option<String> {
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

fn looks_like_qualified_relation_ref(value: &str) -> bool {
    let head = value.split_once('(').map(|(head, _)| head).unwrap_or(value);
    looks_like_qualified_ref(head)
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

fn looks_like_qualified_ref(value: &str) -> bool {
    let Some((schema, rel)) = value.split_once('.') else {
        return false;
    };
    !schema.trim().is_empty()
        && !rel.trim().is_empty()
        && schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && rel.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
// Predictive proposal adapter plugin protocol
// ---------------------------------------------------------------------------

pub const PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1: &str = "canonical_axi_semantics_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PredictiveProposalSemanticLayerV1 {
    Guardrail { report: GuardrailCostReportV1 },
    TrainingExport { export: MaskedTupleTrainingExportV1 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveProposalSemanticInputV1 {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialization_id: Option<MaterializationIdV2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<PredictiveProposalSemanticLayerV1>,
}

impl Default for PredictiveProposalSemanticInputV1 {
    fn default() -> Self {
        Self {
            kind: PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1.to_string(),
            module_name: None,
            materialization_id: None,
            accepted_snapshot_id: None,
            layers: Vec::new(),
        }
    }
}

impl PredictiveProposalSemanticInputV1 {
    pub fn is_empty(&self) -> bool {
        self.kind == PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1
            && self.module_name.is_none()
            && self.materialization_id.is_none()
            && self.accepted_snapshot_id.is_none()
            && self.layers.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PredictiveProposalInputV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_digest_v2: Option<AxiDigest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axi_module_text: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "PredictiveProposalSemanticInputV1::is_empty"
    )]
    pub semantic_input: PredictiveProposalSemanticInputV1,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl PredictiveProposalInputV1 {
    pub fn set_canonical_axi_semantics(
        &mut self,
        module_name: Option<String>,
        materialization_id: Option<MaterializationIdV2>,
        accepted_snapshot_id: Option<AcceptedSnapshotId>,
    ) {
        self.semantic_input.kind = PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1.to_string();
        self.semantic_input.module_name = module_name;
        self.semantic_input.materialization_id = materialization_id;
        self.semantic_input.accepted_snapshot_id = accepted_snapshot_id;
    }

    fn replace_semantic_layer(&mut self, layer: PredictiveProposalSemanticLayerV1) {
        let same_kind = |candidate: &PredictiveProposalSemanticLayerV1| {
            matches!(
                (&layer, candidate),
                (
                    PredictiveProposalSemanticLayerV1::Guardrail { .. },
                    PredictiveProposalSemanticLayerV1::Guardrail { .. }
                ) | (
                    PredictiveProposalSemanticLayerV1::TrainingExport { .. },
                    PredictiveProposalSemanticLayerV1::TrainingExport { .. }
                )
            )
        };
        self.semantic_input
            .layers
            .retain(|candidate| !same_kind(candidate));
        self.semantic_input.layers.push(layer);
    }

    pub fn set_guardrail_layer(&mut self, report: GuardrailCostReportV1) {
        self.replace_semantic_layer(PredictiveProposalSemanticLayerV1::Guardrail { report });
    }

    pub fn set_training_export_layer(&mut self, export: MaskedTupleTrainingExportV1) {
        self.replace_semantic_layer(PredictiveProposalSemanticLayerV1::TrainingExport { export });
    }

    #[cfg(any(
        feature = "llm-ollama",
        feature = "llm-openai",
        feature = "llm-anthropic"
    ))]
    pub fn training_export(&self) -> Option<&MaskedTupleTrainingExportV1> {
        self.semantic_input
            .layers
            .iter()
            .find_map(|layer| match layer {
                PredictiveProposalSemanticLayerV1::TrainingExport { export } => Some(export),
                PredictiveProposalSemanticLayerV1::Guardrail { .. } => None,
            })
    }

    pub fn materialization_id(&self) -> Option<MaterializationIdV2> {
        self.semantic_input.materialization_id.clone()
    }

    pub fn accepted_snapshot_id(&self) -> Option<AcceptedSnapshotId> {
        self.semantic_input.accepted_snapshot_id.clone()
    }

    pub fn validate_canonical_axi_contract(&self) -> Result<()> {
        let Some(axi_text) = self.axi_module_text.as_deref() else {
            return Err(anyhow!(
                "predictive proposal adapter input requires `axi_module_text` with a canonical `.axi` module"
            ));
        };

        let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
        let canonical_digest = canonical.digest();
        let canonical_module_name = canonical.module().module().module_name.as_str();

        let Some(input_digest) = self.revision_digest_v2.as_ref() else {
            return Err(anyhow!(
                "predictive proposal adapter input requires `revision_digest_v2` anchored to the canonical `.axi` input"
            ));
        };
        if input_digest.as_str() != canonical_digest.as_str() {
            return Err(anyhow!(
                "predictive proposal adapter input `revision_digest_v2` `{}` does not match canonical `.axi` digest `{}`",
                input_digest.as_str(),
                canonical_digest.as_str()
            ));
        }

        if self.semantic_input.kind != PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1 {
            return Err(anyhow!(
                "predictive proposal adapter input semantic kind `{}` is unsupported; expected `{}`",
                self.semantic_input.kind,
                PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1
            ));
        }

        if let Some(module_name) = self.semantic_input.module_name.as_deref() {
            if module_name != canonical_module_name {
                return Err(anyhow!(
                    "predictive proposal adapter input semantic module `{module_name}` does not match canonical `.axi` module `{canonical_module_name}`"
                ));
            }
        }

        for layer in &self.semantic_input.layers {
            if let PredictiveProposalSemanticLayerV1::TrainingExport { export } = layer {
                if export.revision_digest_v2.as_str() != canonical_digest.as_str() {
                    return Err(anyhow!(
                        "predictive proposal adapter training export digest `{}` does not match canonical `.axi` digest `{}`",
                        export.revision_digest_v2.as_str(),
                        canonical_digest.as_str()
                    ));
                }
                if export.module_name != canonical_module_name {
                    return Err(anyhow!(
                        "predictive proposal adapter training export module `{}` does not match canonical `.axi` module `{}`",
                        export.module_name,
                        canonical_module_name
                    ));
                }
                let export_canonical =
                    crate::axi_input::require_canonical_axi_text(&export.module_text)?;
                if export_canonical.digest().as_str() != canonical_digest.as_str() {
                    return Err(anyhow!(
                        "predictive proposal adapter training export module text does not match the canonical `.axi` input digest `{}`",
                        canonical_digest.as_str()
                    ));
                }
            }
        }

        Ok(())
    }
}

pub fn validate_predictive_proposal_request(req: &PredictiveProposalRequestV1) -> Result<()> {
    if req.protocol != PREDICTIVE_PROPOSAL_PROTOCOL_V1 {
        return Err(anyhow!(
            "predictive proposal adapter request protocol must be `{PREDICTIVE_PROPOSAL_PROTOCOL_V1}`, got `{}`",
            req.protocol
        ));
    }
    req.input.validate_canonical_axi_contract()
}

fn validate_predictive_proposal_response(
    response: &PredictiveProposalResponseV1,
    expected_trace_id: &ProposalAdapterRunId,
) -> Result<()> {
    validate_predictive_proposal_response_shape(response)?;
    if &response.trace_id != expected_trace_id {
        return Err(anyhow!(
            "predictive proposal response trace id does not match request"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PredictiveProposalObjectiveV1 {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProposalTaskCostV1 {
    pub name: String,
    pub value: f64,
    pub weight: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

pub use axiograph_query::competency_questions::{
    CompetencyCoverageSummaryV1, CompetencyQuestionBundleV1, CompetencyQuestionV1,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PredictiveProposalOptionsV1 {
    #[serde(default)]
    pub max_new_proposals: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub objectives: Vec<PredictiveProposalObjectiveV1>,
    #[serde(default)]
    pub task_costs: Vec<ProposalTaskCostV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizon_steps: Option<usize>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveProposalRequestV1 {
    pub protocol: String,
    pub trace_id: ProposalAdapterRunId,
    pub generated_at_unix_secs: u64,
    pub input: PredictiveProposalInputV1,
    #[serde(default)]
    pub options: PredictiveProposalOptionsV1,
}

#[derive(Debug, Clone)]
pub struct BoundedProposalPlanOptionsV1 {
    pub horizon_steps: usize,
    pub rollouts: usize,
    pub max_new_proposals: usize,
    pub seed: Option<u64>,
    pub goals: Vec<String>,
    pub task_costs: Vec<ProposalTaskCostV1>,
    pub competency_questions: Vec<CompetencyQuestionV1>,
    pub guardrail_profile: String,
    pub guardrail_plane: String,
    pub guardrail_weights: GuardrailCostWeightsV1,
    pub include_guardrail: bool,
    pub validation_profile: String,
    pub validation_plane: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundedProposalPlanStepV1 {
    pub step: usize,
    pub trace_id: ProposalAdapterRunId,
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
pub struct BoundedProposalPlanReportV1 {
    pub version: String,
    pub trace_id: ProposalAdapterRunId,
    pub generated_at_unix_secs: u64,
    pub horizon_steps: usize,
    pub rollouts: usize,
    pub max_new_proposals: usize,
    pub guardrail_profile: String,
    pub guardrail_plane: String,
    pub guardrail_weights: GuardrailCostWeightsV1,
    pub task_costs: Vec<ProposalTaskCostV1>,
    pub task_cost_total: f64,
    #[serde(default)]
    pub competency_questions: Vec<CompetencyQuestionV1>,
    pub steps: Vec<BoundedProposalPlanStepV1>,
}

#[derive(Debug, Clone, Default)]
pub enum ProposalAdapterBackend {
    #[default]
    Disabled,
    Stub,
    Command {
        program: PathBuf,
        args: Vec<String>,
    },
    Http {
        url: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ProposalAdapterState {
    pub backend: ProposalAdapterBackend,
    pub model: Option<String>,
}

impl ProposalAdapterState {
    pub fn status_line(&self) -> String {
        let backend = match &self.backend {
            ProposalAdapterBackend::Disabled => "disabled".to_string(),
            ProposalAdapterBackend::Stub => "stub".to_string(),
            ProposalAdapterBackend::Command { program, args } => {
                if args.iter().any(|s| s == "predictive-proposals-llm") {
                    "llm".to_string()
                } else {
                    format!("command({})", program.display())
                }
            }
            ProposalAdapterBackend::Http { url } => format!("http({url})"),
        };
        let model = self.model.as_deref().unwrap_or("default");
        format!("predictive_proposal: backend={backend} model={model}")
    }

    pub fn propose(
        &self,
        req: &PredictiveProposalRequestV1,
    ) -> Result<PredictiveProposalResponseV1> {
        validate_predictive_proposal_request(req)?;
        let response = match &self.backend {
            ProposalAdapterBackend::Disabled => {
                return Err(anyhow!(
                    "predictive proposal adapter backend is disabled (configure --proposal-adapter-plugin or use stub)"
                ))
            }
            ProposalAdapterBackend::Stub => PredictiveProposalResponseV1 {
                protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
                trace_id: req.trace_id.clone(),
                generated_at_unix_secs: now_unix_secs(),
                proposals: empty_proposals(&req.trace_id),
                notes: vec!["stub backend (no proposals)".to_string()],
                error: None,
            },
            ProposalAdapterBackend::Command { program, args } => {
                run_predictive_proposal_plugin(program, args, req)?
            }
            ProposalAdapterBackend::Http { url } => run_predictive_proposal_http(url, req)?,
        };
        validate_predictive_proposal_response(&response, &req.trace_id)?;
        Ok(response)
    }

    pub fn backend_label(&self) -> String {
        match &self.backend {
            ProposalAdapterBackend::Disabled => "disabled".to_string(),
            ProposalAdapterBackend::Stub => "stub".to_string(),
            ProposalAdapterBackend::Command { program, args } => {
                if args.iter().any(|s| s == "predictive-proposals-llm") {
                    "llm".to_string()
                } else {
                    format!("command:{}", program.display())
                }
            }
            ProposalAdapterBackend::Http { url } => format!("http:{url}"),
        }
    }
}

#[cfg(feature = "proposal-adapter-http")]
fn run_predictive_proposal_http(
    url: &str,
    req: &PredictiveProposalRequestV1,
) -> Result<PredictiveProposalResponseV1> {
    let payload = serde_json::to_vec(req)?;
    if payload.len() > crate::security::MAX_JSON_INPUT_BYTES {
        return Err(anyhow!("predictive proposal request exceeds byte limit"));
    }
    let endpoint = url::Url::parse(url)
        .map_err(|error| anyhow!("invalid predictive proposal adapter URL: {error}"))?;
    if endpoint.scheme() != "https" {
        return Err(anyhow!(
            "predictive proposal HTTP adapters require a public HTTPS endpoint; use the command adapter for local processes"
        ));
    }
    let transport = crate::web::PinnedPublicClient::new(
        &endpoint,
        reqwest::header::HeaderMap::new(),
        Duration::from_secs(120),
    )?;
    let response = transport
        .post()
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload)
        .send()
        .map_err(|e| anyhow!("predictive proposal adapter HTTP backend failed: {e}"))?;
    let resp = transport.verify_response(response)?;
    let status = resp.status();
    let limit = if status.is_success() {
        crate::security::MAX_NETWORK_RESPONSE_BYTES
    } else {
        crate::security::MAX_NETWORK_ERROR_BYTES
    };
    let bytes = crate::security::read_blocking_response_bounded(
        resp,
        limit,
        "predictive proposal adapter response",
    )?;
    if !status.is_success() {
        return Err(anyhow!(
            "predictive proposal adapter http backend returned {status}: {}",
            String::from_utf8_lossy(&bytes).trim()
        ));
    }
    parse_predictive_proposal_response_bounded(
        &bytes,
        crate::security::MAX_NETWORK_RESPONSE_BYTES,
        "predictive proposal adapter response",
    )
}

#[cfg(not(feature = "proposal-adapter-http"))]
fn run_predictive_proposal_http(
    _url: &str,
    _req: &PredictiveProposalRequestV1,
) -> Result<PredictiveProposalResponseV1> {
    Err(anyhow!(
        "predictive proposal adapter http backend is unavailable (enable feature `proposal-adapter-http`)"
    ))
}

fn empty_proposals(trace_id: impl AsRef<str>) -> ProposalsFileV1 {
    let trace_id = trace_id.as_ref();
    ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at: now_unix_secs().to_string(),
        source: ProposalSourceV1 {
            source_type: "predictive_proposal_adapter".to_string(),
            locator: trace_id.to_string(),
        },
        schema_hint: None,
        proposals: Vec::new(),
    }
}

fn run_predictive_proposal_plugin(
    program: &Path,
    args: &[String],
    req: &PredictiveProposalRequestV1,
) -> Result<PredictiveProposalResponseV1> {
    let payload = serde_json::to_vec(req)?;
    let limits = crate::security::ProcessLimits::plugin(Duration::from_secs(120))?;
    let context = format!("predictive proposal adapter plugin `{}`", program.display());
    let mut command = Command::new(program);
    command.args(args);
    let output = crate::security::run_command_bounded(command, &payload, limits, &context)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "predictive proposal adapter plugin `{}` failed (exit={:?}): {}",
            program.display(),
            output.status.code(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| {
        anyhow!(
            "predictive proposal adapter plugin `{}` returned non-utf8 stdout: {e}",
            program.display()
        )
    })?;
    let response = parse_predictive_proposal_response_bounded(
        stdout.as_bytes(),
        axiograph_security::DEFAULT_PLUGIN_STDOUT_BYTES,
        "predictive proposal plugin response",
    )
    .map_err(|e| {
        let preview: String = stdout.chars().take(400).collect();
        anyhow!(
            "predictive proposal adapter plugin `{}` returned invalid JSON: {e}; stdout starts with: {preview:?}",
            program.display()
        )
    })?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Provenance helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProposalAdapterProvenance {
    pub trace_id: ProposalAdapterRunId,
    pub run_id: ProposalAdapterRunId,
    pub backend: String,
    pub model: Option<String>,
    pub revision_digest_v2: Option<AxiDigest>,
    pub materialization_id: Option<MaterializationIdV2>,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: Option<ProposalDigest>,
    pub guardrail_total_cost: Option<f64>,
    pub guardrail_profile: Option<String>,
    pub guardrail_plane: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalAdapterLineage {
    pub trace_id: Option<ProposalAdapterRunId>,
    pub run_id: Option<ProposalAdapterRunId>,
    pub backend: Option<String>,
    pub model: Option<String>,
    pub revision_digest_v2: Option<AxiDigest>,
    pub materialization_id: Option<MaterializationIdV2>,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: Option<ProposalDigest>,
    pub guardrail_total_cost: Option<f64>,
    pub guardrail_profile: Option<String>,
    pub guardrail_plane: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_predictive_proposal_provenance(
    response: &PredictiveProposalResponseV1,
    backend: String,
    model: Option<String>,
    revision_digest_v2: Option<AxiDigest>,
    materialization_id: Option<MaterializationIdV2>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    guardrail_total_cost: Option<f64>,
    guardrail_profile: Option<String>,
    guardrail_plane: Option<String>,
) -> Result<ProposalAdapterProvenance> {
    Ok(ProposalAdapterProvenance {
        trace_id: response.trace_id.clone(),
        run_id: response.trace_id.clone(),
        backend,
        model,
        revision_digest_v2,
        materialization_id,
        accepted_snapshot_id,
        proposals_digest: Some(proposals_digest(&response.proposals)?),
        guardrail_total_cost,
        guardrail_profile,
        guardrail_plane,
    })
}

pub fn apply_predictive_proposal_provenance(
    mut proposals: ProposalsFileV1,
    provenance: &ProposalAdapterProvenance,
) -> ProposalsFileV1 {
    if proposals.generated_at.trim().is_empty() {
        proposals.generated_at = now_unix_secs().to_string();
    }
    proposals.source = ProposalSourceV1 {
        source_type: "predictive_proposal_adapter".to_string(),
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
pub fn extract_predictive_proposal_proposal_lineage(
    meta: &ProposalMetaV1,
) -> ProposalAdapterLineage {
    fn optional_id<T>(meta: &ProposalMetaV1, key: &str) -> Option<T>
    where
        T: From<String>,
    {
        meta.metadata.get(key).cloned().map(T::from)
    }

    ProposalAdapterLineage {
        trace_id: optional_id(meta, "axiograph_predictive_proposal_trace_id"),
        run_id: optional_id(meta, "axiograph_proposal_adapter_run_id"),
        backend: meta
            .metadata
            .get("axiograph_predictive_proposal_backend")
            .cloned(),
        model: meta
            .metadata
            .get("axiograph_predictive_proposal_model")
            .cloned(),
        revision_digest_v2: optional_id(meta, "axiograph_revision_digest_v2"),
        materialization_id: meta
            .metadata
            .get("axiograph_materialization_id")
            .and_then(|value| value.parse().ok()),
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

fn apply_provenance_meta(meta: &mut ProposalMetaV1, provenance: &ProposalAdapterProvenance) {
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
        "axiograph_predictive_proposal_trace_id",
        provenance.trace_id.to_string(),
    );
    set_reserved(
        meta,
        "axiograph_proposal_adapter_run_id",
        provenance.run_id.to_string(),
    );
    set_reserved(
        meta,
        "axiograph_predictive_proposal_backend",
        provenance.backend.clone(),
    );
    set_optional_reserved(
        meta,
        "axiograph_predictive_proposal_model",
        provenance.model.clone(),
    );
    set_optional_reserved(
        meta,
        "axiograph_revision_digest_v2",
        provenance
            .revision_digest_v2
            .as_ref()
            .map(ToString::to_string),
    );
    set_optional_reserved(
        meta,
        "axiograph_materialization_id",
        provenance
            .materialization_id
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

#[cfg(any(
    feature = "llm-ollama",
    feature = "llm-openai",
    feature = "llm-anthropic"
))]
pub(crate) fn predictive_proposal_llm_prompt(req: &PredictiveProposalRequestV1) -> (String, Value) {
    let trace_id = if req.trace_id.as_str().trim().is_empty() {
        default_trace_id()
    } else {
        req.trace_id.clone()
    };
    let opts = &req.options;
    let input = &req.input;

    let training_export_summary = input.training_export().map(|export| {
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
            "revision_digest_v2": export.revision_digest_v2,
            "items": export.items.len(),
            "sample": sample,
        })
    });

    let semantic_layers_summary = input
        .semantic_input
        .layers
        .iter()
        .map(|layer| match layer {
            PredictiveProposalSemanticLayerV1::Guardrail { report } => json!({
                "kind": "guardrail",
                "profile": report.profile,
                "plane": report.plane,
                "total_cost": report.summary.total_cost,
                "term_count": report.summary.term_count,
            }),
            PredictiveProposalSemanticLayerV1::TrainingExport { export } => json!({
                "kind": "training_export",
                "module_name": export.module_name,
                "revision_digest_v2": export.revision_digest_v2,
                "items": export.items.len(),
            }),
        })
        .collect::<Vec<_>>();

    let summary = json!({
        "trace_id": trace_id,
        "generated_at": req.generated_at_unix_secs,
        "goals": opts.goals,
        "objectives": opts.objectives,
        "task_costs": opts.task_costs,
        "max_new_proposals": opts.max_new_proposals,
        "notes": opts.notes,
        "revision_digest_v2": input.revision_digest_v2,
        "semantic_input": {
            "kind": input.semantic_input.kind,
            "module_name": input.semantic_input.module_name,
            "materialization_id": input.semantic_input.materialization_id,
            "accepted_snapshot_id": input.semantic_input.accepted_snapshot_id,
            "layers": semantic_layers_summary,
        },
        "training_export_summary": training_export_summary,
    });

    let prompt = [
        "You are a proposal-adapter assistant for Axiograph.",
        "Return ONLY JSON (no markdown) that conforms to:",
        "ProposalsFileV1 = {",
        "  \"version\": 1,",
        "  \"generated_at\": \"<unix-secs as string>\",",
        "  \"source\": {\"source_type\": \"predictive_proposal\", \"locator\": \"<trace_id>\"},",
        "  \"schema_hint\": null,",
        "  \"proposals\": [ ProposalV1 (entity or relation) ]",
        "}",
        "ProposalV1 entity:",
        "{ \"kind\":\"Entity\", \"proposal_id\":\"...\", \"confidence\":0.0-1.0, \"evidence\":[], \"public_rationale\":\"...\", \"metadata\":{}, \"schema_hint\":null,",
        "  \"entity_id\":\"...\", \"entity_type\":\"...\", \"name\":\"...\", \"attributes\":{}, \"description\":null }",
        "ProposalV1 relation:",
        "{ \"kind\":\"Relation\", \"proposal_id\":\"...\", \"confidence\":0.0-1.0, \"evidence\":[], \"public_rationale\":\"...\", \"metadata\":{}, \"schema_hint\":null,",
        "  \"relation_id\":\"...\", \"rel_type\":\"...\", \"source\":\"...\", \"target\":\"...\",",
        "  \"attributes\":{\"axi_source_field\":\"<declared role>\",\"axi_target_field\":\"<declared role>\",\"ctx\":\"...\",\"time\":\"...\"} }",
        "Rules:",
        "- Propose at most max_new_proposals items.",
        "- Use stable ids (e.g. proposal::<trace_id>::n).",
        "- Keep confidence between 0.55 and 0.9.",
        "- Relation proposals must include `axi_source_field` and `axi_target_field` attributes naming declared roles in the compiled relation; do not rely on source/target naming conventions.",
        "- Treat `axi_module_text` + `semantic_input` as the semantic source of truth.",
        "- `training_export_summary` is an optional derived view for pattern discovery, not the ontology kernel.",
        "- Use only info grounded in `axi_module_text`, `semantic_input`, optional `training_export_summary`, and the stated goals.",
        "- Do NOT infer relationships from mere co-occurrence of string tokens/ids or other low-level implementation artifacts. If you cannot cite a specific typed item/field in the semantic input or training-export summary that supports the proposal, do not propose it.",
    ]
    .join("\n");

    (prompt, summary)
}

pub(crate) fn normalize_predictive_proposal_proposals_value(
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
        let base_id = format!("proposal::{trace_id}::{idx}");
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
            .unwrap_or("predictive proposal adapter proposal")
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
            source_type: "predictive_proposal_adapter".to_string(),
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

pub fn make_predictive_proposal_request(
    input: PredictiveProposalInputV1,
    options: PredictiveProposalOptionsV1,
) -> PredictiveProposalRequestV1 {
    PredictiveProposalRequestV1 {
        protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
        trace_id: default_trace_id(),
        generated_at_unix_secs: now_unix_secs(),
        input,
        options,
    }
}

pub fn run_proposal_rollout_plan(
    db: &PathDB,
    predictive_proposal: &ProposalAdapterState,
    base_input: &PredictiveProposalInputV1,
    options: &BoundedProposalPlanOptionsV1,
) -> Result<BoundedProposalPlanReportV1> {
    if !(1..=MAX_PROPOSAL_ROLLOUT_HORIZON).contains(&options.horizon_steps) {
        return Err(anyhow!(
            "bounded proposal rollout: horizon_steps must be in 1..={MAX_PROPOSAL_ROLLOUT_HORIZON}"
        ));
    }
    if !(1..=MAX_PROPOSAL_ROLLOUTS).contains(&options.rollouts) {
        return Err(anyhow!(
            "bounded proposal rollout: rollouts must be in 1..={MAX_PROPOSAL_ROLLOUTS}"
        ));
    }
    let attempts = options
        .horizon_steps
        .checked_mul(options.rollouts)
        .ok_or_else(|| anyhow!("bounded proposal rollout: attempt count overflow"))?;
    if attempts > MAX_PROPOSAL_ROLLOUT_ATTEMPTS {
        return Err(anyhow!(
            "bounded proposal rollout: horizon*rollouts exceeds {MAX_PROPOSAL_ROLLOUT_ATTEMPTS} adapter attempts"
        ));
    }
    if options.max_new_proposals > MAX_PROPOSAL_PLAN_NEW_PROPOSALS {
        return Err(anyhow!(
            "bounded proposal rollout: max_new_proposals exceeds {MAX_PROPOSAL_PLAN_NEW_PROPOSALS}"
        ));
    }
    if options.goals.len() > MAX_PROPOSAL_PLAN_GOALS {
        return Err(anyhow!(
            "bounded proposal rollout: goal count exceeds {MAX_PROPOSAL_PLAN_GOALS}"
        ));
    }
    if options.task_costs.len() > MAX_PROPOSAL_PLAN_TASK_COSTS {
        return Err(anyhow!(
            "bounded proposal rollout: task-cost count exceeds {MAX_PROPOSAL_PLAN_TASK_COSTS}"
        ));
    }
    if options.competency_questions.len() > MAX_COMPETENCY_QUESTIONS {
        return Err(anyhow!(
            "bounded proposal rollout: competency-question count exceeds {MAX_COMPETENCY_QUESTIONS}"
        ));
    }
    let text_bytes = options
        .goals
        .iter()
        .map(String::len)
        .chain(options.task_costs.iter().map(|cost| {
            cost.name
                .len()
                .saturating_add(cost.unit.len())
                .saturating_add(cost.notes.as_deref().map(str::len).unwrap_or(0))
        }))
        .try_fold(0_usize, |total, length| total.checked_add(length))
        .ok_or_else(|| anyhow!("bounded proposal rollout: text byte count overflow"))?;
    if text_bytes > MAX_PROPOSAL_PLAN_TEXT_BYTES {
        return Err(anyhow!(
            "bounded proposal rollout: goal/task text exceeds {MAX_PROPOSAL_PLAN_TEXT_BYTES} bytes"
        ));
    }
    for task in &options.task_costs {
        for (label, value) in [("value", task.value), ("weight", task.weight)] {
            if !value.is_finite() || !(0.0..=MAX_PROPOSAL_COST_COMPONENT).contains(&value) {
                return Err(anyhow!(
                    "bounded proposal rollout: task-cost {label} must be finite and in 0..={MAX_PROPOSAL_COST_COMPONENT}"
                ));
            }
        }
    }
    for value in [
        options.guardrail_weights.quality_error,
        options.guardrail_weights.quality_warning,
        options.guardrail_weights.quality_info,
        options.guardrail_weights.axi_fact_error,
        options.guardrail_weights.rewrite_rule_error,
        options.guardrail_weights.context_error,
        options.guardrail_weights.modal_error,
    ] {
        if !value.is_finite() || !(0.0..=MAX_PROPOSAL_COST_COMPONENT).contains(&value) {
            return Err(anyhow!(
                "bounded proposal rollout: guardrail weights must be finite and in 0..={MAX_PROPOSAL_COST_COMPONENT}"
            ));
        }
    }

    let mut planning_db = clone_db(db)?;
    let mut steps: Vec<BoundedProposalPlanStepV1> = Vec::new();
    let task_cost_total: f64 = options.task_costs.iter().map(|t| t.value * t.weight).sum();
    if !task_cost_total.is_finite() {
        return Err(anyhow!(
            "bounded proposal rollout: aggregate task cost is not finite"
        ));
    }
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

        type BestRollout = (
            ProposalAdapterRunId,
            ProposalsFileV1,
            GuardrailCostReportV1,
            Option<CompetencyCoverageSummaryV1>,
            bool,
            usize,
            f64,
            Vec<String>,
        );
        let mut best: Option<BestRollout> = None;

        for rollout in 0..options.rollouts {
            let mut input = base_input.clone();
            if options.include_guardrail && options.guardrail_profile != "off" {
                input.set_guardrail_layer(guardrail_before.clone());
            }
            input.notes.push(format!(
                "source=proposal_rollout_plan step={step} rollout={rollout}"
            ));

            let proposal_options = PredictiveProposalOptionsV1 {
                max_new_proposals: options.max_new_proposals,
                seed: options
                    .seed
                    .map(|s| s.wrapping_add((step as u64) * 1_000 + rollout as u64)),
                goals: options.goals.clone(),
                task_costs: options.task_costs.clone(),
                horizon_steps: Some(options.horizon_steps),
                ..Default::default()
            };

            let req = make_predictive_proposal_request(input, proposal_options);
            let mut response = predictive_proposal.propose(&req)?;
            if let Some(err) = response.error.take() {
                return Err(anyhow!("predictive proposal adapter error: {err}"));
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

            let provenance = build_predictive_proposal_provenance(
                &response,
                predictive_proposal.backend_label(),
                predictive_proposal.model.clone(),
                base_input.revision_digest_v2.clone(),
                base_input.materialization_id(),
                base_input.accepted_snapshot_id(),
                Some(guardrail_before.summary.total_cost),
                guardrail_profile_label,
                guardrail_plane_label,
            )?;

            let mut proposals =
                apply_predictive_proposal_provenance(response.proposals, &provenance);
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
        ) =
            best.ok_or_else(|| anyhow!("bounded proposal rollout: no rollout produced proposals"))?;

        apply_proposals_to_db(&mut planning_db, &proposals)?;

        let competency_cost = competency_after.as_ref().map(|c| c.cost).unwrap_or(0.0);
        let competency_delta = match (competency_before.as_ref(), competency_after.as_ref()) {
            (Some(before), Some(after)) => Some(after.coverage - before.coverage),
            _ => None,
        };
        let step_report = BoundedProposalPlanStepV1 {
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

    Ok(BoundedProposalPlanReportV1 {
        version: "proposal_rollout_plan_v1".to_string(),
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
    fn competency_question_text_loads_typed_records() {
        let path = unique_temp_file("competency_questions").with_extension("cq");
        crate::security::write_output_bounded(&path, r#"
        version competency_question_bundle_v1

        question shipment_release:
          ask: Shipment release should be traceable.
          expect: exists RegulatedLine.ShipmentFulfills(shipment=?s, order=?o, work_order=?wo, ctx=?c, time=?t)
          min_rows: 1
          weight: 2.5
          contexts: Accepted, Released
        "#, "CLI output")
        .expect("write cq");

        let questions = load_competency_questions(&path).expect("load cq");
        fs::remove_file(&path).ok();

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "shipment_release");
        assert_eq!(
            questions[0].question.as_deref(),
            Some("Shipment release should be traceable.")
        );
        assert_eq!(
            questions[0].query,
            "select ?f where ?f = RegulatedLine.ShipmentFulfills(shipment=?s, order=?o, work_order=?wo, ctx=?c, time=?t) limit 1"
        );
        assert_eq!(
            questions[0]
                .authoring
                .as_ref()
                .expect("authoring hints")
                .expect,
            vec![
                "exists RegulatedLine.ShipmentFulfills(shipment=?s, order=?o, work_order=?wo, ctx=?c, time=?t)"
                    .to_string()
            ]
        );
        assert_eq!(questions[0].min_rows, 1);
        assert_eq!(questions[0].weight, 2.5);
        assert_eq!(
            questions[0].contexts,
            vec!["Accepted".to_string(), "Released".to_string()]
        );
    }

    #[test]
    fn competency_question_text_allows_unlowered_authored_questions() {
        let questions = parse_competency_question_text(
            r#"
version competency_question_bundle_v1
question missing_lowering:
  ask: Which shipment release rule applies when the ERP hold is active?
  about: shipment release
  given: erp hold is active
  expect: a typed shipment eligibility obligation exists
"#,
        )
        .expect("authored CQ should load even before it has an executable lowering");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "missing_lowering");
        assert!(questions[0].query.is_empty());
        assert!(questions[0].authoring.is_some());
    }

    #[test]
    fn training_export_masks_fields() {
        let axi = r#"
module M
schema S:
  object A
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;
        let opts = MaskedTupleTrainingExportOptionsV1 {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: Vec::new(),
        };
        let export = build_training_export_from_axi_text(axi, &opts).expect("export");
        assert!(!export.items.is_empty());
        for item in &export.items {
            assert_eq!(item.mask_fields.len(), 1);
        }
        let proof = &export.axi_well_typed_proof_v1;
        assert_eq!(proof.module_name, "M");
        assert_eq!(proof.schema_count, 1);
        assert_eq!(proof.instance_count, 1);
    }

    #[test]
    fn training_export_can_exclude_relations() {
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
        let opts = MaskedTupleTrainingExportOptionsV1 {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: vec!["interned_string".to_string()],
        };
        let export = build_training_export_from_axi_text(axi, &opts).expect("export");
        assert!(!export.items.is_empty());
        for item in &export.items {
            assert_ne!(item.relation, "interned_string");
        }
    }

    #[test]
    fn read_training_export_requires_well_typed_proof() {
        let axi = r#"
module Current
schema S:
  object A
  relation R(from: A, to: A)
instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;
        let opts = MaskedTupleTrainingExportOptionsV1 {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
            exclude_relations: Vec::new(),
        };
        let export = build_training_export_from_axi_text(axi, &opts).expect("export");
        let mut json = serde_json::to_value(&export).expect("serialize export");
        let removed = json
            .as_object_mut()
            .expect("export object")
            .remove("axi_well_typed_proof_v1");
        assert!(removed.is_some(), "expected serialized proof field");

        let path = unique_temp_file("training_export_missing_proof");
        crate::security::write_output_bounded(
            &path,
            serde_json::to_string_pretty(&json).expect("serialize malformed export"),
            "CLI output",
        )
        .expect("write malformed export");

        let err = read_training_export(&path).expect_err("missing proof should fail closed");
        let _ = fs::remove_file(&path);
        assert!(err.to_string().contains("axi_well_typed_proof_v1"));
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

        let prov = ProposalAdapterProvenance {
            trace_id: ProposalAdapterRunId::new("proposal::trace"),
            run_id: ProposalAdapterRunId::new("proposal::run"),
            backend: "stub".to_string(),
            model: Some("model".to_string()),
            revision_digest_v2: Some(AxiDigest::new(
                "axi:revision:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )),
            materialization_id: Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-test-materialization-snapshot",
            ])),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:snap")),
            proposals_digest: Some(ProposalDigest::new(
                axiograph_kernel::ProposalIdV2::from_canonical_fields(&[b"proposals"])
                    .to_string(),
            )),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
        };

        proposals = apply_predictive_proposal_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        assert!(meta.confidence <= 1.0);
        assert!(meta
            .metadata
            .contains_key("axiograph_predictive_proposal_trace_id"));
        assert_eq!(
            meta.metadata
                .get("axiograph_proposal_adapter_run_id")
                .map(String::as_str),
            Some("proposal::run")
        );
        assert!(meta
            .metadata
            .contains_key("axiograph_predictive_proposal_backend"));
        assert!(meta
            .metadata
            .contains_key("axiograph_predictive_proposal_model"));
        assert!(meta.metadata.contains_key("axiograph_revision_digest_v2"));
        assert_eq!(
            meta.metadata
                .get("axiograph_materialization_id")
                .map(String::as_str),
            prov.materialization_id
                .as_ref()
                .map(MaterializationIdV2::as_str)
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
            prov.proposals_digest.as_ref().map(ProposalDigest::as_str)
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
                            "axiograph_predictive_proposal_trace_id".to_string(),
                            "spoofed-trace".to_string(),
                        ),
                        (
                            "axiograph_proposal_adapter_run_id".to_string(),
                            "spoofed-run".to_string(),
                        ),
                        (
                            "axiograph_revision_digest_v2".to_string(),
                            "axi:revision:v2:sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                        ),
                        (
                            "axiograph_materialization_id".to_string(),
                            MaterializationIdV2::from_canonical_fields(&[
                                b"spoofed-materialization",
                            ])
                            .to_string(),
                        ),
                        (
                            "axiograph_accepted_snapshot_id".to_string(),
                            "accepted:spoofed".to_string(),
                        ),
                        (
                            "axiograph_predictive_proposal_model".to_string(),
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

        let prov = ProposalAdapterProvenance {
            trace_id: ProposalAdapterRunId::new("proposal::authoritative"),
            run_id: ProposalAdapterRunId::new("proposal::authoritative-run"),
            backend: "stub".to_string(),
            model: None,
            revision_digest_v2: Some(AxiDigest::new(
                "axi:revision:v2:sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            )),
            materialization_id: Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-test-materialization-7",
            ])),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:8")),
            proposals_digest: None,
            guardrail_total_cost: None,
            guardrail_profile: None,
            guardrail_plane: None,
        };

        proposals = apply_predictive_proposal_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        assert_eq!(
            meta.metadata
                .get("axiograph_predictive_proposal_trace_id")
                .map(String::as_str),
            Some("proposal::authoritative")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_proposal_adapter_run_id")
                .map(String::as_str),
            Some("proposal::authoritative-run")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_revision_digest_v2")
                .map(String::as_str),
            Some("axi:revision:v2:sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_materialization_id")
                .map(String::as_str),
            prov.materialization_id
                .as_ref()
                .map(MaterializationIdV2::as_str)
        );
        assert_eq!(
            meta.metadata
                .get("axiograph_accepted_snapshot_id")
                .map(String::as_str),
            Some("accepted:8")
        );
        assert!(
            !meta
                .metadata
                .contains_key("axiograph_predictive_proposal_model"),
            "runtime-owned optional keys should be cleared when provenance omits them"
        );
    }

    #[test]
    fn extracted_predictive_proposal_lineage_rehydrates_typed_ids_from_metadata() {
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

        let prov = ProposalAdapterProvenance {
            trace_id: ProposalAdapterRunId::new("proposal::typed"),
            run_id: ProposalAdapterRunId::new("proposal::typed-run"),
            backend: "plugin".to_string(),
            model: Some("deterministic".to_string()),
            revision_digest_v2: Some(AxiDigest::new(
                "axi:revision:v2:sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            )),
            materialization_id: Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-test-materialization-55",
            ])),
            accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:21")),
            proposals_digest: Some(ProposalDigest::new(
                axiograph_kernel::ProposalIdV2::from_canonical_fields(&[b"proposals-typed"])
                    .to_string(),
            )),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
        };

        proposals = apply_predictive_proposal_provenance(proposals, &prov);
        let meta = match &proposals.proposals[0] {
            ProposalV1::Entity { meta, .. } => meta,
            _ => panic!("unexpected proposal kind"),
        };
        let lineage = extract_predictive_proposal_proposal_lineage(meta);

        assert_eq!(
            lineage.trace_id.as_ref().map(|id| id.as_str()),
            Some("proposal::typed")
        );
        assert_eq!(
            lineage.run_id.as_ref().map(|id| id.as_str()),
            Some("proposal::typed-run")
        );
        assert_eq!(lineage.backend.as_deref(), Some("plugin"));
        assert_eq!(lineage.model.as_deref(), Some("deterministic"));
        assert_eq!(
            lineage.revision_digest_v2.as_ref().map(|id| id.as_str()),
            Some("axi:revision:v2:sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
        );
        assert_eq!(
            lineage.materialization_id.as_ref().map(|id| id.as_str()),
            prov.materialization_id
                .as_ref()
                .map(MaterializationIdV2::as_str)
        );
        assert_eq!(
            lineage.accepted_snapshot_id.as_ref().map(|id| id.as_str()),
            Some("accepted:21")
        );
        assert_eq!(
            lineage.proposals_digest.as_ref().map(|id| id.as_str()),
            prov.proposals_digest.as_ref().map(ProposalDigest::as_str)
        );
        assert_eq!(lineage.guardrail_total_cost, Some(1.25));
        assert_eq!(lineage.guardrail_profile.as_deref(), Some("fast"));
        assert_eq!(lineage.guardrail_plane.as_deref(), Some("both"));
    }

    #[test]
    fn predictive_proposal_request_round_trips_typed_ids_as_string_json() {
        let revision = format!("axi:revision:v2:sha256:{}", "0".repeat(64));
        let materialization_id =
            MaterializationIdV2::from_canonical_fields(&[b"proposal-test-materialization-42"]);
        let req = PredictiveProposalRequestV1 {
            protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
            trace_id: ProposalAdapterRunId::new("proposal::123"),
            generated_at_unix_secs: 123,
            input: PredictiveProposalInputV1 {
                revision_digest_v2: Some(AxiDigest::new(revision.clone())),
                axi_module_text: Some("module Demo\n".to_string()),
                semantic_input: PredictiveProposalSemanticInputV1 {
                    kind: PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1.to_string(),
                    module_name: Some("Demo".to_string()),
                    materialization_id: Some(materialization_id.clone()),
                    accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:9")),
                    layers: Vec::new(),
                },
                notes: vec!["typed".to_string()],
            },
            options: PredictiveProposalOptionsV1::default(),
        };

        let json = serde_json::to_value(&req).expect("serialize request");
        assert_eq!(json["trace_id"], "proposal::123");
        assert_eq!(json["input"]["revision_digest_v2"], revision);
        assert_eq!(
            json["input"]["semantic_input"]["materialization_id"],
            materialization_id.as_str()
        );
        assert_eq!(
            json["input"]["semantic_input"]["accepted_snapshot_id"],
            "accepted:9"
        );

        let round_trip: PredictiveProposalRequestV1 =
            serde_json::from_value(json).expect("deserialize request");
        assert_eq!(round_trip.trace_id.as_str(), "proposal::123");
        assert_eq!(
            round_trip
                .input
                .revision_digest_v2
                .as_ref()
                .map(AxiDigest::as_str),
            Some(revision.as_str())
        );
        assert_eq!(
            round_trip
                .input
                .semantic_input
                .materialization_id
                .as_ref()
                .map(MaterializationIdV2::as_str),
            Some(materialization_id.as_str())
        );
        assert_eq!(
            round_trip
                .input
                .semantic_input
                .accepted_snapshot_id
                .as_ref()
                .map(AcceptedSnapshotId::as_str),
            Some("accepted:9")
        );
    }

    #[test]
    fn predictive_proposal_request_validation_rejects_missing_digest_anchor() {
        let canonical = "module Demo\nschema S:\n  object A\n";
        let req = PredictiveProposalRequestV1 {
            protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
            trace_id: ProposalAdapterRunId::new("proposal::missing-digest"),
            generated_at_unix_secs: 1,
            input: PredictiveProposalInputV1 {
                revision_digest_v2: None,
                axi_module_text: Some(canonical.to_string()),
                semantic_input: PredictiveProposalSemanticInputV1 {
                    kind: PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1.to_string(),
                    module_name: Some("Demo".to_string()),
                    materialization_id: None,
                    accepted_snapshot_id: None,
                    layers: Vec::new(),
                },
                notes: Vec::new(),
            },
            options: PredictiveProposalOptionsV1::default(),
        };

        let err = validate_predictive_proposal_request(&req)
            .expect_err("missing digest anchor must fail");
        assert!(err
            .to_string()
            .contains("requires `revision_digest_v2` anchored to the canonical `.axi` input"));
    }

    fn bounded_plan_options() -> BoundedProposalPlanOptionsV1 {
        BoundedProposalPlanOptionsV1 {
            horizon_steps: 1,
            rollouts: 1,
            max_new_proposals: 1,
            seed: None,
            goals: Vec::new(),
            task_costs: Vec::new(),
            competency_questions: Vec::new(),
            guardrail_profile: "off".to_string(),
            guardrail_plane: "both".to_string(),
            guardrail_weights: GuardrailCostWeightsV1::defaults(),
            include_guardrail: false,
            validation_profile: "fast".to_string(),
            validation_plane: "both".to_string(),
        }
    }

    #[test]
    fn proposal_rollout_rejects_denial_of_service_bounds_before_adapter_execution() {
        let db = PathDB::new();
        let adapter = ProposalAdapterState::default();
        let input = PredictiveProposalInputV1 {
            revision_digest_v2: None,
            axi_module_text: None,
            semantic_input: PredictiveProposalSemanticInputV1::default(),
            notes: Vec::new(),
        };

        let mut options = bounded_plan_options();
        options.horizon_steps = MAX_PROPOSAL_ROLLOUT_HORIZON + 1;
        assert!(run_proposal_rollout_plan(&db, &adapter, &input, &options)
            .unwrap_err()
            .to_string()
            .contains("horizon_steps"));

        let mut options = bounded_plan_options();
        options.horizon_steps = 16;
        options.rollouts = 16;
        assert!(run_proposal_rollout_plan(&db, &adapter, &input, &options)
            .unwrap_err()
            .to_string()
            .contains("adapter attempts"));

        let mut options = bounded_plan_options();
        options.task_costs.push(ProposalTaskCostV1 {
            name: "infinite".to_string(),
            value: f64::INFINITY,
            weight: 1.0,
            unit: String::new(),
            notes: None,
        });
        assert!(run_proposal_rollout_plan(&db, &adapter, &input, &options)
            .unwrap_err()
            .to_string()
            .contains("must be finite"));
    }

    #[test]
    fn predictive_proposal_response_and_plan_report_round_trip_typed_ids_as_strings() {
        let response = PredictiveProposalResponseV1 {
            protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
            trace_id: ProposalAdapterRunId::new("proposal::response"),
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
        assert_eq!(response_json["trace_id"], "proposal::response");
        let response_round_trip: PredictiveProposalResponseV1 =
            serde_json::from_value(response_json).expect("deserialize response");
        assert_eq!(response_round_trip.trace_id.as_str(), "proposal::response");

        let plan = BoundedProposalPlanReportV1 {
            version: "proposal_rollout_plan_v1".to_string(),
            trace_id: ProposalAdapterRunId::new("proposal::plan"),
            generated_at_unix_secs: 77,
            horizon_steps: 1,
            rollouts: 2,
            max_new_proposals: 3,
            guardrail_profile: "fast".to_string(),
            guardrail_plane: "both".to_string(),
            guardrail_weights: GuardrailCostWeightsV1::defaults(),
            task_costs: vec![ProposalTaskCostV1 {
                name: "completion".to_string(),
                value: 1.0,
                weight: 2.0,
                unit: "count".to_string(),
                notes: Some("unit".to_string()),
            }],
            task_cost_total: 2.0,
            competency_questions: Vec::new(),
            steps: vec![BoundedProposalPlanStepV1 {
                step: 0,
                trace_id: ProposalAdapterRunId::new("proposal::plan"),
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
        assert_eq!(plan_json["trace_id"], "proposal::plan");
        assert_eq!(plan_json["steps"][0]["trace_id"], "proposal::plan");
        let plan_round_trip: BoundedProposalPlanReportV1 =
            serde_json::from_value(plan_json).expect("deserialize plan");
        assert_eq!(plan_round_trip.trace_id.as_str(), "proposal::plan");
        assert_eq!(plan_round_trip.steps[0].trace_id.as_str(), "proposal::plan");
    }

    #[test]
    fn build_predictive_proposal_provenance_computes_typed_proposal_digest_and_keeps_run_id_distinct(
    ) {
        let base_response = PredictiveProposalResponseV1 {
            protocol: PREDICTIVE_PROPOSAL_PROTOCOL_V1.to_string(),
            trace_id: ProposalAdapterRunId::new("proposal::digest"),
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

        let provenance = build_predictive_proposal_provenance(
            &base_response,
            "stub".to_string(),
            Some("model".to_string()),
            Some(AxiDigest::new(
                "axi:revision:v2:sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            )),
            Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-digest-materialization-1",
            ])),
            Some(AcceptedSnapshotId::new("accepted:1")),
            Some(1.25),
            Some("fast".to_string()),
            Some("both".to_string()),
        )
        .expect("build provenance");

        assert_eq!(provenance.trace_id.as_str(), "proposal::digest");
        assert_eq!(provenance.run_id.as_str(), "proposal::digest");
        let digest = provenance
            .proposals_digest
            .as_ref()
            .expect("proposal-set digest should be present");
        assert!(digest.as_str().starts_with("axi:object-blob:v2:sha256:"));

        let mut changed = base_response.clone();
        if let ProposalV1::Entity { name, .. } = &mut changed.proposals.proposals[0] {
            *name = "thing-2".to_string();
        }
        let changed_provenance = build_predictive_proposal_provenance(
            &changed,
            "stub".to_string(),
            Some("model".to_string()),
            Some(AxiDigest::new(
                "axi:revision:v2:sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            )),
            Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-digest-materialization-1",
            ])),
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
}
