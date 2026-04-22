//! Minimal compiled schema IR for semantic endpoint selection.
//!
//! This is intentionally narrow: it centralizes relation-role semantics so
//! endpoint choice becomes a compiled schema fact instead of being repeated as
//! local heuristics across import/check paths.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use axiograph_dsl::digest::{axi_digest_v1, axi_fact_id_v1};
use axiograph_dsl::schema_v1::{
    parse_path_expr_v3, CarrierFieldsV1, ConstraintV1, PathExprV3, RewriteRuleV1, RewriteVarTypeV1,
    SchemaV1Instance, SchemaV1Module, SchemaV1Schema, SchemaV1Theory, SetItemV1,
};

use crate::{
    AxiDigest, ConstraintId, EquationId, InstanceId, ObjectTypeId, RelationId, RewriteRuleId,
    RoleId, SchemaId, StableFactId, TheoryId,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    Data,
    Context,
    Temporal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CarrierSource {
    HomotopyConvention,
    EndpointConvention,
    DeclaredOrder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WitnessViewIr {
    None,
    Morphism { from_role: u16, to_role: u16 },
    Homotopy { lhs_role: u16, rhs_role: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleIr {
    pub role_id: RoleId,
    pub name: String,
    pub target_type: String,
    pub order: u16,
    pub kind: RoleKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CarrierSpecIr {
    pub source_role: u16,
    pub target_role: u16,
    pub fiber_roles: Vec<u16>,
    pub source: CarrierSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationSemanticsIr {
    pub relation_id: RelationId,
    pub name: String,
    pub tuple_type_name: String,
    pub roles: Vec<RoleIr>,
    pub carrier: Option<CarrierSpecIr>,
    pub witness_view: WitnessViewIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleInterfaceIr {
    pub role_id: RoleId,
    pub scoped_role_name: String,
    pub relation_name: String,
    pub role_name: String,
    pub declared_target_type: String,
    pub role_kind: RoleKind,
    pub admissible_player_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectSubtypeFamilyIr {
    pub supertype: String,
    pub subtypes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SubtypeRoleProjectionIr {
    pub relations: Vec<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledSchemaIr {
    pub schema_id: SchemaId,
    pub object_types: HashSet<String>,
    pub object_type_ids: HashMap<String, ObjectTypeId>,
    pub supertypes_of: HashMap<String, HashSet<String>>,
    pub subtypes_of: HashMap<String, HashSet<String>>,
    pub relations: HashMap<String, RelationSemanticsIr>,
    pub role_interfaces: HashMap<String, RoleInterfaceIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KernelModuleIr {
    pub module_digest: AxiDigest,
    pub schemas: Vec<CompiledSchemaIr>,
    pub theories: Vec<TheoryIr>,
    pub instances: Vec<InstanceIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceIr {
    pub instance_id: InstanceId,
    pub schema_id: SchemaId,
    pub object_members: Vec<ObjectMembershipIr>,
    pub relation_facts: Vec<RelationFactIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectMembershipIr {
    pub object_type_id: ObjectTypeId,
    pub object_type_name: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationFactIr {
    pub fact_id: StableFactId,
    pub relation_id: RelationId,
    pub relation_name: String,
    pub role_values: Vec<RoleValueIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleValueIr {
    pub role_id: RoleId,
    pub role_name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintIr {
    pub constraint_id: ConstraintId,
    pub kind: String,
    pub summary: String,
    pub relation_name: Option<String>,
    pub relation_id: Option<RelationId>,
    pub field_refs: Vec<String>,
    pub field_role_ids: Vec<RoleId>,
    pub param_fields: Vec<String>,
    pub param_role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathEquationIr {
    pub equation_id: EquationId,
    pub name: String,
    pub lhs: PathExprV3,
    pub rhs: PathExprV3,
    pub relation_refs: Vec<String>,
    pub relation_ids: Vec<RelationId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpaqueEquationIr {
    pub equation_id: EquationId,
    pub name: String,
    pub lhs: String,
    pub rhs: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewriteRuleSource {
    AcceptedAxi,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteRuleIr {
    pub rule_id: RewriteRuleId,
    pub name: String,
    pub orientation: String,
    pub vars: Vec<String>,
    pub lhs: PathExprV3,
    pub rhs: PathExprV3,
    pub endpoint: RewriteEndpointIr,
    pub relation_refs: Vec<String>,
    pub relation_ids: Vec<RelationId>,
    pub source: RewriteRuleSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteEndpointIr {
    pub from_var: String,
    pub to_var: String,
    pub from_type: String,
    pub to_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryIr {
    pub theory_id: TheoryId,
    pub schema_id: SchemaId,
    pub constraints: Vec<ConstraintIr>,
    pub path_equations: Vec<PathEquationIr>,
    pub opaque_equations: Vec<OpaqueEquationIr>,
    pub rewrite_rules: Vec<RewriteRuleIr>,
}

pub const RUNTIME_THEORY_FRAGMENT_SUMMARY_VERSION_V1: &str = "runtime_theory_fragment_summary_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheoryObligationKindIr {
    Constraint,
    PathEquation,
    OpaqueEquation,
    RewriteRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TheoryObligationRefIr {
    Constraint {
        theory_id: TheoryId,
        constraint_id: ConstraintId,
        relation_name: Option<String>,
        summary: String,
    },
    PathEquation {
        theory_id: TheoryId,
        equation_id: EquationId,
        name: String,
    },
    OpaqueEquation {
        theory_id: TheoryId,
        equation_id: EquationId,
        name: String,
    },
    RewriteRule {
        theory_id: TheoryId,
        rule_id: RewriteRuleId,
        name: String,
    },
}

impl TheoryObligationRefIr {
    pub fn obligation_kind(&self) -> TheoryObligationKindIr {
        match self {
            Self::Constraint { .. } => TheoryObligationKindIr::Constraint,
            Self::PathEquation { .. } => TheoryObligationKindIr::PathEquation,
            Self::OpaqueEquation { .. } => TheoryObligationKindIr::OpaqueEquation,
            Self::RewriteRule { .. } => TheoryObligationKindIr::RewriteRule,
        }
    }

    pub fn stable_id(&self) -> String {
        match self {
            Self::Constraint { constraint_id, .. } => constraint_id.to_string(),
            Self::PathEquation { equation_id, .. } => equation_id.to_string(),
            Self::OpaqueEquation { equation_id, .. } => equation_id.to_string(),
            Self::RewriteRule { rule_id, .. } => rule_id.to_string(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Constraint {
                relation_name,
                summary,
                ..
            } => relation_name
                .as_ref()
                .map(|relation_name| format!("{relation_name}: {summary}"))
                .unwrap_or_else(|| summary.clone()),
            Self::PathEquation { name, .. }
            | Self::OpaqueEquation { name, .. }
            | Self::RewriteRule { name, .. } => name.clone(),
        }
    }

    pub fn matches_artifact_id(&self, artifact_id: &str) -> bool {
        let artifact_local = local_name(artifact_id);
        match self {
            Self::Constraint {
                constraint_id,
                relation_name,
                summary,
                ..
            } => {
                let legacy_prefix = strip_trailing_numeric_segment(artifact_id);
                constraint_id.as_str() == artifact_id
                    || legacy_prefix.is_some_and(|prefix| {
                        constraint_id.as_str().starts_with(&format!("{prefix}:"))
                    })
                    || relation_name
                        .as_ref()
                        .is_some_and(|relation| local_name(relation) == artifact_local)
                    || summary == artifact_id
            }
            Self::PathEquation {
                equation_id, name, ..
            }
            | Self::OpaqueEquation {
                equation_id, name, ..
            } => {
                equation_id.as_str() == artifact_id
                    || strip_trailing_numeric_segment(artifact_id)
                        .is_some_and(|normalized| normalized == equation_id.as_str())
                    || local_name(name) == artifact_local
            }
            Self::RewriteRule { rule_id, name, .. } => {
                rule_id.as_str() == artifact_id
                    || strip_trailing_numeric_segment(artifact_id)
                        .is_some_and(|normalized| normalized == rule_id.as_str())
                    || local_name(name) == artifact_local
            }
        }
    }
}

fn strip_trailing_numeric_segment(raw: &str) -> Option<&str> {
    let (prefix, suffix) = raw.rsplit_once(':')?;
    if !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) {
        Some(prefix)
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheorySubjectKindIr {
    Theory,
    Relation,
    Role,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TheorySubjectRefIr {
    Theory {
        theory_id: TheoryId,
    },
    Relation {
        relation_id: RelationId,
        relation_name: String,
    },
    Role {
        relation_id: RelationId,
        relation_name: String,
        role_id: RoleId,
        role_name: String,
    },
}

impl TheorySubjectRefIr {
    pub fn subject_kind(&self) -> TheorySubjectKindIr {
        match self {
            Self::Theory { .. } => TheorySubjectKindIr::Theory,
            Self::Relation { .. } => TheorySubjectKindIr::Relation,
            Self::Role { .. } => TheorySubjectKindIr::Role,
        }
    }

    pub fn stable_id(&self) -> String {
        match self {
            Self::Theory { theory_id } => theory_id.to_string(),
            Self::Relation { relation_id, .. } => relation_id.to_string(),
            Self::Role { role_id, .. } => role_id.to_string(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Theory { theory_id } => theory_id.to_string(),
            Self::Relation { relation_name, .. } => relation_name.clone(),
            Self::Role {
                relation_name,
                role_name,
                ..
            } => format!("{relation_name}.{role_name}"),
        }
    }

    pub fn matches_artifact_id(&self, artifact_id: &str) -> bool {
        let artifact_local = local_name(artifact_id);
        match self {
            Self::Theory { theory_id } => {
                theory_id.as_str() == artifact_id
                    || local_name(theory_id.as_str()) == artifact_local
            }
            Self::Relation {
                relation_id,
                relation_name,
            } => relation_id.as_str() == artifact_id || local_name(relation_name) == artifact_local,
            Self::Role {
                role_id,
                relation_name,
                role_name,
                ..
            } => {
                role_id.as_str() == artifact_id
                    || format!("{relation_name}.{role_name}") == artifact_id
                    || local_name(role_name) == artifact_local
            }
        }
    }
}

/// Rust-side operational classification for a compiled theory obligation.
///
/// This is intentionally outside the trusted-kernel/certificate boundary: it
/// only reports whether the current runtime checker lowers an obligation into
/// its explicit fragment, not whether Lean has certified the obligation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryObligationFragmentStatusV1 {
    RuntimeChecked,
    OpaqueOrOutOfFragment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTheoryObligationTrustClassV1 {
    RuntimeEnforced,
    RuntimeAdvisory,
    ReviewOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryObligationStatusV1 {
    pub obligation_ref: TheoryObligationRefIr,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    pub fragment_status: RuntimeTheoryObligationFragmentStatusV1,
    pub trust_class: RuntimeTheoryObligationTrustClassV1,
    pub detail: String,
}

/// Summary of which compiled theory obligations lower into the current Rust
/// runtime theory fragment.
///
/// This is an operational runtime artifact for reporting/refinement only. It is
/// not a certificate, does not extend the trusted kernel, and does not claim
/// completeness or ontology closure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTheoryFragmentSummaryV1 {
    pub version: String,
    pub theory_ref: TheorySubjectRefIr,
    #[serde(default)]
    pub total_obligations: usize,
    #[serde(default)]
    pub runtime_checked_obligations: usize,
    #[serde(default)]
    pub opaque_or_out_of_fragment_obligations: usize,
    #[serde(default)]
    pub obligation_statuses: Vec<RuntimeTheoryObligationStatusV1>,
    pub trust_boundary: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn local_name(raw: &str) -> &str {
    raw.rsplit('.').next().unwrap_or(raw)
}

impl RelationSemanticsIr {
    pub fn role(&self, name: &str) -> Option<&RoleIr> {
        self.roles.iter().find(|role| role.name == name)
    }

    pub fn carrier_roles(&self) -> Option<(&RoleIr, &RoleIr)> {
        let carrier = self.carrier.as_ref()?;
        let src = self.roles.get(carrier.source_role as usize)?;
        let dst = self.roles.get(carrier.target_role as usize)?;
        Some((src, dst))
    }

    pub fn carrier_field_names(&self) -> Option<(&str, &str)> {
        let (src, dst) = self.carrier_roles()?;
        Some((src.name.as_str(), dst.name.as_str()))
    }

    pub fn morphism_field_names(&self) -> Option<(&str, &str)> {
        match self.witness_view {
            WitnessViewIr::Morphism { from_role, to_role } => {
                let from = self.roles.get(from_role as usize)?;
                let to = self.roles.get(to_role as usize)?;
                Some((from.name.as_str(), to.name.as_str()))
            }
            _ => None,
        }
    }

    pub fn homotopy_field_names(&self) -> Option<(&str, &str)> {
        match self.witness_view {
            WitnessViewIr::Homotopy { lhs_role, rhs_role } => {
                let lhs = self.roles.get(lhs_role as usize)?;
                let rhs = self.roles.get(rhs_role as usize)?;
                Some((lhs.name.as_str(), rhs.name.as_str()))
            }
            _ => None,
        }
    }
}

impl CompiledSchemaIr {
    pub fn relation(&self, name: &str) -> Option<&RelationSemanticsIr> {
        self.relations.get(name)
    }

    pub fn object_type_id(&self, name: &str) -> Option<&ObjectTypeId> {
        self.object_type_ids.get(name)
    }

    pub fn role_interface(&self, relation_name: &str, role_name: &str) -> Option<&RoleInterfaceIr> {
        self.role_interfaces
            .get(&scoped_role_name(relation_name, role_name))
    }

    pub fn has_object_type(&self, name: &str) -> bool {
        self.object_types.contains(name)
    }

    pub fn type_matches_or_subtypes(&self, actual: &str, expected: &str) -> bool {
        actual == expected
            || self
                .supertypes_of
                .get(actual)
                .is_some_and(|supers| supers.contains(expected))
    }

    pub fn admissible_player_types(&self, expected: &str) -> Vec<String> {
        let mut players = self
            .subtypes_of
            .get(expected)
            .cloned()
            .unwrap_or_else(|| HashSet::from([expected.to_string()]))
            .into_iter()
            .collect::<Vec<_>>();
        players.sort();
        players
    }

    pub fn direct_supertypes_of(&self, sub: &str) -> Vec<String> {
        let Some(all_supertypes) = self.supertypes_of.get(sub) else {
            return Vec::new();
        };
        let mut direct = all_supertypes
            .iter()
            .filter(|candidate| candidate.as_str() != sub)
            .filter(|candidate| {
                !all_supertypes.iter().any(|mid| {
                    mid != *candidate
                        && mid.as_str() != sub
                        && self.type_matches_or_subtypes(mid, candidate)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        direct.sort();
        direct
    }

    pub fn direct_subtypes_of(&self, supertype: &str) -> Vec<String> {
        let mut direct = self
            .object_types
            .iter()
            .filter(|candidate| self.is_direct_subtype(candidate, supertype))
            .cloned()
            .collect::<Vec<_>>();
        direct.sort();
        direct
    }

    pub fn is_direct_subtype(&self, sub: &str, supertype: &str) -> bool {
        self.direct_supertypes_of(sub)
            .iter()
            .any(|candidate| candidate == supertype)
    }

    pub fn direct_subtype_families(&self) -> Vec<DirectSubtypeFamilyIr> {
        let mut supertypes = self.object_types.iter().cloned().collect::<Vec<_>>();
        supertypes.sort();
        supertypes
            .into_iter()
            .filter_map(|supertype| {
                let subtypes = self.direct_subtypes_of(&supertype);
                if subtypes.is_empty() {
                    None
                } else {
                    Some(DirectSubtypeFamilyIr {
                        supertype,
                        subtypes,
                    })
                }
            })
            .collect()
    }

    pub fn subtype_role_projection(
        &self,
        _supertype: &str,
        subtypes: &[String],
    ) -> SubtypeRoleProjectionIr {
        let subtype_set = subtypes.iter().map(String::as_str).collect::<HashSet<_>>();
        let mut relations = BTreeSet::new();
        let mut fields = BTreeSet::new();

        for relation in self.relations.values() {
            let mut relation_touches_family = false;
            for role in &relation.roles {
                if subtype_set.contains(role.target_type.as_str()) {
                    relation_touches_family = true;
                    fields.insert(format!("{}.{}", relation.name, role.name));
                }
            }
            if relation_touches_family {
                relations.insert(relation.name.clone());
            }
        }

        SubtypeRoleProjectionIr {
            relations: relations.into_iter().collect(),
            fields: fields.into_iter().collect(),
        }
    }
}

impl TheoryIr {
    pub fn obligation_refs(&self) -> Vec<TheoryObligationRefIr> {
        let constraints =
            self.constraints
                .iter()
                .map(|constraint| TheoryObligationRefIr::Constraint {
                    theory_id: self.theory_id.clone(),
                    constraint_id: constraint.constraint_id.clone(),
                    relation_name: constraint.relation_name.clone(),
                    summary: constraint.summary.clone(),
                });
        let path_equations =
            self.path_equations
                .iter()
                .map(|equation| TheoryObligationRefIr::PathEquation {
                    theory_id: self.theory_id.clone(),
                    equation_id: equation.equation_id.clone(),
                    name: equation.name.clone(),
                });
        let opaque_equations =
            self.opaque_equations
                .iter()
                .map(|equation| TheoryObligationRefIr::OpaqueEquation {
                    theory_id: self.theory_id.clone(),
                    equation_id: equation.equation_id.clone(),
                    name: equation.name.clone(),
                });
        let rewrite_rules =
            self.rewrite_rules
                .iter()
                .map(|rule| TheoryObligationRefIr::RewriteRule {
                    theory_id: self.theory_id.clone(),
                    rule_id: rule.rule_id.clone(),
                    name: rule.name.clone(),
                });
        constraints
            .chain(path_equations)
            .chain(opaque_equations)
            .chain(rewrite_rules)
            .collect()
    }

    pub fn subject_refs(&self) -> Vec<TheorySubjectRefIr> {
        let mut subjects = std::collections::BTreeSet::new();
        for obligation in self.obligation_refs() {
            for subject in self.subject_refs_for_obligation(&obligation) {
                subjects.insert(subject);
            }
        }
        subjects.into_iter().collect()
    }

    pub fn subject_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheorySubjectRefIr> {
        let mut subjects = std::collections::BTreeSet::new();
        subjects.insert(TheorySubjectRefIr::Theory {
            theory_id: self.theory_id.clone(),
        });
        match obligation {
            TheoryObligationRefIr::Constraint { constraint_id, .. } => {
                if let Some(constraint) = self
                    .constraints
                    .iter()
                    .find(|candidate| &candidate.constraint_id == constraint_id)
                {
                    if let (Some(relation_id), Some(relation_name)) =
                        (&constraint.relation_id, &constraint.relation_name)
                    {
                        subjects.insert(TheorySubjectRefIr::Relation {
                            relation_id: relation_id.clone(),
                            relation_name: relation_name.clone(),
                        });
                        for (role_id, role_name) in constraint
                            .field_role_ids
                            .iter()
                            .zip(constraint.field_refs.iter())
                            .chain(
                                constraint
                                    .param_role_ids
                                    .iter()
                                    .zip(constraint.param_fields.iter()),
                            )
                        {
                            subjects.insert(TheorySubjectRefIr::Role {
                                relation_id: relation_id.clone(),
                                relation_name: relation_name.clone(),
                                role_id: role_id.clone(),
                                role_name: role_name.clone(),
                            });
                        }
                    }
                }
            }
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    for (relation_id, relation_name) in equation
                        .relation_ids
                        .iter()
                        .zip(equation.relation_refs.iter())
                    {
                        subjects.insert(TheorySubjectRefIr::Relation {
                            relation_id: relation_id.clone(),
                            relation_name: relation_name.clone(),
                        });
                    }
                }
            }
            TheoryObligationRefIr::OpaqueEquation { .. } => {}
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    for (relation_id, relation_name) in
                        rule.relation_ids.iter().zip(rule.relation_refs.iter())
                    {
                        subjects.insert(TheorySubjectRefIr::Relation {
                            relation_id: relation_id.clone(),
                            relation_name: relation_name.clone(),
                        });
                    }
                }
            }
        }
        subjects.into_iter().collect()
    }

    pub fn obligation_refs_for_subject(
        &self,
        subject: &TheorySubjectRefIr,
    ) -> Vec<TheoryObligationRefIr> {
        self.obligation_refs()
            .into_iter()
            .filter(|obligation| {
                self.subject_refs_for_obligation(obligation)
                    .contains(subject)
            })
            .collect()
    }

    pub fn runtime_fragment_summary(&self) -> RuntimeTheoryFragmentSummaryV1 {
        let theory_ref = TheorySubjectRefIr::Theory {
            theory_id: self.theory_id.clone(),
        };
        let obligation_statuses = self
            .obligation_refs()
            .into_iter()
            .map(|obligation_ref| self.runtime_fragment_status_for_obligation(obligation_ref))
            .collect::<Vec<_>>();
        let runtime_checked_obligations = obligation_statuses
            .iter()
            .filter(|status| {
                status.fragment_status == RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked
            })
            .count();
        let opaque_or_out_of_fragment_obligations = obligation_statuses
            .iter()
            .filter(|status| {
                status.fragment_status
                    == RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment
            })
            .count();

        let mut notes = vec![
            "this summary is a Rust runtime artifact outside the trusted-kernel and certificate boundary"
                .to_string(),
            "runtime_checked means the obligation lowered into the current Rust-side theory fragment and passed its local structural/type checks; it does not by itself mean runtime-enforced, certificate-backed, Lean-checked, complete, or ontology-closed"
                .to_string(),
        ];
        if opaque_or_out_of_fragment_obligations > 0 {
            notes.push(format!(
                "{} obligation(s) remain opaque or outside the current runtime theory fragment",
                opaque_or_out_of_fragment_obligations
            ));
        }

        RuntimeTheoryFragmentSummaryV1 {
            version: RUNTIME_THEORY_FRAGMENT_SUMMARY_VERSION_V1.to_string(),
            theory_ref,
            total_obligations: obligation_statuses.len(),
            runtime_checked_obligations,
            opaque_or_out_of_fragment_obligations,
            obligation_statuses,
            trust_boundary: "outside_trusted_kernel".to_string(),
            completeness_claim: "not_claimed".to_string(),
            ontology_closure_claim: "not_claimed".to_string(),
            notes,
        }
    }

    fn runtime_fragment_status_for_obligation(
        &self,
        obligation_ref: TheoryObligationRefIr,
    ) -> RuntimeTheoryObligationStatusV1 {
        let subject_refs = self.subject_refs_for_obligation(&obligation_ref);
        let (fragment_status, trust_class, detail) = match &obligation_ref {
            TheoryObligationRefIr::Constraint { constraint_id, .. } => self
                .constraints
                .iter()
                .find(|candidate| &candidate.constraint_id == constraint_id)
                .map(runtime_fragment_status_for_constraint)
                .unwrap_or_else(|| {
                    (
                        RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
                        RuntimeTheoryObligationTrustClassV1::ReviewOnly,
                        "constraint is indexed by stable id, but the compiled runtime record is missing"
                            .to_string(),
                    )
                }),
            TheoryObligationRefIr::PathEquation { .. } => (
                RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked,
                RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory,
                "equation lowered into the current parsed path fragment with runtime endpoint checking"
                    .to_string(),
            ),
            TheoryObligationRefIr::OpaqueEquation { .. } => (
                RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
                RuntimeTheoryObligationTrustClassV1::ReviewOnly,
                "equation text is preserved, but it did not lower into the current runtime path fragment"
                    .to_string(),
            ),
            TheoryObligationRefIr::RewriteRule { .. } => (
                RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked,
                RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory,
                "rewrite rule lowered into the current typed path fragment with runtime endpoint checking"
                    .to_string(),
            ),
        };

        RuntimeTheoryObligationStatusV1 {
            label: obligation_ref.display_name(),
            obligation_ref,
            subject_refs,
            fragment_status,
            trust_class,
            detail,
        }
    }
}

fn runtime_fragment_status_for_constraint(
    constraint: &ConstraintIr,
) -> (
    RuntimeTheoryObligationFragmentStatusV1,
    RuntimeTheoryObligationTrustClassV1,
    String,
) {
    match constraint.kind.as_str() {
        "typing" => (
            RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
            RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory,
            "typing constraint stays indexed by stable refs, but its rule body remains opaque runtime metadata"
                .to_string(),
        ),
        "named_block" => (
            RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
            RuntimeTheoryObligationTrustClassV1::ReviewOnly,
            "named-block constraint is preserved for review, but its body remains outside the current runtime theory fragment"
                .to_string(),
        ),
        "unknown" => (
            RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
            RuntimeTheoryObligationTrustClassV1::ReviewOnly,
            "unknown constraint text is preserved, but it remains outside the current runtime theory fragment"
                .to_string(),
        ),
        "functional" | "at_most" | "key" => (
            RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked,
            RuntimeTheoryObligationTrustClassV1::RuntimeEnforced,
            format!(
                "structured `{}` constraint lowered into the current runtime theory fragment with resolved stable refs",
                constraint.kind
            ),
        ),
        "symmetric_where_in" | "symmetric" | "transitive" => (
            RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked,
            RuntimeTheoryObligationTrustClassV1::ReviewOnly,
            format!(
                "structured `{}` constraint lowered into the current runtime theory fragment, but remains review-only in the current runtime trust model",
                constraint.kind
            ),
        ),
        other => (
            RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment,
            RuntimeTheoryObligationTrustClassV1::ReviewOnly,
            format!(
                "constraint kind `{other}` is indexed, but it is not classified inside the current runtime theory fragment"
            ),
        ),
    }
}

pub fn compile_kernel_module_ir(
    module: &SchemaV1Module,
    axi_text: &str,
) -> Result<KernelModuleIr, String> {
    let schemas = module
        .schemas
        .iter()
        .map(compile_schema_ir)
        .collect::<Vec<_>>();

    let schema_by_name = schemas
        .iter()
        .map(|schema| (schema.schema_id.as_str().to_string(), schema))
        .collect::<HashMap<_, _>>();

    let theories = module
        .theories
        .iter()
        .map(|theory| {
            let compiled_schema = schema_by_name.get(&theory.schema).ok_or_else(|| {
                format!(
                    "theory `{}` references unknown schema `{}` in module `{}`",
                    theory.name, theory.schema, module.module_name
                )
            })?;
            compile_theory_ir(compiled_schema, theory)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let instances = module
        .instances
        .iter()
        .map(|instance| {
            let compiled_schema = schema_by_name.get(&instance.schema).ok_or_else(|| {
                format!(
                    "instance `{}` references unknown schema `{}` in module `{}`",
                    instance.name, instance.schema, module.module_name
                )
            })?;
            let schema_ast = module
                .schemas
                .iter()
                .find(|schema| schema.name == instance.schema)
                .ok_or_else(|| {
                    format!(
                        "instance `{}` schema `{}` missing from module `{}`",
                        instance.name, instance.schema, module.module_name
                    )
                })?;
            compile_instance_ir(&module.module_name, compiled_schema, schema_ast, instance)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(KernelModuleIr {
        module_digest: AxiDigest::from_axi_text(axi_text),
        schemas,
        theories,
        instances,
    })
}

pub fn compile_instance_ir(
    module_name: &str,
    compiled_schema: &CompiledSchemaIr,
    schema: &SchemaV1Schema,
    instance: &SchemaV1Instance,
) -> Result<InstanceIr, String> {
    let assignment_by_name = instance
        .assignments
        .iter()
        .map(|assignment| (assignment.name.as_str(), &assignment.value))
        .collect::<HashMap<_, _>>();

    let mut object_members = Vec::new();
    for object_name in &schema.objects {
        let Some(value) = assignment_by_name.get(object_name.as_str()) else {
            continue;
        };
        let object_type_id = compiled_schema
            .object_type_id(object_name)
            .cloned()
            .ok_or_else(|| format!("compiled schema missing object type `{object_name}`"))?;
        let members = value
            .items
            .iter()
            .filter_map(|item| match item {
                SetItemV1::Ident { name } => Some(name.clone()),
                SetItemV1::Tuple { .. } => None,
            })
            .collect::<Vec<_>>();
        object_members.push(ObjectMembershipIr {
            object_type_id,
            object_type_name: object_name.clone(),
            members,
        });
    }

    let mut relation_facts = Vec::new();
    for relation in &schema.relations {
        let Some(value) = assignment_by_name.get(relation.name.as_str()) else {
            continue;
        };
        let relation_ir = compiled_schema
            .relation(&relation.name)
            .ok_or_else(|| format!("compiled schema missing relation `{}`", relation.name))?;
        let role_lookup = relation_ir
            .roles
            .iter()
            .map(|role| (role.name.as_str(), role))
            .collect::<HashMap<_, _>>();

        for item in &value.items {
            let SetItemV1::Tuple { fields } = item else {
                continue;
            };
            let mut role_values = Vec::new();
            let mut fields_in_decl_order = Vec::new();
            for field in &relation.fields {
                let value = fields
                    .iter()
                    .find(|(name, _)| name == &field.field)
                    .map(|(_, value)| value.clone())
                    .ok_or_else(|| {
                        format!(
                            "instance `{}` relation `{}` tuple missing field `{}`",
                            instance.name, relation.name, field.field
                        )
                    })?;
                let role = role_lookup.get(field.field.as_str()).ok_or_else(|| {
                    format!(
                        "compiled relation `{}` missing role `{}`",
                        relation.name, field.field
                    )
                })?;
                role_values.push(RoleValueIr {
                    role_id: role.role_id.clone(),
                    role_name: role.name.clone(),
                    value: value.clone(),
                });
                fields_in_decl_order.push((field.field.clone(), value));
            }

            let fact_fields = fields_in_decl_order
                .iter()
                .map(|(field, value)| (field.as_str(), value.as_str()))
                .collect::<Vec<_>>();

            relation_facts.push(RelationFactIr {
                fact_id: StableFactId::new(axi_fact_id_v1(
                    module_name,
                    &schema.name,
                    &instance.name,
                    &relation.name,
                    &fact_fields,
                )),
                relation_id: relation_ir.relation_id.clone(),
                relation_name: relation.name.clone(),
                role_values,
            });
        }
    }

    Ok(InstanceIr {
        instance_id: InstanceId::new(format!(
            "instance:{}:{}",
            compiled_schema.schema_id.as_str(),
            instance.name
        )),
        schema_id: compiled_schema.schema_id.clone(),
        object_members,
        relation_facts,
    })
}

pub fn compile_schema_ir(schema: &SchemaV1Schema) -> CompiledSchemaIr {
    let object_types = collect_object_types(schema);
    let object_type_ids = object_types
        .iter()
        .map(|object_type| {
            (
                object_type.clone(),
                ObjectTypeId::new(format!("object:{}:{}", schema.name, object_type)),
            )
        })
        .collect::<HashMap<_, _>>();
    let supertypes_of = compute_supertypes_closure(schema, &object_types);
    let subtypes_of = compute_subtypes_closure(&object_types, &supertypes_of);
    let relations = schema
        .relations
        .iter()
        .map(|rel| {
            let tuple_type_name = if object_types.contains(&rel.name) {
                format!("{}Fact", rel.name)
            } else {
                rel.name.clone()
            };
            let roles = rel
                .fields
                .iter()
                .enumerate()
                .map(|(idx, field)| RoleIr {
                    role_id: RoleId::new(format!(
                        "role:{}:{}:{}",
                        schema.name, rel.name, field.field
                    )),
                    name: field.field.clone(),
                    target_type: field.ty.clone(),
                    order: idx as u16,
                    kind: classify_role(
                        field.field.as_str(),
                        field.ty.as_str(),
                        is_context_axis_type(&supertypes_of, field.ty.as_str()),
                        is_temporal_axis_type(&supertypes_of, field.ty.as_str()),
                    ),
                })
                .collect::<Vec<_>>();
            (
                rel.name.clone(),
                compile_relation_semantics(&schema.name, &rel.name, tuple_type_name, roles),
            )
        })
        .collect();
    let role_interfaces = compile_role_interfaces(&relations, &subtypes_of);

    CompiledSchemaIr {
        schema_id: SchemaId::new(schema.name.clone()),
        object_types,
        object_type_ids,
        supertypes_of,
        subtypes_of,
        relations,
        role_interfaces,
    }
}

pub fn compile_relation_semantics(
    schema_name: &str,
    relation_name: &str,
    tuple_type_name: String,
    roles: Vec<RoleIr>,
) -> RelationSemanticsIr {
    let carrier = derive_carrier_spec(relation_name, &roles);
    let witness_view = match carrier.as_ref().map(|c| c.source) {
        Some(CarrierSource::HomotopyConvention) => carrier
            .as_ref()
            .map(|c| WitnessViewIr::Homotopy {
                lhs_role: c.source_role,
                rhs_role: c.target_role,
            })
            .unwrap_or(WitnessViewIr::None),
        Some(CarrierSource::EndpointConvention | CarrierSource::DeclaredOrder) => carrier
            .as_ref()
            .map(|c| WitnessViewIr::Morphism {
                from_role: c.source_role,
                to_role: c.target_role,
            })
            .unwrap_or(WitnessViewIr::None),
        None => WitnessViewIr::None,
    };

    RelationSemanticsIr {
        relation_id: RelationId::new(format!("relation:{}:{}", schema_name, relation_name)),
        name: relation_name.to_string(),
        tuple_type_name,
        roles,
        carrier,
        witness_view,
    }
}

pub fn compile_theory_ir(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
) -> Result<TheoryIr, String> {
    let theory_id = TheoryId::new(format!(
        "theory:{}:{}",
        compiled_schema.schema_id.as_str(),
        theory.name
    ));

    let constraints = theory
        .constraints
        .iter()
        .enumerate()
        .map(|(index, constraint)| {
            compile_constraint_ir(compiled_schema, theory, &theory_id, index, constraint)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut equation_names = HashSet::new();
    let mut path_equations = Vec::new();
    let mut opaque_equations = Vec::new();
    for equation in &theory.equations {
        if equation.name.trim().is_empty() {
            return Err(format!(
                "theory `{}` has an equation with an empty name",
                theory.name
            ));
        }
        if !equation_names.insert(equation.name.clone()) {
            return Err(format!(
                "theory `{}` contains duplicate equation name `{}`",
                theory.name, equation.name
            ));
        }
        let equation_id =
            EquationId::new(format!("equation:{}:{}", theory_id.as_str(), equation.name));
        match (
            parse_path_expr_v3(&equation.lhs),
            parse_path_expr_v3(&equation.rhs),
        ) {
            (Ok(lhs), Ok(rhs)) => {
                let mut env = EquationTypingEnv::default();
                let lhs_endpoints = infer_equation_expr_endpoints(compiled_schema, &mut env, &lhs)
                    .map_err(|message| {
                        format!(
                            "theory `{}` equation `{}` lhs ill-typed: {}",
                            theory.name, equation.name, message
                        )
                    })?;
                let rhs_endpoints = infer_equation_expr_endpoints(compiled_schema, &mut env, &rhs)
                    .map_err(|message| {
                        format!(
                            "theory `{}` equation `{}` rhs ill-typed: {}",
                            theory.name, equation.name, message
                        )
                    })?;
                if lhs_endpoints != rhs_endpoints {
                    return Err(format!(
                        "theory `{}` equation `{}` has mismatched path endpoints lhs=Path({},{}) rhs=Path({},{})",
                        theory.name,
                        equation.name,
                        lhs_endpoints.0,
                        lhs_endpoints.1,
                        rhs_endpoints.0,
                        rhs_endpoints.1,
                    ));
                }
                let mut relation_refs = Vec::new();
                collect_path_relations(&lhs, &mut relation_refs);
                collect_path_relations(&rhs, &mut relation_refs);
                relation_refs.sort();
                relation_refs.dedup();
                let relation_ids = relation_refs
                    .iter()
                    .map(|name| {
                        compiled_schema
                            .relation(name)
                            .map(|relation| relation.relation_id.clone())
                            .ok_or_else(|| {
                                format!(
                                    "theory `{}` equation `{}` references unknown relation `{}`",
                                    theory.name, equation.name, name
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                path_equations.push(PathEquationIr {
                    equation_id,
                    name: equation.name.clone(),
                    lhs,
                    rhs,
                    relation_refs,
                    relation_ids,
                });
            }
            _ => opaque_equations.push(OpaqueEquationIr {
                equation_id,
                name: equation.name.clone(),
                lhs: equation.lhs.clone(),
                rhs: equation.rhs.clone(),
            }),
        }
    }

    let mut rewrite_rule_names = HashSet::new();
    let rewrite_rules = theory
        .rewrite_rules
        .iter()
        .map(|rule| {
            compile_rewrite_rule_ir(
                compiled_schema,
                theory,
                &theory_id,
                rule,
                &mut rewrite_rule_names,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TheoryIr {
        theory_id,
        schema_id: compiled_schema.schema_id.clone(),
        constraints,
        path_equations,
        opaque_equations,
        rewrite_rules,
    })
}

fn compile_constraint_ir(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
    theory_id: &TheoryId,
    _index: usize,
    constraint: &ConstraintV1,
) -> Result<ConstraintIr, String> {
    let relation_name = constraint_relation_name(constraint).map(ToOwned::to_owned);
    let relation_id = relation_name
        .as_ref()
        .and_then(|name| compiled_schema.relation(name))
        .map(|relation| relation.relation_id.clone());
    let (field_refs, param_fields) = constraint_field_refs(
        compiled_schema,
        theory,
        constraint,
        relation_name.as_deref(),
    )?;
    let field_role_ids = relation_name
        .as_deref()
        .and_then(|name| compiled_schema.relation(name))
        .map(|relation| role_ids_for_fields(relation, &field_refs))
        .transpose()?
        .unwrap_or_default();
    let param_role_ids = relation_name
        .as_deref()
        .and_then(|name| compiled_schema.relation(name))
        .map(|relation| role_ids_for_fields(relation, &param_fields))
        .transpose()?
        .unwrap_or_default();
    let constraint_id = stable_constraint_id(
        theory_id,
        constraint,
        relation_name.as_deref(),
        &field_refs,
        &param_fields,
    );
    Ok(ConstraintIr {
        constraint_id,
        kind: constraint_kind_name(constraint).to_string(),
        summary: format_constraint_summary(constraint),
        relation_name,
        relation_id,
        field_refs,
        field_role_ids,
        param_fields,
        param_role_ids,
    })
}

fn stable_constraint_id(
    theory_id: &TheoryId,
    constraint: &ConstraintV1,
    relation_name: Option<&str>,
    field_refs: &[String],
    param_fields: &[String],
) -> ConstraintId {
    let digest_input = format!(
        "{}|{}|{}|{}|{}|{}",
        theory_id.as_str(),
        constraint_kind_name(constraint),
        relation_name.unwrap_or("_"),
        field_refs.join(","),
        param_fields.join(","),
        format_constraint_summary(constraint)
    );
    ConstraintId::new(format!(
        "constraint:{}:{}",
        theory_id.as_str(),
        axi_digest_v1(&digest_input)
    ))
}

fn compile_rewrite_rule_ir(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
    theory_id: &TheoryId,
    rule: &RewriteRuleV1,
    rewrite_rule_names: &mut HashSet<String>,
) -> Result<RewriteRuleIr, String> {
    if rule.name.trim().is_empty() {
        return Err(format!(
            "theory `{}` has a rewrite rule with an empty name",
            theory.name
        ));
    }
    if !rewrite_rule_names.insert(rule.name.clone()) {
        return Err(format!(
            "theory `{}` contains duplicate rewrite rule name `{}`",
            theory.name, rule.name
        ));
    }

    let env = rewrite_typing_env(compiled_schema, theory, rule)?;
    let lhs = infer_rewrite_endpoint(compiled_schema, theory, rule, &env, &rule.lhs)?;
    let rhs = infer_rewrite_endpoint(compiled_schema, theory, rule, &env, &rule.rhs)?;
    if lhs.from_var != rhs.from_var || lhs.to_var != rhs.to_var {
        return Err(format!(
            "theory `{}` rewrite `{}` changes path endpoints (lhs=Path({},{}) rhs=Path({},{}))",
            theory.name, rule.name, lhs.from_var, lhs.to_var, rhs.from_var, rhs.to_var
        ));
    }
    let mut relation_refs = Vec::new();
    collect_path_relations(&rule.lhs, &mut relation_refs);
    collect_path_relations(&rule.rhs, &mut relation_refs);
    relation_refs.sort();
    relation_refs.dedup();
    let relation_ids = relation_refs
        .iter()
        .map(|name| {
            compiled_schema
                .relation(name)
                .map(|relation| relation.relation_id.clone())
                .ok_or_else(|| {
                    format!(
                        "theory `{}` rewrite `{}` references unknown relation `{}`",
                        theory.name, rule.name, name
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RewriteRuleIr {
        rule_id: RewriteRuleId::new(format!("rewrite:{}:{}", theory_id.as_str(), rule.name)),
        name: rule.name.clone(),
        orientation: format!("{:?}", rule.orientation).to_ascii_lowercase(),
        vars: rule.vars.iter().map(|var| var.to_string()).collect(),
        lhs: rule.lhs.clone(),
        rhs: rule.rhs.clone(),
        endpoint: lhs,
        relation_refs,
        relation_ids,
        source: RewriteRuleSource::AcceptedAxi,
    })
}

fn constraint_relation_name(constraint: &ConstraintV1) -> Option<&str> {
    match constraint {
        ConstraintV1::Functional { relation, .. }
        | ConstraintV1::AtMost { relation, .. }
        | ConstraintV1::Typing { relation, .. }
        | ConstraintV1::SymmetricWhereIn { relation, .. }
        | ConstraintV1::Symmetric { relation, .. }
        | ConstraintV1::Transitive { relation, .. }
        | ConstraintV1::Key { relation, .. } => Some(relation.as_str()),
        ConstraintV1::NamedBlock { .. } | ConstraintV1::Unknown { .. } => None,
    }
}

fn constraint_kind_name(constraint: &ConstraintV1) -> &'static str {
    match constraint {
        ConstraintV1::Functional { .. } => "functional",
        ConstraintV1::AtMost { .. } => "at_most",
        ConstraintV1::Typing { .. } => "typing",
        ConstraintV1::SymmetricWhereIn { .. } => "symmetric_where_in",
        ConstraintV1::Symmetric { .. } => "symmetric",
        ConstraintV1::Transitive { .. } => "transitive",
        ConstraintV1::Key { .. } => "key",
        ConstraintV1::NamedBlock { .. } => "named_block",
        ConstraintV1::Unknown { .. } => "unknown",
    }
}

fn format_constraint_summary(constraint: &ConstraintV1) -> String {
    match constraint {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => format!("functional {relation}.{src_field} -> {relation}.{dst_field}"),
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            max,
            params,
        } => match params {
            Some(params) if !params.is_empty() => format!(
                "at_most {max} {relation}.{src_field} -> {relation}.{dst_field} param ({})",
                params.join(", ")
            ),
            _ => format!("at_most {max} {relation}.{src_field} -> {relation}.{dst_field}"),
        },
        ConstraintV1::Typing { relation, rule } => format!("typing {relation}: {rule}"),
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            values,
            ..
        } => format!(
            "symmetric {relation} where {relation}.{field} in {{{}}}",
            values.join(", ")
        ),
        ConstraintV1::Symmetric { relation, .. } => format!("symmetric {relation}"),
        ConstraintV1::Transitive { relation, .. } => format!("transitive {relation}"),
        ConstraintV1::Key { relation, fields } => {
            format!("key {relation}({})", fields.join(", "))
        }
        ConstraintV1::NamedBlock { name, .. } => format!("named_block {name}"),
        ConstraintV1::Unknown { text } => format!("unknown {text}"),
    }
}

#[derive(Debug, Clone)]
struct RewriteTypingEnv {
    object_vars: HashMap<String, String>,
    path_vars: HashMap<String, (String, String)>,
}

#[derive(Debug, Default, Clone)]
struct EquationTypingEnv {
    object_vars: HashMap<String, String>,
}

fn rewrite_typing_env(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
    rule: &RewriteRuleV1,
) -> Result<RewriteTypingEnv, String> {
    let mut env = RewriteTypingEnv {
        object_vars: HashMap::new(),
        path_vars: HashMap::new(),
    };
    let mut pending_paths = Vec::new();

    for var in &rule.vars {
        if env.object_vars.contains_key(&var.name) || env.path_vars.contains_key(&var.name) {
            return Err(format!(
                "theory `{}` rewrite `{}` reuses variable name `{}`",
                theory.name, rule.name, var.name
            ));
        }
        match &var.ty {
            RewriteVarTypeV1::Object { ty } => {
                if !compiled_schema.has_object_type(ty) {
                    return Err(format!(
                        "theory `{}` rewrite `{}` references unknown object type `{}` for variable `{}`",
                        theory.name, rule.name, ty, var.name
                    ));
                }
                env.object_vars.insert(var.name.clone(), ty.clone());
            }
            RewriteVarTypeV1::Path { from, to } => {
                pending_paths.push((var.name.clone(), from.clone(), to.clone()));
            }
        }
    }

    for (path_name, from_var, to_var) in pending_paths {
        if !env.object_vars.contains_key(&from_var) {
            return Err(format!(
                "theory `{}` rewrite `{}` path variable `{}` references unknown endpoint `{}`",
                theory.name, rule.name, path_name, from_var
            ));
        }
        if !env.object_vars.contains_key(&to_var) {
            return Err(format!(
                "theory `{}` rewrite `{}` path variable `{}` references unknown endpoint `{}`",
                theory.name, rule.name, path_name, to_var
            ));
        }
        env.path_vars.insert(path_name, (from_var, to_var));
    }

    Ok(env)
}

fn infer_rewrite_endpoint(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
    rule: &RewriteRuleV1,
    env: &RewriteTypingEnv,
    expr: &PathExprV3,
) -> Result<RewriteEndpointIr, String> {
    match expr {
        PathExprV3::Var { name } => {
            let (from_var, to_var) = env.path_vars.get(name).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unbound path variable `{}`",
                    theory.name, rule.name, name
                )
            })?;
            let from_type = env.object_vars.get(&from_var).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unknown endpoint variable `{}`",
                    theory.name, rule.name, from_var
                )
            })?;
            let to_type = env.object_vars.get(&to_var).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unknown endpoint variable `{}`",
                    theory.name, rule.name, to_var
                )
            })?;
            Ok(RewriteEndpointIr {
                from_var,
                to_var,
                from_type,
                to_type,
            })
        }
        PathExprV3::Reflexive { entity } => {
            let entity_type = env.object_vars.get(entity).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unbound object variable `{}`",
                    theory.name, rule.name, entity
                )
            })?;
            Ok(RewriteEndpointIr {
                from_var: entity.clone(),
                to_var: entity.clone(),
                from_type: entity_type.clone(),
                to_type: entity_type,
            })
        }
        PathExprV3::Step { from, rel, to } => {
            let from_type = env.object_vars.get(from).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unbound object variable `{}`",
                    theory.name, rule.name, from
                )
            })?;
            let to_type = env.object_vars.get(to).cloned().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unbound object variable `{}`",
                    theory.name, rule.name, to
                )
            })?;
            let relation = compiled_schema.relation(rel).ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` references unknown relation `{}`",
                    theory.name, rule.name, rel
                )
            })?;
            let (src_role, dst_role) = relation.carrier_roles().ok_or_else(|| {
                format!(
                    "theory `{}` rewrite `{}` uses relation `{}` without compiled carrier semantics",
                    theory.name, rule.name, rel
                )
            })?;
            if !compiled_schema.type_matches_or_subtypes(&from_type, &src_role.target_type) {
                return Err(format!(
                    "theory `{}` rewrite `{}`: `{}` has type `{}`, expected subtype of `{}` for relation `{}` field `{}`",
                    theory.name,
                    rule.name,
                    from,
                    from_type,
                    src_role.target_type,
                    rel,
                    src_role.name
                ));
            }
            if !compiled_schema.type_matches_or_subtypes(&to_type, &dst_role.target_type) {
                return Err(format!(
                    "theory `{}` rewrite `{}`: `{}` has type `{}`, expected subtype of `{}` for relation `{}` field `{}`",
                    theory.name,
                    rule.name,
                    to,
                    to_type,
                    dst_role.target_type,
                    rel,
                    dst_role.name
                ));
            }
            Ok(RewriteEndpointIr {
                from_var: from.clone(),
                to_var: to.clone(),
                from_type,
                to_type,
            })
        }
        PathExprV3::Trans { left, right } => {
            let lhs = infer_rewrite_endpoint(compiled_schema, theory, rule, env, left)?;
            let rhs = infer_rewrite_endpoint(compiled_schema, theory, rule, env, right)?;
            if lhs.to_var != rhs.from_var {
                return Err(format!(
                    "theory `{}` rewrite `{}` cannot compose trans(...) because left ends at `{}` but right starts at `{}`",
                    theory.name, rule.name, lhs.to_var, rhs.from_var
                ));
            }
            Ok(RewriteEndpointIr {
                from_var: lhs.from_var,
                to_var: rhs.to_var,
                from_type: lhs.from_type,
                to_type: rhs.to_type,
            })
        }
        PathExprV3::Inv { path } => {
            let inner = infer_rewrite_endpoint(compiled_schema, theory, rule, env, path)?;
            Ok(RewriteEndpointIr {
                from_var: inner.to_var,
                to_var: inner.from_var,
                from_type: inner.to_type,
                to_type: inner.from_type,
            })
        }
    }
}

fn collect_path_relations(expr: &PathExprV3, out: &mut Vec<String>) {
    match expr {
        PathExprV3::Var { .. } | PathExprV3::Reflexive { .. } => {}
        PathExprV3::Step { rel, .. } => out.push(rel.clone()),
        PathExprV3::Trans { left, right } => {
            collect_path_relations(left, out);
            collect_path_relations(right, out);
        }
        PathExprV3::Inv { path } => collect_path_relations(path, out),
    }
}

fn constraint_field_refs(
    compiled_schema: &CompiledSchemaIr,
    theory: &SchemaV1Theory,
    constraint: &ConstraintV1,
    relation_name: Option<&str>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let Some(relation_name) = relation_name else {
        return Ok((Vec::new(), Vec::new()));
    };
    let relation = compiled_schema.relation(relation_name).ok_or_else(|| {
        format!(
            "theory `{}` references unknown relation `{}` in schema `{}`",
            theory.name, relation_name, compiled_schema.schema_id
        )
    })?;

    match constraint {
        ConstraintV1::Functional {
            src_field,
            dst_field,
            ..
        } => {
            ensure_relation_fields(
                relation,
                &[src_field.clone(), dst_field.clone()],
                &format!(
                    "theory `{}` functional constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?;
            Ok((vec![src_field.clone(), dst_field.clone()], Vec::new()))
        }
        ConstraintV1::AtMost {
            src_field,
            dst_field,
            params,
            ..
        } => {
            ensure_relation_fields(
                relation,
                &[src_field.clone(), dst_field.clone()],
                &format!(
                    "theory `{}` at_most constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?;
            Ok((
                vec![src_field.clone(), dst_field.clone()],
                normalize_relation_fields(
                    relation,
                    params.as_deref().unwrap_or(&[]),
                    &format!(
                        "theory `{}` at_most constraint on relation `{}`",
                        theory.name, relation_name
                    ),
                )?,
            ))
        }
        ConstraintV1::Typing { rule, .. } => {
            if rule.trim().is_empty() {
                return Err(format!(
                    "theory `{}` typing constraint on relation `{}` has an empty rule name",
                    theory.name, relation_name
                ));
            }
            Ok((Vec::new(), Vec::new()))
        }
        ConstraintV1::SymmetricWhereIn {
            field,
            values,
            carriers,
            params,
            ..
        } => {
            if values.is_empty() {
                return Err(format!(
                    "theory `{}` symmetric where-in constraint on relation `{}` must list at least one value",
                    theory.name, relation_name
                ));
            }
            let mut field_refs = vec![field.clone()];
            field_refs = normalize_relation_fields(
                relation,
                &field_refs,
                &format!(
                    "theory `{}` symmetric where-in constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?;
            field_refs.extend(resolved_carrier_fields(
                relation,
                carriers.as_ref(),
                &format!(
                    "theory `{}` symmetric where-in constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?);
            field_refs.dedup();
            Ok((
                field_refs,
                normalize_relation_fields(
                    relation,
                    params.as_deref().unwrap_or(&[]),
                    &format!(
                        "theory `{}` symmetric where-in constraint on relation `{}`",
                        theory.name, relation_name
                    ),
                )?,
            ))
        }
        ConstraintV1::Symmetric {
            carriers, params, ..
        }
        | ConstraintV1::Transitive {
            carriers, params, ..
        } => Ok((
            resolved_carrier_fields(
                relation,
                carriers.as_ref(),
                &format!(
                    "theory `{}` closure constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?,
            normalize_relation_fields(
                relation,
                params.as_deref().unwrap_or(&[]),
                &format!(
                    "theory `{}` closure constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?,
        )),
        ConstraintV1::Key { fields, .. } => Ok((
            normalize_relation_fields(
                relation,
                fields,
                &format!(
                    "theory `{}` key constraint on relation `{}`",
                    theory.name, relation_name
                ),
            )?,
            Vec::new(),
        )),
        ConstraintV1::NamedBlock { .. } | ConstraintV1::Unknown { .. } => {
            Ok((Vec::new(), Vec::new()))
        }
    }
}

fn normalize_relation_fields(
    relation: &RelationSemanticsIr,
    fields: &[String],
    context: &str,
) -> Result<Vec<String>, String> {
    ensure_relation_fields(relation, fields, context)?;
    let mut seen = HashSet::new();
    for field in fields {
        if !seen.insert(field) {
            return Err(format!("{context} repeats field `{field}`"));
        }
    }
    Ok(fields.to_vec())
}

fn role_ids_for_fields(
    relation: &RelationSemanticsIr,
    fields: &[String],
) -> Result<Vec<RoleId>, String> {
    fields
        .iter()
        .map(|field| {
            relation
                .role(field)
                .map(|role| role.role_id.clone())
                .ok_or_else(|| {
                    format!(
                        "relation `{}` is missing compiled role semantics for field `{}`",
                        relation.name, field
                    )
                })
        })
        .collect()
}

fn ensure_relation_fields(
    relation: &RelationSemanticsIr,
    fields: &[String],
    context: &str,
) -> Result<(), String> {
    for field in fields {
        if relation.role(field).is_none() {
            return Err(format!(
                "{context} references unknown field `{}` on relation `{}`",
                field, relation.name
            ));
        }
    }
    Ok(())
}

fn resolved_carrier_fields(
    relation: &RelationSemanticsIr,
    carriers: Option<&CarrierFieldsV1>,
    context: &str,
) -> Result<Vec<String>, String> {
    if let Some(carriers) = carriers {
        let fields = vec![carriers.left_field.clone(), carriers.right_field.clone()];
        if carriers.left_field == carriers.right_field {
            return Err(format!(
                "{context} must name distinct carrier fields on relation `{}`",
                relation.name
            ));
        }
        ensure_relation_fields(relation, &fields, context)?;
        return Ok(fields);
    }
    Ok(relation
        .carrier_field_names()
        .map(|(left, right)| vec![left.to_string(), right.to_string()])
        .unwrap_or_default())
}

fn infer_equation_expr_endpoints(
    compiled_schema: &CompiledSchemaIr,
    env: &mut EquationTypingEnv,
    expr: &PathExprV3,
) -> Result<(String, String), String> {
    match expr {
        PathExprV3::Var { name } => Err(format!(
            "free path variable `{name}` is not yet supported in runtime-checked path equations"
        )),
        PathExprV3::Reflexive { entity } => Ok((entity.clone(), entity.clone())),
        PathExprV3::Step { from, rel, to } => {
            let relation = compiled_schema.relation(rel).ok_or_else(|| {
                format!(
                    "unknown relation `{}` in schema `{}`",
                    rel, compiled_schema.schema_id
                )
            })?;
            let (src_role, dst_role) = relation.carrier_roles().ok_or_else(|| {
                format!(
                    "relation `{}` in schema `{}` does not expose a compiled carrier pair",
                    rel, compiled_schema.schema_id
                )
            })?;
            unify_object_requirement(
                compiled_schema,
                &mut env.object_vars,
                from,
                &src_role.target_type,
            )?;
            unify_object_requirement(
                compiled_schema,
                &mut env.object_vars,
                to,
                &dst_role.target_type,
            )?;
            Ok((from.clone(), to.clone()))
        }
        PathExprV3::Trans { left, right } => {
            let (a, b) = infer_equation_expr_endpoints(compiled_schema, env, left)?;
            let (c, d) = infer_equation_expr_endpoints(compiled_schema, env, right)?;
            if b != c {
                return Err(format!(
                    "cannot compose paths because the left path ends at `{b}` and the right path starts at `{c}`"
                ));
            }
            Ok((a, d))
        }
        PathExprV3::Inv { path } => {
            let (a, b) = infer_equation_expr_endpoints(compiled_schema, env, path)?;
            Ok((b, a))
        }
    }
}

fn unify_object_requirement(
    compiled_schema: &CompiledSchemaIr,
    object_vars: &mut HashMap<String, String>,
    variable: &str,
    expected_type: &str,
) -> Result<(), String> {
    if !compiled_schema.has_object_type(expected_type) {
        return Err(format!(
            "unknown object type `{}` in schema `{}`",
            expected_type, compiled_schema.schema_id
        ));
    }

    match object_vars.get(variable).cloned() {
        None => {
            object_vars.insert(variable.to_string(), expected_type.to_string());
            Ok(())
        }
        Some(existing) if existing == expected_type => Ok(()),
        Some(existing) if compiled_schema.type_matches_or_subtypes(expected_type, &existing) => {
            object_vars.insert(variable.to_string(), expected_type.to_string());
            Ok(())
        }
        Some(existing) if compiled_schema.type_matches_or_subtypes(&existing, expected_type) => {
            Ok(())
        }
        Some(existing) => Err(format!(
            "variable `{}` is required to have incompatible object types `{}` and `{}`",
            variable, existing, expected_type
        )),
    }
}

const HOMOTOPY_ROLE_PAIRS: &[(&str, &str)] = &[
    ("lhs", "rhs"),
    ("route1", "route2"),
    ("path1", "path2"),
    ("rel1", "rel2"),
    ("i1", "i2"),
    ("s1", "s2"),
    ("left", "right"),
];

const ENDPOINT_ROLE_PAIRS: &[(&str, &str)] =
    &[("from", "to"), ("source", "target"), ("src", "dst")];

pub fn classify_role(
    name: &str,
    target_type: &str,
    is_context_type: bool,
    is_temporal_type: bool,
) -> RoleKind {
    match name {
        "ctx" => RoleKind::Context,
        "time" => RoleKind::Temporal,
        _ if is_context_type || target_type == "Context" => RoleKind::Context,
        _ if is_temporal_type || target_type == "Time" => RoleKind::Temporal,
        _ => RoleKind::Data,
    }
}

fn derive_carrier_spec(relation_name: &str, roles: &[RoleIr]) -> Option<CarrierSpecIr> {
    let homotopy_pair = named_pair_index(roles, HOMOTOPY_ROLE_PAIRS);
    let endpoint_pair = named_pair_index(roles, ENDPOINT_ROLE_PAIRS);

    if let Some((source_role, target_role)) = homotopy_pair
        .filter(|_| endpoint_pair.is_some() || relation_name_hints_homotopy(relation_name))
    {
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::HomotopyConvention,
        ));
    }

    if let Some((source_role, target_role)) = endpoint_pair {
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::EndpointConvention,
        ));
    }

    let non_axis_roles = roles
        .iter()
        .filter(|role| role.kind == RoleKind::Data)
        .map(|role| role.order)
        .collect::<Vec<_>>();
    if non_axis_roles.len() == 2 {
        let source_role = non_axis_roles[0];
        let target_role = non_axis_roles[1];
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::DeclaredOrder,
        ));
    }

    None
}

fn build_carrier_spec(
    roles: &[RoleIr],
    source_role: u16,
    target_role: u16,
    source: CarrierSource,
) -> CarrierSpecIr {
    CarrierSpecIr {
        source_role,
        target_role,
        fiber_roles: axis_role_indexes(roles, source_role, target_role),
        source,
    }
}

fn named_pair_index(roles: &[RoleIr], pairs: &[(&str, &str)]) -> Option<(u16, u16)> {
    for (left, right) in pairs {
        let left_idx = roles
            .iter()
            .find(|role| role.name == *left)
            .map(|role| role.order);
        let right_idx = roles
            .iter()
            .find(|role| role.name == *right)
            .map(|role| role.order);
        if let (Some(left_idx), Some(right_idx)) = (left_idx, right_idx) {
            return Some((left_idx, right_idx));
        }
    }
    None
}

fn axis_role_indexes(roles: &[RoleIr], source_role: u16, target_role: u16) -> Vec<u16> {
    roles
        .iter()
        .filter(|role| role.order != source_role && role.order != target_role)
        .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        .map(|role| role.order)
        .collect()
}

fn relation_name_hints_homotopy(relation_name: &str) -> bool {
    let lowered = relation_name.to_ascii_lowercase();
    lowered.contains("equiv") || lowered.contains("homotopy")
}

fn collect_object_types(schema: &SchemaV1Schema) -> HashSet<String> {
    let mut object_types: HashSet<String> = schema.objects.iter().cloned().collect();
    for subtype in &schema.subtypes {
        object_types.insert(subtype.sub.clone());
        object_types.insert(subtype.sup.clone());
    }
    for relation in &schema.relations {
        for field in &relation.fields {
            object_types.insert(field.ty.clone());
        }
    }
    object_types
}

fn compute_supertypes_closure(
    schema: &SchemaV1Schema,
    object_types: &HashSet<String>,
) -> HashMap<String, HashSet<String>> {
    let mut direct_supers: HashMap<String, Vec<String>> = HashMap::new();
    for ty in object_types {
        direct_supers.entry(ty.clone()).or_default();
    }
    for subtype in &schema.subtypes {
        direct_supers
            .entry(subtype.sub.clone())
            .or_default()
            .push(subtype.sup.clone());
        direct_supers.entry(subtype.sup.clone()).or_default();
    }

    let mut out = HashMap::new();
    for ty in object_types {
        let mut supers = HashSet::from([ty.clone()]);
        let mut stack = direct_supers.get(ty).cloned().unwrap_or_default();
        while let Some(next) = stack.pop() {
            if supers.insert(next.clone()) {
                if let Some(more) = direct_supers.get(&next) {
                    stack.extend(more.iter().cloned());
                }
            }
        }
        out.insert(ty.clone(), supers);
    }
    out
}

fn compute_subtypes_closure(
    object_types: &HashSet<String>,
    supertypes_of: &HashMap<String, HashSet<String>>,
) -> HashMap<String, HashSet<String>> {
    let mut out = HashMap::new();
    for expected in object_types {
        let subtypes = object_types
            .iter()
            .filter(|candidate| {
                supertypes_of
                    .get(*candidate)
                    .is_some_and(|supers| supers.contains(expected))
            })
            .cloned()
            .collect::<HashSet<_>>();
        out.insert(expected.clone(), subtypes);
    }
    out
}

fn compile_role_interfaces(
    relations: &HashMap<String, RelationSemanticsIr>,
    subtypes_of: &HashMap<String, HashSet<String>>,
) -> HashMap<String, RoleInterfaceIr> {
    let mut interfaces = relations
        .values()
        .flat_map(|relation| {
            relation.roles.iter().map(|role| {
                let scoped_role_name = scoped_role_name(&relation.name, &role.name);
                let mut admissible_player_types = subtypes_of
                    .get(&role.target_type)
                    .cloned()
                    .unwrap_or_else(|| HashSet::from([role.target_type.clone()]))
                    .into_iter()
                    .collect::<Vec<_>>();
                admissible_player_types.sort();
                (
                    scoped_role_name.clone(),
                    RoleInterfaceIr {
                        role_id: role.role_id.clone(),
                        scoped_role_name,
                        relation_name: relation.name.clone(),
                        role_name: role.name.clone(),
                        declared_target_type: role.target_type.clone(),
                        role_kind: role.kind,
                        admissible_player_types,
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    interfaces.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
    interfaces.into_iter().collect()
}

fn scoped_role_name(relation_name: &str, role_name: &str) -> String {
    format!("{relation_name}:{role_name}")
}

fn is_context_axis_type(supertypes_of: &HashMap<String, HashSet<String>>, ty: &str) -> bool {
    ty == "Context"
        || supertypes_of
            .get(ty)
            .is_some_and(|supers| supers.contains("Context"))
}

fn is_temporal_axis_type(supertypes_of: &HashMap<String, HashSet<String>>, ty: &str) -> bool {
    ty == "Time"
        || supertypes_of
            .get(ty)
            .is_some_and(|supers| supers.contains("Time"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_dsl::schema_v1::{
        ConstraintV1, EquationV1, FieldDeclV1, RelationDeclV1, RewriteOrientationV1, RewriteRuleV1,
        RewriteVarDeclV1, RewriteVarTypeV1, SchemaV1Schema, SchemaV1Theory,
    };

    fn relation(fields: &[(&str, &str)]) -> RelationDeclV1 {
        RelationDeclV1 {
            name: "R".to_string(),
            fields: fields
                .iter()
                .map(|(field, ty)| FieldDeclV1 {
                    field: (*field).to_string(),
                    ty: (*ty).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn two_non_axis_roles_preserve_declared_order() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string(), "Context".to_string()],
            subtypes: Vec::new(),
            relations: vec![RelationDeclV1 {
                name: "Parent".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "parent".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "child".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "ctx".to_string(),
                        ty: "Context".to_string(),
                    },
                ],
            }],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("Parent").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("parent", "child")));
        assert_eq!(rel.morphism_field_names(), Some(("parent", "child")));
    }

    #[test]
    fn equivalence_prefers_path_pair_and_marks_homotopy() {
        let roles = vec![
            RoleIr {
                role_id: RoleId::new("role:test:RouteEquivalence:from"),
                name: "from".to_string(),
                target_type: "World".to_string(),
                order: 0,
                kind: RoleKind::Data,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteEquivalence:to"),
                name: "to".to_string(),
                target_type: "World".to_string(),
                order: 1,
                kind: RoleKind::Data,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteEquivalence:route1"),
                name: "route1".to_string(),
                target_type: "Route".to_string(),
                order: 2,
                kind: RoleKind::Data,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteEquivalence:route2"),
                name: "route2".to_string(),
                target_type: "Route".to_string(),
                order: 3,
                kind: RoleKind::Data,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteEquivalence:ctx"),
                name: "ctx".to_string(),
                target_type: "Context".to_string(),
                order: 4,
                kind: RoleKind::Context,
            },
        ];

        let rel = compile_relation_semantics(
            "TestSchema",
            "RouteEquivalence",
            "RouteEquivalence".to_string(),
            roles,
        );
        assert_eq!(rel.carrier_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.homotopy_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.morphism_field_names(), None);
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![4])
        );
    }

    #[test]
    fn dual_pairs_choose_homotopy_without_name_hint() {
        let roles = vec![
            RoleIr {
                role_id: RoleId::new("role:test:RouteWitness:from"),
                name: "from".to_string(),
                target_type: "World".to_string(),
                order: 0,
                kind: RoleKind::Context,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteWitness:to"),
                name: "to".to_string(),
                target_type: "World".to_string(),
                order: 1,
                kind: RoleKind::Context,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteWitness:route1"),
                name: "route1".to_string(),
                target_type: "Route".to_string(),
                order: 2,
                kind: RoleKind::Data,
            },
            RoleIr {
                role_id: RoleId::new("role:test:RouteWitness:route2"),
                name: "route2".to_string(),
                target_type: "Route".to_string(),
                order: 3,
                kind: RoleKind::Data,
            },
        ];

        let rel = compile_relation_semantics(
            "TestSchema",
            "RouteWitness",
            "RouteWitness".to_string(),
            roles,
        );
        assert_eq!(rel.carrier_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.homotopy_field_names(), Some(("route1", "route2")));
        assert_eq!(
            rel.carrier.as_ref().map(|carrier| carrier.source),
            Some(CarrierSource::HomotopyConvention)
        );
    }

    #[test]
    fn exactly_two_non_axis_roles_drive_carrier_even_with_context_and_time() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "World".to_string(),
                "Context".to_string(),
                "Time".to_string(),
            ],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("from", "World"),
                ("to", "World"),
                ("ctx", "Context"),
                ("time", "Time"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("from", "to")));
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![2, 3])
        );
    }

    #[test]
    fn extra_data_roles_without_explicit_convention_do_not_get_carrier() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "Person".to_string(),
                "World".to_string(),
                "Context".to_string(),
            ],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("parent", "Person"),
                ("child", "Person"),
                ("scope", "World"),
                ("ctx", "Context"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), None);
        assert_eq!(rel.morphism_field_names(), None);
    }

    #[test]
    fn world_typed_scope_field_remains_data() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string(), "World".to_string()],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("parent", "Person"),
                ("child", "Person"),
                ("scope", "World"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), None);
        assert_eq!(rel.morphism_field_names(), None);
    }

    #[test]
    fn subtypes_of_context_and_time_are_treated_as_axes() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "Person".to_string(),
                "Context".to_string(),
                "ScopedContext".to_string(),
                "Time".to_string(),
                "EventTime".to_string(),
            ],
            subtypes: vec![
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "ScopedContext".to_string(),
                    sup: "Context".to_string(),
                    inclusion: None,
                },
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "EventTime".to_string(),
                    sup: "Time".to_string(),
                    inclusion: None,
                },
            ],
            relations: vec![relation(&[
                ("actor", "Person"),
                ("target", "Person"),
                ("scope_ref", "ScopedContext"),
                ("recorded_at", "EventTime"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("actor", "target")));
        assert_eq!(rel.morphism_field_names(), Some(("actor", "target")));
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![2, 3])
        );
    }

    #[test]
    fn tuple_type_name_avoids_collision_with_implicit_field_types() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Schema_".to_string(), "Migration".to_string()],
            subtypes: Vec::new(),
            relations: vec![
                RelationDeclV1 {
                    name: "SchemaEquiv".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "s1".to_string(),
                            ty: "Schema_".to_string(),
                        },
                        FieldDeclV1 {
                            field: "s2".to_string(),
                            ty: "Schema_".to_string(),
                        },
                        FieldDeclV1 {
                            field: "forward".to_string(),
                            ty: "Migration".to_string(),
                        },
                        FieldDeclV1 {
                            field: "backward".to_string(),
                            ty: "Migration".to_string(),
                        },
                    ],
                },
                RelationDeclV1 {
                    name: "EquivCompose".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "lhs".to_string(),
                            ty: "SchemaEquiv".to_string(),
                        },
                        FieldDeclV1 {
                            field: "rhs".to_string(),
                            ty: "SchemaEquiv".to_string(),
                        },
                    ],
                },
            ],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir
            .relation("SchemaEquiv")
            .expect("schema equivalence relation semantics");
        assert_eq!(rel.tuple_type_name, "SchemaEquivFact");
    }

    #[test]
    fn compile_theory_ir_assigns_deterministic_ids_and_classifies_equations() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string()],
            subtypes: Vec::new(),
            relations: vec![RelationDeclV1 {
                name: "Parent".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "from".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "to".to_string(),
                        ty: "Person".to_string(),
                    },
                ],
            }],
        };

        let compiled = compile_schema_ir(&schema);
        let theory = SchemaV1Theory {
            name: "T".to_string(),
            schema: "S".to_string(),
            constraints: vec![ConstraintV1::Functional {
                relation: "Parent".to_string(),
                src_field: "from".to_string(),
                dst_field: "to".to_string(),
            }],
            equations: vec![
                EquationV1 {
                    name: "parent_path".to_string(),
                    lhs: "step(x,Parent,x)".to_string(),
                    rhs: "step(x,Parent,x)".to_string(),
                },
                EquationV1 {
                    name: "opaque_business_rule".to_string(),
                    lhs: "ParentCompose(a,b,c)".to_string(),
                    rhs: "c".to_string(),
                },
            ],
            rewrite_rules: vec![RewriteRuleV1 {
                name: "parent_refl".to_string(),
                orientation: RewriteOrientationV1::Forward,
                vars: vec![RewriteVarDeclV1 {
                    name: "x".to_string(),
                    ty: RewriteVarTypeV1::Object {
                        ty: "Person".to_string(),
                    },
                }],
                lhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Parent".to_string(),
                    to: "x".to_string(),
                },
                rhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Parent".to_string(),
                    to: "x".to_string(),
                },
            }],
        };

        let ir = compile_theory_ir(&compiled, &theory).expect("compile theory ir");
        assert_eq!(ir.theory_id.as_str(), "theory:S:T");
        assert_eq!(ir.constraints.len(), 1);
        assert_eq!(ir.constraints[0].field_refs, vec!["from", "to"]);
        assert!(ir.constraints[0].param_fields.is_empty());
        assert_eq!(
            ir.constraints[0]
                .field_role_ids
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["role:S:Parent:from", "role:S:Parent:to"]
        );
        assert!(ir.constraints[0].param_role_ids.is_empty());
        assert_eq!(
            ir.constraints[0].relation_id.as_ref().map(|id| id.as_str()),
            Some("relation:S:Parent")
        );
        assert_eq!(ir.path_equations.len(), 1);
        assert_eq!(ir.path_equations[0].relation_refs, vec!["Parent"]);
        assert_eq!(
            ir.path_equations[0]
                .relation_ids
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["relation:S:Parent"]
        );
        assert_eq!(ir.opaque_equations.len(), 1);
        assert_eq!(ir.rewrite_rules.len(), 1);
        assert_eq!(
            ir.rewrite_rules[0].rule_id.as_str(),
            "rewrite:theory:S:T:parent_refl"
        );
        assert_eq!(ir.rewrite_rules[0].endpoint.from_var, "x");
        assert_eq!(ir.rewrite_rules[0].endpoint.to_var, "x");
        assert_eq!(ir.rewrite_rules[0].endpoint.from_type, "Person");
        assert_eq!(ir.rewrite_rules[0].endpoint.to_type, "Person");
        assert_eq!(
            ir.rewrite_rules[0]
                .relation_ids
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["relation:S:Parent"]
        );
        let obligation_refs = ir.obligation_refs();
        assert_eq!(obligation_refs.len(), 4);
        let constraint_obligation = obligation_refs
            .iter()
            .find(|obligation| {
                matches!(
                    obligation,
                    TheoryObligationRefIr::Constraint {
                        theory_id,
                        constraint_id,
                        relation_name,
                        ..
                    } if theory_id.as_str() == "theory:S:T"
                        && constraint_id.as_str().starts_with("constraint:theory:S:T:fnv1a64:")
                        && relation_name.as_deref() == Some("Parent")
                )
            })
            .cloned()
            .expect("constraint obligation");
        assert!(obligation_refs.iter().any(|obligation| matches!(
            obligation,
            TheoryObligationRefIr::PathEquation { equation_id, name, .. }
            if equation_id.as_str() == "equation:theory:S:T:parent_path" && name == "parent_path"
        )));
        assert!(obligation_refs.iter().any(|obligation| matches!(
            obligation,
            TheoryObligationRefIr::OpaqueEquation { equation_id, name, .. }
            if equation_id.as_str() == "equation:theory:S:T:opaque_business_rule"
                && name == "opaque_business_rule"
        )));
        let rewrite_obligation = obligation_refs
            .iter()
            .find(|obligation| {
                matches!(
                    obligation,
                    TheoryObligationRefIr::RewriteRule { rule_id, name, .. }
                    if rule_id.as_str() == "rewrite:theory:S:T:parent_refl" && name == "parent_refl"
                )
            })
            .cloned()
            .expect("rewrite obligation");

        let subject_refs = ir.subject_refs();
        assert!(subject_refs.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Theory { theory_id } if theory_id.as_str() == "theory:S:T"
        )));
        assert!(subject_refs.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Relation { relation_id, relation_name }
            if relation_id.as_str() == "relation:S:Parent" && relation_name == "Parent"
        )));
        assert!(subject_refs.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Role { role_id, relation_name, role_name, .. }
            if role_id.as_str() == "role:S:Parent:from"
                && relation_name == "Parent"
                && role_name == "from"
        )));
        assert!(subject_refs.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Role { role_id, relation_name, role_name, .. }
            if role_id.as_str() == "role:S:Parent:to"
                && relation_name == "Parent"
                && role_name == "to"
        )));

        let constraint_subjects = ir.subject_refs_for_obligation(&constraint_obligation);
        assert!(constraint_subjects.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Theory { theory_id } if theory_id.as_str() == "theory:S:T"
        )));
        assert!(constraint_subjects.iter().any(|subject| matches!(
            subject,
            TheorySubjectRefIr::Relation { relation_name, .. } if relation_name == "Parent"
        )));
        assert_eq!(
            constraint_subjects
                .iter()
                .filter(|subject| matches!(subject, TheorySubjectRefIr::Role { .. }))
                .count(),
            2
        );

        let relation_subject = TheorySubjectRefIr::Relation {
            relation_id: RelationId::new("relation:S:Parent"),
            relation_name: "Parent".to_string(),
        };
        let relation_obligations = ir.obligation_refs_for_subject(&relation_subject);
        assert!(relation_obligations
            .iter()
            .any(|obligation| obligation == &constraint_obligation));
        assert!(relation_obligations
            .iter()
            .any(|obligation| obligation == &rewrite_obligation));

        assert!(constraint_obligation.matches_artifact_id("constraint:theory:S:T:0"));
        assert!(obligation_refs.iter().any(|obligation| {
            matches!(obligation, TheoryObligationRefIr::PathEquation { .. })
                && obligation.matches_artifact_id("equation:theory:S:T:parent_path:0")
        }));
        assert!(rewrite_obligation.matches_artifact_id("rewrite:theory:S:T:parent_refl:0"));
    }

    #[test]
    fn runtime_theory_fragment_summary_marks_opaque_runtime_gaps_explicitly() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string()],
            subtypes: Vec::new(),
            relations: vec![RelationDeclV1 {
                name: "Parent".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "from".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "to".to_string(),
                        ty: "Person".to_string(),
                    },
                ],
            }],
        };

        let compiled = compile_schema_ir(&schema);
        let theory = SchemaV1Theory {
            name: "T".to_string(),
            schema: "S".to_string(),
            constraints: vec![
                ConstraintV1::Key {
                    relation: "Parent".to_string(),
                    fields: vec!["from".to_string(), "to".to_string()],
                },
                ConstraintV1::Typing {
                    relation: "Parent".to_string(),
                    rule: "from,to : Person".to_string(),
                },
            ],
            equations: vec![
                EquationV1 {
                    name: "runtime_path".to_string(),
                    lhs: "step(x,Parent,y)".to_string(),
                    rhs: "step(x,Parent,y)".to_string(),
                },
                EquationV1 {
                    name: "opaque_business_rule".to_string(),
                    lhs: "ParentCompose(a,b,c)".to_string(),
                    rhs: "c".to_string(),
                },
            ],
            rewrite_rules: vec![RewriteRuleV1 {
                name: "parent_refl".to_string(),
                orientation: RewriteOrientationV1::Forward,
                vars: vec![
                    RewriteVarDeclV1 {
                        name: "x".to_string(),
                        ty: RewriteVarTypeV1::Object {
                            ty: "Person".to_string(),
                        },
                    },
                    RewriteVarDeclV1 {
                        name: "y".to_string(),
                        ty: RewriteVarTypeV1::Object {
                            ty: "Person".to_string(),
                        },
                    },
                ],
                lhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Parent".to_string(),
                    to: "y".to_string(),
                },
                rhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Parent".to_string(),
                    to: "y".to_string(),
                },
            }],
        };

        let ir = compile_theory_ir(&compiled, &theory).expect("compile theory ir");
        let summary = ir.runtime_fragment_summary();
        assert_eq!(summary.version, RUNTIME_THEORY_FRAGMENT_SUMMARY_VERSION_V1);
        assert_eq!(summary.trust_boundary, "outside_trusted_kernel");
        assert_eq!(summary.completeness_claim, "not_claimed");
        assert_eq!(summary.ontology_closure_claim, "not_claimed");
        assert_eq!(summary.total_obligations, 5);
        assert_eq!(summary.runtime_checked_obligations, 3);
        assert_eq!(summary.opaque_or_out_of_fragment_obligations, 2);
        assert!(matches!(
            summary.theory_ref,
            TheorySubjectRefIr::Theory { ref theory_id } if theory_id.as_str() == "theory:S:T"
        ));
        assert!(summary.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::Constraint { ref summary, .. }
                    if summary == "typing Parent: from,to : Person"
            ) && status.fragment_status
                == RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory
                && status.detail.contains("opaque runtime metadata")
        }));
        assert!(summary.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::OpaqueEquation { ref name, .. }
                    if name == "opaque_business_rule"
            ) && status.fragment_status
                == RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::ReviewOnly
        }));
        assert!(summary.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::Constraint { ref summary, .. }
                    if summary == "key Parent(from, to)"
            ) && status.fragment_status == RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::RuntimeEnforced
        }));
        assert!(summary.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::RewriteRule { ref name, .. }
                    if name == "parent_refl"
            ) && status.fragment_status == RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory
        }));
        assert!(summary
            .notes
            .iter()
            .any(|note| note.contains("outside the trusted-kernel")));
    }

    #[test]
    fn compile_theory_ir_rejects_rewrite_rule_with_unknown_relation() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string()],
            subtypes: Vec::new(),
            relations: vec![RelationDeclV1 {
                name: "Parent".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "from".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "to".to_string(),
                        ty: "Person".to_string(),
                    },
                ],
            }],
        };

        let compiled = compile_schema_ir(&schema);
        let theory = SchemaV1Theory {
            name: "T".to_string(),
            schema: "S".to_string(),
            constraints: Vec::new(),
            equations: Vec::new(),
            rewrite_rules: vec![RewriteRuleV1 {
                name: "bad".to_string(),
                orientation: RewriteOrientationV1::Forward,
                vars: vec![
                    RewriteVarDeclV1 {
                        name: "x".to_string(),
                        ty: RewriteVarTypeV1::Object {
                            ty: "Person".to_string(),
                        },
                    },
                    RewriteVarDeclV1 {
                        name: "y".to_string(),
                        ty: RewriteVarTypeV1::Object {
                            ty: "Person".to_string(),
                        },
                    },
                ],
                lhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Nope".to_string(),
                    to: "y".to_string(),
                },
                rhs: PathExprV3::Step {
                    from: "x".to_string(),
                    rel: "Parent".to_string(),
                    to: "y".to_string(),
                },
            }],
        };

        let err = compile_theory_ir(&compiled, &theory).expect_err("unknown relation should fail");
        assert!(err.contains("unknown relation `Nope`"));
    }

    #[test]
    fn compile_kernel_module_ir_emits_instances_and_stable_fact_ids() {
        let axi_text = r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(child: Person, parent: Person, ctx: Context)

theory T on S:
  constraint key Parent(child, parent, ctx)

instance I of S:
  Person = {Alice, Bob}
  Context = {FamilyTree}
  Parent = {
    (child=Alice, parent=Bob, ctx=FamilyTree)
  }
"#;
        let module = axiograph_dsl::schema_v1::parse_schema_v1(axi_text).expect("parse module");

        let ir = compile_kernel_module_ir(&module, axi_text).expect("compile kernel module ir");
        assert!(ir.module_digest.as_str().starts_with("fnv1a64:"));
        assert_eq!(ir.schemas.len(), 1);
        assert_eq!(ir.theories.len(), 1);
        assert_eq!(ir.instances.len(), 1);

        let instance = &ir.instances[0];
        assert_eq!(instance.instance_id.as_str(), "instance:S:I");
        assert_eq!(instance.schema_id.as_str(), "S");
        assert!(instance.object_members.iter().any(|membership| {
            membership.object_type_name == "Person"
                && membership.members == vec!["Alice".to_string(), "Bob".to_string()]
        }));
        assert!(instance.object_members.iter().any(|membership| {
            membership.object_type_name == "Context"
                && membership.members == vec!["FamilyTree".to_string()]
        }));
        assert_eq!(instance.relation_facts.len(), 1);
        let fact = &instance.relation_facts[0];
        assert_eq!(fact.relation_name, "Parent");
        assert_eq!(fact.relation_id.as_str(), "relation:S:Parent");
        assert!(fact.fact_id.as_str().starts_with("factfnv1a64:"));
        assert_eq!(
            fact.role_values
                .iter()
                .map(|role| (role.role_name.as_str(), role.value.as_str()))
                .collect::<Vec<_>>(),
            vec![("child", "Alice"), ("parent", "Bob"), ("ctx", "FamilyTree")]
        );
    }

    #[test]
    fn role_interfaces_track_scoped_names_and_admissible_players() {
        let schema = SchemaV1Schema {
            name: "Org".to_string(),
            objects: vec![
                "Person".to_string(),
                "Reviewer".to_string(),
                "Request".to_string(),
            ],
            subtypes: vec![axiograph_dsl::schema_v1::SubtypeDeclV1 {
                sub: "Reviewer".to_string(),
                sup: "Person".to_string(),
                inclusion: None,
            }],
            relations: vec![RelationDeclV1 {
                name: "Approval".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "approver".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "request".to_string(),
                        ty: "Request".to_string(),
                    },
                ],
            }],
        };

        let compiled = compile_schema_ir(&schema);
        let approver = compiled
            .role_interface("Approval", "approver")
            .expect("role interface");
        assert_eq!(approver.scoped_role_name, "Approval:approver");
        assert_eq!(approver.declared_target_type, "Person");
        assert_eq!(
            approver.admissible_player_types,
            vec!["Person".to_string(), "Reviewer".to_string()]
        );
    }

    #[test]
    fn direct_subtype_helpers_surface_immediate_families_and_role_projections() {
        let schema = SchemaV1Schema {
            name: "Plant".to_string(),
            objects: vec![
                "PlantAsset".to_string(),
                "Pump".to_string(),
                "Compressor".to_string(),
                "SpecialPump".to_string(),
                "Batch".to_string(),
                "Context".to_string(),
            ],
            subtypes: vec![
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "Pump".to_string(),
                    sup: "PlantAsset".to_string(),
                    inclusion: None,
                },
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "Compressor".to_string(),
                    sup: "PlantAsset".to_string(),
                    inclusion: None,
                },
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "SpecialPump".to_string(),
                    sup: "Pump".to_string(),
                    inclusion: None,
                },
            ],
            relations: vec![
                RelationDeclV1 {
                    name: "Certification".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "asset".to_string(),
                            ty: "Pump".to_string(),
                        },
                        FieldDeclV1 {
                            field: "batch".to_string(),
                            ty: "Batch".to_string(),
                        },
                    ],
                },
                RelationDeclV1 {
                    name: "Maintenance".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "asset".to_string(),
                            ty: "Compressor".to_string(),
                        },
                        FieldDeclV1 {
                            field: "ctx".to_string(),
                            ty: "Context".to_string(),
                        },
                    ],
                },
            ],
        };

        let compiled = compile_schema_ir(&schema);

        assert_eq!(
            compiled.direct_supertypes_of("Pump"),
            vec!["PlantAsset".to_string()]
        );
        assert_eq!(
            compiled.direct_supertypes_of("SpecialPump"),
            vec!["Pump".to_string()]
        );
        assert!(compiled.is_direct_subtype("Pump", "PlantAsset"));
        assert!(!compiled.is_direct_subtype("SpecialPump", "PlantAsset"));
        assert_eq!(
            compiled.direct_subtypes_of("PlantAsset"),
            vec!["Compressor".to_string(), "Pump".to_string()]
        );

        let family = compiled
            .direct_subtype_families()
            .into_iter()
            .find(|family| family.supertype == "PlantAsset")
            .expect("PlantAsset family");
        assert_eq!(
            family.subtypes,
            vec!["Compressor".to_string(), "Pump".to_string()]
        );

        let projection = compiled.subtype_role_projection(&family.supertype, &family.subtypes);
        assert_eq!(
            projection.relations,
            vec!["Certification".to_string(), "Maintenance".to_string()]
        );
        assert_eq!(
            projection.fields,
            vec![
                "Certification.asset".to_string(),
                "Maintenance.asset".to_string()
            ]
        );
    }
}
