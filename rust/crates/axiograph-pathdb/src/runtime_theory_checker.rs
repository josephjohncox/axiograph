//! Runtime theory checking and closure reporting over compiled semantic IR.
//!
//! This module is intentionally operational: it produces typed reports for
//! authoring, CQ gates, migration, reconciliation, and agent tooling. It is not
//! the Lean trusted checker.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use axiograph_dsl::schema_v1::PathExprV3;

use crate::kernel_ir::{
    CompiledSchemaIr, RelationSemanticsIr, RoleKind, TheoryIr, TheoryObligationRefIr,
    TheorySubjectRefIr, TheoryTransportPlanIr, TheoryTransportStatusIr,
};
use crate::SchemaId;

pub const RUNTIME_THEORY_CHECK_REPORT_VERSION_V1: &str = "runtime_theory_check_report_v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryClosureTierV1 {
    FiniteFragment,
    EvidenceWeighted,
    GlobalIndexed,
}

impl RuntimeTheoryClosureTierV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FiniteFragment => "finite_fragment",
            Self::EvidenceWeighted => "evidence_weighted",
            Self::GlobalIndexed => "global_indexed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryCheckStatusV1 {
    Checked,
    ReviewOnly,
    ResidualObligation,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryCheckSeverityV1 {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryFragmentV1 {
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub supports_structured_constraints: bool,
    pub supports_path_equations: bool,
    pub supports_rewrites: bool,
    pub supports_transports: bool,
    pub supports_weighted_evidence: bool,
    pub terminating_fragment: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryNonClaimV1 {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceWeightSemanticsV1 {
    ThresholdedWorld,
    WeightedLattice,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidencePolicyV1 {
    pub policy_id: String,
    /// Parts per million. The default weight for accepted obligations is
    /// 1_000_000, so the default threshold includes accepted obligations.
    pub threshold_ppm: u32,
    pub semantics: EvidenceWeightSemanticsV1,
    pub weighted_propagation_enabled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub obligation_weights_ppm: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldAssumptionV1 {
    pub world_id: String,
    pub finite: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed_contexts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_worlds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_slices: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undeclared_imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletenessClaimV1 {
    pub claimed: bool,
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyClosureClaimV1 {
    pub claimed: bool,
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub scope: String,
    pub fixpoint_reached: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryJudgmentV1 {
    pub obligation_ref: TheoryObligationRefIr,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    pub status: RuntimeTheoryCheckStatusV1,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub admissible: bool,
    pub complete_under_assumptions: bool,
    pub closed_under_assumptions: bool,
    pub evidence_weight_ppm: u32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryClosureReportV1 {
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub complete: bool,
    pub closed: bool,
    pub fixpoint_reached: bool,
    pub iterations: u32,
    pub considered_obligations: usize,
    pub excluded_obligations: usize,
    pub derived_obligations: usize,
    pub blocked_obligations: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    pub world: WorldAssumptionV1,
    pub evidence_policy: EvidencePolicyV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryCheckReportV1 {
    pub version: String,
    pub schema_id: SchemaId,
    pub theory_ref: TheorySubjectRefIr,
    pub fragment: RuntimeTheoryFragmentV1,
    pub world: WorldAssumptionV1,
    pub evidence_policy: EvidencePolicyV1,
    #[serde(default)]
    pub total_obligations: usize,
    #[serde(default)]
    pub checked_obligations: usize,
    #[serde(default)]
    pub review_only_obligations: usize,
    #[serde(default)]
    pub residual_obligations: usize,
    #[serde(default)]
    pub blocked_obligations: usize,
    #[serde(default)]
    pub excluded_by_evidence: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judgments: Vec<RuntimeTheoryJudgmentV1>,
    pub closure: RuntimeTheoryClosureReportV1,
    pub completeness_claim: CompletenessClaimV1,
    pub ontology_closure_claim: OntologyClosureClaimV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub fn default_world_assumption_v1() -> WorldAssumptionV1 {
    WorldAssumptionV1 {
        world_id: "accepted:current".to_string(),
        finite: true,
        closed_contexts: Vec::new(),
        included_refs: vec!["current_module".to_string()],
        included_worlds: Vec::new(),
        included_slices: Vec::new(),
        included_imports: Vec::new(),
        undeclared_imports: Vec::new(),
    }
}

pub fn default_evidence_policy_v1() -> EvidencePolicyV1 {
    EvidencePolicyV1 {
        policy_id: "thresholded_world_default".to_string(),
        threshold_ppm: 500_000,
        semantics: EvidenceWeightSemanticsV1::ThresholdedWorld,
        weighted_propagation_enabled: false,
        obligation_weights_ppm: BTreeMap::new(),
    }
}

pub fn check_runtime_theory_v1(
    compiled_schema: &CompiledSchemaIr,
    theory: &TheoryIr,
) -> RuntimeTheoryCheckReportV1 {
    check_runtime_theory_with_options_v1(
        compiled_schema,
        theory,
        RuntimeTheoryClosureTierV1::FiniteFragment,
        default_world_assumption_v1(),
        default_evidence_policy_v1(),
        None,
    )
}

pub fn check_runtime_theory_with_options_v1(
    compiled_schema: &CompiledSchemaIr,
    theory: &TheoryIr,
    closure_tier: RuntimeTheoryClosureTierV1,
    world: WorldAssumptionV1,
    evidence_policy: EvidencePolicyV1,
    transport_plan: Option<&TheoryTransportPlanIr>,
) -> RuntimeTheoryCheckReportV1 {
    let mut judgments = theory
        .runtime_fragment_summary()
        .obligation_statuses
        .into_iter()
        .map(|status| {
            judgment_for_obligation(
                compiled_schema,
                theory,
                closure_tier,
                &evidence_policy,
                status.obligation_ref,
            )
        })
        .collect::<Vec<_>>();

    if let Some(plan) = transport_plan {
        apply_transport_classification(plan, closure_tier, &mut judgments);
    }

    if closure_tier == RuntimeTheoryClosureTierV1::EvidenceWeighted {
        for judgment in &mut judgments {
            apply_evidence_threshold(&evidence_policy, judgment);
        }
    }

    let mut non_claims = base_non_claims(closure_tier, &evidence_policy);
    let mut notes = vec![
        "runtime theory checking is operational Rust checking, not a Lean certificate".to_string(),
        "checked obligations are well-scoped and admissible under the declared runtime fragment".to_string(),
    ];

    if closure_tier == RuntimeTheoryClosureTierV1::GlobalIndexed {
        if world.included_refs.is_empty()
            && world.included_worlds.is_empty()
            && world.included_slices.is_empty()
            && world.included_imports.is_empty()
        {
            let residual = "global_indexed closure requires a finite declared ref/world/slice/import universe".to_string();
            for judgment in &mut judgments {
                if judgment.status == RuntimeTheoryCheckStatusV1::Checked {
                    judgment.status = RuntimeTheoryCheckStatusV1::ResidualObligation;
                    judgment.severity = RuntimeTheoryCheckSeverityV1::Warning;
                    judgment.complete_under_assumptions = false;
                    judgment.closed_under_assumptions = false;
                    judgment.residual_obligations.push(residual.clone());
                }
            }
        }
        if !world.undeclared_imports.is_empty() {
            let residual = format!(
                "global_indexed closure rejected undeclared imports: {}",
                world.undeclared_imports.join(", ")
            );
            for judgment in &mut judgments {
                judgment.status = RuntimeTheoryCheckStatusV1::Blocked;
                judgment.severity = RuntimeTheoryCheckSeverityV1::Error;
                judgment.admissible = false;
                judgment.complete_under_assumptions = false;
                judgment.closed_under_assumptions = false;
                judgment.residual_obligations.push(residual.clone());
            }
            non_claims.push(RuntimeTheoryNonClaimV1 {
                code: "undeclared_global_imports".to_string(),
                message: residual,
            });
        }
    }

    let total_obligations = judgments.len();
    let excluded_by_evidence = judgments
        .iter()
        .filter(|judgment| {
            judgment
                .non_claims
                .iter()
                .any(|claim| claim.code == "excluded_by_evidence_threshold")
        })
        .count();
    let checked_obligations = judgments
        .iter()
        .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Checked)
        .count();
    let review_only_obligations = judgments
        .iter()
        .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::ReviewOnly)
        .count();
    let residual_obligations = judgments
        .iter()
        .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::ResidualObligation)
        .count();
    let blocked_obligations = judgments
        .iter()
        .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Blocked)
        .count();

    let residual_ids = judgments
        .iter()
        .filter(|judgment| {
            matches!(
                judgment.status,
                RuntimeTheoryCheckStatusV1::ReviewOnly
                    | RuntimeTheoryCheckStatusV1::ResidualObligation
                    | RuntimeTheoryCheckStatusV1::Blocked
            ) && !judgment
                .non_claims
                .iter()
                .any(|claim| claim.code == "excluded_by_evidence_threshold")
        })
        .map(|judgment| judgment.obligation_ref.stable_id())
        .collect::<Vec<_>>();

    if blocked_obligations > 0 {
        notes.push(format!(
            "{blocked_obligations} obligation(s) are blocking under {}",
            closure_tier.as_str()
        ));
    }
    if residual_obligations > 0 || review_only_obligations > 0 {
        notes.push(format!(
            "{} obligation(s) require review or residual resolver work before strong closure can be claimed",
            residual_obligations + review_only_obligations
        ));
    }

    let considered_obligations = total_obligations.saturating_sub(excluded_by_evidence);
    let complete = blocked_obligations == 0 && residual_obligations == 0;
    let closed = complete && residual_ids.is_empty();
    let fixpoint_reached = blocked_obligations == 0;
    let iterations = if considered_obligations == 0 { 0 } else { 1 };

    let closure = RuntimeTheoryClosureReportV1 {
        closure_tier,
        complete,
        closed,
        fixpoint_reached,
        iterations,
        considered_obligations,
        excluded_obligations: excluded_by_evidence,
        derived_obligations: checked_obligations,
        blocked_obligations,
        residual_obligations: residual_ids.clone(),
        world: world.clone(),
        evidence_policy: evidence_policy.clone(),
        notes: closure_notes(closure_tier, &world, &evidence_policy),
    };

    let completeness_claim = CompletenessClaimV1 {
        claimed: complete,
        closure_tier,
        scope: completeness_scope(closure_tier, &world),
        assumptions: completeness_assumptions(closure_tier, &world, &evidence_policy),
        residual_obligations: residual_ids.clone(),
    };
    let ontology_closure_claim = OntologyClosureClaimV1 {
        claimed: closed,
        closure_tier,
        scope: completeness_scope(closure_tier, &world),
        fixpoint_reached,
        assumptions: completeness_assumptions(closure_tier, &world, &evidence_policy),
        residual_obligations: residual_ids,
    };

    RuntimeTheoryCheckReportV1 {
        version: RUNTIME_THEORY_CHECK_REPORT_VERSION_V1.to_string(),
        schema_id: compiled_schema.schema_id.clone(),
        theory_ref: TheorySubjectRefIr::Theory {
            theory_id: theory.theory_id.clone(),
        },
        fragment: RuntimeTheoryFragmentV1 {
            closure_tier,
            supports_structured_constraints: true,
            supports_path_equations: true,
            supports_rewrites: true,
            supports_transports: true,
            supports_weighted_evidence: evidence_policy.weighted_propagation_enabled,
            terminating_fragment: true,
            notes: vec![
                "finite saturation is currently one-step over compiled obligations; recursive higher-order closure remains residual".to_string(),
            ],
        },
        world,
        evidence_policy,
        total_obligations,
        checked_obligations,
        review_only_obligations,
        residual_obligations,
        blocked_obligations,
        excluded_by_evidence,
        judgments,
        closure,
        completeness_claim,
        ontology_closure_claim,
        non_claims,
        notes,
    }
}

fn judgment_for_obligation(
    compiled_schema: &CompiledSchemaIr,
    theory: &TheoryIr,
    closure_tier: RuntimeTheoryClosureTierV1,
    evidence_policy: &EvidencePolicyV1,
    obligation_ref: TheoryObligationRefIr,
) -> RuntimeTheoryJudgmentV1 {
    let subject_refs = theory.subject_refs_for_obligation(&obligation_ref);
    let mut residual_obligations = Vec::new();
    let mut non_claims = Vec::new();
    let mut status = RuntimeTheoryCheckStatusV1::Checked;
    let mut severity = RuntimeTheoryCheckSeverityV1::Info;
    let mut admissible = true;
    let message: String;

    match &obligation_ref {
        TheoryObligationRefIr::Constraint { constraint_id, .. } => {
            match theory
                .constraints
                .iter()
                .find(|candidate| &candidate.constraint_id == constraint_id)
            {
                Some(constraint) if constraint.relation_name.is_some() && constraint.relation_id.is_none() => {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations.push("constraint relation did not resolve to a stable relation id".to_string());
                    message = "constraint is not admissible because its relation ref is unresolved".to_string();
                }
                Some(constraint) if is_runtime_constraint_kind(&constraint.kind) => {
                    message = format!(
                        "structured `{}` constraint resolved stable relation/role ids",
                        constraint.kind
                    );
                }
                Some(constraint) => {
                    status = RuntimeTheoryCheckStatusV1::ReviewOnly;
                    severity = RuntimeTheoryCheckSeverityV1::Warning;
                    admissible = false;
                    residual_obligations.push(format!(
                        "constraint kind `{}` remains outside the supported runtime closure fragment",
                        constraint.kind
                    ));
                    non_claims.push(RuntimeTheoryNonClaimV1 {
                        code: "constraint_out_of_fragment".to_string(),
                        message: "constraint is addressable but not closed by the runtime checker".to_string(),
                    });
                    message = "constraint is preserved for typed review but not closed".to_string();
                }
                None => {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations.push("compiled constraint record is missing".to_string());
                    message = "constraint index is inconsistent with compiled TheoryIr".to_string();
                }
            }
        }
        TheoryObligationRefIr::PathEquation { equation_id, .. } => {
            match theory
                .path_equations
                .iter()
                .find(|candidate| &candidate.equation_id == equation_id)
            {
                Some(equation) => {
                    let missing = missing_relation_ids(compiled_schema, &equation.relation_ids);
                    if !missing.is_empty() {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(format!(
                            "path equation references missing relation ids: {}",
                            missing.join(", ")
                        ));
                        message = "path equation is not endpoint-safe because a relation id is unresolved".to_string();
                    } else {
                        message = "path equation relation refs resolve and endpoint safety was checked during compiled-IR elaboration".to_string();
                    }
                }
                None => {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations.push("compiled path equation record is missing".to_string());
                    message = "path equation index is inconsistent with compiled TheoryIr".to_string();
                }
            }
        }
        TheoryObligationRefIr::OpaqueEquation { .. } => {
            status = RuntimeTheoryCheckStatusV1::ReviewOnly;
            severity = RuntimeTheoryCheckSeverityV1::Warning;
            admissible = false;
            residual_obligations.push(
                "opaque equation is addressable but outside the runtime closure fragment".to_string(),
            );
            non_claims.push(RuntimeTheoryNonClaimV1 {
                code: "opaque_equation_not_closed".to_string(),
                message: "opaque equation is not runtime-certifiable or closed in this fragment".to_string(),
            });
            message = "opaque equation preserved as a typed review obligation".to_string();
        }
        TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
            match theory
                .rewrite_rules
                .iter()
                .find(|candidate| &candidate.rule_id == rule_id)
            {
                Some(rule) => {
                    let missing = missing_relation_ids(compiled_schema, &rule.relation_ids);
                    if !missing.is_empty() {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(format!(
                            "rewrite references missing relation ids: {}",
                            missing.join(", ")
                        ));
                        message = "rewrite rule is not admissible because a relation id is unresolved".to_string();
                    } else if drops_axis_roles(compiled_schema, &rule.lhs, &rule.rhs) {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(
                            "rewrite drops context or temporal axes from lhs to rhs".to_string(),
                        );
                        message = "rewrite is blocked because context/time roles must be preserved or explicitly reviewed".to_string();
                    } else {
                        message = format!(
                            "rewrite preserves endpoint {} -> {} and keeps relation/context refs admissible",
                            rule.endpoint.from_type, rule.endpoint.to_type
                        );
                    }
                }
                None => {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations.push("compiled rewrite rule record is missing".to_string());
                    message = "rewrite rule index is inconsistent with compiled TheoryIr".to_string();
                }
            }
        }
    }

    let complete_under_assumptions = status == RuntimeTheoryCheckStatusV1::Checked;
    let closed_under_assumptions = status == RuntimeTheoryCheckStatusV1::Checked;
    let evidence_weight_ppm = obligation_weight_ppm(evidence_policy, &obligation_ref);

    RuntimeTheoryJudgmentV1 {
        label: obligation_ref.display_name(),
        obligation_ref,
        subject_refs,
        status,
        severity,
        closure_tier,
        admissible,
        complete_under_assumptions,
        closed_under_assumptions,
        evidence_weight_ppm,
        message,
        residual_obligations,
        non_claims,
    }
}

fn apply_transport_classification(
    plan: &TheoryTransportPlanIr,
    closure_tier: RuntimeTheoryClosureTierV1,
    judgments: &mut [RuntimeTheoryJudgmentV1],
) {
    for item in &plan.items {
        if let Some(judgment) = judgments
            .iter_mut()
            .find(|candidate| candidate.obligation_ref == item.obligation_ref)
        {
            match item.status {
                TheoryTransportStatusIr::Preserved => {
                    judgment.message.push_str("; transport preserved obligation");
                }
                TheoryTransportStatusIr::Transported => {
                    if judgment.status == RuntimeTheoryCheckStatusV1::Checked {
                        judgment.status = RuntimeTheoryCheckStatusV1::ResidualObligation;
                        judgment.severity = RuntimeTheoryCheckSeverityV1::Warning;
                    }
                    judgment.complete_under_assumptions = false;
                    judgment.closed_under_assumptions = false;
                    judgment.residual_obligations.push(format!(
                        "transported under {:?}; review/certification required before strong migration soundness",
                        closure_tier
                    ));
                }
                TheoryTransportStatusIr::MissingObjectImage
                | TheoryTransportStatusIr::MissingArrowImage
                | TheoryTransportStatusIr::OpaqueOrOutOfFragment => {
                    judgment.status = RuntimeTheoryCheckStatusV1::Blocked;
                    judgment.severity = RuntimeTheoryCheckSeverityV1::Error;
                    judgment.admissible = false;
                    judgment.complete_under_assumptions = false;
                    judgment.closed_under_assumptions = false;
                    judgment.residual_obligations.push(item.detail.clone());
                }
            }
        }
    }
}

fn apply_evidence_threshold(
    evidence_policy: &EvidencePolicyV1,
    judgment: &mut RuntimeTheoryJudgmentV1,
) {
    if judgment.evidence_weight_ppm < evidence_policy.threshold_ppm {
        judgment.status = RuntimeTheoryCheckStatusV1::ReviewOnly;
        judgment.severity = RuntimeTheoryCheckSeverityV1::Warning;
        judgment.complete_under_assumptions = true;
        judgment.closed_under_assumptions = true;
        judgment.non_claims.push(RuntimeTheoryNonClaimV1 {
            code: "excluded_by_evidence_threshold".to_string(),
            message: format!(
                "obligation weight {}ppm is below threshold {}ppm and is excluded from the thresholded world",
                judgment.evidence_weight_ppm, evidence_policy.threshold_ppm
            ),
        });
    }
}

fn obligation_weight_ppm(
    evidence_policy: &EvidencePolicyV1,
    obligation_ref: &TheoryObligationRefIr,
) -> u32 {
    evidence_policy
        .obligation_weights_ppm
        .get(&obligation_ref.stable_id())
        .copied()
        .unwrap_or(1_000_000)
}

fn is_runtime_constraint_kind(kind: &str) -> bool {
    matches!(
        kind,
        "functional" | "at_most" | "key" | "symmetric_where_in" | "symmetric" | "transitive"
    )
}

fn missing_relation_ids(compiled_schema: &CompiledSchemaIr, relation_ids: &[crate::RelationId]) -> Vec<String> {
    relation_ids
        .iter()
        .filter(|relation_id| relation_by_id(compiled_schema, relation_id).is_none())
        .map(ToString::to_string)
        .collect()
}

fn relation_by_id<'a>(
    compiled_schema: &'a CompiledSchemaIr,
    relation_id: &crate::RelationId,
) -> Option<&'a RelationSemanticsIr> {
    compiled_schema
        .relations
        .values()
        .find(|relation| &relation.relation_id == relation_id)
}

fn relation_by_name<'a>(
    compiled_schema: &'a CompiledSchemaIr,
    relation_name: &str,
) -> Option<&'a RelationSemanticsIr> {
    compiled_schema.relations.get(relation_name).or_else(|| {
        compiled_schema
            .relations
            .values()
            .find(|relation| relation.name == relation_name)
    })
}

fn drops_axis_roles(
    compiled_schema: &CompiledSchemaIr,
    lhs: &PathExprV3,
    rhs: &PathExprV3,
) -> bool {
    let lhs_axes = axis_role_keys_for_path(compiled_schema, lhs);
    if lhs_axes.is_empty() {
        return false;
    }
    let rhs_axes = axis_role_keys_for_path(compiled_schema, rhs);
    !lhs_axes.is_subset(&rhs_axes)
}

fn axis_role_keys_for_path(
    compiled_schema: &CompiledSchemaIr,
    path: &PathExprV3,
) -> BTreeSet<String> {
    let mut relation_names = BTreeSet::new();
    collect_path_step_relation_names(path, &mut relation_names);
    let mut keys = BTreeSet::new();
    for relation_name in relation_names {
        if let Some(relation) = relation_by_name(compiled_schema, &relation_name) {
            for role in &relation.roles {
                if matches!(role.kind, RoleKind::Context | RoleKind::Temporal) {
                    keys.insert(format!("{:?}:{}:{}", role.kind, role.name, role.target_type));
                }
            }
        }
    }
    keys
}

fn collect_path_step_relation_names(path: &PathExprV3, out: &mut BTreeSet<String>) {
    match path {
        PathExprV3::Step { rel, .. } => {
            out.insert(rel.clone());
        }
        PathExprV3::Trans { left, right } => {
            collect_path_step_relation_names(left, out);
            collect_path_step_relation_names(right, out);
        }
        PathExprV3::Inv { path } => collect_path_step_relation_names(path, out),
        PathExprV3::Var { .. } | PathExprV3::Reflexive { .. } => {}
    }
}

fn base_non_claims(
    closure_tier: RuntimeTheoryClosureTierV1,
    evidence_policy: &EvidencePolicyV1,
) -> Vec<RuntimeTheoryNonClaimV1> {
    let mut claims = vec![
        RuntimeTheoryNonClaimV1 {
            code: "not_lean_certified".to_string(),
            message: "runtime theory check report is not a Lean certificate".to_string(),
        },
        RuntimeTheoryNonClaimV1 {
            code: "not_global_ontology_truth".to_string(),
            message: "closure is scoped to declared worlds, evidence policy, imports, and runtime fragment".to_string(),
        },
        RuntimeTheoryNonClaimV1 {
            code: "not_full_hott".to_string(),
            message: "runtime checker does not claim univalence or full HoTT/topos semantics".to_string(),
        },
    ];

    if closure_tier == RuntimeTheoryClosureTierV1::EvidenceWeighted
        && !evidence_policy.weighted_propagation_enabled
    {
        claims.push(RuntimeTheoryNonClaimV1 {
            code: "weighted_propagation_disabled".to_string(),
            message: "evidence-weighted tier uses thresholded worlds only; semiring/lattice propagation is not enabled".to_string(),
        });
    }

    claims
}

fn closure_notes(
    closure_tier: RuntimeTheoryClosureTierV1,
    world: &WorldAssumptionV1,
    evidence_policy: &EvidencePolicyV1,
) -> Vec<String> {
    let mut notes = Vec::new();
    match closure_tier {
        RuntimeTheoryClosureTierV1::FiniteFragment => {
            notes.push("finite_fragment closure is over accepted finite compiled obligations and terminating runtime rules".to_string());
        }
        RuntimeTheoryClosureTierV1::EvidenceWeighted => {
            notes.push(format!(
                "evidence_weighted closure first filters obligations at {}ppm",
                evidence_policy.threshold_ppm
            ));
        }
        RuntimeTheoryClosureTierV1::GlobalIndexed => {
            notes.push(format!(
                "global_indexed closure is finite over refs/worlds/slices/imports: refs={}, worlds={}, slices={}, imports={}",
                world.included_refs.len(),
                world.included_worlds.len(),
                world.included_slices.len(),
                world.included_imports.len()
            ));
        }
    }
    notes
}

fn completeness_scope(
    closure_tier: RuntimeTheoryClosureTierV1,
    world: &WorldAssumptionV1,
) -> String {
    match closure_tier {
        RuntimeTheoryClosureTierV1::FiniteFragment => {
            format!("finite accepted world `{}`", world.world_id)
        }
        RuntimeTheoryClosureTierV1::EvidenceWeighted => {
            format!("thresholded evidence world `{}`", world.world_id)
        }
        RuntimeTheoryClosureTierV1::GlobalIndexed => {
            format!("declared global indexed universe `{}`", world.world_id)
        }
    }
}

fn completeness_assumptions(
    closure_tier: RuntimeTheoryClosureTierV1,
    world: &WorldAssumptionV1,
    evidence_policy: &EvidencePolicyV1,
) -> Vec<String> {
    let mut assumptions = vec![
        "compiled schema/category/theory ids are the semantic context".to_string(),
        "runtime closure covers only the supported terminating fragment".to_string(),
        "open-world unknowns are not converted into false facts".to_string(),
    ];
    if world.finite {
        assumptions.push("declared world is finite".to_string());
    } else {
        assumptions.push("declared world is not finite; closure is advisory".to_string());
    }
    match closure_tier {
        RuntimeTheoryClosureTierV1::FiniteFragment => {}
        RuntimeTheoryClosureTierV1::EvidenceWeighted => {
            assumptions.push(format!(
                "obligations below {}ppm are outside the thresholded world",
                evidence_policy.threshold_ppm
            ));
        }
        RuntimeTheoryClosureTierV1::GlobalIndexed => {
            assumptions.push("all refs/worlds/slices/imports must be explicitly declared".to_string());
        }
    }
    assumptions
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_dsl::axi_v1::parse_axi_v1;

    fn compiled_fixture(axi: &str) -> (CompiledSchemaIr, TheoryIr) {
        let module = parse_axi_v1(axi).expect("fixture parses");
        let kernel =
            crate::compile_kernel_module_ir(&module, axi).expect("fixture compiles to kernel IR");
        (kernel.schemas[0].clone(), kernel.theories[0].clone())
    }

    #[test]
    fn valid_path_equation_closes_under_finite_fragment() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation p_id:
    step(x,parent,y) =
    step(x,parent,y)
"#,
        );

        let report = check_runtime_theory_v1(&schema, &theory);

        assert_eq!(report.version, RUNTIME_THEORY_CHECK_REPORT_VERSION_V1);
        assert_eq!(report.blocked_obligations, 0);
        assert_eq!(report.checked_obligations, 1);
        assert!(report.closure.complete);
        assert!(report.ontology_closure_claim.claimed);
    }

    #[test]
    fn opaque_equation_remains_addressable_non_claim() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation business_axiom:
    parent is social =
    meaningful business rule
"#,
        );

        let report = check_runtime_theory_v1(&schema, &theory);

        assert_eq!(report.review_only_obligations, 1);
        assert!(!report.ontology_closure_claim.claimed);
        assert!(report.judgments[0]
            .non_claims
            .iter()
            .any(|claim| claim.code == "opaque_equation_not_closed"));
    }

    #[test]
    fn evidence_threshold_excludes_low_weight_obligation_deterministically() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation p_id:
    step(x,parent,y) =
    step(x,parent,y)
"#,
        );
        let obligation = theory.obligation_refs()[0].stable_id();
        let mut policy = default_evidence_policy_v1();
        policy.threshold_ppm = 750_000;
        policy.obligation_weights_ppm.insert(obligation, 250_000);

        let report = check_runtime_theory_with_options_v1(
            &schema,
            &theory,
            RuntimeTheoryClosureTierV1::EvidenceWeighted,
            default_world_assumption_v1(),
            policy,
            None,
        );

        assert_eq!(report.excluded_by_evidence, 1);
        assert_eq!(report.closure.considered_obligations, 0);
        assert!(report.closure.complete);
        assert!(report.judgments[0]
            .non_claims
            .iter()
            .any(|claim| claim.code == "excluded_by_evidence_threshold"));
    }

    #[test]
    fn global_indexed_rejects_undeclared_imports() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation p_id:
    step(x,parent,y) =
    step(x,parent,y)
"#,
        );
        let mut world = default_world_assumption_v1();
        world.undeclared_imports = vec!["external:unknown".to_string()];

        let report = check_runtime_theory_with_options_v1(
            &schema,
            &theory,
            RuntimeTheoryClosureTierV1::GlobalIndexed,
            world,
            default_evidence_policy_v1(),
            None,
        );

        assert_eq!(report.blocked_obligations, 1);
        assert!(!report.completeness_claim.claimed);
        assert!(report
            .non_claims
            .iter()
            .any(|claim| claim.code == "undeclared_global_imports"));
    }

    #[test]
    fn rewrite_that_drops_context_role_is_blocked() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  object Context
  relation ScopedParent(child: Person, parent: Person, ctx: Context)
  relation Parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  rewrite erase_context:
    vars: a: Person, b: Person
    lhs: step(a, ScopedParent, b)
    rhs: step(a, Parent, b)
"#,
        );

        let report = check_runtime_theory_v1(&schema, &theory);

        assert_eq!(report.blocked_obligations, 1);
        assert!(report.judgments[0]
            .residual_obligations
            .iter()
            .any(|residual| residual.contains("drops context or temporal axes")));
    }

    #[test]
    fn invalid_path_endpoint_fails_before_runtime_report() {
        let err = parse_axi_v1(
            r#"
module Family

schema Family:
  object Person
  object Org
  relation parent(child: Person, parent: Person)
  relation member(person: Person, org: Org)

theory FamilyTheory on Family:
  equation bad_endpoint:
    step(x,parent,y) =
    step(x,member,o)
"#,
        )
        .map_err(|err| err.to_string())
        .and_then(|module| crate::compile_kernel_module_ir(&module, "fixture").map(|_| ()))
        .expect_err("endpoint mismatch should fail compile");

        assert!(err.contains("mismatched path endpoints"));
    }

    #[test]
    fn rewrite_with_undeclared_variable_fails_before_runtime_report() {
        let err = parse_axi_v1(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  rewrite bad_var:
    vars: a: Person
    lhs: step(a, parent, b)
    rhs: step(a, parent, b)
"#,
        )
        .map_err(|err| err.to_string())
        .and_then(|module| crate::compile_kernel_module_ir(&module, "fixture").map(|_| ()))
        .expect_err("undeclared rewrite variable should fail compile");

        assert!(err.contains("unbound object variable"));
    }
}
