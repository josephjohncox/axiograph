use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

pub const BEHAVIOR_CASE_REPORT_VERSION_V1: &str = "behavior_case_report_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct BehaviorCaseId(String);

impl BehaviorCaseId {
    #[allow(dead_code)]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BehaviorCaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorGivenV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorStimulusKindV1 {
    Query,
    Command,
    Event,
    ProposalDelta,
    ReviewOnly,
}

impl Default for BehaviorStimulusKindV1 {
    fn default() -> Self {
        Self::Query
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorWhenV1 {
    #[serde(default)]
    pub kind: BehaviorStimulusKindV1,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<crate::world_model::CompetencyQuestionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorThenV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_outcomes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_scopes: Vec<crate::semantic_claim::RuntimeRuleScopeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_target: Option<crate::semantic_claim::SemanticClaimStrengthV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorCaseV1 {
    pub case_id: BehaviorCaseId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given: Option<BehaviorGivenV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<BehaviorWhenV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<BehaviorThenV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorCaseCodegenLanguageV1 {
    Go,
    Python,
    Rust,
    Typescript,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorCaseCodegenRequestV1 {
    #[serde(default = "default_codegen_languages")]
    pub languages: Vec<BehaviorCaseCodegenLanguageV1>,
}

impl Default for BehaviorCaseCodegenRequestV1 {
    fn default() -> Self {
        Self {
            languages: default_codegen_languages(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorCaseCheckRequestV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub behavior_case: BehaviorCaseV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
    #[serde(default)]
    pub codegen: BehaviorCaseCodegenRequestV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorCaseCodegenPreviewV1 {
    pub language: BehaviorCaseCodegenLanguageV1,
    pub file_hint: String,
    pub test_name: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseReceiptV1 {
    pub receipt_id: String,
    pub case_id: BehaviorCaseId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    pub trust_class: crate::semantic_claim::RuntimeRuleTrustClassV1,
    pub strength: crate::semantic_claim::SemanticClaimStrengthV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_scope_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_rule_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub competency_total: usize,
    #[serde(default)]
    pub competency_satisfied: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    pub support_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorCaseReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub behavior_case: BehaviorCaseV1,
    pub semantic_slice_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
    pub context_report: crate::context_report::ContextReportV1,
    pub receipt: CaseReceiptV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codegen_previews: Vec<BehaviorCaseCodegenPreviewV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_unknowns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub fn semantic_slice_selector_for_behavior_case(
    behavior_case: &BehaviorCaseV1,
    overlay: Option<&axiograph_tooling_overlays::ToolingOverlayBundleV1>,
) -> Result<crate::semantic_merge_lattice::SemanticSliceSelectorV1> {
    validate_behavior_case(behavior_case)?;
    let overlay = overlay
        .ok_or_else(|| anyhow!("BehaviorCaseV1 semantic slicing requires a tooling overlay"))?;
    let enriched_context = bounded_context_from_overlay(behavior_case, overlay)?;
    let mut selector =
        crate::context_report::semantic_slice_selector_for_bounded_context(&enriched_context)?;
    selector
        .behavior_case_ids
        .push(behavior_case.case_id.to_string());
    selector.behavior_case_ids.sort();
    selector.behavior_case_ids.dedup();
    selector.label = Some(behavior_case.case_id.to_string());
    Ok(selector)
}

pub fn build_behavior_case_report_from_request(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    request: BehaviorCaseCheckRequestV1,
) -> Result<BehaviorCaseReportV1> {
    let runtime_theory_check = resolve_runtime_theory_check_summary(
        request.runtime_theory_check,
        request.runtime_theory_check_input,
    )?;
    build_behavior_case_report(
        db,
        meta,
        accepted_snapshot_id,
        request.lifecycle_state.as_deref(),
        &request.behavior_case,
        request.overlay.as_ref(),
        request.evolution_preview,
        runtime_theory_check,
        &request.codegen,
    )
}

#[cfg(test)]
pub fn discover_behavior_case_report_from_request_json(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    request_json: &str,
) -> Result<BehaviorCaseReportV1> {
    let request: BehaviorCaseCheckRequestV1 = serde_json::from_str(request_json)
        .map_err(|err| anyhow!("failed to parse behavior case request JSON: {err}"))?;
    build_behavior_case_report_from_request(db, meta, accepted_snapshot_id, request)
}

pub fn build_behavior_case_report(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: Option<&str>,
    behavior_case: &BehaviorCaseV1,
    overlay: Option<&axiograph_tooling_overlays::ToolingOverlayBundleV1>,
    evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    codegen: &BehaviorCaseCodegenRequestV1,
) -> Result<BehaviorCaseReportV1> {
    validate_behavior_case(behavior_case)?;
    let overlay = overlay.ok_or_else(|| {
        anyhow!(
            "BehaviorCaseV1 now requires a tooling overlay; move DDD/fDDD context, implementation surfaces, and coverage edges out of the behavior case JSON"
        )
    })?;
    let enriched_context = bounded_context_from_overlay(behavior_case, overlay)?;
    let context_report = crate::context_report::build_context_report(
        db,
        meta,
        accepted_snapshot_id.clone(),
        lifecycle_state,
        &enriched_context,
        evolution_preview,
        runtime_theory_check.clone(),
    )?;
    let receipt = build_case_receipt(behavior_case, &context_report);
    let semantic_slice_selector =
        semantic_slice_selector_for_behavior_case(behavior_case, Some(overlay))?;
    let codegen_previews =
        build_codegen_previews(behavior_case, &context_report, &receipt, codegen);

    let mut residual_unknowns = context_report
        .residual_unknowns
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if behavior_case.then.is_none() {
        residual_unknowns.insert(
            "behavior case has no typed `then` obligations; expected outcomes remain prose-only"
                .to_string(),
        );
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            residual_unknowns.insert(format!(
                "runtime theory check has {} blocking error(s)",
                summary.blocking_errors
            ));
        }
        residual_unknowns.extend(summary.residual_obligation_ids.iter().map(|id| {
            format!("behavior case is blocked from strong use by residual theory obligation `{id}`")
        }));
    }

    let mut next_actions = context_report
        .next_actions
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if behavior_case.then.is_none() {
        next_actions.insert(
            "add typed `then.rule_scopes` or `then.competency_questions` before relying on this case as a promotion gate"
                .to_string(),
        );
    }
    if codegen_previews.is_empty() {
        next_actions.insert(
            "request Rust or TypeScript codegen previews to bind this behavior case to executable tests"
                .to_string(),
        );
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            next_actions.insert(
                "resolve runtime theory checker blocking judgments before using this behavior case as a gate".to_string(),
            );
        }
    }

    let mut notes = behavior_case.notes.iter().cloned().collect::<BTreeSet<_>>();
    notes.extend(context_report.notes.iter().cloned());
    notes.insert(
        "BehaviorCaseV1 is JSON-only in this tranche; Gherkin/BDD syntax should import/export this same typed payload later"
            .to_string(),
    );
    notes.insert(
        "generated test skeletons are binding previews, not proof objects or completeness claims"
            .to_string(),
    );
    if runtime_theory_check.is_some() {
        notes.insert(
            "behavior case carries a runtime theory-check sidecar, so BDD/fDDD assertions are scoped by typed theory closure assumptions"
                .to_string(),
        );
    }

    Ok(BehaviorCaseReportV1 {
        version: BEHAVIOR_CASE_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        behavior_case: behavior_case.clone(),
        semantic_slice_selector,
        context_report,
        receipt,
        runtime_theory_check,
        codegen_previews,
        residual_unknowns: residual_unknowns.into_iter().collect(),
        next_actions: next_actions.into_iter().collect(),
        notes: notes.into_iter().collect(),
    })
}

fn resolve_runtime_theory_check_summary(
    provided: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
) -> Result<Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>> {
    if let Some(summary) = provided {
        return Ok(Some(summary));
    }
    input
        .as_ref()
        .map(crate::runtime_theory_check::runtime_theory_check_summary_from_input)
        .transpose()
}

fn validate_behavior_case(behavior_case: &BehaviorCaseV1) -> Result<()> {
    if behavior_case.case_id.as_str().trim().is_empty() {
        return Err(anyhow!("behavior case requires a non-empty `case_id`"));
    }
    if behavior_case.title.trim().is_empty() {
        return Err(anyhow!(
            "behavior case `{}` requires a non-empty `title`",
            behavior_case.case_id
        ));
    }
    if let Some(when) = behavior_case.when.as_ref() {
        if when.label.trim().is_empty() {
            return Err(anyhow!(
                "behavior case `{}` has a `when` step with an empty `label`",
                behavior_case.case_id
            ));
        }
    }
    Ok(())
}

fn bounded_context_from_overlay(
    behavior_case: &BehaviorCaseV1,
    overlay: &axiograph_tooling_overlays::ToolingOverlayBundleV1,
) -> Result<crate::context_report::BoundedContextV1> {
    let fddd = overlay.fddd_context_map.as_ref().ok_or_else(|| {
        anyhow!("tooling overlay requires `fddd_context_map` for behavior-case planning")
    })?;
    let mut context = crate::context_report::BoundedContextV1 {
        context_id: crate::context_report::DomainContextId::new(fddd.context_id.clone()),
        label: fddd.label.clone(),
        summary: fddd.summary.clone(),
        scopes: fddd
            .scopes
            .iter()
            .filter_map(scope_from_overlay_ref)
            .collect(),
        surfaces: overlay
            .implementation_surfaces
            .surfaces
            .iter()
            .map(surface_from_overlay)
            .collect(),
        edges: overlay
            .implementation_surfaces
            .coverage_edges
            .iter()
            .map(edge_from_overlay)
            .collect(),
        competency_questions: Vec::new(),
        notes: fddd.notes.clone(),
    };
    if let Some(when) = behavior_case
        .when
        .as_ref()
        .and_then(|when| when.query.clone())
    {
        push_competency_question_if_new(&mut context.competency_questions, when);
    }
    if let Some(then) = behavior_case.then.as_ref() {
        extend_scopes(&mut context.scopes, &then.rule_scopes);
        for question in then.competency_questions.iter().cloned() {
            push_competency_question_if_new(&mut context.competency_questions, question);
        }
    }
    Ok(context)
}

fn surface_from_overlay(
    surface: &axiograph_tooling_overlays::ImplementationSurfaceOverlayV1,
) -> crate::semantic_claim::ImplementationSurfaceRefV1 {
    crate::semantic_claim::ImplementationSurfaceRefV1 {
        surface_id: surface.surface_id.clone(),
        kind: match surface.kind {
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Endpoint => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Endpoint
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Workflow => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Workflow
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Report => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Report
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Job => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Job
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Migration => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Migration
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Config => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Config
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::DocSection => {
                crate::semantic_claim::ImplementationSurfaceKindV1::DocSection
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::AgentTask => {
                crate::semantic_claim::ImplementationSurfaceKindV1::AgentTask
            }
            axiograph_tooling_overlays::ImplementationSurfaceKindV1::Ui
            | axiograph_tooling_overlays::ImplementationSurfaceKindV1::Plc
            | axiograph_tooling_overlays::ImplementationSurfaceKindV1::Simulator
            | axiograph_tooling_overlays::ImplementationSurfaceKindV1::Unknown => {
                crate::semantic_claim::ImplementationSurfaceKindV1::Workflow
            }
        },
        label: surface.label.clone(),
        scopes: surface
            .ontology_refs
            .iter()
            .filter_map(scope_from_overlay_ref)
            .collect(),
        code_refs: surface.code_refs.clone(),
        notes: surface.notes.clone(),
    }
}

fn edge_from_overlay(
    edge: &axiograph_tooling_overlays::CoverageEdgeOverlayV1,
) -> crate::semantic_claim::CoverageEdgeV1 {
    crate::semantic_claim::CoverageEdgeV1 {
        surface_id: edge.surface_id.clone(),
        rule_id: edge.rule_id.clone(),
        status: match edge.status {
            axiograph_tooling_overlays::CoverageStatusV1::Tested => {
                crate::semantic_claim::CoverageStatusV1::Tested
            }
            axiograph_tooling_overlays::CoverageStatusV1::Implemented => {
                crate::semantic_claim::CoverageStatusV1::Implemented
            }
            axiograph_tooling_overlays::CoverageStatusV1::DocumentedOnly => {
                crate::semantic_claim::CoverageStatusV1::DocumentedOnly
            }
            axiograph_tooling_overlays::CoverageStatusV1::OntologyOnly => {
                crate::semantic_claim::CoverageStatusV1::OntologyOnly
            }
            axiograph_tooling_overlays::CoverageStatusV1::Drifted => {
                crate::semantic_claim::CoverageStatusV1::Drifted
            }
            axiograph_tooling_overlays::CoverageStatusV1::Unknown => {
                crate::semantic_claim::CoverageStatusV1::Unknown
            }
        },
        notes: edge.notes.clone(),
    }
}

fn scope_from_overlay_ref(
    reference: &axiograph_tooling_overlays::OverlayRefV1,
) -> Option<crate::semantic_claim::RuntimeRuleScopeV1> {
    let schema = reference.schema.as_deref()?;
    match reference.kind {
        axiograph_tooling_overlays::OverlayRefKindV1::Relation => reference
            .name
            .as_deref()
            .map(|relation| crate::semantic_claim::RuntimeRuleScopeV1::relation(schema, relation)),
        axiograph_tooling_overlays::OverlayRefKindV1::Theory => reference
            .name
            .as_deref()
            .map(|theory| crate::semantic_claim::RuntimeRuleScopeV1::theory(schema, theory)),
        _ => None,
    }
}

fn extend_scopes(
    target: &mut Vec<crate::semantic_claim::RuntimeRuleScopeV1>,
    extra: &[crate::semantic_claim::RuntimeRuleScopeV1],
) {
    let mut seen = target
        .iter()
        .filter_map(crate::semantic_claim::RuntimeRuleScopeV1::inferred_scope_id)
        .collect::<BTreeSet<_>>();
    for scope in extra {
        let normalized = scope.normalized();
        if let Some(scope_id) = normalized.inferred_scope_id() {
            if seen.insert(scope_id) {
                target.push(normalized);
            }
        } else {
            target.push(normalized);
        }
    }
}

fn push_competency_question_if_new(
    target: &mut Vec<crate::world_model::CompetencyQuestionV1>,
    question: crate::world_model::CompetencyQuestionV1,
) {
    if !target
        .iter()
        .any(|existing| existing.name == question.name && existing.query == question.query)
    {
        target.push(question);
    }
}

fn build_case_receipt(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
) -> CaseReceiptV1 {
    let competency_total = context_report
        .competency_coverage
        .as_ref()
        .map(|coverage| coverage.total)
        .unwrap_or_default();
    let competency_satisfied = context_report
        .competency_coverage
        .as_ref()
        .map(|coverage| coverage.satisfied)
        .unwrap_or_default();
    let anchor = context_report
        .accepted_snapshot_id
        .as_ref()
        .map(AcceptedSnapshotId::as_str)
        .unwrap_or("runtime");
    let receipt_id = format!(
        "case_receipt_v1:{}:{}:{}",
        behavior_case.case_id.as_str(),
        context_report.context.context_id.as_str(),
        anchor
    );
    let mut notes = BTreeSet::new();
    notes.insert(
        "receipt records runtime typing, CQ, rule, and coverage status for this behavior case under explicit anchors"
            .to_string(),
    );
    if context_report.strength != crate::semantic_claim::SemanticClaimStrengthV1::Strong {
        notes.insert(
            "receipt is not a strong claim; inspect residual_obligations and next_actions"
                .to_string(),
        );
    }

    CaseReceiptV1 {
        receipt_id,
        case_id: behavior_case.case_id.clone(),
        accepted_snapshot_id: context_report.accepted_snapshot_id.clone(),
        lifecycle_state: context_report.lifecycle_state.clone(),
        trust_class: context_report.trust_class,
        strength: context_report.strength,
        matched_scope_ids: context_report.matched_scope_ids.clone(),
        matched_rule_ids: context_report.matched_rule_ids.clone(),
        surface_ids: context_report.surface_ids.clone(),
        competency_total,
        competency_satisfied,
        residual_obligations: context_report.residual_unknowns.clone(),
        next_actions: context_report.next_actions.clone(),
        support_status: "context_report_only_no_query_witness_emitted".to_string(),
        notes: notes.into_iter().collect(),
    }
}

fn build_codegen_previews(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
    codegen: &BehaviorCaseCodegenRequestV1,
) -> Vec<BehaviorCaseCodegenPreviewV1> {
    let languages = if codegen.languages.is_empty() {
        default_codegen_languages()
    } else {
        let mut deduped = codegen
            .languages
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        deduped.sort();
        deduped
    };
    languages
        .into_iter()
        .map(|language| {
            build_codegen_preview_for_language(language, behavior_case, context_report, receipt)
        })
        .collect()
}

fn build_codegen_preview_for_language(
    language: BehaviorCaseCodegenLanguageV1,
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
) -> BehaviorCaseCodegenPreviewV1 {
    let test_name = sanitize_identifier(behavior_case.case_id.as_str());
    let anchors = receipt_anchors(receipt);
    match language {
        BehaviorCaseCodegenLanguageV1::Rust => {
            let content = rust_test_skeleton(behavior_case, context_report, receipt, &test_name);
            BehaviorCaseCodegenPreviewV1 {
                language,
                file_hint: format!("tests/behavior_cases/{test_name}.rs"),
                test_name,
                content,
                anchors,
                notes: vec![
                    "bind `case_receipt_json` to a real system-under-test assertion before accepting generated tests".to_string(),
                ],
            }
        }
        BehaviorCaseCodegenLanguageV1::Python => {
            let content = python_test_skeleton(behavior_case, context_report, receipt, &test_name);
            BehaviorCaseCodegenPreviewV1 {
                language,
                file_hint: format!("tests/behavior_cases/test_{test_name}.py"),
                test_name,
                content,
                anchors,
                notes: vec![
                    "generated pytest skeleton should be wired to the real application service and Axiograph receipt before acceptance".to_string(),
                ],
            }
        }
        BehaviorCaseCodegenLanguageV1::Go => {
            let content = go_test_skeleton(behavior_case, context_report, receipt, &test_name);
            BehaviorCaseCodegenPreviewV1 {
                language,
                file_hint: format!("internal/behaviorcases/{test_name}_test.go"),
                test_name,
                content,
                anchors,
                notes: vec![
                    "generated Go skeleton keeps Axiograph receipt metadata read-only; bind it to the domain service under test".to_string(),
                ],
            }
        }
        BehaviorCaseCodegenLanguageV1::Typescript => {
            let content =
                typescript_test_skeleton(behavior_case, context_report, receipt, &test_name);
            BehaviorCaseCodegenPreviewV1 {
                language,
                file_hint: format!("tests/behavior-cases/{test_name}.spec.ts"),
                test_name,
                content,
                anchors,
                notes: vec![
                    "generated Vitest skeleton is intentionally read-only over Axiograph receipt metadata".to_string(),
                ],
            }
        }
    }
}

fn rust_test_skeleton(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
    test_name: &str,
) -> String {
    let case_id = behavior_case.case_id.as_str();
    let context_id = context_report.context.context_id.as_str();
    let receipt_json = serde_json::to_string_pretty(receipt).unwrap_or_else(|_| "{}".to_string());
    format!(
        r##"#[test]
fn behavior_case_{test_name}() {{
    let case_id = "{case_id}";
    let context_id = "{context_id}";
    let case_receipt_json = r#"{receipt_json}"#;

    assert!(!case_id.is_empty());
    assert!(!context_id.is_empty());
    assert!(
        case_receipt_json.contains("\"receipt_id\""),
        "wire this BehaviorCaseV1 receipt to the real system under test"
    );
}}
"##
    )
}

fn python_test_skeleton(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
    test_name: &str,
) -> String {
    let case_id = behavior_case.case_id.as_str();
    let context_id = context_report.context.context_id.as_str();
    let receipt_json = serde_json::to_string_pretty(receipt).unwrap_or_else(|_| "{}".to_string());
    format!(
        r##"import json


def test_behavior_case_{test_name}():
    case_id = "{case_id}"
    context_id = "{context_id}"
    case_receipt = json.loads(r'''{receipt_json}''')

    assert case_id
    assert context_id
    assert case_receipt["case_id"] == case_id
    # Bind this receipt to the real application service and domain event assertions.
"##
    )
}

fn go_test_skeleton(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
    test_name: &str,
) -> String {
    let case_id = behavior_case.case_id.as_str();
    let context_id = context_report.context.context_id.as_str();
    let receipt_json = serde_json::to_string_pretty(receipt).unwrap_or_else(|_| "{}".to_string());
    let go_test_name = test_name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.push(first.to_ascii_uppercase());
                    out.extend(chars);
                    out
                }
                None => String::new(),
            }
        })
        .collect::<String>();
    format!(
        r##"package behaviorcases

import (
	"encoding/json"
	"testing"
)

func TestBehaviorCase{go_test_name}(t *testing.T) {{
	caseID := "{case_id}"
	contextID := "{context_id}"
	caseReceiptJSON := `{receipt_json}`

	if caseID == "" || contextID == "" {{
		t.Fatal("missing behavior-case identity")
	}}
	var caseReceipt map[string]any
	if err := json.Unmarshal([]byte(caseReceiptJSON), &caseReceipt); err != nil {{
		t.Fatalf("invalid Axiograph receipt: %v", err)
	}}
	if caseReceipt["case_id"] != caseID {{
		t.Fatalf("receipt case_id mismatch: %v", caseReceipt["case_id"])
	}}
	// Bind this receipt to the real application service and domain event assertions.
}}
"##
    )
}

fn typescript_test_skeleton(
    behavior_case: &BehaviorCaseV1,
    context_report: &crate::context_report::ContextReportV1,
    receipt: &CaseReceiptV1,
    test_name: &str,
) -> String {
    let case_id = behavior_case.case_id.as_str();
    let context_id = context_report.context.context_id.as_str();
    let receipt_json = serde_json::to_string_pretty(receipt).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"import {{ describe, expect, it }} from "vitest";

describe("BehaviorCaseV1:{case_id}", () => {{
  it("{test_name}", async () => {{
    const contextId = "{context_id}";
    const caseReceipt = {receipt_json};

    expect(contextId.length).toBeGreaterThan(0);
    expect(caseReceipt.receipt_id).toBeDefined();
    expect(caseReceipt.case_id).toBe("{case_id}");
  }});
}});
"#
    )
}

fn receipt_anchors(receipt: &CaseReceiptV1) -> Vec<String> {
    let mut anchors = BTreeSet::new();
    if let Some(snapshot) = receipt.accepted_snapshot_id.as_ref() {
        anchors.insert(snapshot.as_str().to_string());
    }
    anchors.extend(receipt.matched_scope_ids.iter().cloned());
    anchors.extend(receipt.matched_rule_ids.iter().cloned());
    anchors.extend(receipt.surface_ids.iter().cloned());
    anchors.into_iter().collect()
}

fn sanitize_identifier(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "behavior_case".to_string()
    } else if trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("case_{trimmed}")
    } else {
        trimmed
    }
}

fn default_codegen_languages() -> Vec<BehaviorCaseCodegenLanguageV1> {
    vec![
        BehaviorCaseCodegenLanguageV1::Rust,
        BehaviorCaseCodegenLanguageV1::Typescript,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db_and_meta() -> Result<(PathDB, MetaPlaneIndex)> {
        let axi = r#"
module Demo

schema Family:
  object Person
  relation Parent(child: Person, parent: Person)

theory FamilyRules on Family:
  constraint functional Parent.child -> Parent.parent

instance I of Family:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db)?;
        Ok((db, meta))
    }

    fn sample_behavior_case() -> BehaviorCaseV1 {
        let relation_scope =
            crate::semantic_claim::RuntimeRuleScopeV1::relation("Family", "Parent");
        BehaviorCaseV1 {
            case_id: BehaviorCaseId::new("family.parent_lookup"),
            title: "Parent lookup returns the accepted parent".to_string(),
            summary: Some("BDD/DDD case for the family lookup bounded context".to_string()),
            given: Some(BehaviorGivenV1 {
                snapshot_label: Some("accepted family snapshot".to_string()),
                world: Some("main".to_string()),
                facts: vec!["parent(child=Alice,parent=Bob)".to_string()],
                anchors: Vec::new(),
                notes: Vec::new(),
            }),
            when: Some(BehaviorWhenV1 {
                kind: BehaviorStimulusKindV1::Query,
                label: "lookup Alice's parent".to_string(),
                query: None,
                notes: Vec::new(),
            }),
            then: Some(BehaviorThenV1 {
                expected_outcomes: vec!["Alice's accepted parent is Bob".to_string()],
                competency_questions: vec![crate::world_model::CompetencyQuestionV1 {
                    name: "family_lookup_returns_bob".to_string(),
                    question: Some("Does the case return Bob as Alice's parent?".to_string()),
                    authoring: None,
                    query: "select ?f where ?f = Family.Parent(child=Alice, parent=Bob) limit 1"
                        .to_string(),
                    min_rows: 1,
                    weight: 1.0,
                    contexts: Vec::new(),
                }],
                rule_scopes: vec![relation_scope],
                trust_target: Some(crate::semantic_claim::SemanticClaimStrengthV1::Strong),
                notes: Vec::new(),
            }),
            notes: Vec::new(),
        }
    }

    fn sample_overlay() -> axiograph_tooling_overlays::ToolingOverlayBundleV1 {
        use axiograph_tooling_overlays::{
            CodegenPlanV1, CoverageEdgeOverlayV1, CoverageModeV1, CoveragePolicyV1,
            CoverageStatusV1, FdddContextMapV1, ImplementationSurfaceKindV1,
            ImplementationSurfaceManifestV1, ImplementationSurfaceOverlayV1, OverlayRefKindV1,
            OverlayRefV1, ToolingOverlayBundleV1, TOOLING_OVERLAY_BUNDLE_VERSION_V1,
        };

        let parent_ref = OverlayRefV1 {
            kind: OverlayRefKindV1::Relation,
            schema: Some("Family".to_string()),
            name: Some("Parent".to_string()),
            stable_id: None,
            role: None,
        };
        ToolingOverlayBundleV1 {
            version: TOOLING_OVERLAY_BUNDLE_VERSION_V1.to_string(),
            fddd_context_map: Some(FdddContextMapV1 {
                context_id: "domain:family_lookup".to_string(),
                label: "Family lookup".to_string(),
                summary: Some("Read model for family lookup".to_string()),
                scopes: vec![parent_ref.clone()],
                bounded_contexts: Vec::new(),
                aggregates: Vec::new(),
                functions: Vec::new(),
                processes: Vec::new(),
                business_rules: Vec::new(),
                notes: Vec::new(),
            }),
            implementation_surfaces: ImplementationSurfaceManifestV1 {
                surfaces: vec![ImplementationSurfaceOverlayV1 {
                    surface_id: "endpoint:family_lookup".to_string(),
                    kind: ImplementationSurfaceKindV1::Endpoint,
                    label: "GET /family/lookup".to_string(),
                    ontology_refs: vec![parent_ref],
                    code_refs: vec!["src/family.rs".to_string()],
                    test_refs: Vec::new(),
                    languages: vec!["rust".to_string()],
                    notes: Vec::new(),
                }],
                coverage_edges: vec![CoverageEdgeOverlayV1 {
                    surface_id: "endpoint:family_lookup".to_string(),
                    rule_id: "schema/family/relation/parent/rule/functional/0".to_string(),
                    status: CoverageStatusV1::Tested,
                    notes: Vec::new(),
                }],
            },
            coverage_policy: CoveragePolicyV1 {
                coverage_mode: CoverageModeV1::Advisory,
                ..CoveragePolicyV1::default()
            },
            codegen_plan: CodegenPlanV1 {
                languages: vec!["rust".to_string(), "typescript".to_string()],
                test_name: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }
    }

    fn sample_runtime_theory_summary() -> crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
        crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
            version: "runtime_theory_check_summary_v1".to_string(),
            report_version: "runtime_theory_check_report_v1".to_string(),
            module_digest: "fnv1a64:behavior-test".to_string(),
            theory_count: 1,
            checked_obligations: 1,
            review_only_obligations: 0,
            residual_obligations: 1,
            blocked_obligations: 0,
            excluded_by_evidence: 0,
            blocking_errors: 0,
            closure_tiers: vec!["finite_fragment".to_string()],
            closure_trace: Default::default(),
            transport_summary: Default::default(),
            completeness_claim: "not_claimed_for_all_obligations".to_string(),
            ontology_closure_claim: "not_claimed_for_all_obligations".to_string(),
            residual_obligation_ids: vec!["behavior/theory/residual".to_string()],
            notes: vec!["test runtime theory sidecar".to_string()],
        }
    }

    #[test]
    fn behavior_case_report_composes_context_trust_receipt_and_codegen() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let report = build_behavior_case_report(
            &db,
            Some(&meta),
            Some(AcceptedSnapshotId::new("accepted:family")),
            Some("accepted"),
            &sample_behavior_case(),
            Some(&sample_overlay()),
            None,
            None,
            &BehaviorCaseCodegenRequestV1::default(),
        )?;

        assert_eq!(report.version, BEHAVIOR_CASE_REPORT_VERSION_V1);
        assert_eq!(report.receipt.case_id.as_str(), "family.parent_lookup");
        assert!(matches!(
            report.receipt.strength,
            crate::semantic_claim::SemanticClaimStrengthV1::Strong
                | crate::semantic_claim::SemanticClaimStrengthV1::Weak
        ));
        assert_eq!(report.receipt.competency_total, 1);
        assert_eq!(report.receipt.competency_satisfied, 1);
        assert_eq!(report.context_report.coverage.tested_rules, 1);
        assert_eq!(
            report.semantic_slice_selector.behavior_case_ids,
            vec!["family.parent_lookup".to_string()]
        );
        assert_eq!(
            report.semantic_slice_selector.relation_object_ids,
            vec!["relation:Family:Parent".to_string()]
        );
        assert_eq!(
            report.semantic_slice_selector.competency_question_names,
            vec!["family_lookup_returns_bob".to_string()]
        );
        assert_eq!(report.codegen_previews.len(), 2);
        assert!(report
            .codegen_previews
            .iter()
            .any(|preview| preview.content.contains("#[test]")));
        assert!(report
            .codegen_previews
            .iter()
            .any(|preview| preview.content.contains("from \"vitest\"")));
        assert!(report.notes.iter().any(|note| note.contains("JSON-only")));
        Ok(())
    }

    #[test]
    fn behavior_case_report_threads_runtime_theory_summary() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let report = build_behavior_case_report(
            &db,
            Some(&meta),
            Some(AcceptedSnapshotId::new("accepted:family")),
            Some("accepted"),
            &sample_behavior_case(),
            Some(&sample_overlay()),
            None,
            Some(sample_runtime_theory_summary()),
            &BehaviorCaseCodegenRequestV1::default(),
        )?;

        assert!(report.runtime_theory_check.is_some());
        assert!(report.context_report.runtime_theory_check.is_some());
        assert!(report
            .residual_unknowns
            .iter()
            .any(|item| item.contains("runtime theory residual obligation")));
        assert!(report
            .notes
            .iter()
            .any(|item| item.contains("runtime theory-check sidecar")));
        Ok(())
    }

    #[test]
    fn behavior_case_request_json_round_trips() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let request = BehaviorCaseCheckRequestV1 {
            version: Some("behavior_case_check_request_v1".to_string()),
            behavior_case: sample_behavior_case(),
            overlay: Some(sample_overlay()),
            lifecycle_state: Some("accepted".to_string()),
            evolution_preview: None,
            runtime_theory_check: None,
            runtime_theory_check_input: None,
            codegen: BehaviorCaseCodegenRequestV1 {
                languages: vec![BehaviorCaseCodegenLanguageV1::Rust],
            },
        };
        let request_json = serde_json::to_string(&request)?;
        let report = discover_behavior_case_report_from_request_json(
            &db,
            Some(&meta),
            Some(AcceptedSnapshotId::new("accepted:family")),
            &request_json,
        )?;

        assert_eq!(report.codegen_previews.len(), 1);
        assert_eq!(
            report.codegen_previews[0].language,
            BehaviorCaseCodegenLanguageV1::Rust
        );
        assert_eq!(
            report.context_report.matched_scope_ids,
            vec!["schema/family/relation/parent".to_string()]
        );
        Ok(())
    }
}
