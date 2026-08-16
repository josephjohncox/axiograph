#![forbid(unsafe_code)]
//! Typed derived projections from the immutable compiled kernel IR.
//!
//! This crate is deliberately outside the semantic kernel. It accepts only a
//! [`CompiledKernelSnapshot`] and emits projection manifests anchored to that
//! snapshot. Backends never become semantic or mutation authority. Readback is
//! compared as finite transport evidence and cannot mutate accepted state.

use axiograph_kernel::{
    CompiledKernelSnapshot, ConstraintIdV2, EquationIdV2, KernelRefV2, ObjectBlobIdV2,
    RefinementPredicateIr, RepositoryIdV2, RevisionDigestV2, RewriteRuleIdV2, RoleKindIr,
    SchemaGeneratorKindIr, SchemaGeneratorRefIr, SchemaIdV2, SchemaObjectRefIr, SemanticKeyV2,
    SnapshotIdV2, TheoryIdV2, TypeExprIr, TypedValueIr,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::str::FromStr;
use thiserror::Error;

pub const PROJECTION_MANIFEST_VERSION_V1: &str = "axiograph_projection_manifest_v1";
pub const READBACK_INVENTORY_VERSION_V1: &str = "axiograph_projection_readback_inventory_v1";
pub const READBACK_REPORT_VERSION_V1: &str = "axiograph_projection_readback_report_v1";
pub const EVIDENCE_ENVELOPE_VERSION_V1: &str = "axiograph_external_evidence_envelope_v1";

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("projection serialization failed: {0}")]
    Serialization(String),
    #[error("projection contains duplicate record id `{0}`")]
    DuplicateProjectionRecord(String),
    #[error("readback contains duplicate record id `{0}`")]
    DuplicateReadbackRecord(String),
    #[error("readback projection id does not match the manifest")]
    ProjectionIdMismatch,
    #[error("unsupported projection manifest version `{0}`")]
    UnsupportedManifestVersion(String),
    #[error("projection manifest id does not commit to its current contents")]
    ManifestIntegrityMismatch,
    #[error("unsupported readback inventory version `{0}`")]
    UnsupportedReadbackVersion(String),
    #[error("invalid readback provenance: {0}")]
    InvalidReadbackProvenance(String),
    #[error("readback backend `{observed}` does not match manifest backend `{expected}`")]
    BackendMismatch { expected: String, observed: String },
    #[error("unsupported projection backend `{0}`")]
    UnsupportedBackend(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionBackendV1 {
    PathDb,
    TypeDb,
    TerminusDb,
    RdfOwl,
    PropertyGraph,
}

impl ProjectionBackendV1 {
    pub const ALL: [Self; 5] = [
        Self::PathDb,
        Self::TypeDb,
        Self::TerminusDb,
        Self::RdfOwl,
        Self::PropertyGraph,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PathDb => "pathdb",
            Self::TypeDb => "typedb",
            Self::TerminusDb => "terminusdb",
            Self::RdfOwl => "rdf_owl",
            Self::PropertyGraph => "property_graph",
        }
    }
}

impl std::fmt::Display for ProjectionBackendV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ProjectionBackendV1 {
    type Err = ProjectionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "pathdb" => Ok(Self::PathDb),
            "typedb" | "type_db" => Ok(Self::TypeDb),
            "terminusdb" | "terminus_db" => Ok(Self::TerminusDb),
            "rdf" | "owl" | "rdf_owl" => Ok(Self::RdfOwl),
            "property_graph" | "propertygraph" => Ok(Self::PropertyGraph),
            other => Err(ProjectionError::UnsupportedBackend(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionSupportTierV1 {
    Primary,
    Supported,
    Experimental,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionAuthorityV1 {
    DerivedOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionMutationAuthorityV1 {
    AxiographSemanticVcsOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalEvidenceAuthorityV1 {
    EvidenceOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionCapabilityV1 {
    RelationObjects,
    NaryRelations,
    TypedRoleProjections,
    SubtypeInclusions,
    DependentIndexes,
    RefinementPredicates,
    ContextWorldAxes,
    EvidenceProvenance,
    Constraints,
    PathEquations,
    RewriteRules,
    HigherPaths,
    FiniteInstances,
    NativeReadback,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityDispositionV1 {
    Native,
    Encoded,
    Sidecar,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDeclarationV1 {
    pub capability: ProjectionCapabilityV1,
    pub disposition: CapabilityDispositionV1,
    pub finite_scope: String,
    pub caveat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackendCapabilityDeclarationV1 {
    pub backend: ProjectionBackendV1,
    pub support_tier: ProjectionSupportTierV1,
    pub native_read_surface: String,
    pub authority: ProjectionAuthorityV1,
    pub mutation_authority: ProjectionMutationAuthorityV1,
    pub capabilities: Vec<CapabilityDeclarationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionAnchorV1 {
    pub repository_id: RepositoryIdV2,
    pub accepted_snapshot_id: SnapshotIdV2,
    pub kernel_ir_digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedTypeExprV1 {
    Object {
        object_type_id: String,
    },
    RelationObject {
        relation_id: String,
    },
    Indexed {
        base: Box<ProjectedTypeExprV1>,
        over_role_ids: Vec<String>,
    },
    Refined {
        base: Box<ProjectedTypeExprV1>,
        predicates: Vec<ProjectedRefinementPredicateV1>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedRefinementPredicateV1 {
    Equals { value: String },
    MemberOf { values: Vec<String> },
    Cardinality { min: u32, max: u32 },
    Key { role_ids: Vec<String> },
    Enum { values: Vec<String> },
    Predicate { name: String, args: Vec<String> },
}

pub fn project_type_expr_v1(type_expr: &TypeExprIr) -> ProjectedTypeExprV1 {
    match type_expr {
        TypeExprIr::Object { object_type_id } => ProjectedTypeExprV1::Object {
            object_type_id: object_type_id.to_string(),
        },
        TypeExprIr::RelationObject { relation_id } => ProjectedTypeExprV1::RelationObject {
            relation_id: relation_id.to_string(),
        },
        TypeExprIr::Indexed { base, over_roles } => ProjectedTypeExprV1::Indexed {
            base: Box::new(project_type_expr_v1(base)),
            over_role_ids: over_roles.iter().map(ToString::to_string).collect(),
        },
        TypeExprIr::Refined { base, predicates } => ProjectedTypeExprV1::Refined {
            base: Box::new(project_type_expr_v1(base)),
            predicates: predicates.iter().map(project_refinement_v1).collect(),
        },
    }
}

fn project_refinement_v1(predicate: &RefinementPredicateIr) -> ProjectedRefinementPredicateV1 {
    match predicate {
        RefinementPredicateIr::Equals { value } => ProjectedRefinementPredicateV1::Equals {
            value: value.clone(),
        },
        RefinementPredicateIr::MemberOf { values } => ProjectedRefinementPredicateV1::MemberOf {
            values: values.clone(),
        },
        RefinementPredicateIr::Cardinality { min, max } => {
            ProjectedRefinementPredicateV1::Cardinality {
                min: *min,
                max: *max,
            }
        }
        RefinementPredicateIr::Key { roles } => ProjectedRefinementPredicateV1::Key {
            role_ids: roles.iter().map(ToString::to_string).collect(),
        },
        RefinementPredicateIr::Enum { values } => ProjectedRefinementPredicateV1::Enum {
            values: values.clone(),
        },
        RefinementPredicateIr::Predicate { name, args } => {
            ProjectedRefinementPredicateV1::Predicate {
                name: name.clone(),
                args: args.clone(),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedObjectRefV1 {
    ObjectType { object_type_id: String },
    RelationObject { relation_id: String },
}

fn project_object_ref(object: &SchemaObjectRefIr) -> ProjectedObjectRefV1 {
    match object {
        SchemaObjectRefIr::ObjectType { object_type_id } => ProjectedObjectRefV1::ObjectType {
            object_type_id: object_type_id.to_string(),
        },
        SchemaObjectRefIr::RelationObject { relation_id } => ProjectedObjectRefV1::RelationObject {
            relation_id: relation_id.to_string(),
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedGeneratorRefV1 {
    RoleProjection { role_id: String },
    SubtypeInclusion { semantic_key: String },
    Explicit { semantic_key: String },
}

fn project_generator_ref(generator: &SchemaGeneratorRefIr) -> ProjectedGeneratorRefV1 {
    match generator {
        SchemaGeneratorRefIr::RoleProjection { role_id } => {
            ProjectedGeneratorRefV1::RoleProjection {
                role_id: role_id.to_string(),
            }
        }
        SchemaGeneratorRefIr::SubtypeInclusion { semantic_key } => {
            ProjectedGeneratorRefV1::SubtypeInclusion {
                semantic_key: semantic_key.to_string(),
            }
        }
        SchemaGeneratorRefIr::Explicit { semantic_key } => ProjectedGeneratorRefV1::Explicit {
            semantic_key: semantic_key.to_string(),
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedValueV1 {
    ObjectElement { value: String },
    RelationFact { fact_id: String },
}

fn project_value(value: &TypedValueIr) -> ProjectedValueV1 {
    match value {
        TypedValueIr::ObjectElement { value } => ProjectedValueV1::ObjectElement {
            value: value.clone(),
        },
        TypedValueIr::RelationFact { fact_id } => ProjectedValueV1::RelationFact {
            fact_id: fact_id.to_string(),
        },
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionRecordEncodingV1 {
    NativeType,
    NativeRelation,
    NativeRole,
    RelationalRow,
    ReifiedNode,
    TypedEdge,
    NamedGraphResource,
    JsonDocument,
    SidecarObligation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectionRecordPayloadV1 {
    Module {
        module_name: String,
        revision: String,
    },
    Schema {
        schema_id: String,
        label: String,
    },
    ObjectType {
        object_type_id: String,
        label: String,
    },
    RelationObject {
        relation_id: String,
        label: String,
        arity: usize,
    },
    RoleProjection {
        relation_id: String,
        role_id: String,
        label: String,
        declared_order: u32,
        role_kind: String,
        type_expr: ProjectedTypeExprV1,
    },
    SchemaGenerator {
        generator: ProjectedGeneratorRefV1,
        label: String,
        source: ProjectedObjectRefV1,
        target: ProjectedObjectRefV1,
        generator_kind: String,
        reversible: bool,
    },
    Theory {
        theory_id: String,
        label: String,
    },
    ConstraintObligation {
        constraint_id: String,
        label: String,
        finite_model_checked: bool,
    },
    PathEquationObligation {
        equation_id: String,
        label: String,
        categorical_path_typed: bool,
    },
    RewriteObligation {
        rewrite_rule_id: String,
        label: String,
        reversible: bool,
    },
    InstanceModel {
        instance_id: String,
        label: String,
        finite_model_validated: bool,
    },
    CarrierElement {
        instance_id: String,
        carrier: ProjectedObjectRefV1,
        value: String,
    },
    FunctionMapping {
        instance_id: String,
        generator: ProjectedGeneratorRefV1,
        source: String,
        target: String,
    },
    RelationFact {
        instance_id: String,
        fact_id: String,
        relation_id: String,
        ordered_role_values: Vec<ProjectedRoleValueV1>,
    },
}

impl ProjectionRecordPayloadV1 {
    fn kind_label(&self) -> &'static str {
        match self {
            Self::Module { .. } => "module",
            Self::Schema { .. } => "schema",
            Self::ObjectType { .. } => "object_type",
            Self::RelationObject { .. } => "relation_object",
            Self::RoleProjection { .. } => "role_projection",
            Self::SchemaGenerator { .. } => "schema_generator",
            Self::Theory { .. } => "theory",
            Self::ConstraintObligation { .. } => "constraint_obligation",
            Self::PathEquationObligation { .. } => "path_equation_obligation",
            Self::RewriteObligation { .. } => "rewrite_obligation",
            Self::InstanceModel { .. } => "instance_model",
            Self::CarrierElement { .. } => "carrier_element",
            Self::FunctionMapping { .. } => "function_mapping",
            Self::RelationFact { .. } => "relation_fact",
        }
    }

    fn display_label(&self) -> String {
        match self {
            Self::Module { module_name, .. } => module_name.clone(),
            Self::Schema { label, .. }
            | Self::ObjectType { label, .. }
            | Self::RelationObject { label, .. }
            | Self::RoleProjection { label, .. }
            | Self::SchemaGenerator { label, .. }
            | Self::Theory { label, .. }
            | Self::ConstraintObligation { label, .. }
            | Self::PathEquationObligation { label, .. }
            | Self::RewriteObligation { label, .. }
            | Self::InstanceModel { label, .. } => label.clone(),
            Self::CarrierElement { value, .. } => value.clone(),
            Self::FunctionMapping { source, target, .. } => format!("{source}_to_{target}"),
            Self::RelationFact { fact_id, .. } => fact_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectedRoleValueV1 {
    pub role_id: String,
    pub value: ProjectedValueV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRecordV1 {
    pub record_id: ObjectBlobIdV2,
    pub source_ref: KernelRefV2,
    pub encoding: ProjectionRecordEncodingV1,
    pub native_key: String,
    pub payload_fingerprint: ObjectBlobIdV2,
    pub payload: ProjectionRecordPayloadV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticLossClassV1 {
    RepresentationChanged,
    NotEnforcedByBackend,
    NotRepresented,
    EvidenceAuthorityBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SemanticLossV1 {
    pub capability: ProjectionCapabilityV1,
    pub class: SemanticLossClassV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<KernelRefV2>,
    pub detail: String,
    pub consequence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemanticLossReportV1 {
    pub finite_scope: String,
    pub losses: Vec<SemanticLossV1>,
    pub representation_only_count: usize,
    pub unenforced_count: usize,
    pub unrepresented_count: usize,
    pub authority_boundary_count: usize,
    pub lossless_semantic_projection_claim: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCoverageReportV1 {
    pub kernel_refs_total: usize,
    pub kernel_refs_covered: usize,
    pub projection_records: usize,
    pub modules: usize,
    pub schemas: usize,
    pub object_types: usize,
    pub relation_objects: usize,
    pub role_projections: usize,
    pub theories: usize,
    pub constraints: usize,
    /// Forward category equations with a checked `SchemaEquationIr`; formal
    /// groupoid and opaque theory equations are not double-counted here.
    pub path_equations: usize,
    pub rewrite_rules: usize,
    pub instance_models: usize,
    pub relation_facts: usize,
    #[serde(default)]
    pub uncovered_kernel_refs: Vec<KernelRefV2>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeArtifactKindV1 {
    PathDbMaterializationPlan,
    TypeQlSchema,
    TerminusDbDocuments,
    RdfOwlDataset,
    PropertyGraphBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeProjectionArtifactV1 {
    pub kind: NativeArtifactKindV1,
    pub media_type: String,
    pub suggested_extension: String,
    pub read_only: bool,
    pub artifact_digest: ObjectBlobIdV2,
    pub body: String,
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectionManifestV1 {
    pub version: String,
    pub projection_id: ObjectBlobIdV2,
    pub anchor: ProjectionAnchorV1,
    pub backend: BackendCapabilityDeclarationV1,
    pub authority: ProjectionAuthorityV1,
    pub mutation_authority: ProjectionMutationAuthorityV1,
    pub records: Vec<ProjectionRecordV1>,
    pub artifact: NativeProjectionArtifactV1,
    pub coverage: ProjectionCoverageReportV1,
    pub semantic_loss: SemanticLossReportV1,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadbackRecordObservationV1 {
    pub record_id: ObjectBlobIdV2,
    pub payload_fingerprint: ObjectBlobIdV2,
    pub native_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadbackProvenanceV1 {
    pub adapter_id: String,
    pub adapter_version: String,
    pub observation_method: String,
    pub backend_locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadbackInventoryV1 {
    pub version: String,
    pub projection_id: ObjectBlobIdV2,
    pub backend: ProjectionBackendV1,
    pub provenance: ReadbackProvenanceV1,
    pub records: Vec<ReadbackRecordObservationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadbackDriftV1 {
    pub record_id: ObjectBlobIdV2,
    pub expected_payload_fingerprint: ObjectBlobIdV2,
    pub observed_payload_fingerprint: ObjectBlobIdV2,
    pub expected_native_key: String,
    pub observed_native_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadbackTransportStatusV1 {
    ExactFiniteRecordMatch,
    DriftDetected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExternalEvidenceEnvelopeV1 {
    pub version: String,
    pub authority: ExternalEvidenceAuthorityV1,
    pub source_projection_id: ObjectBlobIdV2,
    pub accepted_snapshot_anchor: SnapshotIdV2,
    pub backend: ProjectionBackendV1,
    pub provenance: ReadbackProvenanceV1,
    pub observations: Vec<ReadbackRecordObservationV1>,
    pub accepted_state_change: bool,
    pub requires_typed_proposal_review_and_promotion: bool,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadbackReportV1 {
    pub version: String,
    pub projection_id: ObjectBlobIdV2,
    pub backend: ProjectionBackendV1,
    pub transport_status: ReadbackTransportStatusV1,
    pub matched_records: usize,
    pub missing_record_ids: Vec<ObjectBlobIdV2>,
    pub drifted_records: Vec<ReadbackDriftV1>,
    pub unexpected_records: Vec<ReadbackRecordObservationV1>,
    pub semantic_loss: SemanticLossReportV1,
    pub evidence: ExternalEvidenceEnvelopeV1,
    pub semantic_equivalence_claim: bool,
    pub completeness_claim: bool,
    pub ontology_closure_claim: bool,
    pub non_claims: Vec<String>,
}

/// Declare exactly what a backend can represent for this projection contract.
///
/// A `Native` disposition means native storage/query shape for the finite
/// record fragment. It never means that the backend checks Lean obligations or
/// shares Axiograph mutation authority.
pub fn backend_capabilities_v1(backend: ProjectionBackendV1) -> BackendCapabilityDeclarationV1 {
    let (support_tier, native_read_surface) = match backend {
        ProjectionBackendV1::PathDb => (
            ProjectionSupportTierV1::Primary,
            "AxiStore-authenticated SQLite materialization hydrated as PathDB",
        ),
        ProjectionBackendV1::TypeDb => (
            ProjectionSupportTierV1::Primary,
            "TypeQL schema plus adapter-provided read queries",
        ),
        ProjectionBackendV1::TerminusDb => (
            ProjectionSupportTierV1::Supported,
            "JSON-LD documents plus adapter-provided WOQL/RDF reads",
        ),
        ProjectionBackendV1::RdfOwl => (
            ProjectionSupportTierV1::Supported,
            "TriG dataset plus adapter-provided SPARQL reads over reified relation resources",
        ),
        ProjectionBackendV1::PropertyGraph => (
            ProjectionSupportTierV1::Experimental,
            "portable node/edge bundle; backend-specific query dialects are adapters",
        ),
    };

    let capabilities = [
        ProjectionCapabilityV1::RelationObjects,
        ProjectionCapabilityV1::NaryRelations,
        ProjectionCapabilityV1::TypedRoleProjections,
        ProjectionCapabilityV1::SubtypeInclusions,
        ProjectionCapabilityV1::DependentIndexes,
        ProjectionCapabilityV1::RefinementPredicates,
        ProjectionCapabilityV1::ContextWorldAxes,
        ProjectionCapabilityV1::EvidenceProvenance,
        ProjectionCapabilityV1::Constraints,
        ProjectionCapabilityV1::PathEquations,
        ProjectionCapabilityV1::RewriteRules,
        ProjectionCapabilityV1::HigherPaths,
        ProjectionCapabilityV1::FiniteInstances,
        ProjectionCapabilityV1::NativeReadback,
    ]
    .into_iter()
    .map(|capability| CapabilityDeclarationV1 {
        capability,
        disposition: disposition(backend, capability),
        finite_scope: capability_scope(capability).to_string(),
        caveat: capability_caveat(backend, capability).to_string(),
    })
    .collect();

    BackendCapabilityDeclarationV1 {
        backend,
        support_tier,
        native_read_surface: native_read_surface.to_string(),
        authority: ProjectionAuthorityV1::DerivedOnly,
        mutation_authority: ProjectionMutationAuthorityV1::AxiographSemanticVcsOnly,
        capabilities,
    }
}

fn disposition(
    backend: ProjectionBackendV1,
    capability: ProjectionCapabilityV1,
) -> CapabilityDispositionV1 {
    use CapabilityDispositionV1::{Encoded, Native, Sidecar, Unsupported};
    use ProjectionBackendV1::{PathDb, PropertyGraph, RdfOwl, TerminusDb, TypeDb};
    use ProjectionCapabilityV1::*;
    match (backend, capability) {
        (_, HigherPaths) => Unsupported,
        (PathDb | TypeDb, FiniteInstances) => Sidecar,
        (TerminusDb | RdfOwl | PropertyGraph, FiniteInstances) => Encoded,
        (_, NativeReadback) => Encoded,
        (TypeDb, RelationObjects | NaryRelations | TypedRoleProjections) => Native,
        (TerminusDb | RdfOwl, RelationObjects | NaryRelations | TypedRoleProjections) => Encoded,
        (PathDb, RelationObjects | NaryRelations | TypedRoleProjections | SubtypeInclusions) => {
            Encoded
        }
        (PropertyGraph, RelationObjects | NaryRelations | TypedRoleProjections) => Encoded,
        (
            _,
            DependentIndexes | RefinementPredicates | Constraints | PathEquations | RewriteRules,
        ) => Sidecar,
        (_, EvidenceProvenance) => Encoded,
        (_, ContextWorldAxes | SubtypeInclusions) => Encoded,
    }
}

fn capability_scope(capability: ProjectionCapabilityV1) -> &'static str {
    use ProjectionCapabilityV1::*;
    match capability {
        RelationObjects => "finite compiled relation objects and their stable ids",
        NaryRelations => "finite ordered role tuples of arbitrary compiled arity",
        TypedRoleProjections => "compiled role order, kind, target carrier, and type expression",
        SubtypeInclusions => "finite compiled subtype generators and coherence-path inventory",
        DependentIndexes => "explicit finite role-index metadata only",
        RefinementPredicates => "compiled decidable refinement syntax carried as metadata",
        ContextWorldAxes => "explicit context/world/temporal role axes in finite tuples",
        EvidenceProvenance => {
            "typed evidence/provenance links whose projected copies and readback have no accepted authority"
        }
        Constraints => "finite compiled constraint obligations and checked-bit observations",
        PathEquations => "typed finite schema paths and equation obligations",
        RewriteRules => "finite compiled rewrite obligations and reversibility flags",
        HigherPaths => "no general higher-inductive, univalence, or arbitrary homotopy fragment",
        FiniteInstances => "compiled finite carriers, function mappings, and relation facts",
        NativeReadback => "record-id and payload-fingerprint comparison for one anchored manifest",
    }
}

fn capability_caveat(
    backend: ProjectionBackendV1,
    capability: ProjectionCapabilityV1,
) -> &'static str {
    use ProjectionCapabilityV1::*;
    match capability {
        HigherPaths => "unsupported: no backend projection establishes higher-path equality or univalence",
        DependentIndexes | RefinementPredicates => {
            "metadata transport is not backend enforcement of dependent or refinement typing"
        }
        Constraints | PathEquations | RewriteRules => {
            "obligations remain sidecar records; only the trusted checker may discharge its supported fragment"
        }
        EvidenceProvenance => {
            "readback and external provenance remain evidence-plane inputs until typed review and promotion"
        }
        FiniteInstances if matches!(backend, ProjectionBackendV1::PathDb | ProjectionBackendV1::TypeDb) => {
            "finite instances remain manifest records until an authenticated PathDB publisher or backend adapter loads them"
        }
        FiniteInstances => {
            "finite instances use a backend-specific encoded document, resource, node, or edge representation"
        }
        NativeReadback => {
            "an exact finite record match is transport equality, not semantic equivalence or completeness"
        }
        ContextWorldAxes if backend == ProjectionBackendV1::PropertyGraph => {
            "axes are explicit properties/edges, not native possible-world semantics"
        }
        _ => "preservation claim is limited to the declared finite encoding",
    }
}

/// Project one immutable compiled snapshot into one backend-specific, read-only
/// manifest. The output is deterministic for the same snapshot and backend.
pub fn project_snapshot_v1(
    snapshot: &CompiledKernelSnapshot,
    backend: ProjectionBackendV1,
) -> Result<ProjectionManifestV1, ProjectionError> {
    let ir = snapshot.ir();
    let declaration = backend_capabilities_v1(backend);
    let mut records = Vec::new();

    for module in ir.ordered_module_closure() {
        push_record(
            &mut records,
            backend,
            KernelRefV2::Module {
                module_id: module.module_id.clone(),
                revision: module.revision.clone(),
            },
            ProjectionRecordPayloadV1::Module {
                module_name: module.module_name.clone(),
                revision: module.revision.to_string(),
            },
        )?;
    }

    for schema in ir.schemas() {
        let schema_ref = schema_ref(
            &schema.module_id,
            &schema.revision,
            &schema.semantic_key,
            &schema.schema_id,
        );
        push_record(
            &mut records,
            backend,
            schema_ref.clone(),
            ProjectionRecordPayloadV1::Schema {
                schema_id: schema.schema_id.to_string(),
                label: schema.label.clone(),
            },
        )?;
        for object in &schema.objects {
            push_record(
                &mut records,
                backend,
                KernelRefV2::ObjectType {
                    module_id: schema.module_id.clone(),
                    revision: schema.revision.clone(),
                    schema_id: schema.schema_id.clone(),
                    semantic_key: object.semantic_key.clone(),
                    object_type_id: object.object_type_id.clone(),
                },
                ProjectionRecordPayloadV1::ObjectType {
                    object_type_id: object.object_type_id.to_string(),
                    label: object.label.clone(),
                },
            )?;
        }
        for relation in &schema.relations {
            let relation_ref = KernelRefV2::Relation {
                module_id: schema.module_id.clone(),
                revision: schema.revision.clone(),
                schema_id: schema.schema_id.clone(),
                semantic_key: relation.semantic_key.clone(),
                relation_id: relation.relation_id.clone(),
            };
            push_record(
                &mut records,
                backend,
                relation_ref,
                ProjectionRecordPayloadV1::RelationObject {
                    relation_id: relation.relation_id.to_string(),
                    label: relation.label.clone(),
                    arity: relation.roles.len(),
                },
            )?;
            for role in &relation.roles {
                push_record(
                    &mut records,
                    backend,
                    KernelRefV2::Role {
                        module_id: schema.module_id.clone(),
                        revision: schema.revision.clone(),
                        schema_id: schema.schema_id.clone(),
                        relation_id: relation.relation_id.clone(),
                        semantic_key: role.semantic_key.clone(),
                        role_id: role.role_id.clone(),
                    },
                    ProjectionRecordPayloadV1::RoleProjection {
                        relation_id: relation.relation_id.to_string(),
                        role_id: role.role_id.to_string(),
                        label: role.label.clone(),
                        declared_order: role.declared_order,
                        role_kind: role_kind_label(role.kind).to_string(),
                        type_expr: project_type_expr_v1(&role.type_expr),
                    },
                )?;
            }
        }
        for generator in &schema.generators {
            push_record(
                &mut records,
                backend,
                KernelRefV2::Generator {
                    module_id: schema.module_id.clone(),
                    revision: schema.revision.clone(),
                    schema_id: schema.schema_id.clone(),
                    semantic_key: generator.semantic_key.clone(),
                },
                ProjectionRecordPayloadV1::SchemaGenerator {
                    generator: project_generator_ref(&generator.generator_ref),
                    label: generator.label.clone(),
                    source: project_object_ref(&generator.source),
                    target: project_object_ref(&generator.target),
                    generator_kind: generator_kind_label(generator.kind).to_string(),
                    reversible: generator.reversible,
                },
            )?;
        }
    }

    for theory in ir.theories() {
        let theory_ref = KernelRefV2::Theory {
            module_id: theory.module_id.clone(),
            revision: theory.revision.clone(),
            schema_id: theory.schema_id.clone(),
            semantic_key: theory.semantic_key.clone(),
            theory_id: theory.theory_id.clone(),
        };
        push_record(
            &mut records,
            backend,
            theory_ref,
            ProjectionRecordPayloadV1::Theory {
                theory_id: theory.theory_id.to_string(),
                label: theory.label.clone(),
            },
        )?;
        for constraint in &theory.constraints {
            push_record(
                &mut records,
                backend,
                constraint_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &constraint.semantic_key,
                    &constraint.constraint_id,
                ),
                ProjectionRecordPayloadV1::ConstraintObligation {
                    constraint_id: constraint.constraint_id.to_string(),
                    label: constraint.label.clone(),
                    finite_model_checked: constraint.finite_model_checked,
                },
            )?;
        }
        for equation in &theory.equations {
            push_record(
                &mut records,
                backend,
                equation_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &equation.semantic_key,
                    &equation.equation_id,
                ),
                ProjectionRecordPayloadV1::PathEquationObligation {
                    equation_id: equation.equation_id.to_string(),
                    label: equation.label.clone(),
                    categorical_path_typed: equation.schema_equation.is_some(),
                },
            )?;
        }
        for rule in &theory.rewrite_rules {
            push_record(
                &mut records,
                backend,
                rewrite_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &rule.semantic_key,
                    &rule.rewrite_rule_id,
                ),
                ProjectionRecordPayloadV1::RewriteObligation {
                    rewrite_rule_id: rule.rewrite_rule_id.to_string(),
                    label: rule.label.clone(),
                    reversible: rule.reversible,
                },
            )?;
        }
    }

    for instance in ir.instances() {
        let instance_ref = KernelRefV2::Instance {
            module_id: instance.module_id.clone(),
            revision: instance.revision.clone(),
            schema_id: instance.schema_id.clone(),
            semantic_key: instance.semantic_key.clone(),
            instance_id: instance.instance_id.clone(),
        };
        let finite_model_validated = instance.validation.role_projections_total_and_single_valued
            && instance.validation.outputs_in_target_carriers
            && instance.validation.subtype_maps_injective
            && instance.validation.stable_fact_ids_recomputed
            && instance.validation.supported_equations_hold_pointwise
            && instance.validation.supported_constraints_hold;
        push_record(
            &mut records,
            backend,
            instance_ref.clone(),
            ProjectionRecordPayloadV1::InstanceModel {
                instance_id: instance.instance_id.to_string(),
                label: instance.label.clone(),
                finite_model_validated,
            },
        )?;
        for carrier in &instance.carriers {
            for element in &carrier.elements {
                push_record(
                    &mut records,
                    backend,
                    instance_ref.clone(),
                    ProjectionRecordPayloadV1::CarrierElement {
                        instance_id: instance.instance_id.to_string(),
                        carrier: project_object_ref(&carrier.object),
                        value: element.clone(),
                    },
                )?;
            }
        }
        for function in &instance.functions {
            for mapping in &function.mappings {
                push_record(
                    &mut records,
                    backend,
                    instance_ref.clone(),
                    ProjectionRecordPayloadV1::FunctionMapping {
                        instance_id: instance.instance_id.to_string(),
                        generator: project_generator_ref(&function.generator),
                        source: mapping.source.clone(),
                        target: mapping.target.clone(),
                    },
                )?;
            }
        }
        for fact in &instance.facts {
            push_record(
                &mut records,
                backend,
                KernelRefV2::Fact {
                    module_id: instance.module_id.clone(),
                    revision: instance.revision.clone(),
                    schema_id: instance.schema_id.clone(),
                    instance_id: instance.instance_id.clone(),
                    relation_id: fact.relation_id.clone(),
                    fact_id: fact.fact_id.clone(),
                },
                ProjectionRecordPayloadV1::RelationFact {
                    instance_id: instance.instance_id.to_string(),
                    fact_id: fact.fact_id.to_string(),
                    relation_id: fact.relation_id.to_string(),
                    ordered_role_values: fact
                        .ordered_role_values
                        .iter()
                        .map(|role_value| ProjectedRoleValueV1 {
                            role_id: role_value.role_id.to_string(),
                            value: project_value(&role_value.value),
                        })
                        .collect(),
                },
            )?;
        }
    }

    records.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    let mut seen = BTreeSet::new();
    for record in &records {
        if !seen.insert(record.record_id.clone()) {
            return Err(ProjectionError::DuplicateProjectionRecord(
                record.record_id.to_string(),
            ));
        }
    }

    let anchor = ProjectionAnchorV1 {
        repository_id: ir.repository_id().clone(),
        accepted_snapshot_id: ir.accepted_snapshot_id().clone(),
        kernel_ir_digest: ir.ir_digest().clone(),
    };
    let coverage = coverage_report(snapshot, &records);
    let semantic_loss = semantic_loss_report(snapshot, backend);
    let artifact = build_artifact(backend, &records)?;
    let non_claims = vec![
        "this manifest is a derived finite transport artifact, not accepted ontology authority"
            .to_string(),
        "Rust projection generation is untrusted; the VerifyMain import closure remains the trusted checker"
            .to_string(),
        "record coverage does not establish semantic completeness, categorical equivalence, ontology closure, univalence, or general HoTT semantics"
            .to_string(),
        "backend-native writes are drift observations and cannot promote, supersede, retract, or merge accepted .axi state"
            .to_string(),
    ];
    let projection_id = projection_id_from_parts(
        &anchor,
        &declaration,
        &records,
        &artifact,
        &coverage,
        &semantic_loss,
        &non_claims,
    )?;

    Ok(ProjectionManifestV1 {
        version: PROJECTION_MANIFEST_VERSION_V1.to_string(),
        projection_id,
        anchor,
        backend: declaration,
        authority: ProjectionAuthorityV1::DerivedOnly,
        mutation_authority: ProjectionMutationAuthorityV1::AxiographSemanticVcsOnly,
        records,
        artifact,
        coverage,
        semantic_loss,
        non_claims,
    })
}

fn projection_id_from_parts(
    anchor: &ProjectionAnchorV1,
    backend: &BackendCapabilityDeclarationV1,
    records: &[ProjectionRecordV1],
    artifact: &NativeProjectionArtifactV1,
    coverage: &ProjectionCoverageReportV1,
    semantic_loss: &SemanticLossReportV1,
    non_claims: &[String],
) -> Result<ObjectBlobIdV2, ProjectionError> {
    let projection_bytes = serde_json::to_vec(&(
        PROJECTION_MANIFEST_VERSION_V1,
        anchor,
        backend,
        records,
        artifact,
        coverage,
        semantic_loss,
        non_claims,
    ))
    .map_err(|error| ProjectionError::Serialization(error.to_string()))?;
    Ok(ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_projection_manifest_v1",
        backend.backend.as_str().as_bytes(),
        &projection_bytes,
    ]))
}

/// Build a deterministic inventory fixture from a manifest.
///
/// This is test/demo scaffolding, not a backend observation. Production
/// adapters must replace the provenance and records with values actually read
/// from the backend.
pub fn manifest_readback_fixture_v1(manifest: &ProjectionManifestV1) -> ReadbackInventoryV1 {
    ReadbackInventoryV1 {
        version: READBACK_INVENTORY_VERSION_V1.to_string(),
        projection_id: manifest.projection_id.clone(),
        backend: manifest.backend.backend,
        provenance: ReadbackProvenanceV1 {
            adapter_id: "axiograph_manifest_fixture".to_string(),
            adapter_version: env!("CARGO_PKG_VERSION").to_string(),
            observation_method: "manifest_copy_not_backend_observation".to_string(),
            backend_locator: "in_memory_test_fixture".to_string(),
        },
        records: manifest
            .records
            .iter()
            .map(|record| ReadbackRecordObservationV1 {
                record_id: record.record_id.clone(),
                payload_fingerprint: record.payload_fingerprint.clone(),
                native_key: record.native_key.clone(),
            })
            .collect(),
    }
}

/// Compare backend readback with one anchored projection manifest.
///
/// Equality here is deliberately narrow: record ids and payload fingerprints
/// for the finite manifest. Even an exact match remains evidence-only and does
/// not establish semantic equivalence or modify accepted state.
pub fn check_readback_v1(
    manifest: &ProjectionManifestV1,
    inventory: &ReadbackInventoryV1,
) -> Result<ReadbackReportV1, ProjectionError> {
    if manifest.version != PROJECTION_MANIFEST_VERSION_V1 {
        return Err(ProjectionError::UnsupportedManifestVersion(
            manifest.version.clone(),
        ));
    }
    let committed_projection_id = projection_id_from_parts(
        &manifest.anchor,
        &manifest.backend,
        &manifest.records,
        &manifest.artifact,
        &manifest.coverage,
        &manifest.semantic_loss,
        &manifest.non_claims,
    )?;
    if committed_projection_id != manifest.projection_id {
        return Err(ProjectionError::ManifestIntegrityMismatch);
    }
    if inventory.version != READBACK_INVENTORY_VERSION_V1 {
        return Err(ProjectionError::UnsupportedReadbackVersion(
            inventory.version.clone(),
        ));
    }
    validate_readback_provenance(&inventory.provenance)?;
    if inventory.projection_id != manifest.projection_id {
        return Err(ProjectionError::ProjectionIdMismatch);
    }
    if inventory.backend != manifest.backend.backend {
        return Err(ProjectionError::BackendMismatch {
            expected: manifest.backend.backend.to_string(),
            observed: inventory.backend.to_string(),
        });
    }

    let expected = manifest
        .records
        .iter()
        .map(|record| (record.record_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut observed = BTreeMap::new();
    for record in &inventory.records {
        if observed
            .insert(record.record_id.clone(), record.clone())
            .is_some()
        {
            return Err(ProjectionError::DuplicateReadbackRecord(
                record.record_id.to_string(),
            ));
        }
    }

    let mut matched_records = 0;
    let mut missing_record_ids = Vec::new();
    let mut drifted_records = Vec::new();
    for (record_id, expected_record) in &expected {
        match observed.get(record_id) {
            None => missing_record_ids.push(record_id.clone()),
            Some(actual)
                if actual.payload_fingerprint == expected_record.payload_fingerprint
                    && actual.native_key == expected_record.native_key =>
            {
                matched_records += 1;
            }
            Some(actual) => drifted_records.push(ReadbackDriftV1 {
                record_id: record_id.clone(),
                expected_payload_fingerprint: expected_record.payload_fingerprint.clone(),
                observed_payload_fingerprint: actual.payload_fingerprint.clone(),
                expected_native_key: expected_record.native_key.clone(),
                observed_native_key: actual.native_key.clone(),
            }),
        }
    }
    let unexpected_records = observed
        .iter()
        .filter(|(record_id, _)| !expected.contains_key(*record_id))
        .map(|(_, record)| record.clone())
        .collect::<Vec<_>>();
    let transport_status = if missing_record_ids.is_empty()
        && drifted_records.is_empty()
        && unexpected_records.is_empty()
    {
        ReadbackTransportStatusV1::ExactFiniteRecordMatch
    } else {
        ReadbackTransportStatusV1::DriftDetected
    };
    let observations = inventory.records.clone();
    let evidence = ExternalEvidenceEnvelopeV1 {
        version: EVIDENCE_ENVELOPE_VERSION_V1.to_string(),
        authority: ExternalEvidenceAuthorityV1::EvidenceOnly,
        source_projection_id: manifest.projection_id.clone(),
        accepted_snapshot_anchor: manifest.anchor.accepted_snapshot_id.clone(),
        backend: manifest.backend.backend,
        provenance: inventory.provenance.clone(),
        observations,
        accepted_state_change: false,
        requires_typed_proposal_review_and_promotion: true,
        non_claims: vec![
            "backend readback is an external observation, not accepted .axi".to_string(),
            "an exact record match does not discharge constraints, equations, rewrite rules, refinements, or higher paths"
                .to_string(),
        ],
    };

    Ok(ReadbackReportV1 {
        version: READBACK_REPORT_VERSION_V1.to_string(),
        projection_id: manifest.projection_id.clone(),
        backend: manifest.backend.backend,
        transport_status,
        matched_records,
        missing_record_ids,
        drifted_records,
        unexpected_records,
        semantic_loss: manifest.semantic_loss.clone(),
        evidence,
        semantic_equivalence_claim: false,
        completeness_claim: false,
        ontology_closure_claim: false,
        non_claims: vec![
            "readback checks finite transport identity only".to_string(),
            "neither successful readback nor native query execution is a Lean certificate"
                .to_string(),
        ],
    })
}

fn validate_readback_provenance(provenance: &ReadbackProvenanceV1) -> Result<(), ProjectionError> {
    for (field, value) in [
        ("adapter_id", provenance.adapter_id.as_str()),
        ("adapter_version", provenance.adapter_version.as_str()),
        ("observation_method", provenance.observation_method.as_str()),
        ("backend_locator", provenance.backend_locator.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ProjectionError::InvalidReadbackProvenance(format!(
                "{field} must be non-empty"
            )));
        }
    }
    Ok(())
}

fn push_record(
    records: &mut Vec<ProjectionRecordV1>,
    backend: ProjectionBackendV1,
    source_ref: KernelRefV2,
    payload: ProjectionRecordPayloadV1,
) -> Result<(), ProjectionError> {
    let source_bytes = serde_json::to_vec(&source_ref)
        .map_err(|error| ProjectionError::Serialization(error.to_string()))?;
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|error| ProjectionError::Serialization(error.to_string()))?;
    let kind = payload.kind_label();
    let payload_fingerprint = ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_projection_record_payload_v1",
        kind.as_bytes(),
        &payload_bytes,
    ]);
    let record_id = ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_projection_record_v1",
        backend.as_str().as_bytes(),
        kind.as_bytes(),
        &source_bytes,
        &payload_bytes,
    ]);
    let native_key = format!(
        "{}__{}",
        slug(&payload.display_label()),
        short_id(&record_id.to_string())
    );
    records.push(ProjectionRecordV1 {
        record_id,
        source_ref,
        encoding: record_encoding(backend, &payload),
        native_key,
        payload_fingerprint,
        payload,
    });
    Ok(())
}

fn record_encoding(
    backend: ProjectionBackendV1,
    payload: &ProjectionRecordPayloadV1,
) -> ProjectionRecordEncodingV1 {
    use ProjectionBackendV1::*;
    use ProjectionRecordEncodingV1::*;
    use ProjectionRecordPayloadV1::*;
    match payload {
        ConstraintObligation { .. } | PathEquationObligation { .. } | RewriteObligation { .. } => {
            SidecarObligation
        }
        ObjectType { .. } | Schema { .. } | Module { .. } | Theory { .. } if backend == TypeDb => {
            NativeType
        }
        RelationObject { .. } if backend == TypeDb => NativeRelation,
        RoleProjection { .. } if backend == TypeDb => NativeRole,
        RelationFact { .. } | CarrierElement { .. } | FunctionMapping { .. }
            if backend == PathDb =>
        {
            RelationalRow
        }
        RelationObject { .. } | RelationFact { .. } if matches!(backend, RdfOwl | TerminusDb) => {
            NamedGraphResource
        }
        RoleProjection { .. } if matches!(backend, RdfOwl | TerminusDb) => TypedEdge,
        RelationObject { .. } | RelationFact { .. } if backend == PropertyGraph => ReifiedNode,
        RoleProjection { .. } | FunctionMapping { .. } if backend == PropertyGraph => TypedEdge,
        _ => JsonDocument,
    }
}

fn coverage_report(
    snapshot: &CompiledKernelSnapshot,
    records: &[ProjectionRecordV1],
) -> ProjectionCoverageReportV1 {
    let ir = snapshot.ir();
    let covered = records
        .iter()
        .map(|record| record.source_ref.clone())
        .collect::<BTreeSet<_>>();
    let uncovered_kernel_refs = ir
        .refs()
        .iter()
        .filter(|reference| !covered.contains(*reference))
        .cloned()
        .collect::<Vec<_>>();
    ProjectionCoverageReportV1 {
        kernel_refs_total: ir.refs().len(),
        kernel_refs_covered: ir.refs().len() - uncovered_kernel_refs.len(),
        projection_records: records.len(),
        modules: ir.ordered_module_closure().len(),
        schemas: ir.schemas().len(),
        object_types: ir.schemas().iter().map(|schema| schema.objects.len()).sum(),
        relation_objects: ir
            .schemas()
            .iter()
            .map(|schema| schema.relations.len())
            .sum(),
        role_projections: ir
            .schemas()
            .iter()
            .flat_map(|schema| &schema.relations)
            .map(|relation| relation.roles.len())
            .sum(),
        theories: ir.theories().len(),
        constraints: ir
            .theories()
            .iter()
            .map(|theory| theory.constraints.len())
            .sum(),
        path_equations: ir
            .theories()
            .iter()
            .flat_map(|theory| &theory.equations)
            .filter(|equation| equation.schema_equation.is_some())
            .count(),
        rewrite_rules: ir
            .theories()
            .iter()
            .map(|theory| theory.rewrite_rules.len())
            .sum(),
        instance_models: ir.instances().len(),
        relation_facts: ir
            .instances()
            .iter()
            .map(|instance| instance.facts.len())
            .sum(),
        uncovered_kernel_refs,
    }
}

fn semantic_loss_report(
    snapshot: &CompiledKernelSnapshot,
    backend: ProjectionBackendV1,
) -> SemanticLossReportV1 {
    let ir = snapshot.ir();
    let capability = |wanted| disposition(backend, wanted);
    let mut losses = Vec::new();

    for schema in ir.schemas() {
        for relation in &schema.relations {
            let relation_ref = KernelRefV2::Relation {
                module_id: schema.module_id.clone(),
                revision: schema.revision.clone(),
                schema_id: schema.schema_id.clone(),
                semantic_key: relation.semantic_key.clone(),
                relation_id: relation.relation_id.clone(),
            };
            if capability(ProjectionCapabilityV1::RelationObjects)
                != CapabilityDispositionV1::Native
            {
                losses.push(SemanticLossV1 {
                    capability: ProjectionCapabilityV1::RelationObjects,
                    class: SemanticLossClassV1::RepresentationChanged,
                    source_ref: Some(relation_ref.clone()),
                    detail: "canonical relation object is encoded as a row, reified resource, or node"
                        .to_string(),
                    consequence: "readback must reconstruct the relation object and ordered role tuple through this manifest"
                        .to_string(),
                });
            }
            if relation.roles.len() > 2
                && capability(ProjectionCapabilityV1::NaryRelations)
                    != CapabilityDispositionV1::Native
            {
                losses.push(SemanticLossV1 {
                    capability: ProjectionCapabilityV1::NaryRelations,
                    class: SemanticLossClassV1::RepresentationChanged,
                    source_ref: Some(relation_ref),
                    detail: "n-ary relation uses an explicit tuple/reification encoding"
                        .to_string(),
                    consequence: "binary native edges alone are not a lossless read surface"
                        .to_string(),
                });
            }
            for role in &relation.roles {
                let role_ref = KernelRefV2::Role {
                    module_id: schema.module_id.clone(),
                    revision: schema.revision.clone(),
                    schema_id: schema.schema_id.clone(),
                    relation_id: relation.relation_id.clone(),
                    semantic_key: role.semantic_key.clone(),
                    role_id: role.role_id.clone(),
                };
                if capability(ProjectionCapabilityV1::TypedRoleProjections)
                    != CapabilityDispositionV1::Native
                {
                    losses.push(SemanticLossV1 {
                        capability: ProjectionCapabilityV1::TypedRoleProjections,
                        class: SemanticLossClassV1::RepresentationChanged,
                        source_ref: Some(role_ref.clone()),
                        detail: "typed role target/order/kind uses an explicit projection record"
                            .to_string(),
                        consequence: "native edge labels or properties are insufficient without the manifest role record"
                            .to_string(),
                    });
                }
                let mut features = BTreeSet::new();
                type_expr_features(&role.type_expr, &mut features);
                if features.contains(&ProjectionCapabilityV1::DependentIndexes) {
                    losses.push(unenforced_loss(
                        ProjectionCapabilityV1::DependentIndexes,
                        role_ref.clone(),
                        "dependent index metadata is transported but not enforced by the backend",
                    ));
                }
                if features.contains(&ProjectionCapabilityV1::RefinementPredicates) {
                    losses.push(unenforced_loss(
                        ProjectionCapabilityV1::RefinementPredicates,
                        role_ref.clone(),
                        "refinement predicates are transported but not proved by the backend",
                    ));
                }
                if matches!(
                    role.kind,
                    RoleKindIr::Context | RoleKindIr::World | RoleKindIr::Temporal
                ) && capability(ProjectionCapabilityV1::ContextWorldAxes)
                    != CapabilityDispositionV1::Native
                {
                    losses.push(SemanticLossV1 {
                        capability: ProjectionCapabilityV1::ContextWorldAxes,
                        class: SemanticLossClassV1::RepresentationChanged,
                        source_ref: Some(role_ref.clone()),
                        detail: "context/world/temporal axis uses an explicit encoded role".to_string(),
                        consequence: "backend navigation is not itself Kripke, modal, or possible-world semantics"
                            .to_string(),
                    });
                }
                if role.kind == RoleKindIr::Evidence {
                    losses.push(SemanticLossV1 {
                        capability: ProjectionCapabilityV1::EvidenceProvenance,
                        class: SemanticLossClassV1::EvidenceAuthorityBoundary,
                        source_ref: Some(role_ref),
                        detail: "projected evidence/provenance role is readable, but its backend copy and readback are not accepted authority"
                            .to_string(),
                        consequence: "the accepted source remains canonical; new backend evidence requires typed proposal, review, CQ/trust gates, and promotion"
                            .to_string(),
                    });
                }
            }
        }
        for generator in &schema.generators {
            if generator.kind == SchemaGeneratorKindIr::SubtypeInclusion
                && capability(ProjectionCapabilityV1::SubtypeInclusions)
                    != CapabilityDispositionV1::Native
            {
                losses.push(SemanticLossV1 {
                    capability: ProjectionCapabilityV1::SubtypeInclusions,
                    class: SemanticLossClassV1::RepresentationChanged,
                    source_ref: Some(schema_ref(
                        &schema.module_id,
                        &schema.revision,
                        &schema.semantic_key,
                        &schema.schema_id,
                    )),
                    detail: format!(
                        "subtype inclusion `{}` is retained as an explicit generator record",
                        generator.label
                    ),
                    consequence:
                        "backend-native inheritance must not replace compiled subtype coherence"
                            .to_string(),
                });
            }
            if generator.reversible {
                losses.push(SemanticLossV1 {
                    capability: ProjectionCapabilityV1::HigherPaths,
                    class: SemanticLossClassV1::NotRepresented,
                    source_ref: Some(schema_ref(
                        &schema.module_id,
                        &schema.revision,
                        &schema.semantic_key,
                        &schema.schema_id,
                    )),
                    detail: format!(
                        "reversible generator `{}` is carried as a flag, not a checked inverse or higher path",
                        generator.label
                    ),
                    consequence: "no groupoid law, homotopy, univalence, or higher coherence follows from projection"
                        .to_string(),
                });
            }
        }
    }

    for instance in ir.instances() {
        let instance_disposition = capability(ProjectionCapabilityV1::FiniteInstances);
        if instance_disposition != CapabilityDispositionV1::Native {
            let sidecar_only = matches!(
                instance_disposition,
                CapabilityDispositionV1::Sidecar | CapabilityDispositionV1::Unsupported
            );
            losses.push(SemanticLossV1 {
                capability: ProjectionCapabilityV1::FiniteInstances,
                class: if sidecar_only {
                    SemanticLossClassV1::NotRepresented
                } else {
                    SemanticLossClassV1::RepresentationChanged
                },
                source_ref: Some(KernelRefV2::Instance {
                    module_id: instance.module_id.clone(),
                    revision: instance.revision.clone(),
                    schema_id: instance.schema_id.clone(),
                    semantic_key: instance.semantic_key.clone(),
                    instance_id: instance.instance_id.clone(),
                }),
                detail: if sidecar_only {
                    "finite instance records are present in the manifest but absent from the current native artifact"
                        .to_string()
                } else {
                    "finite instance carriers, functions, and facts use the backend-specific encoded representation"
                        .to_string()
                },
                consequence: "an adapter must materialize and read back every declared record before finite transport equality can be reported"
                    .to_string(),
            });
        }
    }

    for theory in ir.theories() {
        for constraint in &theory.constraints {
            losses.push(unenforced_loss(
                ProjectionCapabilityV1::Constraints,
                constraint_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &constraint.semantic_key,
                    &constraint.constraint_id,
                ),
                "constraint obligation is a sidecar record, not a backend-enforced theorem",
            ));
        }
        for equation in &theory.equations {
            losses.push(unenforced_loss(
                ProjectionCapabilityV1::PathEquations,
                equation_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &equation.semantic_key,
                    &equation.equation_id,
                ),
                "path equation is a sidecar obligation, not backend path equality",
            ));
        }
        for rule in &theory.rewrite_rules {
            losses.push(unenforced_loss(
                ProjectionCapabilityV1::RewriteRules,
                rewrite_ref(
                    &theory.module_id,
                    &theory.revision,
                    &theory.theory_id,
                    &rule.semantic_key,
                    &rule.rewrite_rule_id,
                ),
                "rewrite rule is transported as an obligation and is not executed as trusted backend semantics",
            ));
        }
    }

    losses.sort();
    losses.dedup();
    let representation_only_count = losses
        .iter()
        .filter(|loss| loss.class == SemanticLossClassV1::RepresentationChanged)
        .count();
    let unenforced_count = losses
        .iter()
        .filter(|loss| loss.class == SemanticLossClassV1::NotEnforcedByBackend)
        .count();
    let unrepresented_count = losses
        .iter()
        .filter(|loss| loss.class == SemanticLossClassV1::NotRepresented)
        .count();
    let authority_boundary_count = losses
        .iter()
        .filter(|loss| loss.class == SemanticLossClassV1::EvidenceAuthorityBoundary)
        .count();
    SemanticLossReportV1 {
        finite_scope: "one immutable compiled KernelSnapshotIr; exact declared records only"
            .to_string(),
        losses,
        representation_only_count,
        unenforced_count,
        unrepresented_count,
        authority_boundary_count,
        lossless_semantic_projection_claim: false,
    }
}

fn unenforced_loss(
    capability: ProjectionCapabilityV1,
    source_ref: KernelRefV2,
    detail: &str,
) -> SemanticLossV1 {
    SemanticLossV1 {
        capability,
        class: SemanticLossClassV1::NotEnforcedByBackend,
        source_ref: Some(source_ref),
        detail: detail.to_string(),
        consequence: "retain the obligation under its kernel ref and use the trusted checker for supported proof claims"
            .to_string(),
    }
}

fn type_expr_features(type_expr: &TypeExprIr, out: &mut BTreeSet<ProjectionCapabilityV1>) {
    match type_expr {
        TypeExprIr::Object { .. } | TypeExprIr::RelationObject { .. } => {}
        TypeExprIr::Indexed { base, .. } => {
            out.insert(ProjectionCapabilityV1::DependentIndexes);
            type_expr_features(base, out);
        }
        TypeExprIr::Refined { base, .. } => {
            out.insert(ProjectionCapabilityV1::RefinementPredicates);
            type_expr_features(base, out);
        }
    }
}

fn build_artifact(
    backend: ProjectionBackendV1,
    records: &[ProjectionRecordV1],
) -> Result<NativeProjectionArtifactV1, ProjectionError> {
    let (kind, media_type, suggested_extension, body, caveats) = match backend {
        ProjectionBackendV1::PathDb => (
            NativeArtifactKindV1::PathDbMaterializationPlan,
            "application/vnd.axiograph.pathdb-materialization-plan+json",
            "pathdb-plan.json",
            render_json_bundle(backend, records)?,
            vec![
                "this is a typed materialization plan; only AxiStore may publish the durable SQLite .axpd artifact"
                    .to_string(),
                "bare PathDB files and reverse export are not authority surfaces".to_string(),
            ],
        ),
        ProjectionBackendV1::TypeDb => (
            NativeArtifactKindV1::TypeQlSchema,
            "application/vnd.typedb.typeql",
            "tql",
            render_typeql(records),
            vec![
                "TypeQL is a readable derived schema; writes in TypeDB are drift, not accepted .axi mutations"
                    .to_string(),
                "the native artifact is schema text; an adapter must load manifest instance records and emit a readback inventory"
                    .to_string(),
                "sidecar obligation records are present in the manifest and are not enforced by TypeDB"
                    .to_string(),
            ],
        ),
        ProjectionBackendV1::TerminusDb => (
            NativeArtifactKindV1::TerminusDbDocuments,
            "application/ld+json",
            "terminus.jsonld",
            render_json_bundle(backend, records)?,
            vec![
                "TerminusDB branch/history features may mirror projection history but do not own semantic VCS"
                    .to_string(),
                "relation objects are reified documents with explicit role records".to_string(),
            ],
        ),
        ProjectionBackendV1::RdfOwl => (
            NativeArtifactKindV1::RdfOwlDataset,
            "application/trig",
            "trig",
            render_rdf_owl(records)?,
            vec![
                "OWL/RDF terms are a projection of finite kernel records, not the ontology kernel"
                    .to_string(),
                "constraints, refinements, rewrites, and higher paths remain explicit sidecar losses"
                    .to_string(),
            ],
        ),
        ProjectionBackendV1::PropertyGraph => (
            NativeArtifactKindV1::PropertyGraphBundle,
            "application/vnd.axiograph.property-graph+json",
            "property-graph.json",
            render_property_graph(records)?,
            vec![
                "relation objects remain nodes; binary convenience edges are not the canonical relation"
                    .to_string(),
                "the portable bundle makes no Neo4j, AGE, Neptune, JanusGraph, or Memgraph feature-equivalence claim"
                    .to_string(),
            ],
        ),
    };
    let artifact_digest = ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_native_projection_artifact_v1",
        backend.as_str().as_bytes(),
        media_type.as_bytes(),
        body.as_bytes(),
    ]);
    Ok(NativeProjectionArtifactV1 {
        kind,
        media_type: media_type.to_string(),
        suggested_extension: suggested_extension.to_string(),
        read_only: true,
        artifact_digest,
        body,
        caveats,
    })
}

fn render_json_bundle(
    backend: ProjectionBackendV1,
    records: &[ProjectionRecordV1],
) -> Result<String, ProjectionError> {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": "axiograph_native_projection_bundle_v1",
        "backend": backend,
        "authority": "derived_only",
        "mutation_authority": "axiograph_semantic_vcs_only",
        "records": records,
    }))
    .map_err(|error| ProjectionError::Serialization(error.to_string()))
}

fn render_typeql(records: &[ProjectionRecordV1]) -> String {
    let object_keys = records
        .iter()
        .filter_map(|record| match &record.payload {
            ProjectionRecordPayloadV1::ObjectType { object_type_id, .. } => {
                Some((object_type_id.clone(), record.native_key.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let relation_keys = records
        .iter()
        .filter_map(|record| match &record.payload {
            ProjectionRecordPayloadV1::RelationObject { relation_id, .. } => {
                Some((relation_id.clone(), record.native_key.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut roles_by_relation: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut plays_by_object: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut plays_by_relation: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in records {
        if let ProjectionRecordPayloadV1::RoleProjection {
            relation_id,
            declared_order,
            type_expr,
            ..
        } = &record.payload
        {
            roles_by_relation
                .entry(relation_id.clone())
                .or_default()
                .push((*declared_order, record.native_key.clone()));
            if let Some(relation_key) = relation_keys.get(relation_id) {
                let play = format!("plays {relation_key}:{}", record.native_key);
                match projected_type_carrier(type_expr) {
                    ProjectedObjectRefV1::ObjectType { object_type_id } => {
                        plays_by_object
                            .entry(object_type_id)
                            .or_default()
                            .push(play);
                    }
                    ProjectedObjectRefV1::RelationObject { relation_id } => {
                        plays_by_relation.entry(relation_id).or_default().push(play);
                    }
                }
            }
        }
    }
    for roles in roles_by_relation.values_mut() {
        roles.sort();
    }
    for plays in plays_by_object.values_mut() {
        plays.sort();
        plays.dedup();
    }
    for plays in plays_by_relation.values_mut() {
        plays.sort();
        plays.dedup();
    }

    let mut out = String::from(
        "# Derived read-only projection. Accepted .axi and compiled kernel IR remain authoritative.\ndefine\n",
    );
    for (object_id, object_key) in object_keys {
        let plays = plays_by_object.remove(&object_id).unwrap_or_default();
        let suffix = plays
            .iter()
            .map(|play| format!(", {play}"))
            .collect::<String>();
        let _ = writeln!(out, "{object_key} sub entity{suffix};");
    }
    for (relation_id, relation_key) in relation_keys {
        let roles = roles_by_relation.remove(&relation_id).unwrap_or_default();
        let plays = plays_by_relation.remove(&relation_id).unwrap_or_default();
        let role_clause = roles
            .iter()
            .map(|(_, role)| format!(", relates {role}"))
            .collect::<String>();
        let play_clause = plays
            .iter()
            .map(|play| format!(", {play}"))
            .collect::<String>();
        let _ = writeln!(
            out,
            "{relation_key} sub relation{role_clause}{play_clause};"
        );
    }
    out.push_str(
        "\n# Subtype, constraint, equation, rewrite, refinement, and higher-path obligations are in the manifest sidecar.\n",
    );
    out
}

fn projected_type_carrier(type_expr: &ProjectedTypeExprV1) -> ProjectedObjectRefV1 {
    match type_expr {
        ProjectedTypeExprV1::Object { object_type_id } => ProjectedObjectRefV1::ObjectType {
            object_type_id: object_type_id.clone(),
        },
        ProjectedTypeExprV1::RelationObject { relation_id } => {
            ProjectedObjectRefV1::RelationObject {
                relation_id: relation_id.clone(),
            }
        }
        ProjectedTypeExprV1::Indexed { base, .. } | ProjectedTypeExprV1::Refined { base, .. } => {
            projected_type_carrier(base)
        }
    }
}

fn render_rdf_owl(records: &[ProjectionRecordV1]) -> Result<String, ProjectionError> {
    let relation_keys = records
        .iter()
        .filter_map(|record| match &record.payload {
            ProjectionRecordPayloadV1::RelationObject { relation_id, .. } => {
                Some((relation_id.clone(), record.native_key.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let role_keys = records
        .iter()
        .filter_map(|record| match &record.payload {
            ProjectionRecordPayloadV1::RoleProjection { role_id, .. } => {
                Some((role_id.clone(), record.native_key.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut out = String::from(
        "@prefix axp: <https://axiograph.example/projection/> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\naxp:schema {\n",
    );
    for record in records {
        let iri = format!("axp:{}", record.native_key);
        let source = serde_json::to_string(&record.source_ref)
            .map_err(|error| ProjectionError::Serialization(error.to_string()))?;
        let source_literal = turtle_string(&source);
        match &record.payload {
            ProjectionRecordPayloadV1::ObjectType { label, .. } => {
                let _ = writeln!(
                    out,
                    "  {iri} a owl:Class ; rdfs:label {} ; axp:sourceRef {source_literal} .",
                    turtle_string(label)
                );
            }
            ProjectionRecordPayloadV1::RelationObject { label, .. } => {
                let _ = writeln!(
                    out,
                    "  {iri} a owl:Class, axp:RelationObject ; rdfs:label {} ; axp:sourceRef {source_literal} .",
                    turtle_string(label)
                );
            }
            ProjectionRecordPayloadV1::RoleProjection { label, .. } => {
                let _ = writeln!(
                    out,
                    "  {iri} a rdf:Property, axp:RoleProjection ; rdfs:label {} ; axp:sourceRef {source_literal} .",
                    turtle_string(label)
                );
            }
            _ => {}
        }
    }
    out.push_str("}\n\naxp:instances {\n");
    for record in records {
        match &record.payload {
            ProjectionRecordPayloadV1::CarrierElement { value, .. } => {
                let _ = writeln!(
                    out,
                    "  axp:{} a axp:CarrierElement ; axp:value {} ; axp:payloadFingerprint {} .",
                    record.native_key,
                    turtle_string(value),
                    turtle_string(&record.payload_fingerprint.to_string())
                );
            }
            ProjectionRecordPayloadV1::FunctionMapping { source, target, .. } => {
                let _ = writeln!(
                    out,
                    "  axp:{} a axp:FunctionMapping ; axp:source {} ; axp:target {} ; axp:payloadFingerprint {} .",
                    record.native_key,
                    turtle_string(source),
                    turtle_string(target),
                    turtle_string(&record.payload_fingerprint.to_string())
                );
            }
            ProjectionRecordPayloadV1::RelationFact {
                relation_id,
                ordered_role_values,
                ..
            } => {
                let relation_type = relation_keys
                    .get(relation_id)
                    .map(|key| format!(", axp:{key}"))
                    .unwrap_or_default();
                let _ = write!(
                    out,
                    "  axp:{} a axp:RelationFact{}",
                    record.native_key, relation_type
                );
                for role_value in ordered_role_values {
                    let predicate = role_keys
                        .get(&role_value.role_id)
                        .map(String::as_str)
                        .unwrap_or("unresolved_role");
                    let _ = write!(
                        out,
                        " ; axp:{predicate} {}",
                        turtle_string(projected_value_wire(&role_value.value))
                    );
                }
                let _ = writeln!(
                    out,
                    " ; axp:payloadFingerprint {} .",
                    turtle_string(&record.payload_fingerprint.to_string())
                );
            }
            _ => {}
        }
    }
    out.push_str(
        "}\n\n# Obligations and semantic-loss entries remain in the typed JSON manifest.\n",
    );
    Ok(out)
}

fn projected_value_wire(value: &ProjectedValueV1) -> &str {
    match value {
        ProjectedValueV1::ObjectElement { value } => value,
        ProjectedValueV1::RelationFact { fact_id } => fact_id,
    }
}

fn render_property_graph(records: &[ProjectionRecordV1]) -> Result<String, ProjectionError> {
    let nodes = records
        .iter()
        .filter(|record| {
            !matches!(
                record.payload,
                ProjectionRecordPayloadV1::RoleProjection { .. }
                    | ProjectionRecordPayloadV1::FunctionMapping { .. }
            )
        })
        .map(|record| {
            serde_json::json!({
                "id": record.native_key,
                "record_id": record.record_id,
                "label": record.payload.kind_label(),
                "source_ref": record.source_ref,
                "payload_fingerprint": record.payload_fingerprint,
                "payload": record.payload,
            })
        })
        .collect::<Vec<_>>();
    let edges = records
        .iter()
        .filter(|record| {
            matches!(
                record.payload,
                ProjectionRecordPayloadV1::RoleProjection { .. }
                    | ProjectionRecordPayloadV1::FunctionMapping { .. }
            )
        })
        .map(|record| {
            serde_json::json!({
                "id": record.native_key,
                "record_id": record.record_id,
                "type": record.payload.kind_label(),
                "source_ref": record.source_ref,
                "payload_fingerprint": record.payload_fingerprint,
                "payload": record.payload,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({
        "version": "axiograph_property_graph_bundle_v1",
        "authority": "derived_only",
        "relation_object_policy": "relation_objects_are_nodes",
        "nodes": nodes,
        "edges": edges,
    }))
    .map_err(|error| ProjectionError::Serialization(error.to_string()))
}

fn schema_ref(
    module_id: &axiograph_kernel::ModuleIdV2,
    revision: &RevisionDigestV2,
    semantic_key: &SemanticKeyV2,
    schema_id: &SchemaIdV2,
) -> KernelRefV2 {
    KernelRefV2::Schema {
        module_id: module_id.clone(),
        revision: revision.clone(),
        semantic_key: semantic_key.clone(),
        schema_id: schema_id.clone(),
    }
}

fn constraint_ref(
    module_id: &axiograph_kernel::ModuleIdV2,
    revision: &RevisionDigestV2,
    theory_id: &TheoryIdV2,
    semantic_key: &SemanticKeyV2,
    constraint_id: &ConstraintIdV2,
) -> KernelRefV2 {
    KernelRefV2::Constraint {
        module_id: module_id.clone(),
        revision: revision.clone(),
        theory_id: theory_id.clone(),
        semantic_key: semantic_key.clone(),
        constraint_id: constraint_id.clone(),
    }
}

fn equation_ref(
    module_id: &axiograph_kernel::ModuleIdV2,
    revision: &RevisionDigestV2,
    theory_id: &TheoryIdV2,
    semantic_key: &SemanticKeyV2,
    equation_id: &EquationIdV2,
) -> KernelRefV2 {
    KernelRefV2::Equation {
        module_id: module_id.clone(),
        revision: revision.clone(),
        theory_id: theory_id.clone(),
        semantic_key: semantic_key.clone(),
        equation_id: equation_id.clone(),
    }
}

fn rewrite_ref(
    module_id: &axiograph_kernel::ModuleIdV2,
    revision: &RevisionDigestV2,
    theory_id: &TheoryIdV2,
    semantic_key: &SemanticKeyV2,
    rewrite_rule_id: &RewriteRuleIdV2,
) -> KernelRefV2 {
    KernelRefV2::RewriteRule {
        module_id: module_id.clone(),
        revision: revision.clone(),
        theory_id: theory_id.clone(),
        semantic_key: semantic_key.clone(),
        rewrite_rule_id: rewrite_rule_id.clone(),
    }
}

fn role_kind_label(kind: RoleKindIr) -> &'static str {
    match kind {
        RoleKindIr::Data => "data",
        RoleKindIr::Context => "context",
        RoleKindIr::World => "world",
        RoleKindIr::Temporal => "temporal",
        RoleKindIr::Parameter => "parameter",
        RoleKindIr::Evidence => "evidence",
    }
}

fn generator_kind_label(kind: SchemaGeneratorKindIr) -> &'static str {
    match kind {
        SchemaGeneratorKindIr::RoleProjection => "role_projection",
        SchemaGeneratorKindIr::SubtypeInclusion => "subtype_inclusion",
        SchemaGeneratorKindIr::Aspect => "aspect",
        SchemaGeneratorKindIr::Function => "function",
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut underscore = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            underscore = false;
        } else if !underscore && !out.is_empty() {
            out.push('_');
            underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert_str(0, "axiograph_");
    }
    out
}

fn short_id(value: &str) -> &str {
    let tail = value.rsplit(':').next().unwrap_or(value);
    let start = tail.len().saturating_sub(10);
    &tail[start..]
}

fn turtle_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}
