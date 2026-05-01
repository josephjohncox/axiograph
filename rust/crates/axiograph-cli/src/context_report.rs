use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::kernel_ir::TheorySubjectRefIr;
use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

pub const CONTEXT_REPORT_VERSION_V1: &str = "context_report_v1";
pub const CONTEXT_MAP_VERSION_V1: &str = "context_map_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct DomainContextId(String);

impl DomainContextId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DomainContextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundedContextV1 {
    pub context_id: DomainContextId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<crate::semantic_claim::RuntimeRuleScopeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<crate::semantic_claim::ImplementationSurfaceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<crate::semantic_claim::CoverageEdgeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl BoundedContextV1 {
    fn validate(&self) -> Result<()> {
        if self.context_id.as_str().trim().is_empty() {
            return Err(anyhow!("bounded context requires a non-empty `context_id`"));
        }
        if self.label.trim().is_empty() {
            return Err(anyhow!(
                "bounded context `{}` requires a non-empty `label`",
                self.context_id
            ));
        }
        Ok(())
    }

    fn normalized(&self) -> Result<Self> {
        self.validate()?;
        Ok(Self {
            context_id: DomainContextId::new(self.context_id.as_str().trim().to_string()),
            label: self.label.trim().to_string(),
            summary: self
                .summary
                .as_ref()
                .map(|summary| summary.trim().to_string())
                .filter(|summary| !summary.is_empty()),
            scopes: self
                .scopes
                .iter()
                .map(crate::semantic_claim::RuntimeRuleScopeV1::normalized)
                .collect(),
            surfaces: self.surfaces.iter().map(normalize_surface).collect(),
            edges: self.edges.clone(),
            competency_questions: self.competency_questions.clone(),
            notes: self.notes.clone(),
        })
    }

    fn all_scopes(&self) -> Vec<crate::semantic_claim::RuntimeRuleScopeV1> {
        let mut scopes = self
            .scopes
            .iter()
            .cloned()
            .collect::<BTreeSet<crate::semantic_claim::RuntimeRuleScopeV1>>();
        for surface in &self.surfaces {
            scopes.extend(surface.scopes.iter().cloned());
        }
        scopes.into_iter().collect()
    }
}

pub fn semantic_slice_selector_for_bounded_context(
    context: &BoundedContextV1,
) -> Result<crate::semantic_merge_lattice::SemanticSliceSelectorV1> {
    let context = context.normalized()?;
    let mut schema_ids = BTreeSet::new();
    let mut relation_object_ids = BTreeSet::new();
    let mut explicit_ir_refs = BTreeSet::new();

    for scope in context.all_scopes() {
        schema_ids.insert(scope.schema.clone());
        if let Some(scope_id) = scope.inferred_scope_id() {
            explicit_ir_refs.insert(scope_id);
        }
        match scope.scope_class {
            crate::semantic_claim::RuntimeRuleScopeClassV1::Relation => {
                if let Some(relation) = scope.relation.as_deref() {
                    relation_object_ids.insert(format!("relation:{}:{relation}", scope.schema));
                }
            }
            crate::semantic_claim::RuntimeRuleScopeClassV1::Theory => {
                if let Some(theory) = scope.theory.as_deref() {
                    explicit_ir_refs.insert(format!("theory:{}:{theory}", scope.schema));
                }
            }
        }
    }

    Ok(crate::semantic_merge_lattice::SemanticSliceSelectorV1 {
        label: Some(context.context_id.to_string()),
        schema_ids: schema_ids.into_iter().collect(),
        relation_object_ids: relation_object_ids.into_iter().collect(),
        role_ids: Vec::new(),
        theory_obligation_ids: Vec::new(),
        context_ids: vec![context.context_id.to_string()],
        competency_question_names: context
            .competency_questions
            .iter()
            .map(|question| question.name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        behavior_case_ids: Vec::new(),
        implementation_surface_ids: context
            .surfaces
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        world_model_run_ids: Vec::new(),
        explicit_ir_refs: explicit_ir_refs.into_iter().collect(),
    })
}

pub fn context_map_between_bounded_contexts(
    source: &BoundedContextV1,
    target: &BoundedContextV1,
    relationship: ContextMapRelationshipV1,
) -> Result<ContextMapV1> {
    let source = source.normalized()?;
    let target = target.normalized()?;
    let source_selector = semantic_slice_selector_for_bounded_context(&source)?;
    let target_selector = semantic_slice_selector_for_bounded_context(&target)?;
    let source_refs = selector_ref_set(&source_selector);
    let target_refs = selector_ref_set(&target_selector);
    let typed_overlap_refs = source_refs
        .intersection(&target_refs)
        .cloned()
        .collect::<Vec<_>>();
    let merge_policy_hint = match relationship {
        ContextMapRelationshipV1::SharedKernel => "require_explicit_shared_kernel_review",
        ContextMapRelationshipV1::CustomerSupplier => {
            "prefer_target_contract_with_source_compatibility_review"
        }
        ContextMapRelationshipV1::Conformist => "target_context_is_upstream",
        ContextMapRelationshipV1::AntiCorruptionLayer => {
            "require_translation_obligations_before_merge"
        }
        ContextMapRelationshipV1::PublishedLanguage => "merge_only_through_published_language_refs",
        ContextMapRelationshipV1::OpenHostService => "target_context_exports_open_host_contract",
        ContextMapRelationshipV1::SeparateWays => "block_semantic_auto_join_without_review",
        ContextMapRelationshipV1::ReviewOnly => "review_only",
    }
    .to_string();

    let mut residual_obligations = BTreeSet::new();
    if typed_overlap_refs.is_empty()
        && matches!(relationship, ContextMapRelationshipV1::SharedKernel)
    {
        residual_obligations.insert(
            "shared-kernel context map has no typed overlap refs; declare shared schema/relation/theory handles or downgrade the relationship"
                .to_string(),
        );
    }
    if matches!(
        relationship,
        ContextMapRelationshipV1::AntiCorruptionLayer
            | ContextMapRelationshipV1::PublishedLanguage
            | ContextMapRelationshipV1::OpenHostService
    ) {
        residual_obligations.insert(
            "context map needs explicit translation/published-language obligations before automatic semantic merge"
                .to_string(),
        );
    }

    let mut next_actions = BTreeSet::new();
    next_actions.insert(
        "use source_selector and target_selector as semantic merge/rebase slice inputs".to_string(),
    );
    if !residual_obligations.is_empty() {
        next_actions.insert(
            "resolve context-map obligations before promoting cross-context ontology changes"
                .to_string(),
        );
    }

    let digest_input = serde_json::json!({
        "version": CONTEXT_MAP_VERSION_V1,
        "relationship": relationship,
        "source": source.context_id,
        "target": target.context_id,
        "source_selector": source_selector,
        "target_selector": target_selector,
    });
    let map_id = format!(
        "context_map_v1:{}",
        axiograph_dsl::digest::axi_digest_v1(&serde_json::to_string(&digest_input)?)
    );

    Ok(ContextMapV1 {
        version: CONTEXT_MAP_VERSION_V1.to_string(),
        map_id,
        relationship,
        source_context_id: source.context_id,
        target_context_id: target.context_id,
        source_selector,
        target_selector,
        merge_policy_hint,
        typed_overlap_refs,
        residual_obligations: residual_obligations.into_iter().collect(),
        next_actions: next_actions.into_iter().collect(),
        notes: vec![
            "context maps are DDD/fDDD wrapper objects over typed semantic slice selectors; they are not separate ontology kernels".to_string(),
            "cross-context merge authority remains the semantic VCS merge/reconciliation flow".to_string(),
        ],
    })
}

fn selector_ref_set(
    selector: &crate::semantic_merge_lattice::SemanticSliceSelectorV1,
) -> BTreeSet<String> {
    selector
        .schema_ids
        .iter()
        .chain(selector.relation_object_ids.iter())
        .chain(selector.role_ids.iter())
        .chain(selector.theory_obligation_ids.iter())
        .chain(selector.context_ids.iter())
        .chain(selector.competency_question_names.iter())
        .chain(selector.behavior_case_ids.iter())
        .chain(selector.implementation_surface_ids.iter())
        .chain(selector.world_model_run_ids.iter())
        .chain(selector.explicit_ir_refs.iter())
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextReportRequestV1 {
    pub context: BoundedContextV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    pub context: BoundedContextV1,
    pub trust_class: crate::semantic_claim::RuntimeRuleTrustClassV1,
    pub strength: crate::semantic_claim::SemanticClaimStrengthV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_scope_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_scope_refs: Vec<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_rule_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    pub semantic_slice_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_reports: Vec<crate::semantic_claim::BusinessRuleApplicabilityReportV1>,
    pub coverage: crate::semantic_claim::CoverageReportV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_coverage: Option<crate::competency_questions::CompetencyCoverageWithTrustV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_unknowns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContextMapRelationshipV1 {
    SharedKernel,
    CustomerSupplier,
    Conformist,
    AntiCorruptionLayer,
    PublishedLanguage,
    OpenHostService,
    SeparateWays,
    ReviewOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMapV1 {
    pub version: String,
    pub map_id: String,
    pub relationship: ContextMapRelationshipV1,
    pub source_context_id: DomainContextId,
    pub target_context_id: DomainContextId,
    pub source_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
    pub target_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
    pub merge_policy_hint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typed_overlap_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

fn normalize_surface(
    surface: &crate::semantic_claim::ImplementationSurfaceRefV1,
) -> crate::semantic_claim::ImplementationSurfaceRefV1 {
    crate::semantic_claim::ImplementationSurfaceRefV1 {
        surface_id: surface.surface_id.clone(),
        kind: surface.kind,
        label: surface.label.clone(),
        scopes: surface
            .scopes
            .iter()
            .map(crate::semantic_claim::RuntimeRuleScopeV1::normalized)
            .collect(),
        code_refs: surface.code_refs.clone(),
        notes: surface.notes.clone(),
    }
}

fn default_lifecycle_state(
    accepted_snapshot_id: Option<&AcceptedSnapshotId>,
    provided: Option<&str>,
) -> String {
    provided.map(str::to_string).unwrap_or_else(|| {
        if accepted_snapshot_id.is_some() {
            "accepted".to_string()
        } else {
            "runtime_checked".to_string()
        }
    })
}

fn business_rule_report_for_scope(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    scope: &crate::semantic_claim::RuntimeRuleScopeV1,
) -> Result<crate::semantic_claim::BusinessRuleApplicabilityReportV1> {
    let scope = scope.normalized();
    match scope.scope_class {
        crate::semantic_claim::RuntimeRuleScopeClassV1::Relation => {
            let relation = scope.relation.as_deref().ok_or_else(|| {
                anyhow!(
                    "bounded context scope `{}` requires `relation`",
                    scope.scope_id
                )
            })?;
            Ok(
                crate::semantic_claim::business_rule_applicability_for_relation(
                    meta,
                    accepted_snapshot_id,
                    lifecycle_state,
                    &scope.schema,
                    relation,
                ),
            )
        }
        crate::semantic_claim::RuntimeRuleScopeClassV1::Theory => {
            let theory = scope.theory.as_deref().ok_or_else(|| {
                anyhow!(
                    "bounded context scope `{}` requires `theory`",
                    scope.scope_id
                )
            })?;
            Ok(
                crate::semantic_claim::business_rule_applicability_for_theory(
                    meta,
                    accepted_snapshot_id,
                    lifecycle_state,
                    &scope.schema,
                    theory,
                ),
            )
        }
    }
}

fn aggregate_trust_class(
    rule_reports: &[crate::semantic_claim::BusinessRuleApplicabilityReportV1],
    coverage: &crate::semantic_claim::CoverageReportV1,
) -> crate::semantic_claim::RuntimeRuleTrustClassV1 {
    let classes = rule_reports
        .iter()
        .map(|report| report.trust_class)
        .chain(
            coverage
                .surface_reports
                .iter()
                .map(|report| report.trust_class),
        )
        .collect::<Vec<_>>();
    if classes
        .iter()
        .any(|class| *class == crate::semantic_claim::RuntimeRuleTrustClassV1::RuntimeEnforced)
    {
        crate::semantic_claim::RuntimeRuleTrustClassV1::RuntimeEnforced
    } else if classes
        .iter()
        .any(|class| *class == crate::semantic_claim::RuntimeRuleTrustClassV1::RuntimeAdvisory)
    {
        crate::semantic_claim::RuntimeRuleTrustClassV1::RuntimeAdvisory
    } else {
        crate::semantic_claim::RuntimeRuleTrustClassV1::ReviewOnly
    }
}

fn aggregate_strength(
    lifecycle_state: &str,
    rule_reports: &[crate::semantic_claim::BusinessRuleApplicabilityReportV1],
    coverage: &crate::semantic_claim::CoverageReportV1,
    competency_coverage: Option<&crate::competency_questions::CompetencyCoverageWithTrustV1>,
    evolution_preview: Option<&crate::evolution_preview::EvolutionPreviewV1>,
    runtime_theory_check: Option<&crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
) -> crate::semantic_claim::SemanticClaimStrengthV1 {
    let lifecycle = lifecycle_state.trim().to_ascii_lowercase();
    if lifecycle == "conflicted" {
        return crate::semantic_claim::SemanticClaimStrengthV1::Conflicted;
    }
    if rule_reports.is_empty() && competency_coverage.is_none() {
        return crate::semantic_claim::SemanticClaimStrengthV1::Unknown;
    }

    let competency_ok = competency_coverage
        .map(|coverage| coverage.satisfied == coverage.total)
        .unwrap_or(true);
    let preview_ok = evolution_preview.map(|preview| preview.ok).unwrap_or(true);
    let theory_ok = runtime_theory_check
        .map(|summary| {
            summary.blocking_errors == 0
                && summary.completeness_claim.starts_with("claimed_under_")
                && summary.ontology_closure_claim.starts_with("claimed_under_")
        })
        .unwrap_or(true);
    let runtime_coverage_ok = coverage.drifted_rules == 0
        && coverage
            .missing_obligations
            .iter()
            .all(|item| !item.contains("runtime-enforced rule"));
    let rules_strong = !rule_reports.is_empty()
        && rule_reports.iter().all(|report| {
            report.strength == crate::semantic_claim::SemanticClaimStrengthV1::Strong
        });

    if (lifecycle == "accepted" || lifecycle == "certified")
        && rules_strong
        && runtime_coverage_ok
        && competency_ok
        && preview_ok
        && theory_ok
    {
        crate::semantic_claim::SemanticClaimStrengthV1::Strong
    } else {
        crate::semantic_claim::SemanticClaimStrengthV1::Weak
    }
}

pub fn build_context_report_from_request(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    request: ContextReportRequestV1,
) -> Result<ContextReportV1> {
    let runtime_theory_check = resolve_runtime_theory_check_summary(
        request.runtime_theory_check,
        request.runtime_theory_check_input,
    )?;
    build_context_report(
        db,
        meta,
        accepted_snapshot_id,
        request.lifecycle_state.as_deref(),
        &request.context,
        request.evolution_preview,
        runtime_theory_check,
    )
}

pub fn discover_context_report_from_request_json(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    request_json: &str,
) -> Result<ContextReportV1> {
    let request: ContextReportRequestV1 = serde_json::from_str(request_json)
        .map_err(|err| anyhow!("failed to parse context report request JSON: {err}"))?;
    build_context_report_from_request(db, meta, accepted_snapshot_id, request)
}

pub fn build_context_report(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: Option<&str>,
    context: &BoundedContextV1,
    evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
) -> Result<ContextReportV1> {
    let context = context.normalized()?;
    let lifecycle_state = default_lifecycle_state(accepted_snapshot_id.as_ref(), lifecycle_state);
    let meta = match meta {
        Some(meta) => meta.clone(),
        None => MetaPlaneIndex::from_db(db)?,
    };

    let scopes = context.all_scopes();
    let rule_reports = scopes
        .iter()
        .map(|scope| {
            business_rule_report_for_scope(
                &meta,
                accepted_snapshot_id.clone(),
                &lifecycle_state,
                scope,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let coverage = crate::semantic_claim::semantic_coverage_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &context.surfaces,
        &context.edges,
        runtime_theory_check.clone(),
    );

    let mut competency_coverage = if context.competency_questions.is_empty() {
        None
    } else {
        Some(
            crate::competency_questions::evaluate_competency_questions_with_trust(
                db,
                &context.competency_questions,
            )?,
        )
    };
    if let Some(competency_coverage) = competency_coverage.as_mut() {
        competency_coverage.runtime_theory_check = runtime_theory_check.clone();
    }

    let matched_scope_ids = scopes
        .iter()
        .filter_map(crate::semantic_claim::RuntimeRuleScopeV1::inferred_scope_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let matched_scope_refs = scopes
        .iter()
        .filter_map(crate::semantic_claim::RuntimeRuleScopeV1::inferred_scope_ref)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let matched_rule_ids = rule_reports
        .iter()
        .flat_map(|report| report.rules.iter().map(|rule| rule.rule_id.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let surface_ids = context
        .surfaces
        .iter()
        .map(|surface| surface.surface_id.clone())
        .collect::<Vec<_>>();
    let semantic_slice_selector = semantic_slice_selector_for_bounded_context(&context)?;

    let trust_class = aggregate_trust_class(&rule_reports, &coverage);
    let strength = aggregate_strength(
        &lifecycle_state,
        &rule_reports,
        &coverage,
        competency_coverage.as_ref(),
        evolution_preview.as_ref(),
        runtime_theory_check.as_ref(),
    );

    let mut residual_unknowns = rule_reports
        .iter()
        .flat_map(|report| report.missing_obligations.iter().cloned())
        .collect::<BTreeSet<_>>();
    residual_unknowns.extend(coverage.missing_obligations.iter().cloned());
    if matched_scope_ids.is_empty() {
        residual_unknowns.insert(
            "bounded context is not mapped to any ontology relation/theory scope yet".to_string(),
        );
    }
    if matched_rule_ids.is_empty() {
        residual_unknowns
            .insert("no typed business rules were resolved for the bounded context".to_string());
    }
    if let Some(competency_coverage) = competency_coverage.as_ref() {
        let unsatisfied = competency_coverage
            .questions
            .iter()
            .filter(|question| !question.satisfied)
            .count();
        if unsatisfied > 0 {
            residual_unknowns.insert(format!(
                "{unsatisfied} competency question(s) remain unsatisfied inside this bounded context"
            ));
        }
    }
    if let Some(preview) = evolution_preview.as_ref() {
        residual_unknowns.extend(preview.residual_obligations.iter().cloned());
        if let Some(summary) = preview.runtime_theory_check.as_ref() {
            residual_unknowns.extend(summary.residual_obligation_ids.iter().map(|id| {
                format!("attached evolution preview carries residual theory obligation `{id}`")
            }));
        }
        if !preview.ok {
            residual_unknowns.insert(format!(
                "attached evolution preview `{}` is not yet ready for promotion or merge",
                preview.candidate_label
            ));
        }
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            residual_unknowns.insert(format!(
                "runtime theory check has {} blocking error(s)",
                summary.blocking_errors
            ));
        }
        residual_unknowns.extend(
            summary
                .residual_obligation_ids
                .iter()
                .map(|id| format!("runtime theory check leaves residual obligation `{id}`")),
        );
    }

    let mut next_actions = rule_reports
        .iter()
        .flat_map(|report| report.next_actions.iter().cloned())
        .collect::<BTreeSet<_>>();
    next_actions.extend(coverage.next_actions.iter().cloned());
    if let Some(competency_coverage) = competency_coverage.as_ref() {
        if competency_coverage
            .questions
            .iter()
            .any(|question| !question.satisfied)
        {
            if competency_coverage
                .questions
                .iter()
                .any(|question| !question.satisfied && !question.refinement_candidates.is_empty())
            {
                next_actions.insert(
                    "review refinement_candidates on unsatisfied competency questions before treating this bounded context as aligned".to_string(),
                );
            } else {
                next_actions.insert(
                    "repair or re-scope unsatisfied competency questions before treating this bounded context as aligned".to_string(),
                );
            }
        }
    }
    if let Some(preview) = evolution_preview.as_ref() {
        next_actions.extend(preview.exploration_next_actions.iter().cloned());
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            next_actions.insert(
                "resolve blocking RuntimeTheoryCheckReportV1 judgments before treating this bounded context as promotion-ready".to_string(),
            );
        }
        if !summary.residual_obligation_ids.is_empty() {
            next_actions.insert(
                "review runtime theory residual obligations before using this bounded context as a strong fDDD invariant boundary".to_string(),
            );
        }
    }

    let mut notes = context.notes.iter().cloned().collect::<BTreeSet<_>>();
    notes.extend(
        rule_reports
            .iter()
            .flat_map(|report| report.notes.iter().cloned()),
    );
    notes.extend(coverage.notes.iter().cloned());
    notes.insert(
        "bounded-context reports are read-only wrappers over existing semantic-claim, competency-question, trust, and evolution-preview contracts"
            .to_string(),
    );
    if accepted_snapshot_id.is_none() {
        notes.insert(
            "report is not attached to an accepted snapshot id, so lifecycle claims remain runtime-checked only"
                .to_string(),
        );
    }
    if !context.competency_questions.is_empty() {
        notes.insert(
            "competency question trust remains scoped to supplied queries and returned rows; it does not claim full bounded-context completeness"
                .to_string(),
        );
    }
    if evolution_preview.is_some() {
        notes.insert(
            "attached evolution preview is included as a read-only sidecar and does not mutate the current bounded context or snapshot"
                .to_string(),
        );
    }
    if runtime_theory_check.is_some() {
        notes.insert(
            "attached runtime theory check summary scopes closure/completeness to declared world, evidence, ref, and fragment assumptions"
                .to_string(),
        );
    }

    Ok(ContextReportV1 {
        version: CONTEXT_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        lifecycle_state,
        context,
        trust_class,
        strength,
        matched_scope_ids,
        matched_scope_refs,
        matched_rule_ids,
        surface_ids,
        semantic_slice_selector,
        rule_reports,
        coverage,
        competency_coverage,
        evolution_preview,
        runtime_theory_check,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db_and_meta() -> Result<(PathDB, MetaPlaneIndex)> {
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory Rules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db)?;
        Ok((db, meta))
    }

    fn sample_context() -> BoundedContextV1 {
        BoundedContextV1 {
            context_id: DomainContextId::new("domain:family_lookup"),
            label: "Family lookup".to_string(),
            summary: Some("Read-only bounded context for family lookup behavior".to_string()),
            scopes: vec![crate::semantic_claim::RuntimeRuleScopeV1::relation(
                "S", "Parent",
            )],
            surfaces: vec![crate::semantic_claim::ImplementationSurfaceRefV1 {
                surface_id: "endpoint:family_lookup".to_string(),
                kind: crate::semantic_claim::ImplementationSurfaceKindV1::Endpoint,
                label: "GET /family/lookup".to_string(),
                scopes: vec![crate::semantic_claim::RuntimeRuleScopeV1::relation(
                    "S", "Parent",
                )],
                code_refs: vec!["src/family.rs".to_string()],
                notes: vec!["primary read model".to_string()],
            }],
            edges: vec![crate::semantic_claim::CoverageEdgeV1 {
                surface_id: "endpoint:family_lookup".to_string(),
                rule_id: "schema/s/relation/parent/rule/functional/0".to_string(),
                status: crate::semantic_claim::CoverageStatusV1::Tested,
                notes: vec!["covered by endpoint integration test".to_string()],
            }],
            competency_questions: vec![crate::world_model::CompetencyQuestionV1 {
                name: "parent_lookup_returns_bob".to_string(),
                question: Some(
                    "Does the bounded context return Bob as Alice's parent?".to_string(),
                ),
                authoring: None,
                query: "select ?f where ?f = S.Parent(child=Alice, parent=Bob) limit 1".to_string(),
                min_rows: 1,
                weight: 1.0,
                contexts: Vec::new(),
            }],
            notes: vec!["used by read-only API handlers".to_string()],
        }
    }

    fn sample_transport_preview() -> crate::evolution_preview::EvolutionPreviewV1 {
        let axi_text = r#"
module Plant

schema Plant:
  object PlantAsset
  object Pump
  object Compressor
  object Context
  relation installed_at(asset: PlantAsset, site: PlantAsset, ctx: Context)
  subtype Pump < PlantAsset
  subtype Compressor < PlantAsset

theory PlantTransport on Plant:
  constraint key installed_at(asset, site, ctx)
"#;
        let morphism_json = r#"{
  "source_schema": "Plant",
  "target_schema": "Ops",
  "objects": [
    {"source_object": "PlantAsset", "target_object": "Equipment"},
    {"source_object": "Pump", "target_object": "Equipment"},
    {"source_object": "Compressor", "target_object": "Equipment"}
  ],
  "arrows": [
    {"source_arrow": "installed_at", "target_path": ["owned_by", "located_at"]}
  ]
}"#;
        crate::discover_transport_preview_from_inputs(axi_text, Some("Plant"), morphism_json, None)
            .expect("transport preview")
    }

    fn sample_runtime_theory_summary() -> crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
        crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
            version: "runtime_theory_check_summary_v1".to_string(),
            report_version: "runtime_theory_check_report_v1".to_string(),
            module_digest: "fnv1a64:context-test".to_string(),
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
            residual_obligation_ids: vec!["context/theory/residual".to_string()],
            notes: vec!["test runtime theory sidecar".to_string()],
        }
    }

    #[test]
    fn build_context_report_composes_rule_coverage_and_competency_contracts() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let report = build_context_report(
            &db,
            Some(&meta),
            Some(AcceptedSnapshotId::new("accepted:family")),
            Some("accepted"),
            &sample_context(),
            None,
            None,
        )?;

        assert_eq!(report.version, CONTEXT_REPORT_VERSION_V1);
        assert_eq!(report.context.context_id.as_str(), "domain:family_lookup");
        assert_eq!(
            report.trust_class,
            crate::semantic_claim::RuntimeRuleTrustClassV1::RuntimeEnforced
        );
        assert_eq!(
            report.strength,
            crate::semantic_claim::SemanticClaimStrengthV1::Strong
        );
        assert_eq!(report.rule_reports.len(), 1);
        assert_eq!(report.coverage.covered_rules, 1);
        assert_eq!(report.coverage.tested_rules, 1);
        assert_eq!(
            report.matched_scope_ids,
            vec!["schema/s/relation/parent".to_string()]
        );
        assert_eq!(
            report.semantic_slice_selector.relation_object_ids,
            vec!["relation:S:Parent".to_string()]
        );
        assert_eq!(
            report.semantic_slice_selector.implementation_surface_ids,
            vec!["endpoint:family_lookup".to_string()]
        );
        assert_eq!(
            report
                .competency_coverage
                .as_ref()
                .map(|coverage| coverage.satisfied),
            Some(1)
        );
        assert!(report.next_actions.is_empty());
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("read-only wrappers")));
        Ok(())
    }

    #[test]
    fn context_map_emits_merge_ready_slice_selectors() -> Result<()> {
        let source = sample_context();
        let mut target = sample_context();
        target.context_id = DomainContextId::new("domain:family_write_model");
        target.label = "Family write model".to_string();
        target.surfaces[0].surface_id = "workflow:update_family".to_string();
        target.surfaces[0].kind = crate::semantic_claim::ImplementationSurfaceKindV1::Workflow;
        target.surfaces[0].label = "Update family workflow".to_string();

        let context_map = context_map_between_bounded_contexts(
            &source,
            &target,
            ContextMapRelationshipV1::SharedKernel,
        )?;

        assert_eq!(context_map.version, CONTEXT_MAP_VERSION_V1);
        assert_eq!(
            context_map.merge_policy_hint,
            "require_explicit_shared_kernel_review"
        );
        assert!(context_map
            .typed_overlap_refs
            .iter()
            .any(|reference| reference == "relation:S:Parent"));
        assert!(context_map.residual_obligations.is_empty());
        assert_eq!(
            context_map.source_selector.relation_object_ids,
            vec!["relation:S:Parent".to_string()]
        );
        assert_eq!(
            context_map.target_selector.implementation_surface_ids,
            vec!["workflow:update_family".to_string()]
        );
        Ok(())
    }

    #[test]
    fn build_context_report_preserves_optional_evolution_preview_sidecar() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let preview = sample_transport_preview();
        let report = build_context_report(
            &db,
            Some(&meta),
            None,
            None,
            &sample_context(),
            Some(preview.clone()),
            None,
        )?;

        assert_eq!(
            report
                .evolution_preview
                .as_ref()
                .map(|preview| preview.candidate_label.as_str()),
            Some(preview.candidate_label.as_str())
        );
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("read-only sidecar")));
        Ok(())
    }

    #[test]
    fn build_context_report_threads_runtime_theory_into_cq_and_coverage() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let report = build_context_report(
            &db,
            Some(&meta),
            Some(AcceptedSnapshotId::new("accepted:family")),
            Some("accepted"),
            &sample_context(),
            None,
            Some(sample_runtime_theory_summary()),
        )?;

        assert!(report.runtime_theory_check.is_some());
        assert!(report.coverage.runtime_theory_check.is_some());
        assert!(report
            .competency_coverage
            .as_ref()
            .and_then(|coverage| coverage.runtime_theory_check.as_ref())
            .is_some());
        assert!(report
            .residual_unknowns
            .iter()
            .any(|item| item.contains("runtime theory check leaves residual obligation")));
        Ok(())
    }

    #[test]
    fn discover_context_report_from_request_json_round_trips_request_shape() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let request_json = serde_json::to_string(&ContextReportRequestV1 {
            context: sample_context(),
            lifecycle_state: None,
            evolution_preview: None,
            runtime_theory_check: None,
            runtime_theory_check_input: None,
        })?;
        let report =
            discover_context_report_from_request_json(&db, Some(&meta), None, &request_json)?;

        assert_eq!(report.version, CONTEXT_REPORT_VERSION_V1);
        assert_eq!(report.lifecycle_state, "runtime_checked");
        assert_eq!(report.context.context_id.as_str(), "domain:family_lookup");
        assert_eq!(
            report.matched_scope_ids,
            vec!["schema/s/relation/parent".to_string()]
        );
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("read-only wrappers")));
        Ok(())
    }
}
