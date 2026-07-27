use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use axiograph_dsl::schema_v1::{SchemaV1Module, SchemaV1Schema};
use axiograph_pathdb::certificate::AxiWellTypedProofV1;
use axiograph_pathdb::kernel_ir::{
    RelationSemanticsIr, RoleKind, RuntimeModuleIndex, RuntimeSchemaIndex, TheoryIr,
};
use axiograph_pathdb::SchemaId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedAuthoringTrustV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default = "default_non_claimed")]
    pub completeness_claim: String,
    #[serde(default = "default_non_claimed")]
    pub ontology_closure_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedAuthoringSummaryV1 {
    pub lifecycle_state: String,
    pub trust: TypedAuthoringTrustV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_well_typed_proof_v1: Option<AxiWellTypedProofV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_module_ir: Option<RuntimeModuleIndex>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OlogDiagnosticSeverityV1 {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogDiagnosticV1 {
    pub severity: OlogDiagnosticSeverityV1,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OlogTypedHoleKindV1 {
    MissingRelationRoleBinding,
    RoleBindingTypeMismatch,
    ProjectionRoleUnbound,
    PathEndpointMismatch,
    UnknownPathAspect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogTypedHoleV1 {
    pub kind: OlogTypedHoleKindV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_box: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equation_id: Option<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissible_player_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_boxes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_subject_ref: Option<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogBoxV1 {
    pub box_id: String,
    pub object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogRoleBindingV1 {
    pub role: String,
    pub target_box: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogRelationBoxV1 {
    pub box_id: String,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_bindings: Vec<OlogRoleBindingV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OlogAspectKindV1 {
    RelationCarrier { relation: String },
    RelationProjection { relation_box: String, role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogAspectV1 {
    pub aspect_id: String,
    pub from_box: String,
    pub to_box: String,
    pub kind: OlogAspectKindV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OlogPathV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogPathEquationV1 {
    pub equation_id: String,
    pub lhs: OlogPathV1,
    pub rhs: OlogPathV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OlogFragmentV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boxes: Vec<OlogBoxV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_boxes: Vec<OlogRelationBoxV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aspects: Vec<OlogAspectV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_equations: Vec<OlogPathEquationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckedOlogFragmentV1 {
    pub lifecycle_state: String,
    pub ok: bool,
    pub trust: TypedAuthoringTrustV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<SchemaId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_well_typed_proof_v1: Option<AxiWellTypedProofV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_change: Option<crate::evolution_preview::TypedChangeSummaryV1>,
    pub fragment: OlogFragmentV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<OlogDiagnosticV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typed_holes: Vec<OlogTypedHoleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV2>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverCheckOlogReportV1 {
    pub version: String,
    pub checked_olog: CheckedOlogFragmentV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_refinement: Option<OlogRefinementApplyResultV1>,
}

fn empty_quality_report(input: &str) -> crate::quality::QualityReportV1 {
    crate::quality::QualityReportV1 {
        version: "quality_report_v1".to_string(),
        generated_at_unix_secs: 0,
        input: input.to_string(),
        profile: "typed_olog_authoring".to_string(),
        plane: "review".to_string(),
        summary: crate::quality::QualitySummaryV1::default(),
        findings: Vec::new(),
    }
}

fn default_non_claimed() -> String {
    "not_claimed".to_string()
}

fn trust(
    trust_class: &str,
    soundness: &str,
    coverage: &str,
    scope: &str,
    reasons: Vec<String>,
) -> TypedAuthoringTrustV1 {
    TypedAuthoringTrustV1 {
        trust_class: trust_class.to_string(),
        soundness: soundness.to_string(),
        coverage: coverage.to_string(),
        scope: scope.to_string(),
        reasons,
        completeness_claim: default_non_claimed(),
        ontology_closure_claim: default_non_claimed(),
    }
}

pub fn draft_typed_authoring_summary_from_axi_text(axi_text: &str) -> TypedAuthoringSummaryV1 {
    match axiograph_dsl::schema_v1::parse_schema_v1(axi_text) {
        Ok(module) => {
            let compiled_kernel_module_ir =
                axiograph_pathdb::derive_runtime_module_index(&module, axi_text);
            match axiograph_pathdb::validate_axi_v1_module(module) {
                Ok(validated) => {
                    let proof = validated.proof().clone();
                    let mut reasons = vec![
                    "draft parses as canonical .axi".to_string(),
                    "module passed the Rust-side well-typed module gate".to_string(),
                    "review/promotion and Lean-side certificate checking are still separate steps"
                        .to_string(),
                ];
                    let kernel_module_ir = match compiled_kernel_module_ir {
                        Ok(kernel_module_ir) => Some(kernel_module_ir),
                        Err(err) => {
                            reasons.push(format!(
                            "compiled kernel module IR could not be produced for this validated draft: {err}"
                        ));
                            None
                        }
                    };
                    TypedAuthoringSummaryV1 {
                        lifecycle_state: "validated".to_string(),
                        trust: trust(
                            "validated_draft",
                            "rust_side_well_typed_module_check",
                            "draft_module_only",
                            "canonical_axi_draft",
                            reasons,
                        ),
                        axi_well_typed_proof_v1: Some(proof),
                        kernel_module_ir,
                    }
                }
                Err(err) => TypedAuthoringSummaryV1 {
                    lifecycle_state: "draft_only".to_string(),
                    trust: trust(
                        "draft_only",
                        "not_yet_well_typed",
                        "draft_module_only",
                        "canonical_axi_draft",
                        vec![format!(
                            "draft did not pass the Rust-side well-typed module gate: {err}"
                        )],
                    ),
                    axi_well_typed_proof_v1: None,
                    kernel_module_ir: None,
                },
            }
        }
        Err(err) => TypedAuthoringSummaryV1 {
            lifecycle_state: "draft_only".to_string(),
            trust: trust(
                "draft_only",
                "not_yet_well_typed",
                "draft_module_only",
                "canonical_axi_draft",
                vec![format!("draft did not parse as canonical .axi: {err}")],
            ),
            axi_well_typed_proof_v1: None,
            kernel_module_ir: None,
        },
    }
}

fn error(
    diagnostics: &mut Vec<OlogDiagnosticV1>,
    code: &str,
    message: impl Into<String>,
    repair_hint: impl Into<Option<String>>,
) {
    diagnostics.push(OlogDiagnosticV1 {
        severity: OlogDiagnosticSeverityV1::Error,
        code: code.to_string(),
        message: message.into(),
        repair_hint: repair_hint.into(),
    });
}

fn warning(
    diagnostics: &mut Vec<OlogDiagnosticV1>,
    code: &str,
    message: impl Into<String>,
    repair_hint: impl Into<Option<String>>,
) {
    diagnostics.push(OlogDiagnosticV1 {
        severity: OlogDiagnosticSeverityV1::Warning,
        code: code.to_string(),
        message: message.into(),
        repair_hint: repair_hint.into(),
    });
}

fn candidate_boxes_for_types(
    compiled_ir: &RuntimeSchemaIndex,
    object_boxes: &HashMap<String, String>,
    acceptable_types: &[String],
) -> Vec<String> {
    let acceptable: HashSet<&str> = acceptable_types.iter().map(String::as_str).collect();
    let mut candidates = object_boxes
        .iter()
        .filter_map(|(box_id, object_type)| {
            if acceptable.contains(object_type.as_str())
                || acceptable_types
                    .iter()
                    .any(|expected| compiled_ir.type_matches_or_subtypes(object_type, expected))
            {
                Some(box_id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
}

fn olog_bind_role_refinement_candidates(
    relation_box: &str,
    relation: &str,
    role: &str,
    target_type: &str,
    candidate_boxes: &[String],
    retarget: bool,
) -> Vec<crate::typed_refinement::RuntimeRefinementCandidateV2> {
    candidate_boxes
        .iter()
        .map(|target_box| {
            let op = if retarget {
                crate::typed_refinement::OlogRefinementOpV1::RetargetRelationRole {
                    relation_box: relation_box.to_string(),
                    role: role.to_string(),
                    target_box: target_box.clone(),
                }
            } else {
                crate::typed_refinement::OlogRefinementOpV1::BindRelationRole {
                    relation_box: relation_box.to_string(),
                    role: role.to_string(),
                    target_box: target_box.clone(),
                }
            };
            crate::typed_refinement::RuntimeRefinementCandidateV2::new_olog(
                if retarget {
                    crate::typed_refinement::RuntimeRefinementCandidateKindV2::RetargetRelationRole
                } else {
                    crate::typed_refinement::RuntimeRefinementCandidateKindV2::BindRelationRole
                },
                if retarget {
                    format!(
                        "retarget role `{role}` on relation box `{relation_box}` to box `{target_box}`"
                    )
                } else {
                    format!(
                        "bind role `{role}` on relation box `{relation_box}` to box `{target_box}`"
                    )
                },
                op,
                Some(relation.to_string()),
                Some(role.to_string()),
                Some(target_type.to_string()),
                Some(target_box.clone()),
            )
        })
        .collect()
}

fn select_schema<'a>(
    module: &'a SchemaV1Module,
    schema_name: Option<&str>,
) -> Result<&'a SchemaV1Schema, String> {
    match schema_name {
        Some(name) => module
            .schemas
            .iter()
            .find(|schema| schema.name == name)
            .ok_or_else(|| format!("schema `{name}` was not found in module `{}`", module.module_name)),
        None => match module.schemas.as_slice() {
            [schema] => Ok(schema),
            [] => Err(format!(
                "module `{}` does not contain a schema to check against",
                module.module_name
            )),
            _ => Err(format!(
                "module `{}` contains multiple schemas; pass an explicit schema_name to check an olog fragment",
                module.module_name
            )),
        },
    }
}

fn local_name(raw: &str) -> &str {
    raw.rsplit('.').next().unwrap_or(raw)
}

fn relation_subject_refs_for_theory_candidate(
    compiled_ir: &RuntimeSchemaIndex,
    relation_name: &str,
    role_name: Option<&str>,
) -> Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr> {
    let Some(relation) = compiled_ir
        .relations
        .get(relation_name)
        .or_else(|| compiled_ir.relations.get(local_name(relation_name)))
    else {
        return Vec::new();
    };

    let mut refs = vec![axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
        relation_id: relation.relation_id.clone(),
        relation_name: relation.name.clone(),
    }];
    if let Some(role_name) = role_name {
        refs.extend(
            relation
                .roles
                .iter()
                .filter(|role| {
                    role.name == role_name || local_name(&role.name) == local_name(role_name)
                })
                .map(
                    |role| axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                        relation_id: relation.relation_id.clone(),
                        relation_name: relation.name.clone(),
                        role_id: role.role_id.clone(),
                        role_name: role.name.clone(),
                    },
                ),
        );
    }
    refs
}

fn theory_handles_for_relation_role(
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
    relation_name: Option<&str>,
    role_name: Option<&str>,
) -> (
    Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
    Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
) {
    let Some(relation_name) = relation_name else {
        return (None, Vec::new());
    };

    let subjects =
        relation_subject_refs_for_theory_candidate(compiled_ir, relation_name, role_name);
    let mut obligations = BTreeSet::new();
    for subject in &subjects {
        for theory in theories {
            for obligation in theory.obligation_refs_for_subject(subject) {
                obligations.insert(obligation);
            }
        }
    }

    let obligation = if obligations.len() == 1 {
        obligations.into_iter().next()
    } else {
        None
    };
    (obligation, subjects)
}

fn primary_theory_subject_ref(
    theory_subject_refs: &[axiograph_pathdb::kernel_ir::TheorySubjectRefIr],
) -> Option<axiograph_pathdb::kernel_ir::TheorySubjectRefIr> {
    theory_subject_refs
        .iter()
        .find(|subject| {
            !matches!(
                subject,
                axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Theory { .. }
            )
        })
        .or_else(|| theory_subject_refs.first())
        .cloned()
}

fn enrich_olog_refinement_candidate_with_compiled_theory(
    candidate: &mut crate::typed_refinement::RuntimeRefinementCandidateV2,
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
) {
    if candidate.theory_obligation_ref.is_some() || !candidate.theory_subject_refs.is_empty() {
        return;
    }

    let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_relation_role(
        compiled_ir,
        theories,
        candidate.relation.as_deref(),
        candidate.role.as_deref(),
    );
    candidate.theory_subject_ref = primary_theory_subject_ref(&theory_subject_refs);
    candidate.theory_subject_refs = theory_subject_refs;
    candidate.theory_obligation_ref = theory_obligation_ref;
}

fn enrich_olog_hole_with_compiled_theory(
    hole: &mut OlogTypedHoleV1,
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
) {
    if hole.theory_obligation_ref.is_some() || !hole.theory_subject_refs.is_empty() {
        return;
    }

    let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_relation_role(
        compiled_ir,
        theories,
        hole.relation.as_deref(),
        hole.role.as_deref(),
    );
    hole.theory_subject_ref = primary_theory_subject_ref(&theory_subject_refs);
    hole.theory_subject_refs = theory_subject_refs;
    hole.theory_obligation_ref = theory_obligation_ref;
}

fn compile_theories_for_schema(
    module: &SchemaV1Module,
    schema: &SchemaV1Schema,
    compiled_ir: &RuntimeSchemaIndex,
) -> Vec<axiograph_pathdb::kernel_ir::TheoryIr> {
    let mut theories = module
        .theories
        .iter()
        .filter(|theory| theory.schema == schema.name)
        .filter_map(|theory| {
            axiograph_pathdb::kernel_ir::derive_runtime_theory_index(compiled_ir, theory).ok()
        })
        .collect::<Vec<_>>();
    theories.sort_by(|left, right| left.theory_id.cmp(&right.theory_id));
    theories
}

fn enrich_checked_olog_fragment_with_compiled_theory(
    checked: &mut CheckedOlogFragmentV1,
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
) {
    if theories.is_empty() {
        return;
    }

    for hole in &mut checked.typed_holes {
        enrich_olog_hole_with_compiled_theory(hole, compiled_ir, theories);
        for candidate in &mut hole.refinement_candidates {
            enrich_olog_refinement_candidate_with_compiled_theory(candidate, compiled_ir, theories);
        }
    }

    checked.refinement_candidates = checked
        .typed_holes
        .iter()
        .flat_map(|hole| hole.refinement_candidates.clone().into_iter())
        .collect();
}

struct RelationBoxCheck<'a> {
    relation: &'a RelationSemanticsIr,
    bindings: HashMap<String, String>,
}

fn relation_has_hidden_data_roles(relation: &RelationSemanticsIr) -> bool {
    let Some(carrier) = relation.carrier.as_ref() else {
        return false;
    };
    let carrier_roles = HashSet::from([carrier.source_role, carrier.target_role]);
    relation
        .roles
        .iter()
        .any(|role| role.kind == RoleKind::Data && !carrier_roles.contains(&role.order))
}

fn evaluate_path(
    equation_id: &str,
    side: &str,
    path: &OlogPathV1,
    aspect_endpoints: &HashMap<String, (String, String)>,
    diagnostics: &mut Vec<OlogDiagnosticV1>,
    typed_holes: &mut Vec<OlogTypedHoleV1>,
) -> Option<(String, String)> {
    let mut steps = path.steps.iter();
    let first_step = match steps.next() {
        Some(step) => step,
        None => {
            error(
                diagnostics,
                "olog_path_empty",
                format!("path equation `{equation_id}` has an empty {side} side"),
                Some("add at least one aspect step or drop the empty equation".to_string()),
            );
            return None;
        }
    };

    let Some((start, mut current_end)) = aspect_endpoints.get(first_step).cloned() else {
        error(
            diagnostics,
            "olog_path_unknown_aspect",
            format!(
                "path equation `{equation_id}` references unknown aspect `{first_step}` on the {side} side"
            ),
            Some("fix the aspect id or add the missing aspect to the fragment".to_string()),
        );
        typed_holes.push(OlogTypedHoleV1 {
            kind: OlogTypedHoleKindV1::UnknownPathAspect,
            relation_box: None,
            relation: None,
            role: None,
            aspect_id: Some(first_step.clone()),
            equation_id: Some(equation_id.to_string()),
            summary: format!(
                "{side} side of path equation `{equation_id}` starts with unknown aspect `{first_step}`"
            ),
            expected_types: Vec::new(),
            admissible_player_types: Vec::new(),
            candidate_boxes: Vec::new(),
            suggestions: vec![
                "fix the aspect id to an existing aspect in the fragment".to_string(),
                "or add the missing aspect before checking the path equation".to_string(),
            ],
            theory_obligation_ref: None,
            theory_subject_refs: Vec::new(),
            theory_subject_ref: None,
            refinement_candidates: Vec::new(),
        });
        return None;
    };

    for step in steps {
        let Some((next_start, next_end)) = aspect_endpoints.get(step) else {
            error(
                diagnostics,
                "olog_path_unknown_aspect",
                format!(
                    "path equation `{equation_id}` references unknown aspect `{step}` on the {side} side"
                ),
                Some("fix the aspect id or add the missing aspect to the fragment".to_string()),
            );
            typed_holes.push(OlogTypedHoleV1 {
                kind: OlogTypedHoleKindV1::UnknownPathAspect,
                relation_box: None,
                relation: None,
                role: None,
                aspect_id: Some(step.clone()),
                equation_id: Some(equation_id.to_string()),
                summary: format!(
                    "{side} side of path equation `{equation_id}` references unknown aspect `{step}`"
                ),
                expected_types: Vec::new(),
                admissible_player_types: Vec::new(),
                candidate_boxes: Vec::new(),
                suggestions: vec![
                    "fix the aspect id to an existing aspect in the fragment".to_string(),
                    "or add the missing aspect before checking the path equation".to_string(),
                ],
                theory_obligation_ref: None,
                theory_subject_refs: Vec::new(),
                theory_subject_ref: None,
                refinement_candidates: Vec::new(),
            });
            return None;
        };
        if current_end != *next_start {
            error(
                diagnostics,
                "olog_path_endpoint_mismatch",
                format!(
                    "path equation `{equation_id}` does not compose on the {side} side: `{step}` starts at `{next_start}` but the previous step ends at `{current_end}`"
                ),
                Some(
                    "rewrite the path so each aspect starts from the previous aspect's target box"
                        .to_string(),
                ),
            );
            typed_holes.push(OlogTypedHoleV1 {
                kind: OlogTypedHoleKindV1::PathEndpointMismatch,
                relation_box: None,
                relation: None,
                role: None,
                aspect_id: Some(step.clone()),
                equation_id: Some(equation_id.to_string()),
                summary: format!(
                    "{side} side of path equation `{equation_id}` does not compose: expected next aspect to start at `{current_end}`, but `{step}` starts at `{next_start}`"
                ),
                expected_types: vec![current_end.clone()],
                admissible_player_types: Vec::new(),
                candidate_boxes: vec![current_end.clone(), next_start.clone()],
                suggestions: vec![
                    format!(
                        "rewrite the path so the next aspect starts at `{current_end}`"
                    ),
                    format!(
                        "or insert an intermediate aspect that connects `{current_end}` to `{next_start}`"
                    ),
                ],
                theory_obligation_ref: None,
                theory_subject_refs: Vec::new(),
                theory_subject_ref: None,
                refinement_candidates: Vec::new(),
            });
            return None;
        }
        current_end = next_end.clone();
    }

    Some((start, current_end))
}

fn evaluate_olog_fragment(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: &OlogFragmentV1,
) -> (Vec<OlogDiagnosticV1>, Vec<OlogTypedHoleV1>) {
    let mut diagnostics = Vec::new();
    let mut typed_holes = Vec::new();
    let mut all_box_ids = HashSet::new();
    let mut object_boxes = HashMap::new();

    for box_decl in &fragment.boxes {
        if !all_box_ids.insert(box_decl.box_id.clone()) {
            error(
                &mut diagnostics,
                "olog_box_duplicate",
                format!("box id `{}` is declared more than once", box_decl.box_id),
                Some("rename one of the boxes so each box id is unique".to_string()),
            );
            continue;
        }
        if !compiled_ir.has_object_type(&box_decl.object_type) {
            error(
                &mut diagnostics,
                "olog_unknown_object_type",
                format!(
                    "box `{}` references unknown object type `{}` in schema `{}`",
                    box_decl.box_id, box_decl.object_type, compiled_ir.schema_id
                ),
                Some("change the box type to a declared object or add the missing object to the schema".to_string()),
            );
        }
        object_boxes.insert(box_decl.box_id.clone(), box_decl.object_type.clone());
    }

    let mut relation_boxes = HashMap::new();
    for relation_box in &fragment.relation_boxes {
        if !all_box_ids.insert(relation_box.box_id.clone()) {
            error(
                &mut diagnostics,
                "olog_box_duplicate",
                format!(
                    "relation box id `{}` is declared more than once",
                    relation_box.box_id
                ),
                Some("rename the relation box so it does not collide with another box".to_string()),
            );
            continue;
        }

        let Some(relation) = compiled_ir.relation(&relation_box.relation) else {
            error(
                &mut diagnostics,
                "olog_unknown_relation",
                format!(
                    "relation box `{}` references unknown relation `{}`",
                    relation_box.box_id, relation_box.relation
                ),
                Some(
                    "change the relation name or add the missing relation to the schema"
                        .to_string(),
                ),
            );
            continue;
        };

        let mut bindings = HashMap::new();
        for binding in &relation_box.role_bindings {
            if bindings
                .insert(binding.role.clone(), binding.target_box.clone())
                .is_some()
            {
                error(
                    &mut diagnostics,
                    "olog_role_duplicate",
                    format!(
                        "relation box `{}` binds role `{}` more than once",
                        relation_box.box_id, binding.role
                    ),
                    Some("keep exactly one target box per relation role".to_string()),
                );
                continue;
            }

            let Some(role) = relation.roles.iter().find(|role| role.name == binding.role) else {
                error(
                    &mut diagnostics,
                    "olog_unknown_role",
                    format!(
                        "relation box `{}` binds unknown role `{}` for relation `{}`",
                        relation_box.box_id, binding.role, relation_box.relation
                    ),
                    Some(
                        "use a declared relation role name from the compiled schema IR".to_string(),
                    ),
                );
                continue;
            };

            let Some(target_box_type) = object_boxes.get(&binding.target_box) else {
                error(
                    &mut diagnostics,
                    "olog_unknown_target_box",
                    format!(
                        "relation box `{}` binds role `{}` to unknown box `{}`",
                        relation_box.box_id, binding.role, binding.target_box
                    ),
                    Some(
                        "add the missing object box before binding it from a relation box"
                            .to_string(),
                    ),
                );
                continue;
            };

            if !compiled_ir.type_matches_or_subtypes(target_box_type, &role.target_type) {
                let admissible_player_types =
                    compiled_ir.admissible_player_types(&role.target_type);
                let candidate_boxes =
                    candidate_boxes_for_types(compiled_ir, &object_boxes, &admissible_player_types);
                let refinement_candidates = olog_bind_role_refinement_candidates(
                    &relation_box.box_id,
                    &relation_box.relation,
                    &binding.role,
                    &role.target_type,
                    &candidate_boxes,
                    true,
                );
                let repair_hint = compiled_ir
                    .role_interface(&relation_box.relation, &binding.role)
                    .map(|interface| {
                        format!(
                            "retarget the role to a box with one of the admissible player types: {}",
                            interface.admissible_player_types.join(", ")
                        )
                    })
                    .unwrap_or_else(|| {
                        "retarget the role to a box with the expected type or a declared subtype"
                            .to_string()
                    });
                typed_holes.push(OlogTypedHoleV1 {
                    kind: OlogTypedHoleKindV1::RoleBindingTypeMismatch,
                    relation_box: Some(relation_box.box_id.clone()),
                    relation: Some(relation_box.relation.clone()),
                    role: Some(binding.role.clone()),
                    aspect_id: None,
                    equation_id: None,
                    summary: format!(
                        "role `{}` on relation box `{}` expects `{}` but is currently bound to box `{}` of type `{}`",
                        binding.role,
                        relation_box.box_id,
                        role.target_type,
                        binding.target_box,
                        target_box_type
                    ),
                    expected_types: vec![role.target_type.clone()],
                    admissible_player_types,
                    candidate_boxes,
                    suggestions: vec![repair_hint.clone()],
                    theory_obligation_ref: None,
                    theory_subject_refs: Vec::new(),
                    theory_subject_ref: None,
                    refinement_candidates,
                });
                error(
                    &mut diagnostics,
                    "olog_role_type_mismatch",
                    format!(
                        "relation box `{}` binds role `{}` of type `{}` to box `{}` of type `{}`",
                        relation_box.box_id,
                        binding.role,
                        role.target_type,
                        binding.target_box,
                        target_box_type
                    ),
                    Some(repair_hint),
                );
            }
        }

        for role in &relation.roles {
            if !bindings.contains_key(&role.name) {
                let admissible_player_types =
                    compiled_ir.admissible_player_types(&role.target_type);
                let candidate_boxes =
                    candidate_boxes_for_types(compiled_ir, &object_boxes, &admissible_player_types);
                let refinement_candidates = olog_bind_role_refinement_candidates(
                    &relation_box.box_id,
                    &relation_box.relation,
                    &role.name,
                    &role.target_type,
                    &candidate_boxes,
                    false,
                );
                let hint = match role.kind {
                    RoleKind::Context | RoleKind::World => {
                        "bind the context/world role explicitly so scope is not implicit"
                    }
                    RoleKind::Temporal => {
                        "bind the temporal role explicitly so time scope is not implicit"
                    }
                    RoleKind::Evidence => {
                        "bind the evidence role explicitly so provenance remains inspectable"
                    }
                    RoleKind::Data | RoleKind::Parameter => {
                        "bind every declared role so the n-ary relation remains complete"
                    }
                };
                typed_holes.push(OlogTypedHoleV1 {
                    kind: OlogTypedHoleKindV1::MissingRelationRoleBinding,
                    relation_box: Some(relation_box.box_id.clone()),
                    relation: Some(relation_box.relation.clone()),
                    role: Some(role.name.clone()),
                    aspect_id: None,
                    equation_id: None,
                    summary: format!(
                        "relation box `{}` is missing role `{}` for relation `{}`",
                        relation_box.box_id, role.name, relation_box.relation
                    ),
                    expected_types: vec![role.target_type.clone()],
                    admissible_player_types,
                    candidate_boxes: candidate_boxes.clone(),
                    suggestions: if candidate_boxes.is_empty() {
                        vec![hint.to_string()]
                    } else {
                        candidate_boxes
                            .iter()
                            .map(|box_id| format!("bind role `{}` to box `{}`", role.name, box_id))
                            .collect()
                    },
                    theory_obligation_ref: None,
                    theory_subject_refs: Vec::new(),
                    theory_subject_ref: None,
                    refinement_candidates,
                });
                error(
                    &mut diagnostics,
                    "olog_role_missing",
                    format!(
                        "relation box `{}` is missing required role `{}` for relation `{}`",
                        relation_box.box_id, role.name, relation_box.relation
                    ),
                    Some(hint.to_string()),
                );
            }
        }

        relation_boxes.insert(
            relation_box.box_id.clone(),
            RelationBoxCheck { relation, bindings },
        );
    }

    let mut aspect_endpoints = HashMap::new();
    for aspect in &fragment.aspects {
        if aspect_endpoints
            .insert(
                aspect.aspect_id.clone(),
                (aspect.from_box.clone(), aspect.to_box.clone()),
            )
            .is_some()
        {
            error(
                &mut diagnostics,
                "olog_aspect_duplicate",
                format!(
                    "aspect id `{}` is declared more than once",
                    aspect.aspect_id
                ),
                Some("rename the aspect so each aspect id is unique".to_string()),
            );
            continue;
        }

        match &aspect.kind {
            OlogAspectKindV1::RelationCarrier { relation } => {
                let Some(from_type) = object_boxes.get(&aspect.from_box) else {
                    error(
                        &mut diagnostics,
                        "olog_carrier_from_box_missing",
                        format!(
                            "carrier aspect `{}` starts from unknown object box `{}`",
                            aspect.aspect_id, aspect.from_box
                        ),
                        Some("point the aspect at a declared object box".to_string()),
                    );
                    continue;
                };
                let Some(to_type) = object_boxes.get(&aspect.to_box) else {
                    error(
                        &mut diagnostics,
                        "olog_carrier_to_box_missing",
                        format!(
                            "carrier aspect `{}` ends at unknown object box `{}`",
                            aspect.aspect_id, aspect.to_box
                        ),
                        Some("point the aspect at a declared object box".to_string()),
                    );
                    continue;
                };

                let Some(relation_semantics) = compiled_ir.relation(relation) else {
                    error(
                        &mut diagnostics,
                        "olog_unknown_relation",
                        format!(
                            "carrier aspect `{}` references unknown relation `{}`",
                            aspect.aspect_id, relation
                        ),
                        Some(
                            "change the relation name or add the missing relation to the schema"
                                .to_string(),
                        ),
                    );
                    continue;
                };

                let Some((source_role, target_role)) = relation_semantics.carrier_roles() else {
                    error(
                        &mut diagnostics,
                        "olog_relation_not_carrier_like",
                        format!(
                            "carrier aspect `{}` uses relation `{}` which does not elaborate to a direct carrier/source-target view",
                            aspect.aspect_id, relation
                        ),
                        Some("model the relation as a relation box with explicit role projections instead of a direct aspect".to_string()),
                    );
                    continue;
                };

                if relation_has_hidden_data_roles(relation_semantics) {
                    error(
                        &mut diagnostics,
                        "olog_relation_requires_relation_box",
                        format!(
                            "carrier aspect `{}` uses n-ary relation `{}` with additional data roles beyond `{}` and `{}`",
                            aspect.aspect_id, relation, source_role.name, target_role.name
                        ),
                        Some("author this relation as a relation box with explicit projections so no business roles are hidden".to_string()),
                    );
                    continue;
                }

                if !compiled_ir.type_matches_or_subtypes(from_type, &source_role.target_type) {
                    error(
                        &mut diagnostics,
                        "olog_carrier_source_type_mismatch",
                        format!(
                            "carrier aspect `{}` starts from box `{}` of type `{}` but relation `{}` expects `{}` on its source side",
                            aspect.aspect_id,
                            aspect.from_box,
                            from_type,
                            relation,
                            source_role.target_type
                        ),
                        Some("rebind the aspect source box to the relation's carrier source type or a subtype".to_string()),
                    );
                }

                if !compiled_ir.type_matches_or_subtypes(to_type, &target_role.target_type) {
                    error(
                        &mut diagnostics,
                        "olog_carrier_target_type_mismatch",
                        format!(
                            "carrier aspect `{}` ends at box `{}` of type `{}` but relation `{}` expects `{}` on its target side",
                            aspect.aspect_id,
                            aspect.to_box,
                            to_type,
                            relation,
                            target_role.target_type
                        ),
                        Some("rebind the aspect target box to the relation's carrier target type or a subtype".to_string()),
                    );
                }
            }
            OlogAspectKindV1::RelationProjection { relation_box, role } => {
                let Some(rel_box) = relation_boxes.get(relation_box) else {
                    error(
                        &mut diagnostics,
                        "olog_projection_missing_relation_box",
                        format!(
                            "projection aspect `{}` references unknown relation box `{}`",
                            aspect.aspect_id, relation_box
                        ),
                        Some("add the relation box before projecting from it".to_string()),
                    );
                    continue;
                };
                if aspect.from_box != *relation_box {
                    error(
                        &mut diagnostics,
                        "olog_projection_source_mismatch",
                        format!(
                            "projection aspect `{}` must start from relation box `{}` but starts from `{}`",
                            aspect.aspect_id, relation_box, aspect.from_box
                        ),
                        Some("set from_box to the relation box id for a projection aspect".to_string()),
                    );
                }
                let Some(bound_target) = rel_box.bindings.get(role) else {
                    let role_decl = rel_box
                        .relation
                        .roles
                        .iter()
                        .find(|decl| decl.name == *role);
                    let admissible_player_types = role_decl
                        .map(|decl| compiled_ir.admissible_player_types(&decl.target_type))
                        .unwrap_or_default();
                    let candidate_boxes = candidate_boxes_for_types(
                        compiled_ir,
                        &object_boxes,
                        &admissible_player_types,
                    );
                    let refinement_candidates = role_decl
                        .map(|decl| {
                            olog_bind_role_refinement_candidates(
                                relation_box,
                                &rel_box.relation.name,
                                &decl.name,
                                &decl.target_type,
                                &candidate_boxes,
                                false,
                            )
                        })
                        .unwrap_or_default();
                    typed_holes.push(OlogTypedHoleV1 {
                        kind: OlogTypedHoleKindV1::ProjectionRoleUnbound,
                        relation_box: Some(relation_box.clone()),
                        relation: Some(rel_box.relation.name.clone()),
                        role: Some(role.clone()),
                        aspect_id: Some(aspect.aspect_id.clone()),
                        equation_id: None,
                        summary: format!(
                            "projection aspect `{}` cannot project role `{}` because relation box `{}` has no binding for it",
                            aspect.aspect_id, role, relation_box
                        ),
                        expected_types: role_decl
                            .map(|decl| vec![decl.target_type.clone()])
                            .unwrap_or_default(),
                        admissible_player_types,
                        candidate_boxes: candidate_boxes.clone(),
                        suggestions: if candidate_boxes.is_empty() {
                            vec!["bind that role on the relation box before projecting it".to_string()]
                        } else {
                            candidate_boxes
                                .iter()
                                .map(|box_id| {
                                    format!(
                                        "bind role `{role}` on relation box `{relation_box}` to box `{box_id}`"
                                    )
                                })
                                .collect()
                        },
                        theory_obligation_ref: None,
                        theory_subject_refs: Vec::new(),
                        theory_subject_ref: None,
                        refinement_candidates,
                    });
                    error(
                        &mut diagnostics,
                        "olog_projection_unbound_role",
                        format!(
                            "projection aspect `{}` references unbound role `{}` on relation box `{}`",
                            aspect.aspect_id, role, relation_box
                        ),
                        Some("bind that role on the relation box before projecting it".to_string()),
                    );
                    continue;
                };
                if &aspect.to_box != bound_target {
                    error(
                        &mut diagnostics,
                        "olog_projection_target_mismatch",
                        format!(
                            "projection aspect `{}` points to box `{}` but relation box `{}` binds role `{}` to `{}`",
                            aspect.aspect_id, aspect.to_box, relation_box, role, bound_target
                        ),
                        Some("align the projection target with the relation-box role binding".to_string()),
                    );
                }
                if rel_box.relation.roles.iter().all(|decl| decl.name != *role) {
                    error(
                        &mut diagnostics,
                        "olog_unknown_role",
                        format!(
                            "projection aspect `{}` references unknown role `{}` on relation `{}`",
                            aspect.aspect_id, role, rel_box.relation.name
                        ),
                        Some(
                            "use one of the declared relation roles from the compiled schema IR"
                                .to_string(),
                        ),
                    );
                }
            }
        }
    }

    for equation in &fragment.path_equations {
        let lhs = evaluate_path(
            &equation.equation_id,
            "lhs",
            &equation.lhs,
            &aspect_endpoints,
            &mut diagnostics,
            &mut typed_holes,
        );
        let rhs = evaluate_path(
            &equation.equation_id,
            "rhs",
            &equation.rhs,
            &aspect_endpoints,
            &mut diagnostics,
            &mut typed_holes,
        );
        if let (Some((lhs_start, lhs_end)), Some((rhs_start, rhs_end))) = (lhs, rhs) {
            if lhs_start != rhs_start || lhs_end != rhs_end {
                typed_holes.push(OlogTypedHoleV1 {
                    kind: OlogTypedHoleKindV1::PathEndpointMismatch,
                    relation_box: None,
                    relation: None,
                    role: None,
                    aspect_id: None,
                    equation_id: Some(equation.equation_id.clone()),
                    summary: format!(
                        "path equation `{}` compares paths with different endpoints: lhs {} -> {}, rhs {} -> {}",
                        equation.equation_id, lhs_start, lhs_end, rhs_start, rhs_end
                    ),
                    expected_types: vec![lhs_start.clone(), lhs_end.clone()],
                    admissible_player_types: Vec::new(),
                    candidate_boxes: vec![rhs_start.clone(), rhs_end.clone()],
                    suggestions: vec![
                        format!(
                            "rewrite the rhs path so it has endpoints {} -> {}",
                            lhs_start, lhs_end
                        ),
                        "or compare only paths with the same start and end boxes".to_string(),
                    ],
                    theory_obligation_ref: None,
                    theory_subject_refs: Vec::new(),
                    theory_subject_ref: None,
                    refinement_candidates: Vec::new(),
                });
                error(
                    &mut diagnostics,
                    "olog_equation_endpoint_mismatch",
                    format!(
                        "path equation `{}` compares paths with different endpoints: lhs {} -> {}, rhs {} -> {}",
                        equation.equation_id, lhs_start, lhs_end, rhs_start, rhs_end
                    ),
                    Some("only equate paths with the same start and end boxes".to_string()),
                );
            }
        }
    }

    if fragment.relation_boxes.is_empty()
        && fragment
            .aspects
            .iter()
            .any(|aspect| matches!(aspect.kind, OlogAspectKindV1::RelationCarrier { .. }))
    {
        warning(
            &mut diagnostics,
            "olog_relation_object_recommendation",
            "fragment uses direct carrier aspects without any explicit relation boxes".to_string(),
            Some(
                "prefer relation boxes plus projections when the domain concept is n-ary, contextual, or operationally significant".to_string(),
            ),
        );
    }

    (diagnostics, typed_holes)
}

fn aspect_endpoints(fragment: &OlogFragmentV1) -> HashMap<String, (String, String)> {
    fragment
        .aspects
        .iter()
        .map(|aspect| {
            (
                aspect.aspect_id.clone(),
                (aspect.from_box.clone(), aspect.to_box.clone()),
            )
        })
        .collect()
}

fn path_endpoints(
    path: &OlogPathV1,
    aspect_endpoints: &HashMap<String, (String, String)>,
) -> Option<(String, String)> {
    let mut steps = path.steps.iter();
    let first_step = steps.next()?;
    let (start, mut current_end) = aspect_endpoints.get(first_step)?.clone();
    for step in steps {
        let (next_start, next_end) = aspect_endpoints.get(step)?.clone();
        if current_end != next_start {
            return None;
        }
        current_end = next_end;
    }
    Some((start, current_end))
}

#[allow(clippy::too_many_arguments)]
fn role_target_refinement_primitives(
    compiled_ir: &RuntimeSchemaIndex,
    relation_name: &str,
    role_name: &str,
    expected_type: &str,
    actual_type: &str,
    affected_subject_refs: Vec<String>,
    specialize_rationale: String,
    push_rationale: String,
) -> Vec<crate::evolution_preview::EvolutionPrimitiveV1> {
    if actual_type == expected_type
        || !compiled_ir.type_matches_or_subtypes(actual_type, expected_type)
    {
        return Vec::new();
    }

    let mut primitives = vec![
        crate::evolution_preview::EvolutionPrimitiveV1::SpecializeToSubtype {
            from: expected_type.to_string(),
            to: actual_type.to_string(),
            affected_subject_refs: affected_subject_refs.clone(),
            rationale: Some(specialize_rationale),
        },
    ];

    if compiled_ir.is_direct_subtype(actual_type, expected_type) {
        primitives.push(
            crate::evolution_preview::EvolutionPrimitiveV1::PushRelationRoleToSubtype {
                relation: relation_name.to_string(),
                role: role_name.to_string(),
                from_supertype: expected_type.to_string(),
                to_subtype: actual_type.to_string(),
                affected_subject_refs,
                rationale: Some(push_rationale),
            },
        );
    }

    primitives
}

fn relation_binding_specialization_primitives(
    compiled_ir: &RuntimeSchemaIndex,
    relation_box_id: &str,
    relation: &RelationSemanticsIr,
    bindings: &[OlogRoleBindingV1],
    object_boxes: &HashMap<String, String>,
) -> Vec<crate::evolution_preview::EvolutionPrimitiveV1> {
    let mut primitives = Vec::new();
    for binding in bindings {
        let Some(role) = relation.roles.iter().find(|role| role.name == binding.role) else {
            continue;
        };
        let Some(actual_type) = object_boxes.get(&binding.target_box) else {
            continue;
        };
        primitives.extend(role_target_refinement_primitives(
            compiled_ir,
            &relation.name,
            &role.name,
            &role.target_type,
            actual_type,
            vec![binding.target_box.clone()],
            format!(
                "olog relation box `{relation_box_id}` binds role `{}` with narrower box type `{}`",
                binding.role, actual_type
            ),
            format!(
                "typed olog authoring used direct subtype `{}` for `{}` on relation box `{relation_box_id}`; review whether `{}` should be pushed down from `{}`",
                actual_type, binding.role, binding.role, role.target_type
            ),
        ));
    }
    primitives
}

fn fragment_relation_split_primitives(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: &OlogFragmentV1,
    object_boxes: &HashMap<String, String>,
) -> Vec<crate::evolution_preview::EvolutionPrimitiveV1> {
    let mut subtypes_by_relation_role: BTreeMap<(String, String, String), BTreeSet<String>> =
        BTreeMap::new();

    for relation_box in &fragment.relation_boxes {
        let Some(relation) = compiled_ir.relation(&relation_box.relation) else {
            continue;
        };
        for binding in &relation_box.role_bindings {
            let Some(role) = relation.roles.iter().find(|role| role.name == binding.role) else {
                continue;
            };
            let Some(actual_type) = object_boxes.get(&binding.target_box) else {
                continue;
            };
            if actual_type != &role.target_type
                && compiled_ir.is_direct_subtype(actual_type, &role.target_type)
            {
                subtypes_by_relation_role
                    .entry((
                        relation.name.clone(),
                        role.name.clone(),
                        role.target_type.clone(),
                    ))
                    .or_default()
                    .insert(actual_type.clone());
            }
        }
    }

    let mut primitives = Vec::new();
    for ((relation_name, role_name, source_type), subtypes) in subtypes_by_relation_role {
        if subtypes.len() <= 1 {
            continue;
        }
        primitives.push(
            crate::evolution_preview::EvolutionPrimitiveV1::SplitTypeIntoSubtypes {
                source: source_type.clone(),
                subtypes: subtypes.into_iter().collect(),
                discriminator: Some(format!("{relation_name}.{role_name}")),
                affected_subject_refs: Vec::new(),
                rationale: Some(format!(
                    "typed olog authoring bound `{role_name}` across multiple direct subtypes for `{relation_name}`; review whether `{source_type}` should split explicitly along this role"
                )),
            },
        );
    }
    primitives
}

fn aspect_specialization_primitives(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: &OlogFragmentV1,
) -> Vec<crate::evolution_preview::EvolutionPrimitiveV1> {
    let object_boxes = fragment
        .boxes
        .iter()
        .map(|box_decl| (box_decl.box_id.clone(), box_decl.object_type.clone()))
        .collect::<HashMap<_, _>>();
    let relation_boxes = fragment
        .relation_boxes
        .iter()
        .map(|box_decl| (box_decl.box_id.clone(), box_decl))
        .collect::<HashMap<_, _>>();
    let mut primitives = Vec::new();

    for aspect in &fragment.aspects {
        match &aspect.kind {
            OlogAspectKindV1::RelationCarrier { relation } => {
                let Some(relation_semantics) = compiled_ir.relation(relation) else {
                    continue;
                };
                let Some((source_role, target_role)) = relation_semantics.carrier_roles() else {
                    continue;
                };
                if let Some(actual_from) = object_boxes.get(&aspect.from_box) {
                    if actual_from != &source_role.target_type
                        && compiled_ir
                            .type_matches_or_subtypes(actual_from, &source_role.target_type)
                    {
                        primitives.push(
                            crate::evolution_preview::EvolutionPrimitiveV1::SpecializeToSubtype {
                                from: source_role.target_type.clone(),
                                to: actual_from.clone(),
                                affected_subject_refs: vec![aspect.from_box.clone()],
                                rationale: Some(format!(
                                    "carrier aspect `{}` uses narrower source type `{}` for relation `{}`",
                                    aspect.aspect_id, actual_from, relation
                                )),
                            },
                        );
                    }
                }
                if let Some(actual_to) = object_boxes.get(&aspect.to_box) {
                    if actual_to != &target_role.target_type
                        && compiled_ir.type_matches_or_subtypes(actual_to, &target_role.target_type)
                    {
                        primitives.push(
                            crate::evolution_preview::EvolutionPrimitiveV1::SpecializeToSubtype {
                                from: target_role.target_type.clone(),
                                to: actual_to.clone(),
                                affected_subject_refs: vec![aspect.to_box.clone()],
                                rationale: Some(format!(
                                    "carrier aspect `{}` uses narrower target type `{}` for relation `{}`",
                                    aspect.aspect_id, actual_to, relation
                                )),
                            },
                        );
                    }
                }
            }
            OlogAspectKindV1::RelationProjection { relation_box, role } => {
                let Some(relation_box_decl) = relation_boxes.get(relation_box) else {
                    continue;
                };
                let Some(relation_semantics) = compiled_ir.relation(&relation_box_decl.relation)
                else {
                    continue;
                };
                let Some(role_semantics) = relation_semantics
                    .roles
                    .iter()
                    .find(|item| item.name == *role)
                else {
                    continue;
                };
                let Some(actual_to) = object_boxes.get(&aspect.to_box) else {
                    continue;
                };
                if actual_to != &role_semantics.target_type
                    && compiled_ir.type_matches_or_subtypes(actual_to, &role_semantics.target_type)
                {
                    primitives.push(
                        crate::evolution_preview::EvolutionPrimitiveV1::SpecializeToSubtype {
                            from: role_semantics.target_type.clone(),
                            to: actual_to.clone(),
                            affected_subject_refs: vec![aspect.to_box.clone()],
                            rationale: Some(format!(
                                "projection aspect `{}` refines role `{}` of relation `{}` to subtype `{}`",
                                aspect.aspect_id, role, relation_box_decl.relation, actual_to
                            )),
                        },
                    );
                }
            }
        }
    }

    primitives
}

pub fn olog_fragment_typed_change_summary(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: &OlogFragmentV1,
) -> crate::evolution_preview::TypedChangeSummaryV1 {
    let object_boxes = fragment
        .boxes
        .iter()
        .map(|box_decl| (box_decl.box_id.clone(), box_decl.object_type.clone()))
        .collect::<HashMap<_, _>>();
    let aspect_endpoints = aspect_endpoints(fragment);
    let mut primitives = Vec::new();

    for relation_box in &fragment.relation_boxes {
        let Some(relation) = compiled_ir.relation(&relation_box.relation) else {
            continue;
        };
        primitives.push(
            crate::evolution_preview::EvolutionPrimitiveV1::ReifyRelationObject {
                relation: relation.name.clone(),
                tuple_type: relation.tuple_type_name.clone(),
                roles: relation
                    .roles
                    .iter()
                    .map(|role| role.name.clone())
                    .collect(),
                rationale: Some(
                    "typed olog authoring makes the relation explicit as a relation-object"
                        .to_string(),
                ),
            },
        );

        let carrier_role_orders = relation
            .carrier
            .as_ref()
            .map(|carrier| HashSet::from([carrier.source_role, carrier.target_role]))
            .unwrap_or_default();
        let index_roles = relation
            .roles
            .iter()
            .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
            .map(|role| role.name.clone())
            .collect::<Vec<_>>();
        let fiber_roles = relation
            .roles
            .iter()
            .filter(|role| {
                role.kind == RoleKind::Data && !carrier_role_orders.contains(&role.order)
            })
            .map(|role| role.name.clone())
            .collect::<Vec<_>>();
        if !index_roles.is_empty() || !fiber_roles.is_empty() {
            primitives.push(
                crate::evolution_preview::EvolutionPrimitiveV1::IntroduceDependentRelationFamily {
                    relation: relation.name.clone(),
                    tuple_type: relation.tuple_type_name.clone(),
                    index_roles,
                    fiber_roles,
                    rationale: Some(
                        "typed olog authoring exposed indexed/fibered relation structure over explicit roles"
                            .to_string(),
                    ),
                },
            );
        }

        primitives.extend(relation_binding_specialization_primitives(
            compiled_ir,
            &relation_box.box_id,
            relation,
            &relation_box.role_bindings,
            &object_boxes,
        ));
    }

    primitives.extend(fragment_relation_split_primitives(
        compiled_ir,
        fragment,
        &object_boxes,
    ));

    primitives.extend(aspect_specialization_primitives(compiled_ir, fragment));

    for equation in &fragment.path_equations {
        let lhs_endpoints = path_endpoints(&equation.lhs, &aspect_endpoints);
        let rhs_endpoints = path_endpoints(&equation.rhs, &aspect_endpoints);
        let (start_box, end_box) = match (lhs_endpoints, rhs_endpoints) {
            (Some((lhs_start, lhs_end)), Some((rhs_start, rhs_end)))
                if lhs_start == rhs_start && lhs_end == rhs_end =>
            {
                (Some(lhs_start), Some(lhs_end))
            }
            (Some((lhs_start, lhs_end)), _) => (Some(lhs_start), Some(lhs_end)),
            (_, Some((rhs_start, rhs_end))) => (Some(rhs_start), Some(rhs_end)),
            _ => (None, None),
        };
        primitives.push(
            crate::evolution_preview::EvolutionPrimitiveV1::AddPathEquation {
                equation_id: equation.equation_id.clone(),
                start_box,
                end_box,
                lhs_steps: equation.lhs.steps.len(),
                rhs_steps: equation.rhs.steps.len(),
                rationale: Some(
                    "typed olog authoring introduced a commuting-diagram/path-equation obligation"
                        .to_string(),
                ),
            },
        );
    }

    let mut counts = BTreeMap::new();
    counts.insert("boxes_total".to_string(), fragment.boxes.len());
    counts.insert(
        "relation_boxes_total".to_string(),
        fragment.relation_boxes.len(),
    );
    counts.insert("aspects_total".to_string(), fragment.aspects.len());
    counts.insert(
        "path_equations_total".to_string(),
        fragment.path_equations.len(),
    );
    counts.insert("primitives_total".to_string(), primitives.len());

    let dependent_family_count = primitives
        .iter()
        .filter(|primitive| {
            matches!(
                primitive,
                crate::evolution_preview::EvolutionPrimitiveV1::IntroduceDependentRelationFamily { .. }
            )
        })
        .count();

    crate::evolution_preview::TypedChangeSummaryV1 {
        kind: "olog_fragment_delta".to_string(),
        subjects: fragment
            .boxes
            .iter()
            .map(|box_decl| box_decl.box_id.clone())
            .chain(
                fragment
                    .relation_boxes
                    .iter()
                    .map(|box_decl| box_decl.box_id.clone()),
            )
            .chain(
                fragment
                    .path_equations
                    .iter()
                    .map(|equation| equation.equation_id.clone()),
            )
            .collect(),
        primitives,
        counts,
        notes: vec![
            "typed olog authoring delta is derived from the compiled schema IR rather than from storage-local graph edges"
                .to_string(),
            "structural primitives capture relation-object, indexed-family, subtype, and path-equation intent for authoring review"
                .to_string(),
        ],
        schema: crate::evolution_preview::TypedChangeBucketV1 {
            added: fragment.relation_boxes.len(),
            reused: fragment.boxes.len() + fragment.aspects.len(),
            notes: vec![
                "schema bucket counts reflect authoring-visible semantic structure, not yet an accepted-plane schema mutation"
                    .to_string(),
            ],
            ..crate::evolution_preview::TypedChangeBucketV1::default()
        },
        theory: crate::evolution_preview::TypedChangeBucketV1 {
            added: fragment.path_equations.len(),
            notes: vec![
                "path equations are theory-level authoring moves even before promotion".to_string(),
            ],
            ..crate::evolution_preview::TypedChangeBucketV1::default()
        },
        context: crate::evolution_preview::TypedChangeBucketV1 {
            added: dependent_family_count,
            notes: vec![
                "context bucket tracks indexed/dependent relation structure surfaced by explicit context/temporal/fiber roles"
                    .to_string(),
            ],
            ..crate::evolution_preview::TypedChangeBucketV1::default()
        },
        ..crate::evolution_preview::TypedChangeSummaryV1::default()
    }
}

fn proposal_trust_from_typed_authoring(
    trust: &TypedAuthoringTrustV1,
) -> crate::proposals_validate::ProposalValidationTrustContractV1 {
    crate::proposals_validate::ProposalValidationTrustContractV1 {
        trust_class: trust.trust_class.clone(),
        soundness: trust.soundness.clone(),
        coverage: trust.coverage.clone(),
        scope: trust.scope.clone(),
        reasons: trust.reasons.clone(),
    }
}

pub fn build_olog_fragment_evolution_preview(
    compiled_ir: &RuntimeSchemaIndex,
    checked: &CheckedOlogFragmentV1,
) -> crate::evolution_preview::EvolutionPreviewV1 {
    let typed_change = checked
        .typed_change
        .clone()
        .unwrap_or_else(|| olog_fragment_typed_change_summary(compiled_ir, &checked.fragment));
    let mut quality_delta = empty_quality_report("olog_fragment");
    quality_delta.summary.error_count = checked
        .diagnostics
        .iter()
        .filter(|diag| diag.severity == OlogDiagnosticSeverityV1::Error)
        .count();
    quality_delta.summary.warning_count = checked
        .diagnostics
        .iter()
        .filter(|diag| diag.severity == OlogDiagnosticSeverityV1::Warning)
        .count();

    let mut preview = crate::evolution_preview::build_evolution_preview_v1(
        "typed_olog_authoring",
        None,
        format!("olog_fragment:{}", compiled_ir.schema_id),
        typed_change,
        &quality_delta,
        None,
        &proposal_trust_from_typed_authoring(&checked.trust),
        None,
        checked
            .diagnostics
            .iter()
            .map(|diag| format!("{}: {}", diag.code, diag.message)),
        checked.ok,
    );
    preview.refinement_candidates = checked.refinement_candidates.clone();
    preview
}

pub fn build_olog_fragment_evolution_preview_against_axi_text(
    axi_text: &str,
    schema_name: Option<&str>,
    checked: &CheckedOlogFragmentV1,
) -> Option<crate::evolution_preview::EvolutionPreviewV1> {
    let module = axiograph_dsl::schema_v1::parse_schema_v1(axi_text).ok()?;
    let schema = select_schema(&module, schema_name).ok()?;
    let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
    let theories = compile_theories_for_schema(&module, schema, &compiled_ir);
    build_olog_fragment_evolution_preview_against_compiled_semantics_ir(
        &compiled_ir,
        &theories,
        checked,
    )
}

pub fn check_olog_fragment_against_compiled_schema_ir(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: OlogFragmentV1,
) -> CheckedOlogFragmentV1 {
    let (diagnostics, typed_holes) = evaluate_olog_fragment(compiled_ir, &fragment);
    let refinement_candidates = typed_holes
        .iter()
        .flat_map(|hole| hole.refinement_candidates.clone().into_iter())
        .collect::<Vec<_>>();
    let ok = diagnostics
        .iter()
        .all(|diag| diag.severity != OlogDiagnosticSeverityV1::Error);
    let lifecycle_state = if ok {
        "runtime_checked"
    } else {
        "runtime_rejected"
    };

    let typed_change = olog_fragment_typed_change_summary(compiled_ir, &fragment);

    CheckedOlogFragmentV1 {
        lifecycle_state: lifecycle_state.to_string(),
        ok,
        trust: trust(
            if ok {
                "runtime_checked_olog_fragment"
            } else {
                "runtime_rejected_olog_fragment"
            },
            "compiled_schema_ir_runtime_olog_check",
            "typed_olog_fragment_only",
            "compiled_schema_ir",
            vec![
                format!(
                    "fragment was checked against compiled schema `{}`",
                    compiled_ir.schema_id
                ),
                "this is a runtime-usable type check for authoring and exploration, not a completeness or ontology-closure claim".to_string(),
            ],
        ),
        schema_id: Some(compiled_ir.schema_id.clone()),
        axi_well_typed_proof_v1: None,
        typed_change: Some(typed_change),
        fragment,
        diagnostics,
        typed_holes,
        refinement_candidates,
    }
}

pub fn check_olog_fragment_against_compiled_semantics_ir(
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[TheoryIr],
    fragment: OlogFragmentV1,
) -> CheckedOlogFragmentV1 {
    let mut checked = check_olog_fragment_against_compiled_schema_ir(compiled_ir, fragment);
    enrich_checked_olog_fragment_with_compiled_theory(&mut checked, compiled_ir, theories);
    if theories.is_empty() {
        return checked;
    }

    checked.lifecycle_state = if checked.ok {
        "runtime_checked_theory_enriched".to_string()
    } else {
        "runtime_rejected_theory_enriched".to_string()
    };
    checked.trust = trust(
        if checked.ok {
            "runtime_checked_olog_fragment_with_theory"
        } else {
            "runtime_rejected_olog_fragment_with_theory"
        },
        "compiled_schema_and_theory_ir_runtime_olog_check",
        "typed_olog_fragment_only",
        "compiled_schema_ir_and_compiled_theory_ir",
        vec![
            format!(
                "fragment was checked against compiled schema `{}`",
                compiled_ir.schema_id
            ),
            format!(
                "typed holes and refinement candidates were enriched from {} compiled theory item(s)",
                theories.len()
            ),
            "this is a runtime-usable typed authoring claim for the fragment only; completeness and ontology closure remain unclaimed".to_string(),
        ],
    );
    checked
}

pub fn build_olog_fragment_evolution_preview_against_compiled_semantics_ir(
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[TheoryIr],
    checked: &CheckedOlogFragmentV1,
) -> Option<crate::evolution_preview::EvolutionPreviewV1> {
    let mut enriched = checked.clone();
    enrich_checked_olog_fragment_with_compiled_theory(&mut enriched, compiled_ir, theories);
    Some(build_olog_fragment_evolution_preview(
        compiled_ir,
        &enriched,
    ))
}

pub fn check_olog_fragment_against_axi_text(
    axi_text: &str,
    schema_name: Option<&str>,
    fragment: OlogFragmentV1,
) -> CheckedOlogFragmentV1 {
    let module = match axiograph_dsl::schema_v1::parse_schema_v1(axi_text) {
        Ok(module) => module,
        Err(err) => {
            let diagnostics = vec![OlogDiagnosticV1 {
                severity: OlogDiagnosticSeverityV1::Error,
                code: "olog_axi_parse_failed".to_string(),
                message: format!("cannot check olog fragment because the canonical .axi draft did not parse: {err}"),
                repair_hint: Some("fix the canonical .axi draft before checking olog fragments against it".to_string()),
            }];
            return CheckedOlogFragmentV1 {
                lifecycle_state: "draft_only".to_string(),
                ok: false,
                trust: trust(
                    "draft_only",
                    "not_yet_well_typed",
                    "typed_olog_fragment_only",
                    "canonical_axi_draft",
                    vec![
                        "olog fragment could not be checked because the base canonical .axi draft did not parse".to_string(),
                    ],
                ),
                schema_id: None,
                axi_well_typed_proof_v1: None,
                typed_change: None,
                fragment,
                diagnostics,
                typed_holes: Vec::new(),
                refinement_candidates: Vec::new(),
            };
        }
    };

    let schema = match select_schema(&module, schema_name) {
        Ok(schema) => schema,
        Err(err) => {
            let diagnostics = vec![OlogDiagnosticV1 {
                severity: OlogDiagnosticSeverityV1::Error,
                code: "olog_schema_selection_failed".to_string(),
                message: err.clone(),
                repair_hint: Some(
                    "pass an explicit schema_name or simplify the module so the authoring target is unambiguous".to_string(),
                ),
            }];
            return CheckedOlogFragmentV1 {
                lifecycle_state: "draft_only".to_string(),
                ok: false,
                trust: trust(
                    "draft_only",
                    "not_yet_well_typed",
                    "typed_olog_fragment_only",
                    "canonical_axi_draft",
                    vec![
                        "olog fragment could not be scoped to a single schema from the base canonical draft".to_string(),
                    ],
                ),
                schema_id: None,
                axi_well_typed_proof_v1: None,
                typed_change: None,
                fragment,
                diagnostics,
                typed_holes: Vec::new(),
                refinement_candidates: Vec::new(),
            };
        }
    };

    let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
    let theories = compile_theories_for_schema(&module, schema, &compiled_ir);
    let mut checked =
        check_olog_fragment_against_compiled_semantics_ir(&compiled_ir, &theories, fragment);
    checked.lifecycle_state = "parsed_schema_checked".to_string();

    match axiograph_pathdb::validate_axi_v1_module(module) {
        Ok(validated) => {
            let proof = validated.proof().clone();
            checked.axi_well_typed_proof_v1 = Some(proof);
            if checked.ok {
                checked.lifecycle_state = "validated".to_string();
                checked.trust = trust(
                    "validated_olog_fragment",
                    "rust_side_well_typed_module_check_plus_compiled_schema_ir_olog_check",
                    "typed_olog_fragment_only",
                    "canonical_axi_draft_and_selected_schema",
                    vec![
                        "base canonical .axi module passed the Rust-side well-typed module gate".to_string(),
                        format!(
                            "olog fragment was checked against compiled schema `{}`",
                            compiled_ir.schema_id
                        ),
                        "the result is a runtime typing claim for this fragment only; completeness and ontology closure remain unclaimed".to_string(),
                    ],
                );
            } else {
                checked.lifecycle_state = "validated_schema_fragment_errors".to_string();
                checked.trust = trust(
                    "validated_schema_fragment_errors",
                    "rust_side_well_typed_module_check_plus_failed_olog_fragment_check",
                    "typed_olog_fragment_only",
                    "canonical_axi_draft_and_selected_schema",
                    vec![
                        "base canonical .axi module is well-typed, but the olog fragment still violates schema/category typing requirements".to_string(),
                    ],
                );
            }
        }
        Err(err) => {
            error(
                &mut checked.diagnostics,
                "olog_base_module_not_well_typed",
                format!(
                    "olog fragment was checked against a parsed schema, but the base canonical .axi module did not pass the Rust-side well-typed gate: {err}"
                ),
                Some("fix the base module first so authoring claims can be grounded in a validated canonical schema".to_string()),
            );
            checked.ok = false;
            checked.lifecycle_state = "draft_only".to_string();
            checked.trust = trust(
                "draft_only",
                "parsed_schema_only",
                "typed_olog_fragment_only",
                "canonical_axi_draft_and_selected_schema",
                vec![
                    "the fragment could be checked against a parsed schema shape, but the base canonical module is not yet well-typed".to_string(),
                ],
            );
        }
    }

    checked
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OlogRefinementApplyResultV1 {
    pub handle: crate::typed_refinement::RuntimeRefinementHandleV2,
    pub base_fragment: OlogFragmentV1,
    pub refined_fragment: OlogFragmentV1,
    pub checked_fragment: CheckedOlogFragmentV1,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_handle_to_olog_fragment_against_compiled_schema_ir(
    compiled_ir: &RuntimeSchemaIndex,
    fragment: &OlogFragmentV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV2,
) -> anyhow::Result<OlogRefinementApplyResultV1> {
    let current = check_olog_fragment_against_compiled_schema_ir(compiled_ir, fragment.clone());
    let emitted = current
        .refinement_candidates
        .iter()
        .find(|candidate| candidate.handle.id == handle.id)
        .map(|candidate| &candidate.handle)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "olog refinement handle `{}` was not emitted for this compiled schema and fragment",
                handle.id
            )
        })?;
    if emitted != handle {
        return Err(anyhow::anyhow!(
            "olog refinement handle `{}` payload differs from the current candidate",
            handle.id
        ));
    }
    let refined_fragment = crate::typed_refinement::apply_olog_refinement_handle(fragment, handle)?;
    let checked_fragment =
        check_olog_fragment_against_compiled_schema_ir(compiled_ir, refined_fragment.clone());
    Ok(OlogRefinementApplyResultV1 {
        handle: handle.clone(),
        base_fragment: fragment.clone(),
        refined_fragment,
        checked_fragment,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_handle_to_olog_fragment_against_compiled_semantics_ir(
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[TheoryIr],
    fragment: &OlogFragmentV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV2,
) -> anyhow::Result<OlogRefinementApplyResultV1> {
    let current =
        check_olog_fragment_against_compiled_semantics_ir(compiled_ir, theories, fragment.clone());
    let emitted = current
        .refinement_candidates
        .iter()
        .find(|candidate| candidate.handle.id == handle.id)
        .map(|candidate| &candidate.handle)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "olog refinement handle `{}` was not emitted for this compiled theory and fragment",
                handle.id
            )
        })?;
    if emitted != handle {
        return Err(anyhow::anyhow!(
            "olog refinement handle `{}` payload differs from the current candidate",
            handle.id
        ));
    }
    let refined_fragment = crate::typed_refinement::apply_olog_refinement_handle(fragment, handle)?;
    let checked_fragment = check_olog_fragment_against_compiled_semantics_ir(
        compiled_ir,
        theories,
        refined_fragment.clone(),
    );
    Ok(OlogRefinementApplyResultV1 {
        handle: handle.clone(),
        base_fragment: fragment.clone(),
        refined_fragment,
        checked_fragment,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_schema_ir(
    compiled_ir: &RuntimeSchemaIndex,
    checked: &CheckedOlogFragmentV1,
    handle_id: &str,
) -> anyhow::Result<OlogRefinementApplyResultV1> {
    let handle = checked
        .refinement_candidates
        .iter()
        .find(|candidate| candidate.handle.id == handle_id)
        .map(|candidate| candidate.handle.clone())
        .ok_or_else(|| anyhow::anyhow!("unknown olog refinement handle `{handle_id}`"))?;
    apply_runtime_refinement_handle_to_olog_fragment_against_compiled_schema_ir(
        compiled_ir,
        &checked.fragment,
        &handle,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_semantics_ir(
    compiled_ir: &RuntimeSchemaIndex,
    theories: &[TheoryIr],
    checked: &CheckedOlogFragmentV1,
    handle_id: &str,
) -> anyhow::Result<OlogRefinementApplyResultV1> {
    let handle = checked
        .refinement_candidates
        .iter()
        .find(|candidate| candidate.handle.id == handle_id)
        .map(|candidate| candidate.handle.clone())
        .ok_or_else(|| anyhow::anyhow!("unknown olog refinement handle `{handle_id}`"))?;
    apply_runtime_refinement_handle_to_olog_fragment_against_compiled_semantics_ir(
        compiled_ir,
        theories,
        &checked.fragment,
        &handle,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_by_id_to_olog_fragment_against_axi_text(
    axi_text: &str,
    schema_name: Option<&str>,
    checked: &CheckedOlogFragmentV1,
    handle_id: &str,
) -> anyhow::Result<OlogRefinementApplyResultV1> {
    crate::axi_input::require_canonical_axi_text(axi_text)?;
    let module = axiograph_dsl::schema_v1::parse_schema_v1(axi_text)
        .map_err(|err| anyhow::anyhow!("failed to parse canonical .axi draft: {err}"))?;
    let schema = select_schema(&module, schema_name).map_err(|err| anyhow::anyhow!(err))?;
    let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
    let theories = compile_theories_for_schema(&module, schema, &compiled_ir);
    apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_semantics_ir(
        &compiled_ir,
        &theories,
        checked,
        handle_id,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn discover_check_olog_report_against_axi_text(
    axi_text: &str,
    schema_name: Option<&str>,
    fragment: OlogFragmentV1,
    apply_refinement_handle_id: Option<&str>,
) -> anyhow::Result<DiscoverCheckOlogReportV1> {
    crate::axi_input::require_canonical_axi_text(axi_text)?;
    let checked = check_olog_fragment_against_axi_text(axi_text, schema_name, fragment);
    let (checked_olog, applied_refinement) = if let Some(handle_id) = apply_refinement_handle_id {
        let applied = apply_runtime_refinement_by_id_to_olog_fragment_against_axi_text(
            axi_text,
            schema_name,
            &checked,
            handle_id,
        )?;
        (applied.checked_fragment.clone(), Some(applied))
    } else {
        (checked, None)
    };
    let evolution_preview = build_olog_fragment_evolution_preview_against_axi_text(
        axi_text,
        schema_name,
        &checked_olog,
    );
    Ok(DiscoverCheckOlogReportV1 {
        version: "axiograph_discover_check_olog_v1".to_string(),
        checked_olog,
        evolution_preview,
        applied_refinement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_axi() -> &'static str {
        r#"
module Demo

schema S:
  object Person
  object Team
  object Context
  relation Parent(parent: Person, child: Person)
  relation WorksFor(employee: Person, employer: Team, ctx: Context @context)

instance I of S:
  Person = {Alice, Bob}
  Team = {Ops}
  Context = {Prod}
  Parent = {(parent=Bob, child=Alice)}
  WorksFor = {(employee=Alice, employer=Ops, ctx=Prod)}
"#
    }

    fn sample_axi_with_theory() -> &'static str {
        r#"
module Demo

schema S:
  object Person
  object Team
  object Context
  relation WorksFor(employee: Person, employer: Team, ctx: Context @context)

theory T on S:
  constraint functional WorksFor.employee -> WorksFor.employer

instance I of S:
  Person = {Alice}
  Team = {Ops}
  Context = {Prod}
  WorksFor = {(employee=Alice, employer=Ops, ctx=Prod)}
"#
    }

    fn sample_axi_with_role_split() -> &'static str {
        r#"
module Demo

schema S:
  object Person
  object Employee
  object Contractor
  object Task
  subtype Employee <: Person
  subtype Contractor <: Person
  relation Assigned(worker: Person, task: Task)

instance I of S:
  Person = {Alice, Bob}
  Employee = {Alice}
  Contractor = {Bob}
  Task = {TicketA, TicketB}
  Assigned = {
    (worker=Alice, task=TicketA),
    (worker=Bob, task=TicketB)
  }
"#
    }

    #[test]
    fn draft_typed_authoring_summary_reports_validated_module() {
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;

        let summary = draft_typed_authoring_summary_from_axi_text(axi);
        assert_eq!(summary.lifecycle_state, "validated");
        assert_eq!(summary.trust.trust_class, "validated_draft");
        assert_eq!(summary.trust.completeness_claim, "not_claimed");
        assert_eq!(summary.trust.ontology_closure_claim, "not_claimed");
        assert_eq!(
            summary
                .axi_well_typed_proof_v1
                .as_ref()
                .map(|proof| proof.module_name.as_str()),
            Some("Demo")
        );
        let kernel_module_ir = summary
            .kernel_module_ir
            .as_ref()
            .expect("validated draft should expose kernel module ir");
        assert_eq!(kernel_module_ir.instances.len(), 1);
        assert_eq!(
            kernel_module_ir.instances[0].instance_id.as_str(),
            "instance:S:I"
        );
        assert_eq!(kernel_module_ir.instances[0].relation_facts.len(), 1);
        assert_eq!(
            kernel_module_ir.instances[0].relation_facts[0].relation_name,
            "Parent"
        );
    }

    #[test]
    fn draft_typed_authoring_summary_reports_draft_only_parse_failure() {
        let axi = "module Broken\nschema S:\n  object Person\ninstance I of S:\n  Parent = {oops}";
        let summary = draft_typed_authoring_summary_from_axi_text(axi);
        assert_eq!(summary.lifecycle_state, "draft_only");
        assert_eq!(summary.trust.trust_class, "draft_only");
        assert!(summary.axi_well_typed_proof_v1.is_none());
        assert!(summary.kernel_module_ir.is_none());
        assert!(summary
            .trust
            .reasons
            .iter()
            .any(|reason| reason.contains("did not pass") || reason.contains("did not parse")));
    }

    #[test]
    fn check_olog_fragment_accepts_relation_object_and_projection_authoring() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: Some("an employee".to_string()),
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: Some("a team".to_string()),
                },
                OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: Some("a context".to_string()),
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "employer".to_string(),
                        target_box: "team".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: vec![
                OlogAspectV1 {
                    aspect_id: "employee_role".to_string(),
                    from_box: "works_for_fact".to_string(),
                    to_box: "employee".to_string(),
                    kind: OlogAspectKindV1::RelationProjection {
                        relation_box: "works_for_fact".to_string(),
                        role: "employee".to_string(),
                    },
                },
                OlogAspectV1 {
                    aspect_id: "employer_role".to_string(),
                    from_box: "works_for_fact".to_string(),
                    to_box: "team".to_string(),
                    kind: OlogAspectKindV1::RelationProjection {
                        relation_box: "works_for_fact".to_string(),
                        role: "employer".to_string(),
                    },
                },
            ],
            path_equations: Vec::new(),
        };

        let checked = check_olog_fragment_against_axi_text(sample_axi(), Some("S"), fragment);
        assert!(checked.ok, "{:?}", checked.diagnostics);
        assert_eq!(checked.lifecycle_state, "validated");
        assert_eq!(checked.trust.trust_class, "validated_olog_fragment");
        assert!(checked
            .diagnostics
            .iter()
            .all(|diag| diag.severity != OlogDiagnosticSeverityV1::Error));
        let typed_change = checked
            .typed_change
            .as_ref()
            .expect("validated checked fragments should carry typed change summary");
        assert_eq!(typed_change.kind, "olog_fragment_delta");
        assert!(typed_change.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                crate::evolution_preview::EvolutionPrimitiveV1::ReifyRelationObject {
                    relation,
                    ..
                } if relation == "WorksFor"
            )
        }));
        assert!(typed_change.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                crate::evolution_preview::EvolutionPrimitiveV1::IntroduceDependentRelationFamily {
                    relation,
                    ..
                } if relation == "WorksFor"
            )
        }));
    }

    #[test]
    fn check_olog_fragment_rejects_missing_relation_role_binding() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "employer".to_string(),
                        target_box: "team".to_string(),
                    },
                ],
            }],
            aspects: Vec::new(),
            path_equations: Vec::new(),
        };

        let checked = check_olog_fragment_against_axi_text(sample_axi(), Some("S"), fragment);
        assert!(!checked.ok);
        assert_eq!(checked.lifecycle_state, "validated_schema_fragment_errors");
        assert!(checked
            .diagnostics
            .iter()
            .any(|diag| diag.code == "olog_role_missing" && diag.message.contains("ctx")));
    }

    #[test]
    fn check_olog_fragment_rejects_direct_carrier_for_nary_relation_with_hidden_data_roles() {
        let axi = r#"
module Billing

schema BillingSchema:
  object Account
  object Currency
  relation Transfer(from: Account, to: Account, currency: Currency)

instance I of BillingSchema:
  Account = {A1, A2}
  Currency = {USD}
  Transfer = {(from=A1, to=A2, currency=USD)}
"#;
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "source".to_string(),
                    object_type: "Account".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "target".to_string(),
                    object_type: "Account".to_string(),
                    label: None,
                },
            ],
            relation_boxes: Vec::new(),
            aspects: vec![OlogAspectV1 {
                aspect_id: "transfer_to".to_string(),
                from_box: "source".to_string(),
                to_box: "target".to_string(),
                kind: OlogAspectKindV1::RelationCarrier {
                    relation: "Transfer".to_string(),
                },
            }],
            path_equations: Vec::new(),
        };

        let checked = check_olog_fragment_against_axi_text(axi, Some("BillingSchema"), fragment);
        assert!(!checked.ok);
        assert!(checked
            .diagnostics
            .iter()
            .any(|diag| diag.code == "olog_relation_requires_relation_box"));
    }

    #[test]
    fn check_olog_fragment_rejects_path_equations_with_mismatched_endpoints() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "parent".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "child".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
            ],
            relation_boxes: Vec::new(),
            aspects: vec![OlogAspectV1 {
                aspect_id: "parent_of".to_string(),
                from_box: "parent".to_string(),
                to_box: "child".to_string(),
                kind: OlogAspectKindV1::RelationCarrier {
                    relation: "Parent".to_string(),
                },
            }],
            path_equations: vec![OlogPathEquationV1 {
                equation_id: "bad_eq".to_string(),
                lhs: OlogPathV1 {
                    steps: vec!["parent_of".to_string()],
                },
                rhs: OlogPathV1 {
                    steps: vec!["parent_of".to_string(), "parent_of".to_string()],
                },
            }],
        };

        let checked = check_olog_fragment_against_axi_text(sample_axi(), Some("S"), fragment);
        assert!(!checked.ok);
        assert!(checked
            .diagnostics
            .iter()
            .any(|diag| diag.code == "olog_path_endpoint_mismatch"
                || diag.code == "olog_equation_endpoint_mismatch"));
    }

    #[test]
    fn check_olog_fragment_emits_path_equation_change_summary() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "parent".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "child".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
            ],
            relation_boxes: Vec::new(),
            aspects: vec![OlogAspectV1 {
                aspect_id: "parent_of".to_string(),
                from_box: "parent".to_string(),
                to_box: "child".to_string(),
                kind: OlogAspectKindV1::RelationCarrier {
                    relation: "Parent".to_string(),
                },
            }],
            path_equations: vec![OlogPathEquationV1 {
                equation_id: "parent_eq".to_string(),
                lhs: OlogPathV1 {
                    steps: vec!["parent_of".to_string()],
                },
                rhs: OlogPathV1 {
                    steps: vec!["parent_of".to_string()],
                },
            }],
        };

        let checked = check_olog_fragment_against_axi_text(sample_axi(), Some("S"), fragment);
        assert!(checked.ok, "{:?}", checked.diagnostics);
        let typed_change = checked
            .typed_change
            .as_ref()
            .expect("checked fragments should carry typed change summary");
        assert_eq!(typed_change.theory.added, 1);
        assert!(typed_change.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                crate::evolution_preview::EvolutionPrimitiveV1::AddPathEquation {
                    equation_id,
                    start_box,
                    end_box,
                    lhs_steps,
                    rhs_steps,
                    ..
                } if equation_id == "parent_eq"
                    && start_box.as_deref() == Some("parent")
                    && end_box.as_deref() == Some("child")
                    && *lhs_steps == 1
                    && *rhs_steps == 1
            )
        }));
    }

    #[test]
    fn compiled_ir_olog_check_allows_subtype_targets() {
        let axi = r#"
module Demo

schema S:
  object Person
  object Employee
  object Team
  subtype Employee < Person
  relation Assigned(person: Person, team: Team)

instance I of S:
  Person = {Alice}
  Employee = {Alice}
  Team = {Ops}
  Assigned = {(person=Alice, team=Ops)}
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let checked = check_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Employee".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: Vec::new(),
                aspects: vec![OlogAspectV1 {
                    aspect_id: "assigned_to".to_string(),
                    from_box: "employee".to_string(),
                    to_box: "team".to_string(),
                    kind: OlogAspectKindV1::RelationCarrier {
                        relation: "Assigned".to_string(),
                    },
                }],
                path_equations: Vec::new(),
            },
        );
        assert!(checked.ok, "{:?}", checked.diagnostics);
        assert_eq!(checked.lifecycle_state, "runtime_checked");
    }

    #[test]
    fn compiled_ir_olog_check_reports_admissible_player_types_in_role_repairs() {
        let axi = r#"
module Demo

schema S:
  object Person
  object Employee
  object Team
  subtype Employee < Person
  relation Assigned(person: Person, team: Team)
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let checked = check_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "team_as_person".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "assigned".to_string(),
                    relation: "Assigned".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "person".to_string(),
                            target_box: "team_as_person".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "team".to_string(),
                            target_box: "team".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        let mismatch = checked
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "olog_role_type_mismatch")
            .expect("role mismatch diagnostic");
        assert_eq!(
            mismatch.repair_hint.as_deref(),
            Some("retarget the role to a box with one of the admissible player types: Employee, Person")
        );
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| hole.kind == OlogTypedHoleKindV1::RoleBindingTypeMismatch)
            .expect("role mismatch hole");
        assert_eq!(hole.relation_box.as_deref(), Some("assigned"));
        assert_eq!(hole.relation.as_deref(), Some("Assigned"));
        assert_eq!(hole.role.as_deref(), Some("person"));
        assert_eq!(hole.expected_types, vec!["Person".to_string()]);
        assert_eq!(
            hole.admissible_player_types,
            vec!["Employee".to_string(), "Person".to_string()]
        );
        assert_eq!(hole.candidate_boxes, Vec::<String>::new());
        assert!(hole
            .suggestions
            .iter()
            .any(|suggestion| suggestion.contains("admissible player types")));
        assert!(hole.refinement_candidates.is_empty());
    }

    #[test]
    fn compiled_ir_olog_check_emits_typed_hole_for_missing_relation_role_binding() {
        let axi = r#"
module Demo

schema S:
  object Person
  object Employee
  object Team
  object Context
  subtype Employee < Person
  relation WorksFor(employee: Person, employer: Team, ctx: Context @context)
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let checked = check_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Employee".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| hole.kind == OlogTypedHoleKindV1::MissingRelationRoleBinding)
            .expect("missing role hole");
        assert_eq!(hole.relation_box.as_deref(), Some("works_for"));
        assert_eq!(hole.relation.as_deref(), Some("WorksFor"));
        assert_eq!(hole.role.as_deref(), Some("employer"));
        assert_eq!(hole.expected_types, vec!["Team".to_string()]);
        assert_eq!(hole.admissible_player_types, vec!["Team".to_string()]);
        assert_eq!(hole.candidate_boxes, vec!["team".to_string()]);
        assert!(hole
            .suggestions
            .iter()
            .any(|suggestion| suggestion == "bind role `employer` to box `team`"));
        assert_eq!(checked.refinement_candidates.len(), 1);
        let candidate = hole
            .refinement_candidates
            .first()
            .expect("expected olog refinement candidate");
        assert!(matches!(
            candidate.kind,
            crate::typed_refinement::RuntimeRefinementCandidateKindV2::BindRelationRole
        ));
        assert!(matches!(
            candidate.handle.domain(),
            crate::typed_refinement::RuntimeRefinementDomainV2::OlogAuthoring
        ));
        assert_eq!(candidate.target_box.as_deref(), Some("team"));
    }

    #[test]
    fn compiled_semantics_olog_check_attaches_theory_handles_without_axi_text() {
        let module =
            axiograph_dsl::schema_v1::parse_schema_v1(sample_axi_with_theory()).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let theories = compile_theories_for_schema(&module, schema, &compiled_ir);

        let checked = check_olog_fragment_against_compiled_semantics_ir(
            &compiled_ir,
            &theories,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        assert_eq!(checked.lifecycle_state, "runtime_rejected_theory_enriched");
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| {
                hole.kind == OlogTypedHoleKindV1::MissingRelationRoleBinding
                    && hole.role.as_deref() == Some("employer")
            })
            .expect("missing role hole");
        assert!(matches!(
            hole.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(hole.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employer"
        )));
        assert!(checked.refinement_candidates.iter().any(|candidate| {
            candidate.target_box.as_deref() == Some("team")
                && matches!(
                    candidate.theory_obligation_ref.as_ref(),
                    Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                        relation_name,
                        ..
                    }) if relation_name.as_deref() == Some("WorksFor")
                )
        }));
    }

    #[test]
    fn validated_olog_role_mismatch_hole_attaches_compiled_theory_handles() {
        let checked = check_olog_fragment_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "wrong_employee".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "wrong_employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "employer".to_string(),
                            target_box: "team".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| {
                hole.kind == OlogTypedHoleKindV1::RoleBindingTypeMismatch
                    && hole.role.as_deref() == Some("employee")
            })
            .expect("role mismatch hole");
        assert!(matches!(
            hole.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(matches!(
            hole.theory_subject_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                relation_name,
                ..
            }) if relation_name == "WorksFor"
        ));
        assert!(hole.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employee"
        )));
        let candidate = hole
            .refinement_candidates
            .first()
            .expect("retarget candidate");
        assert_eq!(candidate.target_box.as_deref(), Some("employee"));
        assert!(matches!(
            candidate.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(candidate.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employee"
        )));
    }

    #[test]
    fn validated_olog_missing_role_hole_and_candidates_attach_compiled_theory_handles() {
        let checked = check_olog_fragment_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| {
                hole.kind == OlogTypedHoleKindV1::MissingRelationRoleBinding
                    && hole.role.as_deref() == Some("employer")
            })
            .expect("missing role hole");
        assert!(matches!(
            hole.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(hole.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employer"
        )));
        let candidate = hole
            .refinement_candidates
            .first()
            .expect("bind role candidate");
        assert_eq!(candidate.target_box.as_deref(), Some("team"));
        assert!(matches!(
            candidate.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(candidate.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employer"
        )));
        assert!(checked.refinement_candidates.iter().any(|candidate| {
            candidate.target_box.as_deref() == Some("team")
                && matches!(
                    candidate.theory_obligation_ref.as_ref(),
                    Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                        relation_name,
                        ..
                    }) if relation_name.as_deref() == Some("WorksFor")
                )
        }));
    }

    #[test]
    fn validated_olog_projection_unbound_hole_attaches_compiled_theory_handles() {
        let checked = check_olog_fragment_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: vec![OlogAspectV1 {
                    aspect_id: "employer_role".to_string(),
                    from_box: "works_for".to_string(),
                    to_box: "team".to_string(),
                    kind: OlogAspectKindV1::RelationProjection {
                        relation_box: "works_for".to_string(),
                        role: "employer".to_string(),
                    },
                }],
                path_equations: Vec::new(),
            },
        );
        assert!(!checked.ok);
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| {
                hole.kind == OlogTypedHoleKindV1::ProjectionRoleUnbound
                    && hole.aspect_id.as_deref() == Some("employer_role")
            })
            .expect("projection role-unbound hole");
        assert!(matches!(
            hole.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(hole.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employer"
        )));
        let candidate = hole
            .refinement_candidates
            .first()
            .expect("projection repair candidate");
        assert_eq!(candidate.target_box.as_deref(), Some("team"));
        assert!(matches!(
            candidate.theory_obligation_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                relation_name,
                ..
            }) if relation_name.as_deref() == Some("WorksFor")
        ));
        assert!(candidate.theory_subject_refs.iter().any(|subject| matches!(
            subject,
            axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                relation_name,
                role_name,
                ..
            } if relation_name == "WorksFor" && role_name == "employer"
        )));
    }

    #[test]
    fn compiled_ir_olog_check_emits_typed_hole_for_path_endpoint_mismatch() {
        let axi = r#"
module Demo

schema S:
  object Person
  object Team
  object Context
  relation WorksFor(employee: Person, employer: Team, ctx: Context @context)
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let checked = check_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "employer".to_string(),
                            target_box: "team".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: vec![
                    OlogAspectV1 {
                        aspect_id: "employee_role".to_string(),
                        from_box: "works_for".to_string(),
                        to_box: "employee".to_string(),
                        kind: OlogAspectKindV1::RelationProjection {
                            relation_box: "works_for".to_string(),
                            role: "employee".to_string(),
                        },
                    },
                    OlogAspectV1 {
                        aspect_id: "employer_role".to_string(),
                        from_box: "works_for".to_string(),
                        to_box: "team".to_string(),
                        kind: OlogAspectKindV1::RelationProjection {
                            relation_box: "works_for".to_string(),
                            role: "employer".to_string(),
                        },
                    },
                ],
                path_equations: vec![OlogPathEquationV1 {
                    equation_id: "bad_path".to_string(),
                    lhs: OlogPathV1 {
                        steps: vec!["employee_role".to_string(), "employer_role".to_string()],
                    },
                    rhs: OlogPathV1 {
                        steps: vec!["employee_role".to_string()],
                    },
                }],
            },
        );
        assert!(!checked.ok);
        let hole = checked
            .typed_holes
            .iter()
            .find(|hole| {
                hole.kind == OlogTypedHoleKindV1::PathEndpointMismatch
                    && hole.equation_id.as_deref() == Some("bad_path")
                    && hole.aspect_id.as_deref() == Some("employer_role")
            })
            .expect("path endpoint mismatch hole");
        assert_eq!(hole.expected_types, vec!["employee".to_string()]);
        assert_eq!(
            hole.candidate_boxes,
            vec!["employee".to_string(), "works_for".to_string()]
        );
        assert!(hole
            .suggestions
            .iter()
            .any(|suggestion| suggestion.contains("insert an intermediate aspect")));
        assert!(hole.refinement_candidates.is_empty());
    }

    #[test]
    fn compiled_ir_olog_runtime_refinement_can_bind_missing_role() {
        let axi = r#"
module Demo

schema S:
  object Person
  object Employee
  object Team
  object Context
  subtype Employee < Person
  relation WorksFor(employee: Person, employer: Team, ctx: Context @context)
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let checked = check_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Employee".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        let handle_id = checked
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected shared authoring refinement handle");
        let applied = apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_schema_ir(
            &compiled_ir,
            &checked,
            &handle_id,
        )
        .expect("apply missing-role refinement");
        assert_eq!(applied.handle.id, handle_id);
        let rel_box = applied
            .refined_fragment
            .relation_boxes
            .iter()
            .find(|relation_box| relation_box.box_id == "works_for")
            .expect("refined relation box");
        assert!(rel_box
            .role_bindings
            .iter()
            .any(|binding| binding.role == "employer" && binding.target_box == "team"));
        assert!(
            applied.checked_fragment.ok,
            "{:?}",
            applied.checked_fragment.diagnostics
        );
    }

    #[test]
    fn compiled_semantics_olog_runtime_refinement_preserves_theory_handles() {
        let module =
            axiograph_dsl::schema_v1::parse_schema_v1(sample_axi_with_theory()).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let theories = compile_theories_for_schema(&module, schema, &compiled_ir);
        let checked = check_olog_fragment_against_compiled_semantics_ir(
            &compiled_ir,
            &theories,
            OlogFragmentV1 {
                boxes: vec![
                    OlogBoxV1 {
                        box_id: "employee".to_string(),
                        object_type: "Person".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "team".to_string(),
                        object_type: "Team".to_string(),
                        label: None,
                    },
                    OlogBoxV1 {
                        box_id: "ctx".to_string(),
                        object_type: "Context".to_string(),
                        label: None,
                    },
                ],
                relation_boxes: vec![OlogRelationBoxV1 {
                    box_id: "works_for".to_string(),
                    relation: "WorksFor".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "employee".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "ctx".to_string(),
                            target_box: "ctx".to_string(),
                        },
                    ],
                }],
                aspects: Vec::new(),
                path_equations: Vec::new(),
            },
        );
        let handle_id = checked
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected shared authoring refinement handle");
        let applied =
            apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_semantics_ir(
                &compiled_ir,
                &theories,
                &checked,
                &handle_id,
            )
            .expect("apply missing-role refinement");
        assert_eq!(applied.handle.id, handle_id);
        assert_eq!(
            applied.checked_fragment.lifecycle_state,
            "runtime_checked_theory_enriched"
        );
        assert!(
            applied.checked_fragment.ok,
            "{:?}",
            applied.checked_fragment.diagnostics
        );
        let rel_box = applied
            .refined_fragment
            .relation_boxes
            .iter()
            .find(|relation_box| relation_box.box_id == "works_for")
            .expect("refined relation box");
        assert!(rel_box
            .role_bindings
            .iter()
            .any(|binding| binding.role == "employer" && binding.target_box == "team"));
    }

    #[test]
    fn axi_text_olog_runtime_refinement_uses_compiled_semantics() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: Vec::new(),
            path_equations: Vec::new(),
        };
        let checked =
            check_olog_fragment_against_axi_text(sample_axi_with_theory(), Some("S"), fragment);
        let handle_id = checked
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected shared authoring refinement handle");
        let applied = apply_runtime_refinement_by_id_to_olog_fragment_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            &checked,
            &handle_id,
        )
        .expect("apply missing-role refinement against axi text");
        assert_eq!(applied.handle.id, handle_id);
        assert_eq!(
            applied.checked_fragment.lifecycle_state,
            "runtime_checked_theory_enriched"
        );
        assert!(
            applied.checked_fragment.ok,
            "{:?}",
            applied.checked_fragment.diagnostics
        );
        assert!(applied.checked_fragment.typed_holes.is_empty());
        let rel_box = applied
            .refined_fragment
            .relation_boxes
            .iter()
            .find(|relation_box| relation_box.box_id == "works_for")
            .expect("refined relation box");
        assert!(rel_box
            .role_bindings
            .iter()
            .any(|binding| binding.role == "employer" && binding.target_box == "team"));
    }

    #[test]
    fn discover_check_olog_report_can_apply_refinement() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: Vec::new(),
            path_equations: Vec::new(),
        };
        let checked = check_olog_fragment_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            fragment.clone(),
        );
        let handle_id = checked
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected shared authoring refinement handle");
        let report = discover_check_olog_report_against_axi_text(
            sample_axi_with_theory(),
            Some("S"),
            fragment,
            Some(handle_id.as_str()),
        )
        .expect("build discover check olog report");
        assert_eq!(report.version, "axiograph_discover_check_olog_v1");
        assert!(report.checked_olog.ok);
        assert_eq!(
            report.checked_olog.lifecycle_state,
            "runtime_checked_theory_enriched"
        );
        let applied = report
            .applied_refinement
            .as_ref()
            .expect("applied refinement");
        assert_eq!(applied.handle.id, handle_id);
        let rel_box = applied
            .refined_fragment
            .relation_boxes
            .iter()
            .find(|relation_box| relation_box.box_id == "works_for")
            .expect("refined relation box");
        assert!(rel_box
            .role_bindings
            .iter()
            .any(|binding| binding.role == "employer" && binding.target_box == "team"));
        assert!(report
            .evolution_preview
            .as_ref()
            .is_some_and(|preview| preview.ok));
    }

    #[test]
    fn olog_fragment_typed_change_summary_emits_relation_family_and_path_primitives() {
        let module = axiograph_dsl::schema_v1::parse_schema_v1(sample_axi()).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "employer".to_string(),
                        target_box: "team".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: vec![
                OlogAspectV1 {
                    aspect_id: "employee_role".to_string(),
                    from_box: "works_for_fact".to_string(),
                    to_box: "employee".to_string(),
                    kind: OlogAspectKindV1::RelationProjection {
                        relation_box: "works_for_fact".to_string(),
                        role: "employee".to_string(),
                    },
                },
                OlogAspectV1 {
                    aspect_id: "employer_role".to_string(),
                    from_box: "works_for_fact".to_string(),
                    to_box: "team".to_string(),
                    kind: OlogAspectKindV1::RelationProjection {
                        relation_box: "works_for_fact".to_string(),
                        role: "employer".to_string(),
                    },
                },
            ],
            path_equations: vec![OlogPathEquationV1 {
                equation_id: "employee_path".to_string(),
                lhs: OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
                rhs: OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
            }],
        };

        let summary = olog_fragment_typed_change_summary(&compiled_ir, &fragment);
        assert_eq!(summary.kind, "olog_fragment_delta");
        assert_eq!(summary.context.added, 1);
        assert_eq!(summary.theory.added, 1);
        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::ReifyRelationObject {
                relation,
                tuple_type,
                ..
            } if relation == "WorksFor" && tuple_type == "WorksFor"
        )));
        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::IntroduceDependentRelationFamily {
                relation,
                index_roles,
                ..
            } if relation == "WorksFor" && index_roles == &vec!["ctx".to_string()]
        )));
        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::AddPathEquation {
                equation_id,
                start_box,
                end_box,
                ..
            } if equation_id == "employee_path"
                && start_box.as_deref() == Some("works_for_fact")
                && end_box.as_deref() == Some("employee")
        )));
    }

    #[test]
    fn olog_fragment_typed_change_summary_emits_role_pushdown_and_split_primitives() {
        let module =
            axiograph_dsl::schema_v1::parse_schema_v1(sample_axi_with_role_split()).expect("parse");
        let schema = select_schema(&module, Some("S")).expect("schema");
        let compiled_ir = axiograph_pathdb::kernel_ir::derive_runtime_schema_index(schema);
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Employee".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "contractor".to_string(),
                    object_type: "Contractor".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "task_a".to_string(),
                    object_type: "Task".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "task_b".to_string(),
                    object_type: "Task".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![
                OlogRelationBoxV1 {
                    box_id: "assigned_employee".to_string(),
                    relation: "Assigned".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "worker".to_string(),
                            target_box: "employee".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "task".to_string(),
                            target_box: "task_a".to_string(),
                        },
                    ],
                },
                OlogRelationBoxV1 {
                    box_id: "assigned_contractor".to_string(),
                    relation: "Assigned".to_string(),
                    role_bindings: vec![
                        OlogRoleBindingV1 {
                            role: "worker".to_string(),
                            target_box: "contractor".to_string(),
                        },
                        OlogRoleBindingV1 {
                            role: "task".to_string(),
                            target_box: "task_b".to_string(),
                        },
                    ],
                },
            ],
            aspects: Vec::new(),
            path_equations: Vec::new(),
        };

        let summary = olog_fragment_typed_change_summary(&compiled_ir, &fragment);

        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::PushRelationRoleToSubtype {
                relation,
                role,
                from_supertype,
                to_subtype,
                ..
            } if relation == "Assigned"
                && role == "worker"
                && from_supertype == "Person"
                && to_subtype == "Employee"
        )));
        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::PushRelationRoleToSubtype {
                relation,
                role,
                from_supertype,
                to_subtype,
                ..
            } if relation == "Assigned"
                && role == "worker"
                && from_supertype == "Person"
                && to_subtype == "Contractor"
        )));
        assert!(summary.primitives.iter().any(|primitive| matches!(
            primitive,
            crate::evolution_preview::EvolutionPrimitiveV1::SplitTypeIntoSubtypes {
                source,
                subtypes,
                discriminator,
                ..
            } if source == "Person"
                && discriminator.as_deref() == Some("Assigned.worker")
                && subtypes.iter().any(|ty| ty == "Employee")
                && subtypes.iter().any(|ty| ty == "Contractor")
        )));
    }

    #[test]
    fn build_olog_fragment_evolution_preview_emits_directed_authoring_guidance() {
        let fragment = OlogFragmentV1 {
            boxes: vec![
                OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "employer".to_string(),
                        target_box: "team".to_string(),
                    },
                    OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: vec![OlogAspectV1 {
                aspect_id: "employee_role".to_string(),
                from_box: "works_for_fact".to_string(),
                to_box: "employee".to_string(),
                kind: OlogAspectKindV1::RelationProjection {
                    relation_box: "works_for_fact".to_string(),
                    role: "employee".to_string(),
                },
            }],
            path_equations: vec![OlogPathEquationV1 {
                equation_id: "employee_path".to_string(),
                lhs: OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
                rhs: OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
            }],
        };

        let checked = check_olog_fragment_against_axi_text(sample_axi(), Some("S"), fragment);
        let preview = build_olog_fragment_evolution_preview_against_axi_text(
            sample_axi(),
            Some("S"),
            &checked,
        )
        .expect("olog evolution preview");

        assert!(checked.ok, "{:?}", checked.diagnostics);
        assert_eq!(preview.kind, "typed_olog_authoring");
        assert_eq!(preview.semantic_delta.delta_kind, "olog_fragment_delta");
        assert!(preview
            .semantic_delta
            .changed_layers
            .iter()
            .any(|layer| layer == "schema"));
        assert!(preview
            .semantic_delta
            .changed_layers
            .iter()
            .any(|layer| layer == "theory"));
        assert!(preview
            .exploration_next_actions
            .iter()
            .any(|action| action.contains("indexed relation family `WorksFor`")));
        assert!(preview
            .exploration_next_actions
            .iter()
            .any(|action| action.contains("path equation `employee_path`")));
    }
}
