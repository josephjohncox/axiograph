//! Minimal compiled schema IR for semantic endpoint selection.
//!
//! This is intentionally narrow: it centralizes relation-role semantics so
//! endpoint choice becomes a compiled schema fact instead of being repeated as
//! local heuristics across import/check paths.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};

use axiograph_dsl::schema_v1::{
    parse_path_expr_v3, CarrierFieldsV1, ConstraintV1, PathExprV3, RewriteRuleV1, RewriteVarTypeV1,
    SchemaV1Instance, SchemaV1Module, SchemaV1Schema, SchemaV1Theory, SetItemV1,
};
use axiograph_kernel::{
    revision_digest_v2, runtime_fact_id_v2, CanonicalCompiler, CanonicalModuleSource,
    CompiledKernelSnapshot, KernelCompilationRequest, KernelRefV2, RepositoryIdV2, ScopeAxisIr,
    SnapshotIdV2,
};

use crate::{
    axi_module_typecheck::{validate_axi_v1_module, Module},
    migration::{MigrationFunctorKindV1, SchemaMorphismV1},
    AxiDigest, ConstraintId, EquationId, InstanceId, ObjectTypeId, RelationId, RewriteRuleId,
    RoleId, SchemaId, StableFactId, TheoryId, Validated,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    Data,
    Context,
    World,
    Temporal,
    Parameter,
    Evidence,
}

impl RoleKind {
    pub fn scope_axis(self) -> Option<ScopeAxisIr> {
        match self {
            Self::Context => Some(ScopeAxisIr::Context),
            Self::World => Some(ScopeAxisIr::World),
            Self::Temporal => Some(ScopeAxisIr::Temporal),
            Self::Data | Self::Parameter | Self::Evidence => None,
        }
    }
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

fn serialize_string_set<S>(value: &HashSet<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.iter().collect::<BTreeSet<_>>().serialize(serializer)
}

fn serialize_string_map<S, V>(value: &HashMap<String, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    V: Serialize,
{
    value
        .iter()
        .collect::<BTreeMap<_, _>>()
        .serialize(serializer)
}

fn serialize_string_set_map<S>(
    value: &HashMap<String, HashSet<String>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value
        .iter()
        .map(|(key, items)| (key, items.iter().collect::<BTreeSet<_>>()))
        .collect::<BTreeMap<_, _>>()
        .serialize(serializer)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSchemaIndex {
    pub schema_id: SchemaId,
    #[serde(serialize_with = "serialize_string_set")]
    pub object_types: HashSet<String>,
    #[serde(serialize_with = "serialize_string_map")]
    pub object_type_ids: HashMap<String, ObjectTypeId>,
    #[serde(serialize_with = "serialize_string_set_map")]
    pub supertypes_of: HashMap<String, HashSet<String>>,
    #[serde(serialize_with = "serialize_string_set_map")]
    pub subtypes_of: HashMap<String, HashSet<String>>,
    #[serde(serialize_with = "serialize_string_map")]
    pub relations: HashMap<String, RelationSemanticsIr>,
    #[serde(serialize_with = "serialize_string_map")]
    pub role_interfaces: HashMap<String, RoleInterfaceIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeModuleIndex {
    pub module_digest: AxiDigest,
    pub schemas: Vec<RuntimeSchemaIndex>,
    pub theories: Vec<TheoryIr>,
    pub instances: Vec<InstanceIr>,
    /// Serialized read-only citations copied from the exact canonical snapshot.
    /// They cannot mint accepted authority after deserialization.
    pub canonical_citations: Vec<CanonicalKernelCitationIr>,
    /// Canonical authority retained by in-process derived consumers. Serialized
    /// runtime indexes intentionally omit this handle and cannot reconstruct it.
    #[serde(skip)]
    canonical_snapshot: Option<CompiledKernelSnapshot>,
}

pub const RUNTIME_SEMANTIC_INDEX_VERSION: &str = "runtime_semantic_index_v3";

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(deny_unknown_fields)]
pub struct CanonicalKernelCitationIr {
    #[schemars(with = "serde_json::Value")]
    pub reference: KernelRefV2,
    pub label: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeIrRef {
    /// Exact typed citation copied from `KernelSnapshotIr::refs`.
    Canonical {
        citation: CanonicalKernelCitationIr,
    },
    /// Runtime-theory diagnostics are execution-plane analysis refs, not
    /// alternate category objects or accepted semantic identities.
    Theory {
        theory_id: TheoryId,
        schema_id: SchemaId,
    },
    TheoryObligation {
        obligation: TheoryObligationRefIr,
    },
    TheorySubject {
        theory_id: TheoryId,
        subject: TheorySubjectRefIr,
    },
    Instance {
        instance_id: InstanceId,
        schema_id: SchemaId,
    },
    StableFact {
        instance_id: InstanceId,
        fact_id: StableFactId,
        relation_id: RelationId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSemanticIndex {
    pub version: String,
    pub module_digest: AxiDigest,
    pub refs: Vec<RuntimeIrRef>,
    pub total_refs: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl RuntimeSemanticIndex {
    pub fn contains_ref(&self, reference: &RuntimeIrRef) -> bool {
        self.refs.iter().any(|candidate| candidate == reference)
    }

    pub fn unresolved_refs(&self, references: &[RuntimeIrRef]) -> Vec<RuntimeIrRef> {
        let declared = self.refs.iter().collect::<BTreeSet<_>>();
        references
            .iter()
            .filter(|reference| !declared.contains(reference))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn validate_refs(&self, references: &[RuntimeIrRef]) -> Result<(), String> {
        let unresolved = self.unresolved_refs(references);
        if unresolved.is_empty() {
            return Ok(());
        }
        Err(format!(
            "strict semantic report references {} undeclared derived runtime IR ref(s): {}",
            unresolved.len(),
            unresolved
                .iter()
                .map(RuntimeIrRef::stable_label)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl RuntimeIrRef {
    pub fn stable_label(&self) -> String {
        match self {
            Self::Canonical { citation } => citation.reference.stable_label(),
            Self::Theory {
                theory_id,
                schema_id,
            } => format!("theory:{schema_id}:{theory_id}"),
            Self::TheoryObligation { obligation } => {
                format!("theory_obligation:{}", obligation.stable_id())
            }
            Self::TheorySubject { theory_id, subject } => {
                format!("theory_subject:{theory_id}:{}", subject.stable_id())
            }
            Self::Instance {
                instance_id,
                schema_id,
            } => format!("instance:{schema_id}:{instance_id}"),
            Self::StableFact {
                instance_id,
                fact_id,
                relation_id,
            } => format!("stable_fact:{instance_id}:{relation_id}:{fact_id}"),
        }
    }
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
pub struct TheoryTouchedRoleIr {
    pub relation_id: RelationId,
    pub relation_name: String,
    pub role_id: RoleId,
    pub role_name: String,
    pub role_kind: RoleKind,
    pub target_type: String,
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
    pub touched_roles: Vec<TheoryTouchedRoleIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpaqueEquationIr {
    pub equation_id: EquationId,
    pub name: String,
    pub lhs: String,
    pub rhs: String,
    /// Canonical citation when the sole `SchemaPresentationIr` accepted this
    /// source as a typed forward generator equation. The runtime parser does
    /// not duplicate that category presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_formation_ref: Option<KernelRefV2>,
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
    pub touched_roles: Vec<TheoryTouchedRoleIr>,
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

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum TheoryObligationKindIr {
    Constraint,
    PathEquation,
    OpaqueEquation,
    RewriteRule,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
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
                constraint_id.as_str() == artifact_id
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
            } => equation_id.as_str() == artifact_id || local_name(name) == artifact_local,
            Self::RewriteRule { rule_id, name, .. } => {
                rule_id.as_str() == artifact_id || local_name(name) == artifact_local
            }
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum TheorySubjectKindIr {
    Theory,
    Relation,
    Role,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
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

pub const THEORY_ADDRESS_INDEX_VERSION_V1: &str = "theory_address_index_v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheoryPathSideIr {
    Lhs,
    Rhs,
}

impl TheoryPathSideIr {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lhs => "lhs",
            Self::Rhs => "rhs",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheoryPathExprKindIr {
    Var,
    Reflexive,
    Step,
    Trans,
    Inv,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheoryVariableKindIr {
    Object,
    Path,
    Endpoint,
}

impl TheoryVariableKindIr {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Path => "path",
            Self::Endpoint => "endpoint",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryPathExpressionRefIr {
    pub expression_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub side: TheoryPathSideIr,
    pub expression_index: u32,
    pub expression_kind: TheoryPathExprKindIr,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryPathStepRefIr {
    pub step_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub side: TheoryPathSideIr,
    pub step_index: u32,
    pub expression_index: u32,
    pub relation_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_id: Option<RelationId>,
    pub from_var: String,
    pub to_var: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryVariableRefIr {
    pub variable_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub variable_name: String,
    pub variable_kind: TheoryVariableKindIr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryEndpointRefIr {
    pub endpoint_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub side: TheoryPathSideIr,
    pub from_var: String,
    pub to_var: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryContextAxisRefIr {
    pub context_axis_id: String,
    pub obligation_ref: TheoryObligationRefIr,
    pub relation_id: RelationId,
    pub relation_name: String,
    pub role_id: RoleId,
    pub role_name: String,
    pub role_kind: RoleKind,
    pub target_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryTransportItemRefIr {
    pub transport_item_id: String,
    pub obligation_ref: TheoryObligationRefIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryObligationDependencyIr {
    pub obligation_ref: TheoryObligationRefIr,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryAddressIndexV1 {
    pub version: String,
    pub theory_ref: TheorySubjectRefIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligation_refs: Vec<TheoryObligationRefIr>,
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
    pub dependencies: Vec<TheoryObligationDependencyIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
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

pub const THEORY_OBLIGATION_GRAPH_VERSION_V1: &str = "theory_obligation_graph_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TheoryObligationGraphNodeKindV1 {
    Theory,
    Obligation,
    Subject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryObligationGraphNodeV1 {
    pub node_id: String,
    pub kind: TheoryObligationGraphNodeKindV1,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_ref: Option<TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_status: Option<RuntimeTheoryObligationFragmentStatusV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_class: Option<RuntimeTheoryObligationTrustClassV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TheoryObligationGraphEdgeKindV1 {
    TheoryContainsObligation,
    ObligationTouchesSubject,
    SubjectSupportsObligation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryObligationGraphEdgeV1 {
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: TheoryObligationGraphEdgeKindV1,
    pub detail: String,
}

/// Deterministic graph of compiled theory obligations and their typed subjects.
///
/// This is the runtime-addressable object that query refinement, CQ repair,
/// migration authoring, and semantic reconciliation should share when they need
/// to point at "the rule/equation/constraint affected by this change". It is a
/// typed operational graph, not a proof of completeness or ontology closure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryObligationGraphV1 {
    pub version: String,
    pub theory_ref: TheorySubjectRefIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<TheoryObligationGraphNodeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<TheoryObligationGraphEdgeV1>,
    #[serde(default)]
    pub total_obligations: usize,
    #[serde(default)]
    pub runtime_checked_obligations: usize,
    #[serde(default)]
    pub opaque_or_out_of_fragment_obligations: usize,
    pub trust_boundary: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub const THEORY_TRANSPORT_PLAN_VERSION_V1: &str = "theory_transport_plan_v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TheoryTransportStatusIr {
    Preserved,
    Transported,
    MissingObjectImage,
    MissingArrowImage,
    OpaqueOrOutOfFragment,
}

impl TheoryTransportStatusIr {
    pub fn requires_resolver(self) -> bool {
        !matches!(self, Self::Preserved)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryTransportItemIr {
    pub operator: MigrationFunctorKindV1,
    pub obligation_ref: TheoryObligationRefIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<TheorySubjectRefIr>,
    pub status: TheoryTransportStatusIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transport_basis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_object_images: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_arrow_images: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TheoryTransportPlanIr {
    pub version: String,
    pub operator: MigrationFunctorKindV1,
    pub source_theory: TheorySubjectRefIr,
    pub source_schema_id: SchemaId,
    pub target_schema: String,
    #[serde(default)]
    pub total_obligations: usize,
    #[serde(default)]
    pub preserved_obligations: usize,
    #[serde(default)]
    pub transported_obligations: usize,
    #[serde(default)]
    pub blocked_obligations: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<TheoryTransportItemIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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

impl RuntimeSchemaIndex {
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
                    for role in &equation.touched_roles {
                        subjects.insert(TheorySubjectRefIr::Role {
                            relation_id: role.relation_id.clone(),
                            relation_name: role.relation_name.clone(),
                            role_id: role.role_id.clone(),
                            role_name: role.role_name.clone(),
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
                    for role in &rule.touched_roles {
                        subjects.insert(TheorySubjectRefIr::Role {
                            relation_id: role.relation_id.clone(),
                            relation_name: role.relation_name.clone(),
                            role_id: role.role_id.clone(),
                            role_name: role.role_name.clone(),
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

    pub fn relation_names_for_obligation(&self, obligation: &TheoryObligationRefIr) -> Vec<String> {
        let mut relation_names = BTreeSet::new();
        match obligation {
            TheoryObligationRefIr::Constraint { constraint_id, .. } => {
                if let Some(constraint) = self
                    .constraints
                    .iter()
                    .find(|candidate| &candidate.constraint_id == constraint_id)
                {
                    if let Some(relation_name) = constraint.relation_name.as_ref() {
                        relation_names.insert(relation_name.clone());
                    }
                }
            }
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    relation_names.extend(equation.relation_refs.iter().cloned());
                }
            }
            TheoryObligationRefIr::OpaqueEquation { .. } => {}
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    relation_names.extend(rule.relation_refs.iter().cloned());
                }
            }
        }
        relation_names.into_iter().collect()
    }

    pub fn address_index(&self) -> TheoryAddressIndexV1 {
        let obligation_refs = self.obligation_refs();
        let dependencies = obligation_refs
            .iter()
            .map(|obligation| self.obligation_dependencies(obligation))
            .collect::<Vec<_>>();

        let path_expression_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.path_expression_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let path_step_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.path_step_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let variable_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.variable_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let endpoint_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.endpoint_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let context_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.context_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let transport_item_refs = dependencies
            .iter()
            .flat_map(|dependency| dependency.transport_item_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        TheoryAddressIndexV1 {
            version: THEORY_ADDRESS_INDEX_VERSION_V1.to_string(),
            theory_ref: TheorySubjectRefIr::Theory {
                theory_id: self.theory_id.clone(),
            },
            obligation_refs,
            subject_refs: self.subject_refs(),
            path_expression_refs,
            path_step_refs,
            variable_refs,
            endpoint_refs,
            context_refs,
            transport_item_refs,
            dependencies,
            notes: vec![
                "theory address index is a deterministic runtime navigation surface, not a proof object".to_string(),
                "transport item refs are stable resolver handles; concrete transport status still comes from TheoryTransportPlanIr".to_string(),
            ],
        }
    }

    pub fn obligation_dependencies(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> TheoryObligationDependencyIr {
        TheoryObligationDependencyIr {
            obligation_ref: obligation.clone(),
            subject_refs: self.subject_refs_for_obligation(obligation),
            path_expression_refs: self.path_expression_refs_for_obligation(obligation),
            path_step_refs: self.path_step_refs_for_obligation(obligation),
            variable_refs: self.variable_refs_for_obligation(obligation),
            endpoint_refs: self.endpoint_refs_for_obligation(obligation),
            context_refs: self.context_refs_for_obligation(obligation),
            transport_item_refs: self.transport_item_refs_for_obligation(obligation),
        }
    }

    pub fn path_expression_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryPathExpressionRefIr> {
        let mut refs = Vec::new();
        match obligation {
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    let mut lhs_index = 0;
                    collect_path_expression_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Lhs,
                        &equation.lhs,
                        &mut lhs_index,
                        &mut refs,
                    );
                    let mut rhs_index = 0;
                    collect_path_expression_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Rhs,
                        &equation.rhs,
                        &mut rhs_index,
                        &mut refs,
                    );
                }
            }
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    let mut lhs_index = 0;
                    collect_path_expression_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Lhs,
                        &rule.lhs,
                        &mut lhs_index,
                        &mut refs,
                    );
                    let mut rhs_index = 0;
                    collect_path_expression_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Rhs,
                        &rule.rhs,
                        &mut rhs_index,
                        &mut refs,
                    );
                }
            }
            TheoryObligationRefIr::Constraint { .. }
            | TheoryObligationRefIr::OpaqueEquation { .. } => {}
        }
        refs.sort();
        refs
    }

    pub fn path_step_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryPathStepRefIr> {
        let relation_ids = self.relation_id_lookup_for_obligation(obligation);
        let mut refs = Vec::new();
        match obligation {
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    let mut lhs_expr_index = 0;
                    let mut lhs_step_index = 0;
                    collect_path_step_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Lhs,
                        &equation.lhs,
                        &relation_ids,
                        &mut lhs_expr_index,
                        &mut lhs_step_index,
                        &mut refs,
                    );
                    let mut rhs_expr_index = 0;
                    let mut rhs_step_index = 0;
                    collect_path_step_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Rhs,
                        &equation.rhs,
                        &relation_ids,
                        &mut rhs_expr_index,
                        &mut rhs_step_index,
                        &mut refs,
                    );
                }
            }
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    let mut lhs_expr_index = 0;
                    let mut lhs_step_index = 0;
                    collect_path_step_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Lhs,
                        &rule.lhs,
                        &relation_ids,
                        &mut lhs_expr_index,
                        &mut lhs_step_index,
                        &mut refs,
                    );
                    let mut rhs_expr_index = 0;
                    let mut rhs_step_index = 0;
                    collect_path_step_refs_for_path(
                        obligation,
                        TheoryPathSideIr::Rhs,
                        &rule.rhs,
                        &relation_ids,
                        &mut rhs_expr_index,
                        &mut rhs_step_index,
                        &mut refs,
                    );
                }
            }
            TheoryObligationRefIr::Constraint { .. }
            | TheoryObligationRefIr::OpaqueEquation { .. } => {}
        }
        refs.sort();
        refs
    }

    pub fn variable_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryVariableRefIr> {
        let mut refs = BTreeSet::new();
        match obligation {
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    collect_path_variable_refs(obligation, &equation.lhs, &mut refs);
                    collect_path_variable_refs(obligation, &equation.rhs, &mut refs);
                }
            }
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    for declaration in &rule.vars {
                        if let Some(variable_ref) =
                            variable_ref_for_rewrite_declaration(obligation, declaration)
                        {
                            refs.insert(variable_ref);
                        }
                    }
                    if refs.is_empty() {
                        collect_path_variable_refs(obligation, &rule.lhs, &mut refs);
                        collect_path_variable_refs(obligation, &rule.rhs, &mut refs);
                    }
                }
            }
            TheoryObligationRefIr::Constraint { .. }
            | TheoryObligationRefIr::OpaqueEquation { .. } => {}
        }
        refs.into_iter().collect()
    }

    pub fn endpoint_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryEndpointRefIr> {
        let mut refs = Vec::new();
        match obligation {
            TheoryObligationRefIr::PathEquation { equation_id, .. } => {
                if let Some(equation) = self
                    .path_equations
                    .iter()
                    .find(|candidate| &candidate.equation_id == equation_id)
                {
                    if let Some((from_var, to_var)) = path_endpoint_vars(&equation.lhs) {
                        refs.push(theory_endpoint_ref(
                            obligation,
                            TheoryPathSideIr::Lhs,
                            from_var,
                            to_var,
                            None,
                            None,
                        ));
                    }
                    if let Some((from_var, to_var)) = path_endpoint_vars(&equation.rhs) {
                        refs.push(theory_endpoint_ref(
                            obligation,
                            TheoryPathSideIr::Rhs,
                            from_var,
                            to_var,
                            None,
                            None,
                        ));
                    }
                }
            }
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => {
                if let Some(rule) = self
                    .rewrite_rules
                    .iter()
                    .find(|candidate| &candidate.rule_id == rule_id)
                {
                    for side in [TheoryPathSideIr::Lhs, TheoryPathSideIr::Rhs] {
                        refs.push(theory_endpoint_ref(
                            obligation,
                            side,
                            rule.endpoint.from_var.clone(),
                            rule.endpoint.to_var.clone(),
                            Some(rule.endpoint.from_type.clone()),
                            Some(rule.endpoint.to_type.clone()),
                        ));
                    }
                }
            }
            TheoryObligationRefIr::Constraint { .. }
            | TheoryObligationRefIr::OpaqueEquation { .. } => {}
        }
        refs.sort();
        refs
    }

    pub fn context_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryContextAxisRefIr> {
        touched_roles_for_obligation(self, obligation)
            .into_iter()
            .filter(|role| matches!(role.role_kind, RoleKind::Context | RoleKind::Temporal))
            .map(|role| theory_context_axis_ref(obligation, role))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn transport_item_refs_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> Vec<TheoryTransportItemRefIr> {
        if self.obligation_refs().contains(obligation) {
            vec![TheoryTransportItemRefIr {
                transport_item_id: format!("transport_item:{}", obligation.stable_id()),
                obligation_ref: obligation.clone(),
            }]
        } else {
            Vec::new()
        }
    }

    pub fn theory_transport_plan(
        &self,
        compiled_schema: &RuntimeSchemaIndex,
        morphism: &SchemaMorphismV1,
        operator: MigrationFunctorKindV1,
    ) -> TheoryTransportPlanIr {
        build_theory_transport_plan_ir(compiled_schema, self, morphism, operator)
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
                "{opaque_or_out_of_fragment_obligations} obligation(s) remain opaque or outside the current runtime theory fragment"
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
            completeness_claim:
                "use RuntimeTheoryCheckReportV1 for scoped runtime completeness claims".to_string(),
            ontology_closure_claim:
                "use RuntimeTheoryCheckReportV1 for scoped runtime closure claims".to_string(),
            notes,
        }
    }

    pub fn obligation_graph(&self) -> TheoryObligationGraphV1 {
        let summary = self.runtime_fragment_summary();
        let theory_ref = TheorySubjectRefIr::Theory {
            theory_id: self.theory_id.clone(),
        };
        let theory_node_id = theory_graph_subject_node_id(&theory_ref);
        let mut nodes = vec![TheoryObligationGraphNodeV1 {
            node_id: theory_node_id.clone(),
            kind: TheoryObligationGraphNodeKindV1::Theory,
            label: self.theory_id.to_string(),
            obligation_ref: None,
            subject_ref: Some(theory_ref.clone()),
            fragment_status: None,
            trust_class: None,
        }];
        let mut edges = Vec::new();
        let mut subject_nodes = BTreeSet::new();

        for status in &summary.obligation_statuses {
            let obligation_node_id = theory_graph_obligation_node_id(&status.obligation_ref);
            nodes.push(TheoryObligationGraphNodeV1 {
                node_id: obligation_node_id.clone(),
                kind: TheoryObligationGraphNodeKindV1::Obligation,
                label: status.label.clone(),
                obligation_ref: Some(status.obligation_ref.clone()),
                subject_ref: None,
                fragment_status: Some(status.fragment_status),
                trust_class: Some(status.trust_class),
            });
            edges.push(TheoryObligationGraphEdgeV1 {
                source_node_id: theory_node_id.clone(),
                target_node_id: obligation_node_id.clone(),
                kind: TheoryObligationGraphEdgeKindV1::TheoryContainsObligation,
                detail: "compiled theory contains this runtime-addressable obligation".to_string(),
            });

            for subject_ref in &status.subject_refs {
                let subject_node_id = theory_graph_subject_node_id(subject_ref);
                if subject_nodes.insert(subject_node_id.clone()) {
                    nodes.push(TheoryObligationGraphNodeV1 {
                        node_id: subject_node_id.clone(),
                        kind: TheoryObligationGraphNodeKindV1::Subject,
                        label: subject_ref.display_name(),
                        obligation_ref: None,
                        subject_ref: Some(subject_ref.clone()),
                        fragment_status: None,
                        trust_class: None,
                    });
                }
                edges.push(TheoryObligationGraphEdgeV1 {
                    source_node_id: obligation_node_id.clone(),
                    target_node_id: subject_node_id.clone(),
                    kind: TheoryObligationGraphEdgeKindV1::ObligationTouchesSubject,
                    detail: "obligation references this typed theory subject".to_string(),
                });
                edges.push(TheoryObligationGraphEdgeV1 {
                    source_node_id: subject_node_id,
                    target_node_id: obligation_node_id.clone(),
                    kind: TheoryObligationGraphEdgeKindV1::SubjectSupportsObligation,
                    detail: "subject can be used to find affected obligations for repair, migration, or reconciliation".to_string(),
                });
            }
        }

        nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        nodes.dedup_by(|a, b| a.node_id == b.node_id);
        edges.sort_by(|a, b| {
            a.source_node_id
                .cmp(&b.source_node_id)
                .then_with(|| a.target_node_id.cmp(&b.target_node_id))
                .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
        });
        edges.dedup_by(|a, b| {
            a.source_node_id == b.source_node_id
                && a.target_node_id == b.target_node_id
                && a.kind == b.kind
        });

        TheoryObligationGraphV1 {
            version: THEORY_OBLIGATION_GRAPH_VERSION_V1.to_string(),
            theory_ref,
            nodes,
            edges,
            total_obligations: summary.total_obligations,
            runtime_checked_obligations: summary.runtime_checked_obligations,
            opaque_or_out_of_fragment_obligations: summary
                .opaque_or_out_of_fragment_obligations,
            trust_boundary: summary.trust_boundary,
            completeness_claim: summary.completeness_claim,
            ontology_closure_claim: summary.ontology_closure_claim,
            notes: vec![
                "theory obligation graph is a deterministic runtime index for typed exploration, CQ repair, migration, and reconciliation"
                    .to_string(),
                "graph edges are operational dependency links, not categorical completeness or Lean proof edges"
                    .to_string(),
            ],
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

    fn relation_id_lookup_for_obligation(
        &self,
        obligation: &TheoryObligationRefIr,
    ) -> BTreeMap<String, RelationId> {
        match obligation {
            TheoryObligationRefIr::PathEquation { equation_id, .. } => self
                .path_equations
                .iter()
                .find(|candidate| &candidate.equation_id == equation_id)
                .map(|equation| {
                    equation
                        .relation_refs
                        .iter()
                        .cloned()
                        .zip(equation.relation_ids.iter().cloned())
                        .collect()
                })
                .unwrap_or_default(),
            TheoryObligationRefIr::RewriteRule { rule_id, .. } => self
                .rewrite_rules
                .iter()
                .find(|candidate| &candidate.rule_id == rule_id)
                .map(|rule| {
                    rule.relation_refs
                        .iter()
                        .cloned()
                        .zip(rule.relation_ids.iter().cloned())
                        .collect()
                })
                .unwrap_or_default(),
            TheoryObligationRefIr::Constraint { .. }
            | TheoryObligationRefIr::OpaqueEquation { .. } => BTreeMap::new(),
        }
    }
}

fn collect_path_expression_refs_for_path(
    obligation_ref: &TheoryObligationRefIr,
    side: TheoryPathSideIr,
    path: &PathExprV3,
    expression_index: &mut u32,
    out: &mut Vec<TheoryPathExpressionRefIr>,
) {
    let current_index = *expression_index;
    *expression_index += 1;
    out.push(TheoryPathExpressionRefIr {
        expression_id: format!(
            "path_expr:{}:{}:{}",
            obligation_ref.stable_id(),
            side.as_str(),
            current_index
        ),
        obligation_ref: obligation_ref.clone(),
        side,
        expression_index: current_index,
        expression_kind: path_expr_kind(path),
        expression: path.to_string(),
    });

    match path {
        PathExprV3::Trans { left, right } => {
            collect_path_expression_refs_for_path(
                obligation_ref,
                side,
                left,
                expression_index,
                out,
            );
            collect_path_expression_refs_for_path(
                obligation_ref,
                side,
                right,
                expression_index,
                out,
            );
        }
        PathExprV3::Inv { path } => {
            collect_path_expression_refs_for_path(
                obligation_ref,
                side,
                path,
                expression_index,
                out,
            );
        }
        PathExprV3::Var { .. } | PathExprV3::Reflexive { .. } | PathExprV3::Step { .. } => {}
    }
}

fn collect_path_step_refs_for_path(
    obligation_ref: &TheoryObligationRefIr,
    side: TheoryPathSideIr,
    path: &PathExprV3,
    relation_ids: &BTreeMap<String, RelationId>,
    expression_index: &mut u32,
    step_index: &mut u32,
    out: &mut Vec<TheoryPathStepRefIr>,
) {
    let current_expression_index = *expression_index;
    *expression_index += 1;
    match path {
        PathExprV3::Step { from, rel, to } => {
            let current_step_index = *step_index;
            *step_index += 1;
            out.push(TheoryPathStepRefIr {
                step_id: format!(
                    "path_step:{}:{}:{}",
                    obligation_ref.stable_id(),
                    side.as_str(),
                    current_step_index
                ),
                obligation_ref: obligation_ref.clone(),
                side,
                step_index: current_step_index,
                expression_index: current_expression_index,
                relation_name: rel.clone(),
                relation_id: relation_ids.get(rel).cloned(),
                from_var: from.clone(),
                to_var: to.clone(),
            });
        }
        PathExprV3::Trans { left, right } => {
            collect_path_step_refs_for_path(
                obligation_ref,
                side,
                left,
                relation_ids,
                expression_index,
                step_index,
                out,
            );
            collect_path_step_refs_for_path(
                obligation_ref,
                side,
                right,
                relation_ids,
                expression_index,
                step_index,
                out,
            );
        }
        PathExprV3::Inv { path } => {
            collect_path_step_refs_for_path(
                obligation_ref,
                side,
                path,
                relation_ids,
                expression_index,
                step_index,
                out,
            );
        }
        PathExprV3::Var { .. } | PathExprV3::Reflexive { .. } => {}
    }
}

fn path_expr_kind(path: &PathExprV3) -> TheoryPathExprKindIr {
    match path {
        PathExprV3::Var { .. } => TheoryPathExprKindIr::Var,
        PathExprV3::Reflexive { .. } => TheoryPathExprKindIr::Reflexive,
        PathExprV3::Step { .. } => TheoryPathExprKindIr::Step,
        PathExprV3::Trans { .. } => TheoryPathExprKindIr::Trans,
        PathExprV3::Inv { .. } => TheoryPathExprKindIr::Inv,
    }
}

fn collect_path_variable_refs(
    obligation_ref: &TheoryObligationRefIr,
    path: &PathExprV3,
    out: &mut BTreeSet<TheoryVariableRefIr>,
) {
    match path {
        PathExprV3::Var { name } => {
            out.insert(theory_variable_ref(
                obligation_ref,
                name,
                TheoryVariableKindIr::Path,
                None,
                None,
                None,
            ));
        }
        PathExprV3::Reflexive { entity } => {
            out.insert(theory_variable_ref(
                obligation_ref,
                entity,
                TheoryVariableKindIr::Endpoint,
                None,
                None,
                None,
            ));
        }
        PathExprV3::Step { from, to, .. } => {
            out.insert(theory_variable_ref(
                obligation_ref,
                from,
                TheoryVariableKindIr::Endpoint,
                None,
                None,
                None,
            ));
            out.insert(theory_variable_ref(
                obligation_ref,
                to,
                TheoryVariableKindIr::Endpoint,
                None,
                None,
                None,
            ));
        }
        PathExprV3::Trans { left, right } => {
            collect_path_variable_refs(obligation_ref, left, out);
            collect_path_variable_refs(obligation_ref, right, out);
        }
        PathExprV3::Inv { path } => {
            collect_path_variable_refs(obligation_ref, path, out);
        }
    }
}

fn variable_ref_for_rewrite_declaration(
    obligation_ref: &TheoryObligationRefIr,
    declaration: &str,
) -> Option<TheoryVariableRefIr> {
    let (name, raw_ty) = declaration.split_once(':')?;
    let name = name.trim();
    let ty = raw_ty.trim();
    if let Some(inner) = ty
        .strip_prefix("Path(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (from, to) = inner.split_once(',')?;
        return Some(theory_variable_ref(
            obligation_ref,
            name,
            TheoryVariableKindIr::Path,
            None,
            Some(from.trim().to_string()),
            Some(to.trim().to_string()),
        ));
    }

    Some(theory_variable_ref(
        obligation_ref,
        name,
        TheoryVariableKindIr::Object,
        Some(ty.to_string()),
        None,
        None,
    ))
}

fn theory_variable_ref(
    obligation_ref: &TheoryObligationRefIr,
    variable_name: &str,
    variable_kind: TheoryVariableKindIr,
    object_type: Option<String>,
    path_from: Option<String>,
    path_to: Option<String>,
) -> TheoryVariableRefIr {
    TheoryVariableRefIr {
        variable_id: format!(
            "theory_var:{}:{}:{}",
            obligation_ref.stable_id(),
            variable_kind.as_str(),
            variable_name
        ),
        obligation_ref: obligation_ref.clone(),
        variable_name: variable_name.to_string(),
        variable_kind,
        object_type,
        path_from,
        path_to,
    }
}

fn path_endpoint_vars(path: &PathExprV3) -> Option<(String, String)> {
    match path {
        PathExprV3::Var { .. } => None,
        PathExprV3::Reflexive { entity } => Some((entity.clone(), entity.clone())),
        PathExprV3::Step { from, to, .. } => Some((from.clone(), to.clone())),
        PathExprV3::Trans { left, right } => {
            let (from, _) = path_endpoint_vars(left)?;
            let (_, to) = path_endpoint_vars(right)?;
            Some((from, to))
        }
        PathExprV3::Inv { path } => {
            let (from, to) = path_endpoint_vars(path)?;
            Some((to, from))
        }
    }
}

fn theory_endpoint_ref(
    obligation_ref: &TheoryObligationRefIr,
    side: TheoryPathSideIr,
    from_var: String,
    to_var: String,
    from_type: Option<String>,
    to_type: Option<String>,
) -> TheoryEndpointRefIr {
    TheoryEndpointRefIr {
        endpoint_id: format!(
            "theory_endpoint:{}:{}",
            obligation_ref.stable_id(),
            side.as_str()
        ),
        obligation_ref: obligation_ref.clone(),
        side,
        from_var,
        to_var,
        from_type,
        to_type,
    }
}

fn touched_roles_for_obligation<'a>(
    theory: &'a TheoryIr,
    obligation: &TheoryObligationRefIr,
) -> Vec<&'a TheoryTouchedRoleIr> {
    match obligation {
        TheoryObligationRefIr::PathEquation { equation_id, .. } => theory
            .path_equations
            .iter()
            .find(|candidate| &candidate.equation_id == equation_id)
            .map(|equation| equation.touched_roles.iter().collect())
            .unwrap_or_default(),
        TheoryObligationRefIr::RewriteRule { rule_id, .. } => theory
            .rewrite_rules
            .iter()
            .find(|candidate| &candidate.rule_id == rule_id)
            .map(|rule| rule.touched_roles.iter().collect())
            .unwrap_or_default(),
        TheoryObligationRefIr::Constraint { .. } | TheoryObligationRefIr::OpaqueEquation { .. } => {
            Vec::new()
        }
    }
}

fn theory_context_axis_ref(
    obligation_ref: &TheoryObligationRefIr,
    role: &TheoryTouchedRoleIr,
) -> TheoryContextAxisRefIr {
    TheoryContextAxisRefIr {
        context_axis_id: format!(
            "context_axis:{}:{}",
            obligation_ref.stable_id(),
            role.role_id.as_str()
        ),
        obligation_ref: obligation_ref.clone(),
        relation_id: role.relation_id.clone(),
        relation_name: role.relation_name.clone(),
        role_id: role.role_id.clone(),
        role_name: role.role_name.clone(),
        role_kind: role.role_kind,
        target_type: role.target_type.clone(),
    }
}

fn theory_graph_obligation_node_id(obligation_ref: &TheoryObligationRefIr) -> String {
    format!("theory_obligation:{}", obligation_ref.stable_id())
}

fn theory_graph_subject_node_id(subject_ref: &TheorySubjectRefIr) -> String {
    let kind = match subject_ref.subject_kind() {
        TheorySubjectKindIr::Theory => "theory",
        TheorySubjectKindIr::Relation => "relation",
        TheorySubjectKindIr::Role => "role",
    };
    format!("theory_subject:{kind}:{}", subject_ref.stable_id())
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

pub fn build_theory_transport_plan_ir(
    compiled_schema: &RuntimeSchemaIndex,
    theory: &TheoryIr,
    morphism: &SchemaMorphismV1,
    operator: MigrationFunctorKindV1,
) -> TheoryTransportPlanIr {
    let items = theory
        .obligation_refs()
        .into_iter()
        .map(|obligation_ref| {
            theory_transport_item_ir(
                compiled_schema,
                theory,
                morphism,
                operator.clone(),
                obligation_ref,
            )
        })
        .collect::<Vec<_>>();
    let preserved_obligations = items
        .iter()
        .filter(|item| item.status == TheoryTransportStatusIr::Preserved)
        .count();
    let transported_obligations = items
        .iter()
        .filter(|item| item.status == TheoryTransportStatusIr::Transported)
        .count();
    let blocked_obligations = items
        .iter()
        .filter(|item| {
            matches!(
                item.status,
                TheoryTransportStatusIr::MissingObjectImage
                    | TheoryTransportStatusIr::MissingArrowImage
                    | TheoryTransportStatusIr::OpaqueOrOutOfFragment
            )
        })
        .count();

    let mut notes = Vec::new();
    if compiled_schema.schema_id.as_str() != morphism.source_schema {
        notes.push(format!(
            "morphism source schema `{}` does not match compiled schema `{}`; transport plan is still emitted for review but should not be materialized",
            morphism.source_schema, compiled_schema.schema_id
        ));
    }
    if blocked_obligations > 0 {
        notes.push(format!(
            "{blocked_obligations} theory obligation(s) require resolver work before transport can be treated as well-typed"
        ));
    }

    TheoryTransportPlanIr {
        version: THEORY_TRANSPORT_PLAN_VERSION_V1.to_string(),
        operator,
        source_theory: TheorySubjectRefIr::Theory {
            theory_id: theory.theory_id.clone(),
        },
        source_schema_id: compiled_schema.schema_id.clone(),
        target_schema: morphism.target_schema.clone(),
        total_obligations: items.len(),
        preserved_obligations,
        transported_obligations,
        blocked_obligations,
        items,
        non_claims: vec![
            "runtime theory transport is a typed planning artifact, not a Lean certificate"
                .to_string(),
            "preserved means relation/role object images are identity-like under this runtime morphism view; it is not a proof of full semantic conservativity"
                .to_string(),
            "transported means the obligation remains addressable but needs review/certification before strong migration soundness can be claimed"
                .to_string(),
            "no completeness or ontology-closure claim is made for transported obligations"
                .to_string(),
        ],
        notes,
    }
}

fn theory_transport_item_ir(
    compiled_schema: &RuntimeSchemaIndex,
    theory: &TheoryIr,
    morphism: &SchemaMorphismV1,
    operator: MigrationFunctorKindV1,
    obligation_ref: TheoryObligationRefIr,
) -> TheoryTransportItemIr {
    let subject_refs = theory.subject_refs_for_obligation(&obligation_ref);
    let relation_names = theory.relation_names_for_obligation(&obligation_ref);
    let mut transport_basis = BTreeSet::new();
    let mut missing_object_images = BTreeSet::new();
    let mut missing_arrow_images = BTreeSet::new();
    let mut transported = false;

    if matches!(obligation_ref, TheoryObligationRefIr::OpaqueEquation { .. }) {
        return TheoryTransportItemIr {
            operator,
            obligation_ref,
            subject_refs,
            status: TheoryTransportStatusIr::OpaqueOrOutOfFragment,
            transport_basis: Vec::new(),
            missing_object_images: Vec::new(),
            missing_arrow_images: Vec::new(),
            detail: "opaque equation is preserved as review text, but it is outside the runtime path/rewrite transport fragment".to_string(),
        };
    }

    for relation_name in &relation_names {
        match morphism.arrow_image(relation_name) {
            Some(target_path) => {
                let target_path_label = if target_path.is_empty() {
                    "identity".to_string()
                } else {
                    target_path.join(" ; ")
                };
                transport_basis.insert(format!("arrow {relation_name} -> {target_path_label}"));
                if !(target_path.len() == 1 && target_path[0].as_str() == relation_name.as_str()) {
                    transported = true;
                }
            }
            None => {
                missing_arrow_images.insert(relation_name.clone());
            }
        }

        if let Some(relation) = compiled_schema
            .relation(relation_name)
            .or_else(|| compiled_schema.relation(local_name(relation_name)))
        {
            for role in &relation.roles {
                match morphism.object_image(&role.target_type) {
                    Some(target_object) => {
                        transport_basis
                            .insert(format!("object {} -> {target_object}", role.target_type));
                        if target_object != role.target_type {
                            transported = true;
                        }
                    }
                    None => {
                        missing_object_images.insert(role.target_type.clone());
                    }
                }
            }
        }
    }

    let missing_object_images = missing_object_images.into_iter().collect::<Vec<_>>();
    let missing_arrow_images = missing_arrow_images.into_iter().collect::<Vec<_>>();
    let transport_basis = transport_basis.into_iter().collect::<Vec<_>>();
    let status = if !missing_object_images.is_empty() {
        TheoryTransportStatusIr::MissingObjectImage
    } else if !missing_arrow_images.is_empty() {
        TheoryTransportStatusIr::MissingArrowImage
    } else if transported {
        TheoryTransportStatusIr::Transported
    } else {
        TheoryTransportStatusIr::Preserved
    };

    let detail = match status {
        TheoryTransportStatusIr::Preserved => {
            "obligation is preserved by identity-like object and arrow images in the runtime morphism view"
                .to_string()
        }
        TheoryTransportStatusIr::Transported => {
            "obligation remains typed and addressable, but non-identity object or arrow images require transport review".to_string()
        }
        TheoryTransportStatusIr::MissingObjectImage => format!(
            "obligation cannot be transported until object image(s) are supplied: {}",
            missing_object_images.join(", ")
        ),
        TheoryTransportStatusIr::MissingArrowImage => format!(
            "obligation cannot be transported until arrow image(s) are supplied: {}",
            missing_arrow_images.join(", ")
        ),
        TheoryTransportStatusIr::OpaqueOrOutOfFragment => {
            "obligation is outside the runtime transport fragment".to_string()
        }
    };

    TheoryTransportItemIr {
        operator,
        obligation_ref,
        subject_refs,
        status,
        transport_basis,
        missing_object_images,
        missing_arrow_images,
        detail,
    }
}

/// Derive PathDB's non-authoritative runtime index after the canonical compiler
/// has accepted the exact source bytes. The returned runtime index retains the
/// immutable canonical snapshot for in-process authority checks; serialization
/// drops that handle, so deserialized runtime indexes remain citations only.
pub fn derive_runtime_module_index(
    module: &SchemaV1Module,
    axi_text: &str,
) -> Result<RuntimeModuleIndex, String> {
    let source = CanonicalModuleSource::parse(axi_text.as_bytes().to_vec()).map_err(|error| {
        format!("cannot derive runtime index from non-canonical source: {error}")
    })?;
    if source.parsed() != module {
        return Err(
            "refusing to derive runtime index: supplied AST does not match the canonical .axi source"
                .to_string(),
        );
    }
    let canonical_snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"axiograph:pathdb-runtime-index"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[axi_text.as_bytes()]),
        root_module: source.parsed().module_name.clone(),
        modules: vec![source],
    })
    .map_err(|error| format!("canonical compiler rejected runtime-index input: {error}"))?;

    let validated = validate_axi_v1_module(module.clone())
        .map_err(|error| format!("canonical .axi module failed validation: {error}"))?;
    derive_runtime_index_from_validated(
        validated.module(),
        AxiDigest::from_axi_text(axi_text),
        canonical_snapshot,
    )
}

/// Validate a package-shaped adapter AST against an already compiled canonical
/// snapshot. Exact sources must match every module revision in canonical closure
/// order; the returned lifecycle witness remains a derived PathDB boundary.
pub fn validate_runtime_package_adapter(
    canonical_snapshot: &CompiledKernelSnapshot,
    sources: &[CanonicalModuleSource],
) -> Result<Module<Validated>, String> {
    let mut source_by_name = BTreeMap::new();
    for source in sources {
        let name = source.parsed().module_name.clone();
        if source_by_name.insert(name.clone(), source).is_some() {
            return Err(format!("duplicate package source `{name}`"));
        }
    }
    let closure = canonical_snapshot.ir().ordered_module_closure();
    if source_by_name.len() != closure.len() {
        return Err(format!(
            "runtime package source count {} does not match canonical closure count {}",
            source_by_name.len(),
            closure.len()
        ));
    }
    let root = closure
        .iter()
        .find(|module| &module.module_id == canonical_snapshot.ir().root_module_id())
        .ok_or_else(|| "canonical snapshot root module is absent from its closure".to_string())?;
    let mut adapter = SchemaV1Module {
        module_name: root.module_name.clone(),
        imports: Vec::new(),
        schemas: Vec::new(),
        theories: Vec::new(),
        instances: Vec::new(),
    };
    for compiled_module in closure {
        let source = source_by_name
            .get(&compiled_module.module_name)
            .ok_or_else(|| {
                format!(
                    "runtime package is missing canonical module `{}`",
                    compiled_module.module_name
                )
            })?;
        if source.revision() != &compiled_module.revision {
            return Err(format!(
                "runtime package module `{}` exact-byte revision does not match canonical snapshot",
                compiled_module.module_name
            ));
        }
        adapter.schemas.extend(source.parsed().schemas.clone());
        adapter.theories.extend(source.parsed().theories.clone());
        adapter.instances.extend(source.parsed().instances.clone());
    }
    validate_axi_v1_module(adapter)
        .map_err(|error| format!("derived runtime package adapter failed validation: {error}"))
}

/// Derive one runtime citation index from an exact-source canonical package.
/// This never recompiles or reconstructs semantic authority: the supplied
/// immutable snapshot licenses the derived adapter and is retained in-process.
pub fn derive_runtime_package_index(
    canonical_snapshot: &CompiledKernelSnapshot,
    sources: &[CanonicalModuleSource],
) -> Result<RuntimeModuleIndex, String> {
    let validated = validate_runtime_package_adapter(canonical_snapshot, sources)?;
    let root_name = validated.module().module_name.as_str();
    let root_source = sources
        .iter()
        .find(|source| source.parsed().module_name == root_name)
        .ok_or_else(|| format!("runtime package root source `{root_name}` is missing"))?;
    derive_runtime_index_from_validated(
        validated.module(),
        AxiDigest::from_axi_text(root_source.exact_text()),
        canonical_snapshot.clone(),
    )
}

fn attach_canonical_equation_formation_refs(
    source_theory: &SchemaV1Theory,
    runtime_theory: &mut TheoryIr,
    canonical_snapshot: &CompiledKernelSnapshot,
) {
    let matching_schemas = canonical_snapshot
        .ir()
        .schemas()
        .iter()
        .filter(|schema| schema.label == source_theory.schema)
        .collect::<Vec<_>>();
    let [schema] = matching_schemas.as_slice() else {
        return;
    };
    let matching_theories = canonical_snapshot
        .ir()
        .theories()
        .iter()
        .filter(|theory| theory.schema_id == schema.schema_id && theory.label == source_theory.name)
        .collect::<Vec<_>>();
    let [canonical_theory] = matching_theories.as_slice() else {
        return;
    };
    for opaque in &mut runtime_theory.opaque_equations {
        let Some(equation) = canonical_theory
            .equations
            .iter()
            .find(|equation| equation.label == opaque.name && equation.schema_equation.is_some())
        else {
            continue;
        };
        opaque.canonical_formation_ref = canonical_snapshot
            .ir()
            .refs()
            .iter()
            .find(|reference| {
                matches!(
                    reference,
                    KernelRefV2::Equation { equation_id, .. }
                        if equation_id == &equation.equation_id
                )
            })
            .cloned();
    }
}

fn derive_runtime_index_from_validated(
    module: &SchemaV1Module,
    module_digest: AxiDigest,
    canonical_snapshot: CompiledKernelSnapshot,
) -> Result<RuntimeModuleIndex, String> {
    let schemas = module
        .schemas
        .iter()
        .map(derive_runtime_schema_index)
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
            let mut runtime_theory = derive_runtime_theory_index(compiled_schema, theory)?;
            attach_canonical_equation_formation_refs(
                theory,
                &mut runtime_theory,
                &canonical_snapshot,
            );
            Ok(runtime_theory)
        })
        .collect::<Result<Vec<_>, String>>()?;

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
            derive_runtime_instance_index(
                &module.module_name,
                compiled_schema,
                schema_ast,
                instance,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let canonical_citations = canonical_snapshot
        .ir()
        .refs()
        .iter()
        .cloned()
        .map(|reference| CanonicalKernelCitationIr {
            label: canonical_snapshot
                .ir()
                .label_for_ref(&reference)
                .unwrap_or_else(|| reference.stable_label()),
            reference,
        })
        .collect();
    Ok(RuntimeModuleIndex {
        module_digest,
        schemas,
        theories,
        instances,
        canonical_citations,
        canonical_snapshot: Some(canonical_snapshot),
    })
}

impl RuntimeModuleIndex {
    /// Canonical authority retained from the exact-byte compilation that
    /// produced this derived runtime index. `None` means the index came from a
    /// serialized runtime artifact and must not be treated as authoritative.
    pub fn canonical_snapshot(&self) -> Option<&CompiledKernelSnapshot> {
        self.canonical_snapshot.as_ref()
    }

    pub fn runtime_semantic_index(&self) -> RuntimeSemanticIndex {
        build_runtime_semantic_index(self)
    }
}

pub fn build_runtime_semantic_index(module: &RuntimeModuleIndex) -> RuntimeSemanticIndex {
    let refs = module
        .canonical_citations
        .iter()
        .cloned()
        .map(|citation| RuntimeIrRef::Canonical { citation })
        .collect::<Vec<_>>();
    RuntimeSemanticIndex {
        version: RUNTIME_SEMANTIC_INDEX_VERSION.to_string(),
        module_digest: module.module_digest.clone(),
        total_refs: refs.len(),
        refs,
        notes: vec![
            "this runtime surface is a read-only citation projection of KernelSnapshotIr::refs; it is not a second category or proof object".to_string(),
            "serialized citations retain canonical typed references but cannot reconstruct the accepted snapshot handle".to_string(),
        ],
    }
}

pub fn derive_runtime_instance_index(
    module_name: &str,
    compiled_schema: &RuntimeSchemaIndex,
    schema: &SchemaV1Schema,
    instance: &SchemaV1Instance,
) -> Result<InstanceIr, String> {
    let mut object_members = Vec::new();
    for object_name in &schema.objects {
        let assignments = instance
            .assignments
            .iter()
            .filter(|assignment| assignment.name == *object_name)
            .collect::<Vec<_>>();
        if assignments.is_empty() {
            continue;
        }
        let object_type_id = compiled_schema
            .object_type_id(object_name)
            .cloned()
            .ok_or_else(|| format!("compiled schema missing object type `{object_name}`"))?;
        let members = assignments
            .iter()
            .flat_map(|assignment| assignment.value.items.iter())
            .filter_map(|item| match item {
                SetItemV1::Ident { name } => Some(name.clone()),
                SetItemV1::Tuple { .. } => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        object_members.push(ObjectMembershipIr {
            object_type_id,
            object_type_name: object_name.clone(),
            members,
        });
    }

    let mut relation_facts = Vec::new();
    let mut seen_fact_ids = HashMap::new();
    for relation in &schema.relations {
        let relation_ir = compiled_schema
            .relation(&relation.name)
            .ok_or_else(|| format!("compiled schema missing relation `{}`", relation.name))?;
        let role_lookup = relation_ir
            .roles
            .iter()
            .map(|role| (role.name.as_str(), role))
            .collect::<HashMap<_, _>>();

        for item in instance
            .assignments
            .iter()
            .filter(|assignment| assignment.name == relation.name)
            .flat_map(|assignment| assignment.value.items.iter())
        {
            let SetItemV1::Tuple { fields, .. } = item else {
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

            let fact_id = StableFactId::new(runtime_fact_id_v2(
                module_name,
                &schema.name,
                &instance.name,
                &relation.name,
                &fact_fields,
            ));
            let fact = RelationFactIr {
                fact_id,
                relation_id: relation_ir.relation_id.clone(),
                relation_name: relation.name.clone(),
                role_values,
            };
            insert_relation_fact(&mut seen_fact_ids, &mut relation_facts, fact)?;
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

fn insert_relation_fact(
    seen: &mut HashMap<StableFactId, RelationFactIr>,
    facts: &mut Vec<RelationFactIr>,
    fact: RelationFactIr,
) -> Result<(), String> {
    if let Some(existing) = seen.get(&fact.fact_id) {
        if existing != &fact {
            return Err(format!(
                "fact id collision `{}`: differing typed payloads must not overwrite or deduplicate",
                fact.fact_id
            ));
        }
        return Ok(());
    }
    seen.insert(fact.fact_id.clone(), fact.clone());
    facts.push(fact);
    Ok(())
}

pub fn derive_runtime_schema_index(schema: &SchemaV1Schema) -> RuntimeSchemaIndex {
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
                    target_type: field.ty.referenced_name().to_string(),
                    order: idx as u16,
                    kind: role_kind_from_decl(field.kind),
                })
                .collect::<Vec<_>>();
            (
                rel.name.clone(),
                derive_relation_semantics(&schema.name, &rel.name, tuple_type_name, roles),
            )
        })
        .collect();
    let role_interfaces = derive_role_interfaces(&relations, &subtypes_of);

    RuntimeSchemaIndex {
        schema_id: SchemaId::new(schema.name.clone()),
        object_types,
        object_type_ids,
        supertypes_of,
        subtypes_of,
        relations,
        role_interfaces,
    }
}

pub fn derive_relation_semantics(
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
        relation_id: RelationId::new(format!("relation:{schema_name}:{relation_name}")),
        name: relation_name.to_string(),
        tuple_type_name,
        roles,
        carrier,
        witness_view,
    }
}

pub fn derive_runtime_theory_index(
    compiled_schema: &RuntimeSchemaIndex,
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
            derive_constraint_index(compiled_schema, theory, &theory_id, index, constraint)
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
                let touched_roles =
                    touched_roles_for_relation_refs(compiled_schema, &relation_refs);
                path_equations.push(PathEquationIr {
                    equation_id,
                    name: equation.name.clone(),
                    lhs,
                    rhs,
                    relation_refs,
                    relation_ids,
                    touched_roles,
                });
            }
            _ => opaque_equations.push(OpaqueEquationIr {
                equation_id,
                name: equation.name.clone(),
                lhs: equation.lhs.clone(),
                rhs: equation.rhs.clone(),
                canonical_formation_ref: None,
            }),
        }
    }

    let mut rewrite_rule_names = HashSet::new();
    let rewrite_rules = theory
        .rewrite_rules
        .iter()
        .map(|rule| {
            derive_rewrite_rule_index(
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

fn derive_constraint_index(
    compiled_schema: &RuntimeSchemaIndex,
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
        revision_digest_v2(&digest_input)
    ))
}

fn derive_rewrite_rule_index(
    compiled_schema: &RuntimeSchemaIndex,
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
    let touched_roles = touched_roles_for_relation_refs(compiled_schema, &relation_refs);

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
        touched_roles,
        source: RewriteRuleSource::AcceptedAxi,
    })
}

fn touched_roles_for_relation_refs(
    compiled_schema: &RuntimeSchemaIndex,
    relation_refs: &[String],
) -> Vec<TheoryTouchedRoleIr> {
    let mut roles = Vec::new();
    for relation_name in relation_refs {
        if let Some(relation) = compiled_schema.relation(relation_name) {
            roles.extend(relation.roles.iter().map(|role| TheoryTouchedRoleIr {
                relation_id: relation.relation_id.clone(),
                relation_name: relation.name.clone(),
                role_id: role.role_id.clone(),
                role_name: role.name.clone(),
                role_kind: role.kind,
                target_type: role.target_type.clone(),
            }));
        }
    }
    roles.sort_by(|left, right| {
        left.relation_id
            .as_str()
            .cmp(right.relation_id.as_str())
            .then_with(|| left.role_id.as_str().cmp(right.role_id.as_str()))
    });
    roles.dedup_by(|left, right| left.role_id == right.role_id);
    roles
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
    compiled_schema: &RuntimeSchemaIndex,
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
    compiled_schema: &RuntimeSchemaIndex,
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
    compiled_schema: &RuntimeSchemaIndex,
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
    compiled_schema: &RuntimeSchemaIndex,
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
    compiled_schema: &RuntimeSchemaIndex,
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
            "variable `{variable}` is required to have incompatible object types `{existing}` and `{expected_type}`"
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

pub(crate) fn role_kind_from_decl(kind: axiograph_dsl::schema_v1::RoleKindV1) -> RoleKind {
    match kind {
        axiograph_dsl::schema_v1::RoleKindV1::Data => RoleKind::Data,
        axiograph_dsl::schema_v1::RoleKindV1::Context => RoleKind::Context,
        axiograph_dsl::schema_v1::RoleKindV1::World => RoleKind::World,
        axiograph_dsl::schema_v1::RoleKindV1::Temporal => RoleKind::Temporal,
        axiograph_dsl::schema_v1::RoleKindV1::Parameter => RoleKind::Parameter,
        axiograph_dsl::schema_v1::RoleKindV1::Evidence => RoleKind::Evidence,
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

fn derive_role_interfaces(
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
