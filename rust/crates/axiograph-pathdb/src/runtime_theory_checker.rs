//! Runtime theory checking and closure reporting over compiled semantic IR.
//!
//! This module is intentionally operational: it produces typed reports for
//! authoring, CQ gates, migration, reconciliation, and agent tooling. It is not
//! the Lean trusted checker.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use axiograph_dsl::schema_v1::PathExprV3;

use crate::kernel_ir::{
    CompiledSchemaIr, KernelRefV1, RelationSemanticsIr, RoleKind, TheoryContextAxisRefIr,
    TheoryEndpointRefIr, TheoryIr, TheoryObligationDependencyIr, TheoryObligationRefIr,
    TheoryPathExpressionRefIr, TheoryPathStepRefIr, TheorySubjectRefIr, TheoryTransportItemRefIr,
    TheoryTransportPlanIr, TheoryTransportStatusIr, TheoryVariableRefIr,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryAxisRoleV1 {
    pub relation_id: String,
    pub relation_name: String,
    pub role_id: String,
    pub role_name: String,
    pub role_kind: RoleKind,
    pub target_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryTypedEndpointV1 {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_var: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_type: Option<String>,
    #[serde(default)]
    pub lhs_steps: usize,
    #[serde(default)]
    pub rhs_steps: usize,
    #[serde(default)]
    pub endpoint_preserved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub axis_roles: Vec<RuntimeTheoryAxisRoleV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryAdmissibilityDiagnosticV1 {
    pub code: String,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub admissible: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryAdmissibilityCheckV1 {
    pub check_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub code: String,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub admissible: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_expression_refs: Vec<TheoryPathExpressionRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_step_refs: Vec<TheoryPathStepRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variable_refs: Vec<TheoryVariableRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_refs: Vec<TheoryEndpointRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_refs: Vec<TheoryContextAxisRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transport_item_refs: Vec<TheoryTransportItemRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryClosureStepKindV1 {
    CheckedSeed,
    EvidenceFiltered,
    ReviewResidual,
    BlockingError,
    FixpointReached,
    FixpointBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryClosureStepV1 {
    pub step_index: u32,
    pub kind: RuntimeTheoryClosureStepKindV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_ref: Option<TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kernel_refs: Vec<KernelRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_subjects: Vec<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_obligations: Vec<String>,
    pub status: RuntimeTheoryCheckStatusV1,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub admissible: bool,
    pub complete_under_assumptions: bool,
    pub closed_under_assumptions: bool,
    pub evidence_weight_ppm: u32,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_status: Option<TheoryTransportStatusIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transport_basis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_object_images: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_arrow_images: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissibility_diagnostics: Vec<RuntimeTheoryAdmissibilityDiagnosticV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissibility_checks: Vec<RuntimeTheoryAdmissibilityCheckV1>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryAssumptionKindV1 {
    Fragment,
    World,
    Context,
    Evidence,
    RefUniverse,
    Import,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryAssumptionEffectV1 {
    SupportsClaim,
    NarrowsClaim,
    Residual,
    BlocksClaim,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryAssumptionDiagnosticV1 {
    pub assumption_id: String,
    pub kind: RuntimeTheoryAssumptionKindV1,
    pub effect: RuntimeTheoryAssumptionEffectV1,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimV1>,
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
    pub kernel_refs: Vec<KernelRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    pub status: RuntimeTheoryCheckStatusV1,
    pub severity: RuntimeTheoryCheckSeverityV1,
    pub closure_tier: RuntimeTheoryClosureTierV1,
    pub admissible: bool,
    pub complete_under_assumptions: bool,
    pub closed_under_assumptions: bool,
    pub evidence_weight_ppm: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_endpoint: Option<RuntimeTheoryTypedEndpointV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_status: Option<TheoryTransportStatusIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transport_basis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_object_images: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_arrow_images: Vec<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissibility_diagnostics: Vec<RuntimeTheoryAdmissibilityDiagnosticV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissibility_checks: Vec<RuntimeTheoryAdmissibilityCheckV1>,
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
    pub assumption_diagnostics: Vec<RuntimeTheoryAssumptionDiagnosticV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<RuntimeTheoryClosureStepV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryCheckReportV1 {
    pub version: String,
    pub schema_id: SchemaId,
    pub theory_ref: TheorySubjectRefIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kernel_refs: Vec<KernelRefV1>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissibility_checks: Vec<RuntimeTheoryAdmissibilityCheckV1>,
    pub closure: RuntimeTheoryClosureReportV1,
    pub completeness_claim: CompletenessClaimV1,
    pub ontology_closure_claim: OntologyClosureClaimV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumption_diagnostics: Vec<RuntimeTheoryAssumptionDiagnosticV1>,
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

    let mut evidence_propagation_iterations = 0;
    if closure_tier == RuntimeTheoryClosureTierV1::EvidenceWeighted
        && evidence_policy.weighted_propagation_enabled
        && evidence_policy.semantics == EvidenceWeightSemanticsV1::WeightedLattice
    {
        evidence_propagation_iterations = propagate_weighted_evidence(&mut judgments);
    }

    if closure_tier == RuntimeTheoryClosureTierV1::EvidenceWeighted {
        for judgment in &mut judgments {
            apply_evidence_threshold(&evidence_policy, judgment);
        }
    }

    let assumption_diagnostics =
        build_assumption_diagnostics(closure_tier, &world, &evidence_policy);
    let assumption_residuals = assumption_residuals(&assumption_diagnostics);

    let mut non_claims = base_non_claims(closure_tier, &evidence_policy);
    let mut notes = vec![
        "runtime theory checking is operational Rust checking, not a Lean certificate".to_string(),
        "checked obligations are well-scoped and admissible under the declared runtime fragment"
            .to_string(),
    ];

    if closure_tier == RuntimeTheoryClosureTierV1::GlobalIndexed {
        if world.included_refs.is_empty()
            && world.included_worlds.is_empty()
            && world.included_slices.is_empty()
            && world.included_imports.is_empty()
        {
            let residual =
                "global_indexed closure requires a finite declared ref/world/slice/import universe"
                    .to_string();
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
        .chain(assumption_residuals.iter().cloned())
        .collect::<Vec<_>>();

    if evidence_propagation_iterations > 0 {
        notes.push(format!(
            "weighted evidence propagation reached a finite fixpoint in {evidence_propagation_iterations} iteration(s)"
        ));
    }
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
    if !assumption_residuals.is_empty() {
        notes.push(format!(
            "{} declared assumption(s) block or leave residual closure claims",
            assumption_residuals.len()
        ));
    }

    let considered_obligations = total_obligations.saturating_sub(excluded_by_evidence);
    let complete = blocked_obligations == 0 && residual_obligations == 0 && residual_ids.is_empty();
    let closed = complete && residual_ids.is_empty();
    let fixpoint_reached = blocked_obligations == 0 && residual_ids.is_empty();
    let iterations = if considered_obligations == 0 {
        evidence_propagation_iterations
    } else {
        1 + evidence_propagation_iterations
    };
    let closure_steps =
        build_closure_steps(&judgments, fixpoint_reached, closed, &assumption_residuals);

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
        assumption_diagnostics: assumption_diagnostics.clone(),
        steps: closure_steps,
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

    let admissibility_checks = judgments
        .iter()
        .flat_map(|judgment| judgment.admissibility_checks.iter().cloned())
        .collect::<Vec<_>>();
    let kernel_refs = report_kernel_refs(theory, &judgments);

    RuntimeTheoryCheckReportV1 {
        version: RUNTIME_THEORY_CHECK_REPORT_VERSION_V1.to_string(),
        schema_id: compiled_schema.schema_id.clone(),
        theory_ref: TheorySubjectRefIr::Theory {
            theory_id: theory.theory_id.clone(),
        },
        kernel_refs,
        fragment: RuntimeTheoryFragmentV1 {
            closure_tier,
            supports_structured_constraints: true,
            supports_path_equations: true,
            supports_rewrites: true,
            supports_transports: true,
            supports_weighted_evidence: evidence_policy.weighted_propagation_enabled
                && evidence_policy.semantics == EvidenceWeightSemanticsV1::WeightedLattice,
            terminating_fragment: true,
            notes: vec![
                "finite saturation is traced over compiled obligations and their typed subject dependencies; recursive higher-order closure remains residual".to_string(),
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
        admissibility_checks,
        judgments,
        closure,
        completeness_claim,
        ontology_closure_claim,
        assumption_diagnostics,
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
    let mut typed_endpoint = None;
    let mut admissibility_diagnostics = Vec::new();
    let message: String;

    match &obligation_ref {
        TheoryObligationRefIr::Constraint { constraint_id, .. } => {
            match theory
                .constraints
                .iter()
                .find(|candidate| &candidate.constraint_id == constraint_id)
            {
                Some(constraint)
                    if constraint.relation_name.is_some() && constraint.relation_id.is_none() =>
                {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations.push(
                        "constraint relation did not resolve to a stable relation id".to_string(),
                    );
                    message = "constraint is not admissible because its relation ref is unresolved"
                        .to_string();
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
                        message: "constraint is addressable but not closed by the runtime checker"
                            .to_string(),
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
                    typed_endpoint =
                        Some(typed_endpoint_for_path_equation(compiled_schema, equation));
                    let missing = missing_relation_ids(compiled_schema, &equation.relation_ids);
                    if !missing.is_empty() {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(format!(
                            "path equation references missing relation ids: {}",
                            missing.join(", ")
                        ));
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "path_equation_relation_ids_missing",
                            severity,
                            false,
                            "path equation references relation ids missing from the compiled schema",
                            &subject_refs,
                            equation.relation_refs.clone(),
                        ));
                        message = "path equation is not endpoint-safe because a relation id is unresolved".to_string();
                    } else {
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "path_equation_endpoint_preserved",
                            RuntimeTheoryCheckSeverityV1::Info,
                            true,
                            "path equation endpoints matched during compiled-IR elaboration",
                            &subject_refs,
                            equation.relation_refs.clone(),
                        ));
                        message = "path equation relation refs resolve and endpoint safety was checked during compiled-IR elaboration".to_string();
                    }
                }
                None => {
                    status = RuntimeTheoryCheckStatusV1::Blocked;
                    severity = RuntimeTheoryCheckSeverityV1::Error;
                    admissible = false;
                    residual_obligations
                        .push("compiled path equation record is missing".to_string());
                    message =
                        "path equation index is inconsistent with compiled TheoryIr".to_string();
                }
            }
        }
        TheoryObligationRefIr::OpaqueEquation { .. } => {
            status = RuntimeTheoryCheckStatusV1::ReviewOnly;
            severity = RuntimeTheoryCheckSeverityV1::Warning;
            admissible = false;
            residual_obligations.push(
                "opaque equation is addressable but outside the runtime closure fragment"
                    .to_string(),
            );
            non_claims.push(RuntimeTheoryNonClaimV1 {
                code: "opaque_equation_not_closed".to_string(),
                message: "opaque equation is not runtime-certifiable or closed in this fragment"
                    .to_string(),
            });
            admissibility_diagnostics.push(admissibility_diagnostic(
                "opaque_equation_addressable_review_only",
                severity,
                false,
                "opaque equation keeps a stable obligation handle but has no runtime path endpoint proof",
                &subject_refs,
                Vec::new(),
            ));
            message = "opaque equation preserved as a typed review obligation".to_string();
        }
        TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
            match theory
                .rewrite_rules
                .iter()
                .find(|candidate| &candidate.rule_id == rule_id)
            {
                Some(rule) => {
                    typed_endpoint = Some(typed_endpoint_for_rewrite_rule(compiled_schema, rule));
                    let missing = missing_relation_ids(compiled_schema, &rule.relation_ids);
                    if !missing.is_empty() {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(format!(
                            "rewrite references missing relation ids: {}",
                            missing.join(", ")
                        ));
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "rewrite_relation_ids_missing",
                            severity,
                            false,
                            "rewrite references relation ids missing from the compiled schema",
                            &subject_refs,
                            rule.relation_refs.clone(),
                        ));
                        message =
                            "rewrite rule is not admissible because a relation id is unresolved"
                                .to_string();
                    } else if drops_axis_roles(compiled_schema, &rule.lhs, &rule.rhs) {
                        status = RuntimeTheoryCheckStatusV1::Blocked;
                        severity = RuntimeTheoryCheckSeverityV1::Error;
                        admissible = false;
                        residual_obligations.push(
                            "rewrite drops context or temporal axes from lhs to rhs".to_string(),
                        );
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "rewrite_axis_roles_dropped",
                            severity,
                            false,
                            "rewrite drops context or temporal role keys; make the residual explicit or preserve the scoped relation",
                            &subject_refs,
                            rule.relation_refs.clone(),
                        ));
                        message = "rewrite is blocked because context/time roles must be preserved or explicitly reviewed".to_string();
                    } else {
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "rewrite_endpoint_preserved",
                            RuntimeTheoryCheckSeverityV1::Info,
                            true,
                            "rewrite endpoints and declared variables remain stable across lhs and rhs",
                            &subject_refs,
                            rule.relation_refs.clone(),
                        ));
                        admissibility_diagnostics.push(admissibility_diagnostic(
                            "rewrite_axis_roles_preserved",
                            RuntimeTheoryCheckSeverityV1::Info,
                            true,
                            "rewrite preserves all context and temporal role keys present on the lhs",
                            &subject_refs,
                            rule.relation_refs.clone(),
                        ));
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
                    residual_obligations
                        .push("compiled rewrite rule record is missing".to_string());
                    message =
                        "rewrite rule index is inconsistent with compiled TheoryIr".to_string();
                }
            }
        }
    }

    let complete_under_assumptions = status == RuntimeTheoryCheckStatusV1::Checked;
    let closed_under_assumptions = status == RuntimeTheoryCheckStatusV1::Checked;
    let evidence_weight_ppm = obligation_weight_ppm(evidence_policy, &obligation_ref);
    let dependency = theory.obligation_dependencies(&obligation_ref);
    let admissibility_checks = admissibility_diagnostics
        .iter()
        .map(|diagnostic| admissibility_check(&obligation_ref, diagnostic, &dependency))
        .collect();
    let kernel_refs = theory_kernel_refs(theory, &obligation_ref, &subject_refs);

    RuntimeTheoryJudgmentV1 {
        label: obligation_ref.display_name(),
        obligation_ref,
        kernel_refs,
        subject_refs,
        status,
        severity,
        closure_tier,
        admissible,
        complete_under_assumptions,
        closed_under_assumptions,
        evidence_weight_ppm,
        typed_endpoint,
        transport_status: None,
        transport_basis: Vec::new(),
        missing_object_images: Vec::new(),
        missing_arrow_images: Vec::new(),
        message,
        residual_obligations,
        non_claims,
        admissibility_diagnostics,
        admissibility_checks,
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
            judgment.transport_status = Some(item.status);
            judgment.transport_basis = item.transport_basis.clone();
            judgment.missing_object_images = item.missing_object_images.clone();
            judgment.missing_arrow_images = item.missing_arrow_images.clone();
            match item.status {
                TheoryTransportStatusIr::Preserved => {
                    judgment
                        .message
                        .push_str("; transport preserved obligation");
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

fn propagate_weighted_evidence(judgments: &mut [RuntimeTheoryJudgmentV1]) -> u32 {
    let mut iterations = 0;
    let max_iterations = judgments.len().saturating_add(1) as u32;
    loop {
        let mut subject_min_weights: BTreeMap<String, u32> = BTreeMap::new();
        for judgment in judgments.iter() {
            for subject in dependency_subjects(&judgment.subject_refs) {
                subject_min_weights
                    .entry(subject.stable_id())
                    .and_modify(|weight| *weight = (*weight).min(judgment.evidence_weight_ppm))
                    .or_insert(judgment.evidence_weight_ppm);
            }
        }

        let mut changed = false;
        for judgment in judgments.iter_mut() {
            let propagated = dependency_subjects(&judgment.subject_refs)
                .into_iter()
                .filter_map(|subject| subject_min_weights.get(&subject.stable_id()).copied())
                .min()
                .unwrap_or(judgment.evidence_weight_ppm);
            if propagated < judgment.evidence_weight_ppm {
                judgment.evidence_weight_ppm = propagated;
                changed = true;
            }
        }

        if !changed {
            break;
        }
        iterations += 1;
        if iterations >= max_iterations {
            break;
        }
    }
    iterations
}

fn build_closure_steps(
    judgments: &[RuntimeTheoryJudgmentV1],
    fixpoint_reached: bool,
    closed: bool,
    assumption_residuals: &[String],
) -> Vec<RuntimeTheoryClosureStepV1> {
    let mut steps = judgments
        .iter()
        .enumerate()
        .map(|(index, judgment)| {
            let excluded_by_evidence = is_excluded_by_evidence_threshold(judgment);
            let kind = if excluded_by_evidence {
                RuntimeTheoryClosureStepKindV1::EvidenceFiltered
            } else {
                match judgment.status {
                    RuntimeTheoryCheckStatusV1::Checked => {
                        RuntimeTheoryClosureStepKindV1::CheckedSeed
                    }
                    RuntimeTheoryCheckStatusV1::ReviewOnly
                    | RuntimeTheoryCheckStatusV1::ResidualObligation => {
                        RuntimeTheoryClosureStepKindV1::ReviewResidual
                    }
                    RuntimeTheoryCheckStatusV1::Blocked => {
                        RuntimeTheoryClosureStepKindV1::BlockingError
                    }
                }
            };
            RuntimeTheoryClosureStepV1 {
                step_index: index as u32,
                kind,
                obligation_ref: Some(judgment.obligation_ref.clone()),
                kernel_refs: judgment.kernel_refs.clone(),
                dependency_subjects: dependency_subjects(&judgment.subject_refs),
                derived_obligations: if judgment.status == RuntimeTheoryCheckStatusV1::Checked {
                    vec![judgment.obligation_ref.stable_id()]
                } else {
                    Vec::new()
                },
                status: judgment.status,
                severity: judgment.severity,
                admissible: judgment.admissible,
                complete_under_assumptions: judgment.complete_under_assumptions,
                closed_under_assumptions: judgment.closed_under_assumptions,
                evidence_weight_ppm: judgment.evidence_weight_ppm,
                detail: judgment.message.clone(),
                transport_status: judgment.transport_status,
                transport_basis: judgment.transport_basis.clone(),
                missing_object_images: judgment.missing_object_images.clone(),
                missing_arrow_images: judgment.missing_arrow_images.clone(),
                residual_obligations: judgment.residual_obligations.clone(),
                non_claims: judgment.non_claims.clone(),
                admissibility_diagnostics: judgment.admissibility_diagnostics.clone(),
                admissibility_checks: judgment.admissibility_checks.clone(),
            }
        })
        .collect::<Vec<_>>();

    steps.push(RuntimeTheoryClosureStepV1 {
        step_index: steps.len() as u32,
        kind: if fixpoint_reached {
            RuntimeTheoryClosureStepKindV1::FixpointReached
        } else {
            RuntimeTheoryClosureStepKindV1::FixpointBlocked
        },
        obligation_ref: None,
        kernel_refs: judgments
            .iter()
            .flat_map(|judgment| judgment.kernel_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        dependency_subjects: Vec::new(),
        derived_obligations: if closed {
            judgments
                .iter()
                .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Checked)
                .map(|judgment| judgment.obligation_ref.stable_id())
                .collect()
        } else {
            Vec::new()
        },
        status: if fixpoint_reached {
            RuntimeTheoryCheckStatusV1::Checked
        } else if judgments
            .iter()
            .any(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Blocked)
        {
            RuntimeTheoryCheckStatusV1::Blocked
        } else {
            RuntimeTheoryCheckStatusV1::ResidualObligation
        },
        severity: if fixpoint_reached {
            RuntimeTheoryCheckSeverityV1::Info
        } else if judgments
            .iter()
            .any(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Blocked)
        {
            RuntimeTheoryCheckSeverityV1::Error
        } else {
            RuntimeTheoryCheckSeverityV1::Warning
        },
        admissible: fixpoint_reached,
        complete_under_assumptions: fixpoint_reached,
        closed_under_assumptions: closed,
        evidence_weight_ppm: 1_000_000,
        detail: if fixpoint_reached {
            "runtime closure reached a finite fixpoint over in-scope typed obligations".to_string()
        } else {
            "runtime closure stopped with blocking or residual obligations".to_string()
        },
        transport_status: None,
        transport_basis: Vec::new(),
        missing_object_images: Vec::new(),
        missing_arrow_images: Vec::new(),
        residual_obligations: judgments
            .iter()
            .filter(|judgment| {
                matches!(
                    judgment.status,
                    RuntimeTheoryCheckStatusV1::ReviewOnly
                        | RuntimeTheoryCheckStatusV1::ResidualObligation
                        | RuntimeTheoryCheckStatusV1::Blocked
                ) && !is_excluded_by_evidence_threshold(judgment)
            })
            .map(|judgment| judgment.obligation_ref.stable_id())
            .chain(assumption_residuals.iter().cloned())
            .collect(),
        non_claims: Vec::new(),
        admissibility_diagnostics: Vec::new(),
        admissibility_checks: Vec::new(),
    });

    steps
}

fn dependency_subjects(subject_refs: &[TheorySubjectRefIr]) -> Vec<TheorySubjectRefIr> {
    subject_refs
        .iter()
        .filter(|subject| !matches!(subject, TheorySubjectRefIr::Theory { .. }))
        .cloned()
        .collect()
}

fn theory_kernel_refs(
    theory: &TheoryIr,
    obligation_ref: &TheoryObligationRefIr,
    subject_refs: &[TheorySubjectRefIr],
) -> Vec<KernelRefV1> {
    let mut refs = BTreeSet::new();
    refs.insert(KernelRefV1::Theory {
        theory_id: theory.theory_id.clone(),
        schema_id: theory.schema_id.clone(),
    });
    refs.insert(KernelRefV1::TheoryObligation {
        obligation: obligation_ref.clone(),
    });
    for subject in subject_refs {
        refs.insert(KernelRefV1::TheorySubject {
            theory_id: theory.theory_id.clone(),
            subject: subject.clone(),
        });
    }
    refs.into_iter().collect()
}

fn report_kernel_refs(
    theory: &TheoryIr,
    judgments: &[RuntimeTheoryJudgmentV1],
) -> Vec<KernelRefV1> {
    let mut refs = BTreeSet::new();
    refs.insert(KernelRefV1::Theory {
        theory_id: theory.theory_id.clone(),
        schema_id: theory.schema_id.clone(),
    });
    for judgment in judgments {
        refs.extend(judgment.kernel_refs.iter().cloned());
    }
    refs.into_iter().collect()
}

fn is_excluded_by_evidence_threshold(judgment: &RuntimeTheoryJudgmentV1) -> bool {
    judgment
        .non_claims
        .iter()
        .any(|claim| claim.code == "excluded_by_evidence_threshold")
}

fn admissibility_diagnostic(
    code: &str,
    severity: RuntimeTheoryCheckSeverityV1,
    admissible: bool,
    message: &str,
    subject_refs: &[TheorySubjectRefIr],
    relation_names: Vec<String>,
) -> RuntimeTheoryAdmissibilityDiagnosticV1 {
    RuntimeTheoryAdmissibilityDiagnosticV1 {
        code: code.to_string(),
        severity,
        admissible,
        message: message.to_string(),
        subject_refs: dependency_subjects(subject_refs),
        relation_names,
    }
}

fn admissibility_check(
    obligation_ref: &TheoryObligationRefIr,
    diagnostic: &RuntimeTheoryAdmissibilityDiagnosticV1,
    dependency: &TheoryObligationDependencyIr,
) -> RuntimeTheoryAdmissibilityCheckV1 {
    RuntimeTheoryAdmissibilityCheckV1 {
        check_id: format!(
            "admissibility_check:{}:{}",
            obligation_ref.stable_id(),
            diagnostic.code
        ),
        obligation_ref: obligation_ref.clone(),
        code: diagnostic.code.clone(),
        severity: diagnostic.severity,
        admissible: diagnostic.admissible,
        message: diagnostic.message.clone(),
        subject_refs: dependency_subjects(&dependency.subject_refs),
        path_expression_refs: dependency.path_expression_refs.clone(),
        path_step_refs: dependency.path_step_refs.clone(),
        variable_refs: dependency.variable_refs.clone(),
        endpoint_refs: dependency.endpoint_refs.clone(),
        context_refs: dependency.context_refs.clone(),
        transport_item_refs: dependency.transport_item_refs.clone(),
        relation_names: diagnostic.relation_names.clone(),
    }
}

fn assumption_residuals(diagnostics: &[RuntimeTheoryAssumptionDiagnosticV1]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.effect,
                RuntimeTheoryAssumptionEffectV1::Residual
                    | RuntimeTheoryAssumptionEffectV1::BlocksClaim
            )
        })
        .map(|diagnostic| format!("assumption:{}", diagnostic.assumption_id))
        .collect()
}

fn build_assumption_diagnostics(
    closure_tier: RuntimeTheoryClosureTierV1,
    world: &WorldAssumptionV1,
    evidence_policy: &EvidencePolicyV1,
) -> Vec<RuntimeTheoryAssumptionDiagnosticV1> {
    let mut diagnostics = vec![
        assumption_diagnostic(
            "runtime_fragment_terminating",
            RuntimeTheoryAssumptionKindV1::Fragment,
            RuntimeTheoryAssumptionEffectV1::SupportsClaim,
            RuntimeTheoryCheckSeverityV1::Info,
            closure_tier,
            "runtime closure is over the declared terminating Rust fragment, not full HoTT/topos semantics",
            Vec::new(),
            vec![RuntimeTheoryNonClaimV1 {
                code: "not_full_hott".to_string(),
                message: "no univalence, higher inductive, or topos-complete proof is claimed"
                    .to_string(),
            }],
        ),
        if world.finite {
            assumption_diagnostic(
                "world_is_finite",
                RuntimeTheoryAssumptionKindV1::World,
                RuntimeTheoryAssumptionEffectV1::SupportsClaim,
                RuntimeTheoryCheckSeverityV1::Info,
                closure_tier,
                "declared world is finite for runtime saturation",
                vec![world.world_id.clone()],
                Vec::new(),
            )
        } else {
            assumption_diagnostic(
                "world_is_finite",
                RuntimeTheoryAssumptionKindV1::World,
                RuntimeTheoryAssumptionEffectV1::Residual,
                RuntimeTheoryCheckSeverityV1::Warning,
                closure_tier,
                "declared world is not finite, so runtime closure remains advisory",
                vec![world.world_id.clone()],
                vec![RuntimeTheoryNonClaimV1 {
                    code: "nonfinite_world_not_closed".to_string(),
                    message: "finite runtime saturation is not a closure proof for this world"
                        .to_string(),
                }],
            )
        },
    ];

    diagnostics.push(if world.closed_contexts.is_empty() {
        assumption_diagnostic(
            "context_scope_named",
            RuntimeTheoryAssumptionKindV1::Context,
            RuntimeTheoryAssumptionEffectV1::NarrowsClaim,
            RuntimeTheoryCheckSeverityV1::Info,
            closure_tier,
            "no named closed context set is declared; context/time roles remain typed axes but are not exhaustively closed as worlds",
            Vec::new(),
            vec![RuntimeTheoryNonClaimV1 {
                code: "context_axes_not_world_complete".to_string(),
                message: "role preservation is checked, but exhaustive context/world closure is not claimed"
                    .to_string(),
            }],
        )
    } else {
        assumption_diagnostic(
            "context_scope_named",
            RuntimeTheoryAssumptionKindV1::Context,
            RuntimeTheoryAssumptionEffectV1::SupportsClaim,
            RuntimeTheoryCheckSeverityV1::Info,
            closure_tier,
            "closed context set is explicitly declared",
            world.closed_contexts.clone(),
            Vec::new(),
        )
    });

    match closure_tier {
        RuntimeTheoryClosureTierV1::FiniteFragment => {
            diagnostics.push(assumption_diagnostic(
                "finite_ref_universe",
                RuntimeTheoryAssumptionKindV1::RefUniverse,
                RuntimeTheoryAssumptionEffectV1::SupportsClaim,
                RuntimeTheoryCheckSeverityV1::Info,
                closure_tier,
                "finite_fragment uses the accepted module/ref slice supplied to the checker",
                world.included_refs.clone(),
                Vec::new(),
            ));
        }
        RuntimeTheoryClosureTierV1::EvidenceWeighted => {
            let (effect, severity, message, non_claims) = if evidence_policy
                .weighted_propagation_enabled
                && evidence_policy.semantics == EvidenceWeightSemanticsV1::WeightedLattice
            {
                (
                    RuntimeTheoryAssumptionEffectV1::SupportsClaim,
                    RuntimeTheoryCheckSeverityV1::Info,
                    "weighted_lattice evidence propagation is enabled over shared typed subjects",
                    Vec::new(),
                )
            } else if evidence_policy.weighted_propagation_enabled {
                (
                    RuntimeTheoryAssumptionEffectV1::NarrowsClaim,
                    RuntimeTheoryCheckSeverityV1::Warning,
                    "weighted propagation was requested, but only weighted_lattice semantics carry that runtime claim",
                    vec![RuntimeTheoryNonClaimV1 {
                        code: "weighted_propagation_not_claimed".to_string(),
                        message: "evidence weights are retained, but lattice propagation is not claimed"
                            .to_string(),
                    }],
                )
            } else {
                (
                    RuntimeTheoryAssumptionEffectV1::NarrowsClaim,
                    RuntimeTheoryCheckSeverityV1::Info,
                    "evidence tier uses thresholded-world filtering without weighted propagation",
                    vec![RuntimeTheoryNonClaimV1 {
                        code: "threshold_only_evidence".to_string(),
                        message: "no graded or semiring-style evidence closure is claimed"
                            .to_string(),
                    }],
                )
            };
            diagnostics.push(assumption_diagnostic(
                "evidence_policy",
                RuntimeTheoryAssumptionKindV1::Evidence,
                effect,
                severity,
                closure_tier,
                message,
                vec![
                    format!("policy={}", evidence_policy.policy_id),
                    format!("threshold_ppm={}", evidence_policy.threshold_ppm),
                    format!("semantics={:?}", evidence_policy.semantics),
                ],
                non_claims,
            ));
        }
        RuntimeTheoryClosureTierV1::GlobalIndexed => {
            let declared = world
                .included_refs
                .iter()
                .chain(world.included_worlds.iter())
                .chain(world.included_slices.iter())
                .chain(world.included_imports.iter())
                .cloned()
                .collect::<Vec<_>>();
            diagnostics.push(if declared.is_empty() {
                assumption_diagnostic(
                    "global_indexed_universe_declared",
                    RuntimeTheoryAssumptionKindV1::RefUniverse,
                    RuntimeTheoryAssumptionEffectV1::Residual,
                    RuntimeTheoryCheckSeverityV1::Warning,
                    closure_tier,
                    "global_indexed closure requires a finite declared ref/world/slice/import universe",
                    Vec::new(),
                    Vec::new(),
                )
            } else {
                assumption_diagnostic(
                    "global_indexed_universe_declared",
                    RuntimeTheoryAssumptionKindV1::RefUniverse,
                    RuntimeTheoryAssumptionEffectV1::SupportsClaim,
                    RuntimeTheoryCheckSeverityV1::Info,
                    closure_tier,
                    "global_indexed closure is scoped to the declared finite universe",
                    declared,
                    Vec::new(),
                )
            });
        }
    }

    if !world.undeclared_imports.is_empty() {
        diagnostics.push(assumption_diagnostic(
            "undeclared_imports_absent",
            RuntimeTheoryAssumptionKindV1::Import,
            RuntimeTheoryAssumptionEffectV1::BlocksClaim,
            RuntimeTheoryCheckSeverityV1::Error,
            closure_tier,
            "undeclared imports are outside the finite closure universe",
            world.undeclared_imports.clone(),
            Vec::new(),
        ));
    }

    diagnostics
}

fn assumption_diagnostic(
    assumption_id: &str,
    kind: RuntimeTheoryAssumptionKindV1,
    effect: RuntimeTheoryAssumptionEffectV1,
    severity: RuntimeTheoryCheckSeverityV1,
    closure_tier: RuntimeTheoryClosureTierV1,
    message: &str,
    values: Vec<String>,
    non_claims: Vec<RuntimeTheoryNonClaimV1>,
) -> RuntimeTheoryAssumptionDiagnosticV1 {
    RuntimeTheoryAssumptionDiagnosticV1 {
        assumption_id: assumption_id.to_string(),
        kind,
        effect,
        severity,
        closure_tier,
        message: message.to_string(),
        values,
        non_claims,
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

fn missing_relation_ids(
    compiled_schema: &CompiledSchemaIr,
    relation_ids: &[crate::RelationId],
) -> Vec<String> {
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

fn typed_endpoint_for_path_equation(
    compiled_schema: &CompiledSchemaIr,
    equation: &crate::kernel_ir::PathEquationIr,
) -> RuntimeTheoryTypedEndpointV1 {
    let relation_names = relation_names_for_paths(&[&equation.lhs, &equation.rhs]);
    RuntimeTheoryTypedEndpointV1 {
        source: "path_equation".to_string(),
        from_var: None,
        to_var: None,
        from_type: None,
        to_type: None,
        lhs_steps: path_step_count(&equation.lhs),
        rhs_steps: path_step_count(&equation.rhs),
        endpoint_preserved: true,
        relation_ids: equation
            .relation_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        axis_roles: axis_roles_for_relation_names(compiled_schema, &relation_names),
        relation_names,
    }
}

fn typed_endpoint_for_rewrite_rule(
    compiled_schema: &CompiledSchemaIr,
    rule: &crate::kernel_ir::RewriteRuleIr,
) -> RuntimeTheoryTypedEndpointV1 {
    let relation_names = relation_names_for_paths(&[&rule.lhs, &rule.rhs]);
    RuntimeTheoryTypedEndpointV1 {
        source: "rewrite_rule".to_string(),
        from_var: Some(rule.endpoint.from_var.clone()),
        to_var: Some(rule.endpoint.to_var.clone()),
        from_type: Some(rule.endpoint.from_type.clone()),
        to_type: Some(rule.endpoint.to_type.clone()),
        lhs_steps: path_step_count(&rule.lhs),
        rhs_steps: path_step_count(&rule.rhs),
        endpoint_preserved: true,
        relation_ids: rule.relation_ids.iter().map(ToString::to_string).collect(),
        axis_roles: axis_roles_for_relation_names(compiled_schema, &relation_names),
        relation_names,
    }
}

fn relation_names_for_paths(paths: &[&PathExprV3]) -> Vec<String> {
    let mut relation_names = BTreeSet::new();
    for path in paths {
        collect_path_step_relation_names(path, &mut relation_names);
    }
    relation_names.into_iter().collect()
}

fn path_step_count(path: &PathExprV3) -> usize {
    match path {
        PathExprV3::Step { .. } => 1,
        PathExprV3::Trans { left, right } => path_step_count(left) + path_step_count(right),
        PathExprV3::Inv { path } => path_step_count(path),
        PathExprV3::Var { .. } | PathExprV3::Reflexive { .. } => 0,
    }
}

fn axis_roles_for_relation_names(
    compiled_schema: &CompiledSchemaIr,
    relation_names: &[String],
) -> Vec<RuntimeTheoryAxisRoleV1> {
    let mut roles = relation_names
        .iter()
        .filter_map(|relation_name| relation_by_name(compiled_schema, relation_name))
        .flat_map(|relation| {
            relation.roles.iter().filter_map(move |role| {
                if matches!(role.kind, RoleKind::Context | RoleKind::Temporal) {
                    Some(RuntimeTheoryAxisRoleV1 {
                        relation_id: relation.relation_id.to_string(),
                        relation_name: relation.name.clone(),
                        role_id: role.role_id.to_string(),
                        role_name: role.name.clone(),
                        role_kind: role.kind,
                        target_type: role.target_type.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    roles.sort_by(|left, right| {
        (
            left.relation_name.as_str(),
            left.role_name.as_str(),
            left.target_type.as_str(),
        )
            .cmp(&(
                right.relation_name.as_str(),
                right.role_name.as_str(),
                right.target_type.as_str(),
            ))
    });
    roles.dedup_by(|left, right| {
        left.relation_id == right.relation_id
            && left.role_id == right.role_id
            && left.role_kind == right.role_kind
    });
    roles
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
                    keys.insert(format!(
                        "{:?}:{}:{}",
                        role.kind, role.name, role.target_type
                    ));
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
    if closure_tier == RuntimeTheoryClosureTierV1::EvidenceWeighted
        && evidence_policy.weighted_propagation_enabled
        && evidence_policy.semantics != EvidenceWeightSemanticsV1::WeightedLattice
    {
        claims.push(RuntimeTheoryNonClaimV1 {
            code: "weighted_propagation_requires_weighted_lattice".to_string(),
            message:
                "weighted propagation is only claimed for the weighted_lattice evidence semantics"
                    .to_string(),
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
            if evidence_policy.weighted_propagation_enabled
                && evidence_policy.semantics == EvidenceWeightSemanticsV1::WeightedLattice
            {
                notes.push("weighted_lattice propagation conservatively lowers obligations across shared non-theory subjects before thresholding".to_string());
            }
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
            assumptions
                .push("all refs/worlds/slices/imports must be explicitly declared".to_string());
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
        let endpoint = report.judgments[0]
            .typed_endpoint
            .as_ref()
            .expect("typed path-equation endpoint payload");
        assert_eq!(endpoint.source, "path_equation");
        assert_eq!(endpoint.lhs_steps, 1);
        assert_eq!(endpoint.rhs_steps, 1);
        assert_eq!(endpoint.relation_names, vec!["parent".to_string()]);
        assert!(endpoint.endpoint_preserved);
        assert!(report.closure.steps.iter().any(|step| {
            step.kind == RuntimeTheoryClosureStepKindV1::CheckedSeed
                && step
                    .obligation_ref
                    .as_ref()
                    .is_some_and(|obligation| obligation.display_name() == "p_id")
        }));
        assert_eq!(
            report.closure.steps.last().map(|step| step.kind),
            Some(RuntimeTheoryClosureStepKindV1::FixpointReached)
        );
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
        assert!(!report.completeness_claim.claimed);
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
        assert!(report
            .closure
            .steps
            .iter()
            .any(|step| { step.kind == RuntimeTheoryClosureStepKindV1::EvidenceFiltered }));
        assert!(report.judgments[0]
            .non_claims
            .iter()
            .any(|claim| claim.code == "excluded_by_evidence_threshold"));
    }

    #[test]
    fn weighted_lattice_propagates_low_weight_across_shared_relation_subject() {
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

  equation p_id_again:
    step(a,parent,b) =
    step(a,parent,b)
"#,
        );
        let obligations = theory.obligation_refs();
        let mut policy = default_evidence_policy_v1();
        policy.threshold_ppm = 750_000;
        policy.semantics = EvidenceWeightSemanticsV1::WeightedLattice;
        policy.weighted_propagation_enabled = true;
        policy
            .obligation_weights_ppm
            .insert(obligations[0].stable_id(), 250_000);
        policy
            .obligation_weights_ppm
            .insert(obligations[1].stable_id(), 1_000_000);

        let report = check_runtime_theory_with_options_v1(
            &schema,
            &theory,
            RuntimeTheoryClosureTierV1::EvidenceWeighted,
            default_world_assumption_v1(),
            policy,
            None,
        );

        assert!(report.fragment.supports_weighted_evidence);
        assert_eq!(report.excluded_by_evidence, 2);
        assert!(report
            .judgments
            .iter()
            .all(|judgment| judgment.evidence_weight_ppm == 250_000));
        assert_eq!(report.closure.iterations, 1);
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("weighted evidence propagation")));
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
    fn transport_status_is_structured_in_runtime_judgment_and_closure_step() {
        let (schema, theory) = compiled_fixture(
            r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint functional parent.child -> parent.parent
"#,
        );
        let morphism = crate::migration::SchemaMorphismV1 {
            source_schema: schema.schema_id.to_string(),
            target_schema: "FamilyV2".to_string(),
            objects: vec![crate::migration::ObjectMappingV1 {
                source_object: "Person".to_string(),
                target_object: "Human".to_string(),
            }],
            arrows: vec![crate::migration::ArrowMappingV1 {
                source_arrow: "parent".to_string(),
                target_path: vec!["parent".to_string()],
            }],
        };
        let transport_plan = theory.theory_transport_plan(
            &schema,
            &morphism,
            crate::migration::MigrationFunctorKindV1::DeltaF,
        );

        let report = check_runtime_theory_with_options_v1(
            &schema,
            &theory,
            RuntimeTheoryClosureTierV1::FiniteFragment,
            default_world_assumption_v1(),
            default_evidence_policy_v1(),
            Some(&transport_plan),
        );

        assert_eq!(
            report.judgments[0].transport_status,
            Some(TheoryTransportStatusIr::Transported)
        );
        assert_eq!(
            report.judgments[0].status,
            RuntimeTheoryCheckStatusV1::ResidualObligation
        );
        assert!(report.judgments[0]
            .transport_basis
            .iter()
            .any(|basis| basis == "object Person -> Human"));
        assert_eq!(
            report.closure.steps[0].transport_status,
            Some(TheoryTransportStatusIr::Transported)
        );
        assert!(!report.ontology_closure_claim.claimed);
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
        let endpoint = report.judgments[0]
            .typed_endpoint
            .as_ref()
            .expect("typed rewrite endpoint payload");
        assert_eq!(endpoint.source, "rewrite_rule");
        assert_eq!(endpoint.from_type.as_deref(), Some("Person"));
        assert_eq!(endpoint.to_type.as_deref(), Some("Person"));
        assert_eq!(endpoint.lhs_steps, 1);
        assert_eq!(endpoint.rhs_steps, 1);
        assert!(endpoint
            .axis_roles
            .iter()
            .any(|role| role.role_kind == RoleKind::Context && role.role_name == "ctx"));
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
