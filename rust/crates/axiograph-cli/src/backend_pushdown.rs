#![allow(dead_code)]

use anyhow::{bail, Result};
use axiograph_pathdb::kernel_ir::{
    compile_schema_category_ir, CompiledSchemaIr, KernelRefV1, RelationSemanticsIr, RoleKind,
    WitnessViewIr,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::accepted_plane::{
    BackendCapabilityProfileV1, ProjectionBackendEngineV1, ProjectionCapabilityProfileV1,
    ProjectionCarrierEdgeMappingV1, ProjectionContextAxisBindingV1,
    ProjectionContextMappingStrategyV1, ProjectionContextMappingV1, ProjectionMutationAuthorityV1,
    ProjectionNativeQueryAccessV1, ProjectionObjectMappingV1, ProjectionRelationTupleEncodingV1,
};

const BACKEND_PUSHDOWN_PLAN_VERSION_V1: &str = "backend_pushdown_plan_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendNativeQueryDialectV1 {
    TypeQl,
    Woql,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreservedLowerTierInterfaceKindV1 {
    NativeQuery,
    RdfDataset,
    ShaclValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushdownRoleKindV1 {
    Data,
    Context,
    Temporal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushdownWitnessViewV1 {
    None,
    Morphism,
    Homotopy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedPushdownRolePlanV1 {
    pub role_name: String,
    pub scoped_role_name: String,
    pub target_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissible_player_types: Vec<String>,
    pub role_kind: PushdownRoleKindV1,
    pub backend_slot: String,
    #[serde(default)]
    pub preserved_explicitly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedPushdownRelationPlanV1 {
    pub relation_name: String,
    pub tuple_type_name: String,
    pub tuple_encoding: ProjectionRelationTupleEncodingV1,
    pub tuple_label: String,
    pub witness_view: PushdownWitnessViewV1,
    #[serde(default)]
    pub role_plans: Vec<TypedPushdownRolePlanV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier_edge: Option<ProjectionCarrierEdgeMappingV1>,
    #[serde(default)]
    pub preserved_losslessly: bool,
    #[serde(default)]
    pub semantic_losses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreservedLowerTierInterfaceV1 {
    pub interface_kind: PreservedLowerTierInterfaceKindV1,
    pub surface_label: String,
    pub preservation_class: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticLiftingContractV1 {
    pub source_surface_label: String,
    pub lifted_into: String,
    pub preservation_claim: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownSubtypeFamilyV1 {
    pub supertype: String,
    #[serde(default)]
    pub subtypes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownCompiledIrEvidenceV1 {
    pub source_surface: String,
    pub object_type_count: usize,
    pub relation_count: usize,
    pub role_count: usize,
    pub nary_relation_count: usize,
    pub context_axis_count: usize,
    pub direct_subtype_edge_count: usize,
    #[serde(default)]
    pub relation_object_names: Vec<String>,
    #[serde(default)]
    pub direct_subtype_families: Vec<BackendPushdownSubtypeFamilyV1>,
    #[serde(default)]
    pub kernel_ref_labels: Vec<String>,
    #[serde(default)]
    pub kernel_refs: Vec<KernelRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownCapabilityDecisionV1 {
    pub profile: String,
    pub capability: String,
    pub required_for: String,
    pub satisfied: bool,
    pub plan_effect: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendNativeProjectionNoteV1 {
    pub surface_label: String,
    pub backend_native_shape: String,
    pub compiled_ir_basis: String,
    #[serde(default)]
    pub kernel_ref_labels: Vec<String>,
    #[serde(default)]
    pub kernel_refs: Vec<KernelRefV1>,
    pub read_contract: String,
    pub mutation_authority: ProjectionMutationAuthorityV1,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendNativeProjectionArtifactKindV1 {
    TypeQlSchema,
    TypeQlReadQuery,
    RdfTurtleSchema,
    WoqlReadQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendNativeProjectionArtifactV1 {
    pub artifact_label: String,
    pub artifact_kind: BackendNativeProjectionArtifactKindV1,
    pub surface_label: String,
    pub media_type: String,
    pub generated_from: String,
    #[serde(default)]
    pub kernel_ref_labels: Vec<String>,
    #[serde(default)]
    pub kernel_refs: Vec<KernelRefV1>,
    pub read_contract: String,
    pub mutation_authority: ProjectionMutationAuthorityV1,
    pub body: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownCoreV1 {
    pub version: String,
    pub backend: BackendCapabilityProfileV1,
    pub projection: ProjectionCapabilityProfileV1,
    pub compiled_ir_evidence: BackendPushdownCompiledIrEvidenceV1,
    #[serde(default)]
    pub capability_decisions: Vec<BackendPushdownCapabilityDecisionV1>,
    pub native_query_dialect: BackendNativeQueryDialectV1,
    #[serde(default)]
    pub object_plans: Vec<ProjectionObjectMappingV1>,
    #[serde(default)]
    pub relation_plans: Vec<TypedPushdownRelationPlanV1>,
    pub context_mapping: ProjectionContextMappingV1,
    #[serde(default)]
    pub native_projection_notes: Vec<BackendNativeProjectionNoteV1>,
    #[serde(default)]
    pub native_artifacts: Vec<BackendNativeProjectionArtifactV1>,
    #[serde(default)]
    pub preserved_interfaces: Vec<PreservedLowerTierInterfaceV1>,
    #[serde(default)]
    pub lifting_contracts: Vec<SemanticLiftingContractV1>,
    #[serde(default)]
    pub trust_caveats: Vec<String>,
    #[serde(default)]
    pub semantic_losses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeDbPushdownPlanV1 {
    pub core: BackendPushdownCoreV1,
    pub typeql_relation_kind: String,
    pub typeql_role_prefix: String,
    pub typed_projection_shape: TypeDbTypedProjectionShapeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminusDbPushdownPlanV1 {
    pub core: BackendPushdownCoreV1,
    pub schema_graph_label: String,
    pub instance_graph_label: String,
    pub rdf_vcs_projection_shape: TerminusDbRdfVcsProjectionShapeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeDbTypedProjectionShapeV1 {
    pub relation_roles_native: bool,
    pub nary_relations_native: bool,
    pub subtype_hierarchy_native: bool,
    pub schema_constraints_native: bool,
    pub typed_query_validation_native: bool,
    pub native_read_surface: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminusDbRdfVcsProjectionShapeV1 {
    pub rdf_named_graph_dataset_readable: bool,
    pub named_graph_context_world_mapping: bool,
    pub schema_instance_graph_separation: bool,
    pub branch_history_mirroring: bool,
    pub native_read_surface: String,
    #[serde(default)]
    pub branch_history_caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "backend_engine", content = "plan", rename_all = "snake_case")]
pub enum BackendPushdownPlanV1 {
    TypeDb(TypeDbPushdownPlanV1),
    TerminusDb(TerminusDbPushdownPlanV1),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownRelationTransportSummaryV1 {
    pub total_relations: usize,
    pub lossless_relations: usize,
    pub reduced_relations: usize,
    pub carrier_edge_views: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownContextTransportSummaryV1 {
    pub strategy: ProjectionContextMappingStrategyV1,
    #[serde(default)]
    pub axis_bindings: Vec<ProjectionContextAxisBindingV1>,
    #[serde(default)]
    pub anchor_scoped_query_pushdown: bool,
    #[serde(default)]
    pub context_scoped_query_pushdown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownTrustContractV1 {
    pub trust_class: String,
    pub soundness: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendPushdownOperationalSurfaceV1 {
    pub backend_label: String,
    pub native_query_dialect: BackendNativeQueryDialectV1,
    pub native_query_access: ProjectionNativeQueryAccessV1,
    pub compiled_ir_evidence: BackendPushdownCompiledIrEvidenceV1,
    #[serde(default)]
    pub kernel_ref_labels: Vec<String>,
    #[serde(default)]
    pub kernel_refs: Vec<KernelRefV1>,
    #[serde(default)]
    pub capability_decisions: Vec<BackendPushdownCapabilityDecisionV1>,
    #[serde(default)]
    pub native_projection_notes: Vec<BackendNativeProjectionNoteV1>,
    #[serde(default)]
    pub native_artifacts: Vec<BackendNativeProjectionArtifactV1>,
    pub trust: BackendPushdownTrustContractV1,
    pub relation_transport: BackendPushdownRelationTransportSummaryV1,
    pub context_transport: BackendPushdownContextTransportSummaryV1,
    #[serde(default)]
    pub preserved_interfaces: Vec<PreservedLowerTierInterfaceV1>,
    #[serde(default)]
    pub lifting_contracts: Vec<SemanticLiftingContractV1>,
    #[serde(default)]
    pub residual_obligations: Vec<String>,
    #[serde(default)]
    pub reconciliation_boundary: Vec<String>,
}

impl BackendPushdownPlanV1 {
    pub fn core(&self) -> &BackendPushdownCoreV1 {
        match self {
            Self::TypeDb(plan) => &plan.core,
            Self::TerminusDb(plan) => &plan.core,
        }
    }

    pub fn operational_surface(&self) -> BackendPushdownOperationalSurfaceV1 {
        let core = self.core();
        let total_relations = core.relation_plans.len();
        let lossless_relations = core
            .relation_plans
            .iter()
            .filter(|relation| relation.preserved_losslessly)
            .count();
        let carrier_edge_views = core
            .relation_plans
            .iter()
            .filter(|relation| relation.carrier_edge.is_some())
            .count();
        let mut residual_obligations = core.semantic_losses.clone();
        if core.projection.allows_native_read_queries() {
            push_unique(
                &mut residual_obligations,
                "backend-native query results must be lifted through the compiled IR and trust contracts before they can justify review, promotion, or reconciliation".to_string(),
            );
        }
        if !core.projection.supports_anchor_scoped_query_pushdown {
            push_unique(
                &mut residual_obligations,
                "anchor-scoped rechecks stay on the Axiograph side because pushdown is not anchor-scoped by construction".to_string(),
            );
        }
        if !core.projection.supports_context_scoped_query_pushdown
            && !core.context_mapping.axis_bindings.is_empty()
        {
            push_unique(
                &mut residual_obligations,
                "context/world filters require Axiograph-side reindexing over the preserved axis bindings before operational decisions are made".to_string(),
            );
        }

        let mut reconciliation_boundary = vec![
            "semantic refs, CQ gates, and persisted reconciliation previews remain authoritative in Axiograph".to_string(),
            "projection mappings and backend-native history are typed transport aids, not merge state".to_string(),
        ];
        if core.backend.branching_and_merge
            || core.backend.diff_and_patch
            || core.backend.immutable_history
        {
            reconciliation_boundary.push(
                "backend-native history may mirror collaboration state, but merge decisions still flow through SemReconciliationV1 and stored validation reports".to_string(),
            );
        }
        let trust = BackendPushdownTrustContractV1 {
            trust_class: "projected_read_surface".to_string(),
            soundness: "projection_transport_and_lifting_contract_only".to_string(),
            completeness_claim: "not_claimed".to_string(),
            ontology_closure_claim: "not_claimed".to_string(),
            notes: vec![
                "native backend reads are useful lower-tier interfaces, but operational semantics still lift through accepted .axi, compiled IR, anchors, and trust contracts".to_string(),
                "this surface does not claim complete answers, exhaustive backend pushdown, or full ontology closure".to_string(),
            ],
        };

        BackendPushdownOperationalSurfaceV1 {
            backend_label: core.backend.backend_label.clone(),
            native_query_dialect: core.native_query_dialect.clone(),
            native_query_access: core.projection.native_query_access.clone(),
            compiled_ir_evidence: core.compiled_ir_evidence.clone(),
            kernel_ref_labels: core.compiled_ir_evidence.kernel_ref_labels.clone(),
            kernel_refs: core.compiled_ir_evidence.kernel_refs.clone(),
            capability_decisions: core.capability_decisions.clone(),
            native_projection_notes: core.native_projection_notes.clone(),
            native_artifacts: core.native_artifacts.clone(),
            trust,
            relation_transport: BackendPushdownRelationTransportSummaryV1 {
                total_relations,
                lossless_relations,
                reduced_relations: total_relations.saturating_sub(lossless_relations),
                carrier_edge_views,
            },
            context_transport: BackendPushdownContextTransportSummaryV1 {
                strategy: core.context_mapping.strategy.clone(),
                axis_bindings: core.context_mapping.axis_bindings.clone(),
                anchor_scoped_query_pushdown: core.projection.supports_anchor_scoped_query_pushdown,
                context_scoped_query_pushdown: core
                    .projection
                    .supports_context_scoped_query_pushdown,
            },
            preserved_interfaces: core.preserved_interfaces.clone(),
            lifting_contracts: core.lifting_contracts.clone(),
            residual_obligations,
            reconciliation_boundary,
        }
    }
}

pub(crate) fn build_backend_pushdown_plan(
    compiled_ir: &CompiledSchemaIr,
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Result<BackendPushdownPlanV1> {
    enforce_axiograph_only_mutation(projection)?;
    match backend.backend_engine {
        ProjectionBackendEngineV1::TypeDb => Ok(BackendPushdownPlanV1::TypeDb(
            build_typedb_pushdown_plan(compiled_ir, backend, projection)?,
        )),
        ProjectionBackendEngineV1::TerminusDb => Ok(BackendPushdownPlanV1::TerminusDb(
            build_terminusdb_pushdown_plan(compiled_ir, backend, projection)?,
        )),
        _ => bail!(
            "backend pushdown planning is currently implemented only for TypeDB and TerminusDB"
        ),
    }
}

fn build_typedb_pushdown_plan(
    compiled_ir: &CompiledSchemaIr,
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Result<TypeDbPushdownPlanV1> {
    if !backend.native_type_system
        || !backend.native_nary_relations
        || !backend.relationship_entities
    {
        bail!("typedb pushdown requires native types, native n-ary relations, and relationship entities");
    }
    if !backend.constraints || !backend.schema_management || !backend.typed_query_validation {
        bail!(
            "typedb pushdown requires native constraints, schema management, and typed query validation"
        );
    }
    if !projection.preserves_relation_objects || !projection.preserves_nary_relation_objects {
        bail!("typedb pushdown requires relation-object and n-ary preservation");
    }

    let compiled_ir_evidence = compiled_ir_evidence(compiled_ir);
    let object_plans = build_object_plans(compiled_ir);
    let relation_plans = compiled_relation_plans(
        compiled_ir,
        projection,
        ProjectionRelationTupleEncodingV1::RelationshipEntity,
        BackendNativeQueryDialectV1::TypeQl,
        false,
    );

    let context_mapping = if projection.preserves_context_world_axes
        && projection.supports_context_scoped_query_pushdown
    {
        ProjectionContextMappingV1 {
            strategy: ProjectionContextMappingStrategyV1::SeparateNamespaces,
            axis_bindings: compiled_context_axis_bindings(compiled_ir, "scope"),
        }
    } else {
        ProjectionContextMappingV1 {
            strategy: ProjectionContextMappingStrategyV1::TupleProperties,
            axis_bindings: compiled_context_axis_bindings(compiled_ir, "tuple"),
        }
    };

    let mut trust_caveats = common_trust_caveats(backend, projection);
    trust_caveats.push(
        "typeql queries are treated as read-only projected lenses; semantic authority stays in accepted .axi plus compiled IR"
            .to_string(),
    );

    let semantic_losses = collect_global_semantic_losses(compiled_ir, projection);
    let capability_decisions = typedb_capability_decisions(backend, projection);
    let native_projection_notes = typedb_native_projection_notes(&compiled_ir_evidence, projection);
    let native_artifacts = typedb_native_artifacts(
        &object_plans,
        &relation_plans,
        &context_mapping,
        &compiled_ir_evidence,
        projection,
    );
    let preserved_interfaces = typedb_preserved_interfaces(projection);
    let lifting_contracts = typedb_lifting_contracts();
    let typed_projection_shape = TypeDbTypedProjectionShapeV1 {
        relation_roles_native: backend.native_type_system
            && backend.relationship_entities
            && projection.preserves_relation_objects,
        nary_relations_native: backend.native_nary_relations
            && projection.preserves_nary_relation_objects,
        subtype_hierarchy_native: backend.native_type_system && backend.schema_management,
        schema_constraints_native: backend.constraints && backend.schema_management,
        typed_query_validation_native: backend.typed_query_validation,
        native_read_surface: "typeql_read_only_projected_schema".to_string(),
        caveats: vec![
            "TypeDB is the highest-fidelity typed target, but accepted .axi and compiled IR still own semantic authority"
                .to_string(),
            "native schema mutations are not accepted ontology mutations".to_string(),
        ],
    };

    let core = BackendPushdownCoreV1 {
        version: BACKEND_PUSHDOWN_PLAN_VERSION_V1.to_string(),
        backend: backend.clone(),
        projection: projection.clone(),
        compiled_ir_evidence,
        capability_decisions,
        native_query_dialect: BackendNativeQueryDialectV1::TypeQl,
        object_plans,
        relation_plans,
        context_mapping,
        native_projection_notes,
        native_artifacts,
        preserved_interfaces,
        lifting_contracts,
        trust_caveats,
        semantic_losses,
    };
    validate_backend_pushdown_kernel_refs(compiled_ir, &core)?;

    Ok(TypeDbPushdownPlanV1 {
        core,
        typeql_relation_kind: "relation".to_string(),
        typeql_role_prefix: "role".to_string(),
        typed_projection_shape,
    })
}

fn build_terminusdb_pushdown_plan(
    compiled_ir: &CompiledSchemaIr,
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Result<TerminusDbPushdownPlanV1> {
    if !backend.named_graphs || !backend.schema_management {
        bail!("terminusdb pushdown requires named graphs and schema management");
    }
    if !backend.schema_instance_separation {
        bail!("terminusdb pushdown requires schema/instance separation");
    }
    if !projection.preserves_relation_objects {
        bail!("terminusdb pushdown requires relation-object preservation");
    }

    let compiled_ir_evidence = compiled_ir_evidence(compiled_ir);
    let object_plans = build_object_plans(compiled_ir);
    let relation_plans = compiled_relation_plans(
        compiled_ir,
        projection,
        ProjectionRelationTupleEncodingV1::ReifiedFact,
        BackendNativeQueryDialectV1::Woql,
        true,
    );

    let context_mapping = if projection.preserves_context_world_axes {
        ProjectionContextMappingV1 {
            strategy: ProjectionContextMappingStrategyV1::NamedGraphs,
            axis_bindings: compiled_context_axis_bindings(compiled_ir, "graph"),
        }
    } else {
        ProjectionContextMappingV1 {
            strategy: ProjectionContextMappingStrategyV1::SidecarIndex,
            axis_bindings: compiled_context_axis_bindings(compiled_ir, "sidecar"),
        }
    };

    let mut trust_caveats = common_trust_caveats(backend, projection);
    trust_caveats.push(
        "woql/document-native querying remains read-only and may expose a reduced shape relative to typed Axiograph authoring and certificates"
            .to_string(),
    );
    if backend.can_mirror_semantic_vcs_workspace() {
        trust_caveats.push(
            "backend-native history can assist materialization management, but semantic refs, promotion, and reconciliation remain Axiograph-native"
                .to_string(),
        );
    }

    let mut semantic_losses = collect_global_semantic_losses(compiled_ir, projection);
    if !projection.supports_anchor_scoped_query_pushdown {
        semantic_losses.push(
            "anchor-scoped pushdown is unavailable, so backend-native queries must be rechecked against Axiograph anchors"
                .to_string(),
        );
    }
    let preserved_interfaces = terminusdb_preserved_interfaces(projection);
    let lifting_contracts = terminusdb_lifting_contracts();
    let capability_decisions = terminusdb_capability_decisions(backend, projection);
    let native_projection_notes =
        terminusdb_native_projection_notes(&compiled_ir_evidence, backend, projection);
    let native_artifacts = terminusdb_native_artifacts(
        &object_plans,
        &relation_plans,
        &context_mapping,
        &compiled_ir_evidence,
        backend,
        projection,
    );
    let rdf_vcs_projection_shape = TerminusDbRdfVcsProjectionShapeV1 {
        rdf_named_graph_dataset_readable: backend.named_graphs,
        named_graph_context_world_mapping: backend.named_graphs
            && projection.preserves_context_world_axes,
        schema_instance_graph_separation: backend.schema_instance_separation,
        branch_history_mirroring: backend.can_mirror_semantic_vcs_workspace(),
        native_read_surface: "woql_over_projected_rdf_named_graphs".to_string(),
        branch_history_caveats: vec![
            "backend branches/history may mirror projected materialization state, not semantic merge authority"
                .to_string(),
            "Axiograph semantic refs, review gates, trust contracts, and reconciliation reports remain the authoritative history"
                .to_string(),
        ],
    };

    let core = BackendPushdownCoreV1 {
        version: BACKEND_PUSHDOWN_PLAN_VERSION_V1.to_string(),
        backend: backend.clone(),
        projection: projection.clone(),
        compiled_ir_evidence,
        capability_decisions,
        native_query_dialect: BackendNativeQueryDialectV1::Woql,
        object_plans,
        relation_plans,
        context_mapping,
        native_projection_notes,
        native_artifacts,
        preserved_interfaces,
        lifting_contracts,
        trust_caveats,
        semantic_losses,
    };
    validate_backend_pushdown_kernel_refs(compiled_ir, &core)?;

    Ok(TerminusDbPushdownPlanV1 {
        core,
        schema_graph_label: "schema/main".to_string(),
        instance_graph_label: "instance/main".to_string(),
        rdf_vcs_projection_shape,
    })
}

fn enforce_axiograph_only_mutation(projection: &ProjectionCapabilityProfileV1) -> Result<()> {
    if matches!(
        projection.mutation_authority,
        ProjectionMutationAuthorityV1::BackendWritableMirror
    ) {
        bail!(
            "backend pushdown plans are read-only projections; backend mutation authority is not allowed"
        );
    }
    Ok(())
}

fn validate_backend_pushdown_kernel_refs(
    compiled_ir: &CompiledSchemaIr,
    core: &BackendPushdownCoreV1,
) -> Result<()> {
    let declared = compiled_schema_kernel_refs(compiled_ir)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut cited = BTreeSet::new();
    cited.extend(core.compiled_ir_evidence.kernel_refs.iter().cloned());
    for note in &core.native_projection_notes {
        cited.extend(note.kernel_refs.iter().cloned());
    }
    for artifact in &core.native_artifacts {
        cited.extend(artifact.kernel_refs.iter().cloned());
    }
    let unresolved = cited
        .difference(&declared)
        .cloned()
        .collect::<Vec<KernelRefV1>>();
    if unresolved.is_empty() {
        return Ok(());
    }
    bail!(
        "backend pushdown plan cites {} undeclared KernelRefV1 handle(s): {}",
        unresolved.len(),
        unresolved
            .iter()
            .map(KernelRefV1::stable_label)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn compiled_ir_evidence(compiled_ir: &CompiledSchemaIr) -> BackendPushdownCompiledIrEvidenceV1 {
    let kernel_refs = compiled_schema_kernel_refs(compiled_ir);
    let kernel_ref_labels = kernel_ref_labels(&kernel_refs);
    let mut relation_object_names = compiled_ir
        .relations
        .values()
        .map(|relation| relation.tuple_type_name.clone())
        .collect::<Vec<_>>();
    relation_object_names.sort();
    relation_object_names.dedup();

    let role_count = compiled_ir
        .relations
        .values()
        .map(|relation| relation.roles.len())
        .sum();
    let nary_relation_count = compiled_ir
        .relations
        .values()
        .filter(|relation| relation.roles.len() > 2)
        .count();
    let context_axis_count = compiled_ir
        .relations
        .values()
        .flat_map(|relation| relation.roles.iter())
        .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        .map(|role| role.name.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let direct_subtype_families = compiled_ir
        .direct_subtype_families()
        .into_iter()
        .map(|family| BackendPushdownSubtypeFamilyV1 {
            supertype: family.supertype,
            subtypes: family.subtypes,
        })
        .collect::<Vec<_>>();
    let direct_subtype_edge_count = direct_subtype_families
        .iter()
        .map(|family| family.subtypes.len())
        .sum();

    BackendPushdownCompiledIrEvidenceV1 {
        source_surface: "compiled_schema_ir".to_string(),
        object_type_count: compiled_ir.object_types.len(),
        relation_count: compiled_ir.relations.len(),
        role_count,
        nary_relation_count,
        context_axis_count,
        direct_subtype_edge_count,
        relation_object_names,
        direct_subtype_families,
        kernel_ref_labels,
        kernel_refs,
    }
}

fn compiled_schema_kernel_refs(compiled_ir: &CompiledSchemaIr) -> Vec<KernelRefV1> {
    let category = compile_schema_category_ir(compiled_ir);
    let mut refs = BTreeSet::new();
    refs.insert(KernelRefV1::Schema {
        schema_id: category.schema_id.clone(),
    });
    for object in &category.objects {
        refs.insert(KernelRefV1::SchemaObject {
            schema_id: category.schema_id.clone(),
            object: object.object.clone(),
        });
    }
    for arrow in &category.arrows {
        refs.insert(KernelRefV1::SchemaArrow {
            schema_id: category.schema_id.clone(),
            arrow: arrow.arrow_ref.clone(),
        });
    }
    refs.into_iter().collect()
}

fn kernel_ref_labels(refs: &[KernelRefV1]) -> Vec<String> {
    refs.iter().map(KernelRefV1::stable_label).collect()
}

fn typedb_capability_decisions(
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendPushdownCapabilityDecisionV1> {
    vec![
        capability_decision(
            "backend",
            "native_type_system",
            "object types, subtype labels, and role-player typing",
            backend.native_type_system,
            "compiled IR object and subtype facts lower into TypeDB types",
        ),
        capability_decision(
            "backend",
            "native_nary_relations",
            "relations with more than two roles",
            backend.native_nary_relations,
            "compiled IR n-ary relation objects lower into native TypeDB relations",
        ),
        capability_decision(
            "backend",
            "relationship_entities",
            "relation-as-object preservation",
            backend.relationship_entities,
            "Axiograph relation objects remain readable as relationship entities",
        ),
        capability_decision(
            "backend",
            "constraints",
            "schema constraints and typed role-player validation",
            backend.constraints,
            "schema constraints are pushed into the TypeDB schema layer where supported",
        ),
        capability_decision(
            "backend",
            "typed_query_validation",
            "read-only TypeQL query checking",
            backend.typed_query_validation,
            "native TypeQL reads can be treated as typed lower-tier queries",
        ),
        capability_decision(
            "projection",
            "preserves_relation_objects",
            "relation-as-object semantics",
            projection.preserves_relation_objects,
            "relation objects remain the canonical tuple units in the projection plan",
        ),
        capability_decision(
            "projection",
            "preserves_nary_relation_objects",
            "n-ary relation transport",
            projection.preserves_nary_relation_objects,
            "higher-arity tuples are not decomposed into lossy binary edges",
        ),
        capability_decision(
            "projection",
            "mutation_authority",
            "Axiograph-owned semantic mutation",
            projection.requires_axiograph_mutation_authority(),
            "backend-native writes are excluded from accepted ontology mutation",
        ),
    ]
}

fn terminusdb_capability_decisions(
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendPushdownCapabilityDecisionV1> {
    vec![
        capability_decision(
            "backend",
            "named_graphs",
            "RDF dataset and context/world graph mapping",
            backend.named_graphs,
            "compiled IR context axes lower into named graph slots when the projection preserves them",
        ),
        capability_decision(
            "backend",
            "schema_instance_separation",
            "schema graph and instance graph split",
            backend.schema_instance_separation,
            "schema/main and instance/main remain separate readable graph surfaces",
        ),
        capability_decision(
            "backend",
            "immutable_history",
            "projected materialization history",
            backend.immutable_history,
            "backend history can mirror projection state without becoming semantic history",
        ),
        capability_decision(
            "backend",
            "branching_and_merge",
            "projected collaboration workspace mirroring",
            backend.branching_and_merge,
            "backend branches may mirror Axiograph branches but do not decide semantic merges",
        ),
        capability_decision(
            "backend",
            "diff_and_patch",
            "backend-native projected diffs",
            backend.diff_and_patch,
            "backend diffs are transport aids and must lift into Axiograph reconciliation",
        ),
        capability_decision(
            "projection",
            "preserves_relation_objects",
            "RDF reified-fact relation object readability",
            projection.preserves_relation_objects,
            "relation objects stay canonical even when represented as RDF-shaped facts",
        ),
        capability_decision(
            "projection",
            "preserves_context_world_axes",
            "named graph context/world mapping",
            projection.preserves_context_world_axes,
            "context/world axes can be read from named graph bindings instead of sidecar-only metadata",
        ),
        capability_decision(
            "projection",
            "mutation_authority",
            "Axiograph-owned semantic mutation",
            projection.requires_axiograph_mutation_authority(),
            "backend-native document or graph writes are excluded from accepted ontology mutation",
        ),
    ]
}

fn capability_decision(
    profile: &str,
    capability: &str,
    required_for: &str,
    satisfied: bool,
    plan_effect: &str,
) -> BackendPushdownCapabilityDecisionV1 {
    BackendPushdownCapabilityDecisionV1 {
        profile: profile.to_string(),
        capability: capability.to_string(),
        required_for: required_for.to_string(),
        satisfied,
        plan_effect: plan_effect.to_string(),
    }
}

fn typedb_native_projection_notes(
    evidence: &BackendPushdownCompiledIrEvidenceV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendNativeProjectionNoteV1> {
    vec![
        BackendNativeProjectionNoteV1 {
            surface_label: "typeql".to_string(),
            backend_native_shape:
                "native TypeDB schema with object types, relation types, scoped roles, subtype hierarchy, and n-ary relation objects"
                    .to_string(),
            compiled_ir_basis: format!(
                "{} relations, {} roles, {} n-ary relations, {} direct subtype edges",
                evidence.relation_count,
                evidence.role_count,
                evidence.nary_relation_count,
                evidence.direct_subtype_edge_count
            ),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: native_read_contract(projection),
            mutation_authority: projection.mutation_authority.clone(),
            caveats: vec![
                "TypeQL reads are useful native lower-tier queries, not accepted semantic mutations"
                    .to_string(),
                "answers must lift through accepted .axi, compiled IR anchors, and trust contracts before review or promotion"
                    .to_string(),
            ],
        },
        BackendNativeProjectionNoteV1 {
            surface_label: "typedb_schema_constraints".to_string(),
            backend_native_shape:
                "schema constraints, relation roles, player types, and subtype declarations are pushed into the typed backend where capability profiles allow"
                    .to_string(),
            compiled_ir_basis: format!(
                "relation objects: {}; subtype families: {}",
                evidence.relation_object_names.join(", "),
                evidence.direct_subtype_families.len()
            ),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: "schema-readable, mutation-authority-excluded".to_string(),
            mutation_authority: projection.mutation_authority.clone(),
            caveats: vec![
                "backend schema validity is a transport check and does not replace Lean-checked or Axiograph-accepted semantics"
                    .to_string(),
            ],
        },
    ]
}

fn terminusdb_native_projection_notes(
    evidence: &BackendPushdownCompiledIrEvidenceV1,
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendNativeProjectionNoteV1> {
    let rdf_shape = if projection.preserves_context_world_axes {
        "RDF/VCS-shaped dataset with schema and instance graphs plus named graph bindings for context/world axes"
    } else {
        "RDF/VCS-shaped dataset with schema and instance graphs; context/world axes require Axiograph sidecar metadata"
    };
    let mut rdf_caveats = vec![
        "named graphs make context/world structure readable but do not recover full typed ontology closure"
            .to_string(),
        "relation-as-object remains canonical above the RDF projection".to_string(),
    ];
    if !projection.preserves_context_world_axes {
        rdf_caveats.push(
            "this projection profile does not preserve context/world axes as native named graph bindings"
                .to_string(),
        );
    }

    vec![
        BackendNativeProjectionNoteV1 {
            surface_label: "rdf_named_graph_dataset".to_string(),
            backend_native_shape: rdf_shape.to_string(),
            compiled_ir_basis: format!(
                "{} relation objects projected as reified facts across {} context axes",
                evidence.relation_count, evidence.context_axis_count
            ),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: native_read_contract(projection),
            mutation_authority: projection.mutation_authority.clone(),
            caveats: rdf_caveats,
        },
        BackendNativeProjectionNoteV1 {
            surface_label: "terminusdb_branch_history".to_string(),
            backend_native_shape:
                "backend-native branch, history, and diff surfaces over projected materialization state"
                    .to_string(),
            compiled_ir_basis: format!(
                "compiled IR digest-owned projection with {} relations and {} direct subtype edges",
                evidence.relation_count, evidence.direct_subtype_edge_count
            ),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: if backend.can_mirror_semantic_vcs_workspace() {
                "readable VCS mirror, not semantic VCS authority".to_string()
            } else {
                "history mirror unavailable in this capability profile".to_string()
            },
            mutation_authority: projection.mutation_authority.clone(),
            caveats: vec![
                "backend branches may help inspect projection drift but cannot promote, supersede, retract, or merge accepted ontology state"
                    .to_string(),
                "schema/theory review history remains in Axiograph manifests and reconciliation reports"
                    .to_string(),
            ],
        },
    ]
}

fn native_read_contract(projection: &ProjectionCapabilityProfileV1) -> String {
    match projection.native_query_access {
        ProjectionNativeQueryAccessV1::None => "native reads disabled".to_string(),
        ProjectionNativeQueryAccessV1::ReadOnlyPartial => {
            "read-only partial lower-tier lens".to_string()
        }
        ProjectionNativeQueryAccessV1::ReadOnlyAnchorScoped => {
            "read-only anchor-scoped lower-tier lens".to_string()
        }
    }
}

fn typedb_native_artifacts(
    object_plans: &[ProjectionObjectMappingV1],
    relation_plans: &[TypedPushdownRelationPlanV1],
    context_mapping: &ProjectionContextMappingV1,
    evidence: &BackendPushdownCompiledIrEvidenceV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendNativeProjectionArtifactV1> {
    vec![
        BackendNativeProjectionArtifactV1 {
            artifact_label: "typedb_typeql_schema".to_string(),
            artifact_kind: BackendNativeProjectionArtifactKindV1::TypeQlSchema,
            surface_label: "typeql".to_string(),
            media_type: "application/vnd.typedb.typeql".to_string(),
            generated_from: "compiled_schema_ir + backend_pushdown_plan_v1".to_string(),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: "schema-readable, mutation-authority-excluded".to_string(),
            mutation_authority: projection.mutation_authority.clone(),
            body: render_typeql_schema(object_plans, relation_plans, context_mapping, evidence),
            caveats: vec![
                "generated TypeQL is a native-readable projection artifact, not the accepted ontology source"
                    .to_string(),
                "TypeDB schema writes must not be treated as accepted .axi promotion or semantic mutation"
                    .to_string(),
            ],
        },
        BackendNativeProjectionArtifactV1 {
            artifact_label: "typedb_typeql_read_queries".to_string(),
            artifact_kind: BackendNativeProjectionArtifactKindV1::TypeQlReadQuery,
            surface_label: "typeql".to_string(),
            media_type: "application/vnd.typedb.typeql".to_string(),
            generated_from: "typed_relation_pushdown_plan_v1".to_string(),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: native_read_contract(projection),
            mutation_authority: projection.mutation_authority.clone(),
            body: render_typeql_read_queries(relation_plans),
            caveats: vec![
                "queries are read-only lower-tier lenses over projected relation objects"
                    .to_string(),
                "answers require Axiograph lifting before review, promotion, reconciliation, or trust claims"
                    .to_string(),
            ],
        },
    ]
}

fn terminusdb_native_artifacts(
    object_plans: &[ProjectionObjectMappingV1],
    relation_plans: &[TypedPushdownRelationPlanV1],
    context_mapping: &ProjectionContextMappingV1,
    evidence: &BackendPushdownCompiledIrEvidenceV1,
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<BackendNativeProjectionArtifactV1> {
    let branch_history_contract = if backend.can_mirror_semantic_vcs_workspace() {
        "readable projected VCS mirror, not semantic VCS authority"
    } else {
        "backend VCS mirror unavailable for this capability profile"
    };

    vec![
        BackendNativeProjectionArtifactV1 {
            artifact_label: "terminusdb_rdf_named_graph_schema".to_string(),
            artifact_kind: BackendNativeProjectionArtifactKindV1::RdfTurtleSchema,
            surface_label: "rdf_named_graph_dataset".to_string(),
            media_type: "text/turtle".to_string(),
            generated_from: "compiled_schema_ir + backend_pushdown_plan_v1".to_string(),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: native_read_contract(projection),
            mutation_authority: projection.mutation_authority.clone(),
            body: render_terminusdb_rdf_turtle(
                object_plans,
                relation_plans,
                context_mapping,
                evidence,
            ),
            caveats: vec![
                "RDF named graphs are native-readable materializations of accepted state, not the semantic kernel"
                    .to_string(),
                "relation objects remain canonical above the RDF reified-fact projection"
                    .to_string(),
            ],
        },
        BackendNativeProjectionArtifactV1 {
            artifact_label: "terminusdb_woql_read_queries".to_string(),
            artifact_kind: BackendNativeProjectionArtifactKindV1::WoqlReadQuery,
            surface_label: "woql".to_string(),
            media_type: "text/vnd.terminusdb.woql".to_string(),
            generated_from: "typed_relation_pushdown_plan_v1".to_string(),
            kernel_ref_labels: evidence.kernel_ref_labels.clone(),
            kernel_refs: evidence.kernel_refs.clone(),
            read_contract: format!("{}, {}", native_read_contract(projection), branch_history_contract),
            mutation_authority: projection.mutation_authority.clone(),
            body: render_terminusdb_woql_queries(relation_plans),
            caveats: vec![
                "WOQL reads inspect the projected materialization and must lift into Axiograph anchors before use"
                    .to_string(),
                "TerminusDB branch/history/diff features may mirror transport state but cannot promote, supersede, retract, or merge accepted ontology state"
                    .to_string(),
            ],
        },
    ]
}

fn render_typeql_schema(
    object_plans: &[ProjectionObjectMappingV1],
    relation_plans: &[TypedPushdownRelationPlanV1],
    context_mapping: &ProjectionContextMappingV1,
    evidence: &BackendPushdownCompiledIrEvidenceV1,
) -> String {
    let mut out = String::new();
    out.push_str("define\n");
    out.push_str("axiograph_object sub entity,\n");
    out.push_str("    owns axiograph_label;\n");
    out.push_str("axiograph_label sub attribute, value string;\n\n");

    for object_name in projection_object_names(object_plans, relation_plans, evidence) {
        let type_label = typeql_type_label(&object_name);
        let supertype = direct_supertype(&object_name, evidence)
            .map(typeql_type_label)
            .unwrap_or_else(|| "axiograph_object".to_string());
        write!(&mut out, "{type_label} sub {supertype}").expect("write TypeQL object");
        for play in typeql_plays_for_object(&object_name, relation_plans) {
            write!(&mut out, ",\n    plays {play}").expect("write TypeQL plays");
        }
        out.push_str(";\n");
    }

    if !context_mapping.axis_bindings.is_empty() {
        out.push('\n');
        for axis in &context_mapping.axis_bindings {
            writeln!(
                &mut out,
                "axiograph_context_axis_{} sub axiograph_object;",
                typeql_type_label(&axis.axis_name)
            )
            .expect("write TypeQL context axis");
        }
    }

    out.push('\n');
    for relation in relation_plans {
        let relation_label = typeql_type_label(&relation.relation_name);
        if relation.role_plans.is_empty() {
            writeln!(&mut out, "{relation_label} sub relation;").expect("write TypeQL relation");
            continue;
        }
        write!(&mut out, "{relation_label} sub relation").expect("write TypeQL relation");
        for role in &relation.role_plans {
            write!(
                &mut out,
                ",\n    relates {}",
                typeql_role_label(&role.role_name)
            )
            .expect("write TypeQL role");
        }
        out.push_str(";\n");
    }

    out
}

fn render_typeql_read_queries(relation_plans: &[TypedPushdownRelationPlanV1]) -> String {
    let mut out = String::new();
    for relation in relation_plans {
        let relation_label = typeql_type_label(&relation.relation_name);
        let relation_var = typeql_var(&relation.relation_name);
        writeln!(
            &mut out,
            "# Read {} as a projected relation object.",
            relation.relation_name
        )
        .expect("write TypeQL read comment");
        if relation.role_plans.is_empty() {
            writeln!(&mut out, "match ${relation_var} isa {relation_label};")
                .expect("write TypeQL match");
            writeln!(&mut out, "get ${relation_var};\n").expect("write TypeQL get");
            continue;
        }

        let role_bindings = relation
            .role_plans
            .iter()
            .map(|role| {
                format!(
                    "{}: ${}",
                    typeql_role_label(&role.role_name),
                    typeql_relation_role_var(relation, role)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            &mut out,
            "match ${relation_var} ({role_bindings}) isa {relation_label};"
        )
        .expect("write TypeQL relation match");
        for role in &relation.role_plans {
            writeln!(
                &mut out,
                "${} isa {};",
                typeql_relation_role_var(relation, role),
                typeql_type_label(&role.target_type)
            )
            .expect("write TypeQL role type");
        }
        let mut get_vars = vec![format!("${relation_var}")];
        get_vars.extend(
            relation
                .role_plans
                .iter()
                .map(|role| format!("${}", typeql_relation_role_var(relation, role))),
        );
        writeln!(&mut out, "get {};\n", get_vars.join(", ")).expect("write TypeQL get");
    }
    out
}

fn render_terminusdb_rdf_turtle(
    object_plans: &[ProjectionObjectMappingV1],
    relation_plans: &[TypedPushdownRelationPlanV1],
    context_mapping: &ProjectionContextMappingV1,
    evidence: &BackendPushdownCompiledIrEvidenceV1,
) -> String {
    let mut out = String::new();
    out.push_str("@prefix ax: <https://axiograph.dev/projection/> .\n");
    out.push_str("@prefix axs: <https://axiograph.dev/schema/> .\n");
    out.push_str("@prefix axr: <https://axiograph.dev/relation/> .\n");
    out.push_str("@prefix axrole: <https://axiograph.dev/role/> .\n");
    out.push_str("@prefix axgraph: <https://axiograph.dev/graph/> .\n");
    out.push_str("\n");
    out.push_str("axgraph:schema_main a ax:SchemaGraph .\n");
    out.push_str("axgraph:instance_main a ax:InstanceGraph .\n\n");

    for object_name in projection_object_names(object_plans, relation_plans, evidence) {
        write!(
            &mut out,
            "axs:{} a ax:ObjectType",
            rdf_local_name(&object_name)
        )
        .expect("write RDF object");
        if let Some(supertype) = direct_supertype(&object_name, evidence) {
            write!(
                &mut out,
                " ;\n    ax:subtypeOf axs:{}",
                rdf_local_name(supertype)
            )
            .expect("write RDF subtype");
        }
        out.push_str(" .\n");
    }

    if !context_mapping.axis_bindings.is_empty() {
        out.push('\n');
        for axis in &context_mapping.axis_bindings {
            writeln!(
                &mut out,
                "axgraph:{} a ax:ContextAxis ;\n    ax:backendSlot \"{}\" .",
                rdf_local_name(&axis.axis_name),
                rdf_literal(&axis.backend_slot)
            )
            .expect("write RDF context axis");
        }
    }

    out.push('\n');
    for relation in relation_plans {
        let relation_resource = rdf_local_name(&relation.relation_name);
        writeln!(
            &mut out,
            "axr:{relation_resource} a ax:RelationType ;\n    ax:tupleType axs:{} ;\n    ax:tupleEncoding \"{}\" ;\n    ax:witnessView \"{}\" .",
            rdf_local_name(&relation.tuple_type_name),
            tuple_encoding_label(&relation.tuple_encoding),
            witness_view_label(&relation.witness_view)
        )
        .expect("write RDF relation");
        for role in &relation.role_plans {
            let role_resource = rdf_role_local_name(relation, role);
            writeln!(
                &mut out,
                "axr:{relation_resource} ax:role axrole:{role_resource} ."
            )
            .expect("write RDF role link");
            writeln!(
                &mut out,
                "axrole:{role_resource} a ax:Role ;\n    ax:roleName \"{}\" ;\n    ax:targetType axs:{} ;\n    ax:backendPredicate \"{}\" .",
                rdf_literal(&role.role_name),
                rdf_local_name(&role.target_type),
                rdf_literal(&role.backend_slot)
            )
            .expect("write RDF role");
        }
        if let Some(carrier) = &relation.carrier_edge {
            writeln!(
                &mut out,
                "axr:{relation_resource} ax:losslessCarrierEdge \"{}\" ;\n    ax:sourceRole \"{}\" ;\n    ax:targetRole \"{}\" .",
                rdf_literal(&carrier.edge_label),
                rdf_literal(&carrier.source_role),
                rdf_literal(&carrier.target_role)
            )
            .expect("write RDF carrier");
        }
        out.push('\n');
    }

    out
}

fn render_terminusdb_woql_queries(relation_plans: &[TypedPushdownRelationPlanV1]) -> String {
    let mut out = String::new();
    for relation in relation_plans {
        let relation_label = rdf_local_name(&relation.relation_name);
        let fact_var = format!("v:{}_fact", rdf_local_name(&relation.relation_name));
        writeln!(
            &mut out,
            "// Read {} reified facts from the projected instance graph.",
            relation.relation_name
        )
        .expect("write WOQL comment");
        writeln!(&mut out, "WOQL.and(").expect("write WOQL open");
        writeln!(
            &mut out,
            "  WOQL.quad(\"{fact_var}\", \"rdf:type\", \"axr:{relation_label}\", \"instance/main\"){}",
            if relation.role_plans.is_empty() { "" } else { "," }
        )
        .expect("write WOQL rdf type");
        for (idx, role) in relation.role_plans.iter().enumerate() {
            let suffix = if idx + 1 == relation.role_plans.len() {
                ""
            } else {
                ","
            };
            writeln!(
                &mut out,
                "  WOQL.quad(\"{fact_var}\", \"axrole:{}\", \"v:{}\", \"instance/main\"){suffix}",
                rdf_role_local_name(relation, role),
                rdf_role_local_name(relation, role)
            )
            .expect("write WOQL role");
        }
        out.push_str(");\n\n");
    }
    out
}

fn projection_object_names(
    object_plans: &[ProjectionObjectMappingV1],
    relation_plans: &[TypedPushdownRelationPlanV1],
    evidence: &BackendPushdownCompiledIrEvidenceV1,
) -> Vec<String> {
    let mut names = object_plans
        .iter()
        .map(|object| object.object_name.clone())
        .collect::<BTreeSet<_>>();
    for relation in relation_plans {
        names.insert(relation.tuple_type_name.clone());
        for role in &relation.role_plans {
            names.insert(role.target_type.clone());
        }
    }
    for family in &evidence.direct_subtype_families {
        names.insert(family.supertype.clone());
        names.extend(family.subtypes.iter().cloned());
    }
    names.into_iter().collect()
}

fn direct_supertype<'a>(
    object_name: &str,
    evidence: &'a BackendPushdownCompiledIrEvidenceV1,
) -> Option<&'a str> {
    evidence
        .direct_subtype_families
        .iter()
        .find(|family| family.subtypes.iter().any(|subtype| subtype == object_name))
        .map(|family| family.supertype.as_str())
}

fn typeql_plays_for_object(
    object_name: &str,
    relation_plans: &[TypedPushdownRelationPlanV1],
) -> Vec<String> {
    let mut plays = BTreeSet::new();
    for relation in relation_plans {
        for role in &relation.role_plans {
            if role.target_type == object_name {
                plays.insert(format!(
                    "{}:{}",
                    typeql_type_label(&relation.relation_name),
                    typeql_role_label(&role.role_name)
                ));
            }
        }
    }
    plays.into_iter().collect()
}

fn typeql_relation_role_var(
    relation: &TypedPushdownRelationPlanV1,
    role: &TypedPushdownRolePlanV1,
) -> String {
    format!(
        "{}_{}",
        typeql_var(&relation.relation_name),
        typeql_var(&role.role_name)
    )
}

fn typeql_type_label(label: &str) -> String {
    backend_identifier(label, "type")
}

fn typeql_role_label(label: &str) -> String {
    format!("role_{}", backend_identifier(label, "slot"))
}

fn typeql_var(label: &str) -> String {
    backend_identifier(label, "v")
}

fn rdf_local_name(label: &str) -> String {
    backend_identifier(label, "node")
}

fn rdf_role_local_name(
    relation: &TypedPushdownRelationPlanV1,
    role: &TypedPushdownRolePlanV1,
) -> String {
    format!(
        "{}_{}",
        rdf_local_name(&relation.relation_name),
        rdf_local_name(&role.role_name)
    )
}

fn backend_identifier(label: &str, fallback_prefix: &str) -> String {
    let normalized = normalized_backend_label(label);
    let mut identifier = if normalized.is_empty() {
        fallback_prefix.to_string()
    } else {
        normalized
    };
    if identifier
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        identifier = format!("{fallback_prefix}_{identifier}");
    }
    if is_native_keyword(&identifier) {
        identifier = format!("{identifier}_{fallback_prefix}");
    }
    identifier
}

fn is_native_keyword(identifier: &str) -> bool {
    matches!(
        identifier,
        "and"
            | "attribute"
            | "define"
            | "delete"
            | "entity"
            | "fetch"
            | "from"
            | "get"
            | "has"
            | "insert"
            | "isa"
            | "match"
            | "owns"
            | "plays"
            | "relates"
            | "relation"
            | "rule"
            | "sub"
            | "then"
            | "to"
            | "type"
            | "undefine"
            | "value"
            | "when"
    )
}

fn rdf_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn tuple_encoding_label(encoding: &ProjectionRelationTupleEncodingV1) -> &'static str {
    match encoding {
        ProjectionRelationTupleEncodingV1::RelationNode => "relation_node",
        ProjectionRelationTupleEncodingV1::ReifiedFact => "reified_fact",
        ProjectionRelationTupleEncodingV1::RelationshipEntity => "relationship_entity",
    }
}

fn witness_view_label(view: &PushdownWitnessViewV1) -> &'static str {
    match view {
        PushdownWitnessViewV1::None => "none",
        PushdownWitnessViewV1::Morphism => "morphism",
        PushdownWitnessViewV1::Homotopy => "homotopy",
    }
}

fn build_object_plans(compiled_ir: &CompiledSchemaIr) -> Vec<ProjectionObjectMappingV1> {
    let mut object_names = compiled_ir.object_types.iter().cloned().collect::<Vec<_>>();
    object_names.sort();
    object_names
        .into_iter()
        .map(|object_name| ProjectionObjectMappingV1 {
            backend_label: normalized_backend_label(&object_name),
            object_name,
        })
        .collect()
}

fn compiled_relation_plans(
    compiled_ir: &CompiledSchemaIr,
    projection: &ProjectionCapabilityProfileV1,
    tuple_encoding: ProjectionRelationTupleEncodingV1,
    native_query_dialect: BackendNativeQueryDialectV1,
    materialize_carrier_edge: bool,
) -> Vec<TypedPushdownRelationPlanV1> {
    let mut relation_names = compiled_ir.relations.keys().cloned().collect::<Vec<_>>();
    relation_names.sort();
    relation_names
        .into_iter()
        .filter_map(|name| compiled_ir.relation(&name))
        .map(|relation| {
            typed_relation_pushdown_plan(
                compiled_ir,
                relation,
                projection,
                tuple_encoding.clone(),
                native_query_dialect.clone(),
                materialize_carrier_edge,
            )
        })
        .collect()
}

fn typed_relation_pushdown_plan(
    compiled_ir: &CompiledSchemaIr,
    relation: &RelationSemanticsIr,
    projection: &ProjectionCapabilityProfileV1,
    tuple_encoding: ProjectionRelationTupleEncodingV1,
    native_query_dialect: BackendNativeQueryDialectV1,
    materialize_carrier_edge: bool,
) -> TypedPushdownRelationPlanV1 {
    let role_plans = relation
        .roles
        .iter()
        .map(|role| {
            let preserved_explicitly = match role.kind {
                RoleKind::Data => true,
                RoleKind::Context | RoleKind::Temporal => projection.preserves_context_world_axes,
            };
            let scoped_role_name = format!("{}:{}", relation.name, role.name);
            let admissible_player_types = compiled_ir
                .role_interface(&relation.name, &role.name)
                .map(|interface| interface.admissible_player_types.clone())
                .unwrap_or_else(|| compiled_ir.admissible_player_types(&role.target_type));
            TypedPushdownRolePlanV1 {
                role_name: role.name.clone(),
                scoped_role_name,
                target_type: role.target_type.clone(),
                admissible_player_types,
                role_kind: pushdown_role_kind(role.kind),
                backend_slot: role_backend_slot(
                    relation.name.as_str(),
                    role.name.as_str(),
                    &native_query_dialect,
                ),
                preserved_explicitly,
            }
        })
        .collect::<Vec<_>>();

    let mut semantic_losses = Vec::new();
    if relation
        .roles
        .iter()
        .any(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        && !projection.preserves_context_world_axes
    {
        semantic_losses.push(
            "context/temporal roles are preserved only as reduced projection metadata".to_string(),
        );
    }
    if relation.roles.len() > 2 && !projection.preserves_nary_relation_objects {
        semantic_losses.push(
            "n-ary tuple structure is degraded because the projection profile does not preserve n-ary relation objects"
                .to_string(),
        );
    }

    let carrier_edge =
        if materialize_carrier_edge && projection.binary_carrier_edges_only_when_lossless {
            relation.carrier.as_ref().and_then(|carrier| {
                let source_role = relation.roles.get(carrier.source_role as usize)?;
                let target_role = relation.roles.get(carrier.target_role as usize)?;
                Some(ProjectionCarrierEdgeMappingV1 {
                    edge_label: format!("{}__carrier", normalized_backend_label(&relation.name)),
                    source_role: source_role.name.clone(),
                    target_role: target_role.name.clone(),
                })
            })
        } else {
            None
        };

    TypedPushdownRelationPlanV1 {
        relation_name: relation.name.clone(),
        tuple_type_name: relation.tuple_type_name.clone(),
        tuple_encoding,
        tuple_label: normalized_backend_label(&relation.tuple_type_name),
        witness_view: pushdown_witness_view(relation.witness_view),
        role_plans,
        carrier_edge,
        preserved_losslessly: semantic_losses.is_empty(),
        semantic_losses,
    }
}

fn compiled_context_axis_bindings(
    compiled_ir: &CompiledSchemaIr,
    prefix: &str,
) -> Vec<ProjectionContextAxisBindingV1> {
    let mut bindings = compiled_ir
        .relations
        .values()
        .flat_map(|relation| relation.roles.iter())
        .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        .map(|role| ProjectionContextAxisBindingV1 {
            axis_name: role.name.clone(),
            backend_slot: format!("{prefix}:{}", normalized_backend_label(&role.name)),
        })
        .collect::<Vec<_>>();
    bindings.sort_by(|lhs, rhs| lhs.axis_name.cmp(&rhs.axis_name));
    bindings.dedup_by(|lhs, rhs| lhs.axis_name == rhs.axis_name);
    bindings
}

fn typedb_preserved_interfaces(
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<PreservedLowerTierInterfaceV1> {
    let mut interfaces = vec![PreservedLowerTierInterfaceV1 {
        interface_kind: PreservedLowerTierInterfaceKindV1::NativeQuery,
        surface_label: "typeql".to_string(),
        preservation_class: match projection.native_query_access {
            ProjectionNativeQueryAccessV1::None => "disabled".to_string(),
            ProjectionNativeQueryAccessV1::ReadOnlyPartial => "read_only_partial".to_string(),
            ProjectionNativeQueryAccessV1::ReadOnlyAnchorScoped => {
                "read_only_anchor_scoped".to_string()
            }
        },
        caveats: vec![
            "native TypeQL remains a lower-tier interface under Axiograph semantic authority"
                .to_string(),
            "mutation, lifecycle, trust contracts, and promotion remain Axiograph-only".to_string(),
        ],
    }];
    if !projection.preserves_context_world_axes {
        interfaces[0]
            .caveats
            .push("context/world semantics may be reduced in the native TypeQL view".to_string());
    }
    interfaces
}

fn typedb_lifting_contracts() -> Vec<SemanticLiftingContractV1> {
    vec![SemanticLiftingContractV1 {
        source_surface_label: "typeql".to_string(),
        lifted_into: "accepted .axi + compiled schema ir + trust contracts".to_string(),
        preservation_claim: "typed relation/role structure is preserved natively, then lifted into anchor-scoped typed semantics"
            .to_string(),
        caveats: vec![
            "native backend answers remain lower-tier until reinterpreted under Axiograph anchors and lifecycle state"
                .to_string(),
        ],
    }]
}

fn terminusdb_preserved_interfaces(
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<PreservedLowerTierInterfaceV1> {
    let mut interfaces = vec![
        PreservedLowerTierInterfaceV1 {
            interface_kind: PreservedLowerTierInterfaceKindV1::NativeQuery,
            surface_label: "woql".to_string(),
            preservation_class: match projection.native_query_access {
                ProjectionNativeQueryAccessV1::None => "disabled".to_string(),
                ProjectionNativeQueryAccessV1::ReadOnlyPartial => "read_only_partial".to_string(),
                ProjectionNativeQueryAccessV1::ReadOnlyAnchorScoped => {
                    "read_only_anchor_scoped".to_string()
                }
            },
            caveats: vec![
                "WOQL is preserved as a readable lower-tier interface over the projected dataset"
                    .to_string(),
                "native WOQL results remain reduced until lifted through Axiograph anchors and trust contracts"
                    .to_string(),
            ],
        },
        PreservedLowerTierInterfaceV1 {
            interface_kind: PreservedLowerTierInterfaceKindV1::RdfDataset,
            surface_label: "rdf_named_graph_dataset".to_string(),
            preservation_class: "preserved_lower_tier_interface".to_string(),
            caveats: vec![
                "named graphs preserve context/world indexing but do not by themselves recover the full higher typed ontology semantics"
                    .to_string(),
                "relation-objects remain canonical even when readable as RDF-style reified facts".to_string(),
            ],
        },
        PreservedLowerTierInterfaceV1 {
            interface_kind: PreservedLowerTierInterfaceKindV1::ShaclValidation,
            surface_label: "shacl_over_projected_rdf".to_string(),
            preservation_class: "closed_world_validation_interface".to_string(),
            caveats: vec![
                "SHACL remains a lower-tier validation interface, not the semantic kernel".to_string(),
                "validation outcomes must still be lifted into typed obligations and trust contracts in Axiograph"
                    .to_string(),
            ],
        },
    ];
    if !projection.preserves_context_world_axes {
        interfaces[1].caveats.push(
            "context/world structure is only partially preserved in the projected RDF view"
                .to_string(),
        );
    }
    interfaces
}

fn terminusdb_lifting_contracts() -> Vec<SemanticLiftingContractV1> {
    vec![
        SemanticLiftingContractV1 {
            source_surface_label: "rdf_named_graph_dataset".to_string(),
            lifted_into: "accepted .axi + compiled schema ir + typed relation-objects".to_string(),
            preservation_claim: "projected RDF remains readable as a preserved lower-tier dataset and lifts back into relation-object semantics"
                .to_string(),
            caveats: vec![
                "named-graph readability does not itself confer full ontology closure or lifecycle semantics"
                    .to_string(),
            ],
        },
        SemanticLiftingContractV1 {
            source_surface_label: "shacl_over_projected_rdf".to_string(),
            lifted_into: "typed review obligations + CQ gates + trust contracts".to_string(),
            preservation_claim: "SHACL validation is preserved as a lower-tier closed-world interface and lifted into Axiograph review semantics"
                .to_string(),
            caveats: vec![
                "SHACL validation alone does not replace the higher typed semantic kernel".to_string(),
            ],
        },
    ]
}

fn common_trust_caveats(
    backend: &BackendCapabilityProfileV1,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<String> {
    let mut caveats = Vec::new();
    match projection.native_query_access {
        ProjectionNativeQueryAccessV1::None => {
            caveats.push("native backend querying is disabled for this projection".to_string());
        }
        ProjectionNativeQueryAccessV1::ReadOnlyPartial => {
            caveats.push(
                "native backend querying is read-only and may expose only a partial projected surface"
                    .to_string(),
            );
        }
        ProjectionNativeQueryAccessV1::ReadOnlyAnchorScoped => {
            caveats.push(
                "native backend querying is read-only and expected to stay anchor-scoped"
                    .to_string(),
            );
        }
    }
    if !projection.supports_anchor_scoped_query_pushdown {
        caveats.push(
            "backend-native queries are not anchor-scoped by construction and require Axiograph-side trust checks"
                .to_string(),
        );
    }
    if backend.branching_and_merge || backend.diff_and_patch || backend.immutable_history {
        caveats.push(
            "backend history features do not replace semantic refs, CQ gates, or reconciliation in Axiograph"
                .to_string(),
        );
    }
    caveats
}

fn collect_global_semantic_losses(
    compiled_ir: &CompiledSchemaIr,
    projection: &ProjectionCapabilityProfileV1,
) -> Vec<String> {
    let mut losses = Vec::new();
    if !projection.preserves_context_world_axes
        && compiled_ir.relations.values().any(|relation| {
            relation
                .roles
                .iter()
                .any(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        })
    {
        losses.push(
            "context/world structure is reduced in the backend projection and must remain explicit in Axiograph trust contracts"
                .to_string(),
        );
    }
    if !projection.preserves_provenance_links {
        losses.push(
            "provenance links are not fully preserved in the projected backend surface".to_string(),
        );
    }
    if !projection.preserves_evidence_objects {
        losses.push(
            "evidence-plane objects are not fully materialized in the projected backend surface"
                .to_string(),
        );
    }
    losses
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn pushdown_role_kind(kind: RoleKind) -> PushdownRoleKindV1 {
    match kind {
        RoleKind::Data => PushdownRoleKindV1::Data,
        RoleKind::Context => PushdownRoleKindV1::Context,
        RoleKind::Temporal => PushdownRoleKindV1::Temporal,
    }
}

fn pushdown_witness_view(view: WitnessViewIr) -> PushdownWitnessViewV1 {
    match view {
        WitnessViewIr::None => PushdownWitnessViewV1::None,
        WitnessViewIr::Morphism { .. } => PushdownWitnessViewV1::Morphism,
        WitnessViewIr::Homotopy { .. } => PushdownWitnessViewV1::Homotopy,
    }
}

fn role_backend_slot(
    relation_name: &str,
    role_name: &str,
    native_query_dialect: &BackendNativeQueryDialectV1,
) -> String {
    match native_query_dialect {
        BackendNativeQueryDialectV1::TypeQl => {
            format!(
                "role:{}:{}",
                normalized_backend_label(relation_name),
                normalized_backend_label(role_name)
            )
        }
        BackendNativeQueryDialectV1::Woql => {
            format!(
                "predicate:{}:{}",
                normalized_backend_label(relation_name),
                normalized_backend_label(role_name)
            )
        }
    }
}

fn normalized_backend_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 4);
    let mut prev_was_sep = false;
    for (idx, ch) in label.chars().enumerate() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if idx > 0 && !prev_was_sep {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch.to_ascii_lowercase());
            }
            prev_was_sep = false;
        } else if !prev_was_sep {
            out.push('_');
            prev_was_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_dsl::schema_v1::{FieldDeclV1, RelationDeclV1, SchemaV1Schema, SubtypeDeclV1};
    use axiograph_pathdb::kernel_ir::compile_schema_ir;

    fn sample_schema() -> SchemaV1Schema {
        SchemaV1Schema {
            name: "Commerce".to_string(),
            objects: vec![
                "Person".to_string(),
                "Reviewer".to_string(),
                "Request".to_string(),
                "Context".to_string(),
                "Time".to_string(),
            ],
            subtypes: vec![
                SubtypeDeclV1 {
                    sub: "Reviewer".to_string(),
                    sup: "Person".to_string(),
                    inclusion: None,
                },
                SubtypeDeclV1 {
                    sub: "ReviewContext".to_string(),
                    sup: "Context".to_string(),
                    inclusion: None,
                },
            ],
            relations: vec![
                RelationDeclV1 {
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
                        FieldDeclV1 {
                            field: "ctx".to_string(),
                            ty: "ReviewContext".to_string(),
                        },
                        FieldDeclV1 {
                            field: "time".to_string(),
                            ty: "Time".to_string(),
                        },
                    ],
                },
                RelationDeclV1 {
                    name: "EscalatesTo".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "from".to_string(),
                            ty: "Request".to_string(),
                        },
                        FieldDeclV1 {
                            field: "to".to_string(),
                            ty: "Request".to_string(),
                        },
                    ],
                },
            ],
        }
    }

    fn typed_projection() -> ProjectionCapabilityProfileV1 {
        ProjectionCapabilityProfileV1 {
            preserves_relation_objects: true,
            binary_carrier_edges_only_when_lossless: true,
            preserves_nary_relation_objects: true,
            preserves_context_world_axes: true,
            preserves_evidence_objects: true,
            preserves_provenance_links: true,
            supports_anchor_scoped_query_pushdown: true,
            supports_context_scoped_query_pushdown: true,
            native_query_access: ProjectionNativeQueryAccessV1::ReadOnlyAnchorScoped,
            mutation_authority: ProjectionMutationAuthorityV1::AxiographOnly,
        }
    }

    #[test]
    fn typedb_pushdown_plan_preserves_relation_objects_and_context_axes() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let plan = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::typedb_primary(),
            &typed_projection(),
        )
        .expect("typedb plan");

        let BackendPushdownPlanV1::TypeDb(plan) = plan else {
            panic!("expected typedb plan");
        };
        assert_eq!(
            plan.core.native_query_dialect,
            BackendNativeQueryDialectV1::TypeQl
        );
        assert_eq!(
            plan.core.context_mapping.strategy,
            ProjectionContextMappingStrategyV1::SeparateNamespaces
        );
        assert!(plan.core.projection.requires_axiograph_mutation_authority());
        assert_eq!(plan.core.preserved_interfaces.len(), 1);
        assert_eq!(
            plan.core.preserved_interfaces[0].interface_kind,
            PreservedLowerTierInterfaceKindV1::NativeQuery
        );
        assert!(plan
            .core
            .lifting_contracts
            .iter()
            .any(|contract| contract.source_surface_label == "typeql"));
        assert!(plan
            .core
            .trust_caveats
            .iter()
            .any(|msg| msg.contains("read-only")));
        assert_eq!(plan.core.compiled_ir_evidence.relation_count, 2);
        assert_eq!(plan.core.compiled_ir_evidence.nary_relation_count, 1);
        assert!(!plan.core.compiled_ir_evidence.kernel_refs.is_empty());
        assert!(plan
            .core
            .compiled_ir_evidence
            .kernel_ref_labels
            .iter()
            .any(|label| label.contains("schema_object") && label.contains("Approval")));
        assert!(plan
            .core
            .compiled_ir_evidence
            .kernel_ref_labels
            .iter()
            .any(|label| label.contains("schema_arrow") && label.contains("approver")));
        assert!(plan
            .core
            .compiled_ir_evidence
            .direct_subtype_families
            .iter()
            .any(|family| family.supertype == "Person"));
        assert!(plan.core.capability_decisions.iter().any(|decision| {
            decision.profile == "backend"
                && decision.capability == "constraints"
                && decision.satisfied
        }));
        assert!(plan.core.native_projection_notes.iter().any(|note| {
            note.surface_label == "typedb_schema_constraints"
                && note.backend_native_shape.contains("subtype declarations")
                && !note.kernel_refs.is_empty()
        }));
        assert!(plan.typed_projection_shape.relation_roles_native);
        assert!(plan.typed_projection_shape.nary_relations_native);
        assert!(plan.typed_projection_shape.subtype_hierarchy_native);
        assert!(plan.typed_projection_shape.schema_constraints_native);
        assert!(plan.typed_projection_shape.typed_query_validation_native);
        let typeql_schema = plan
            .core
            .native_artifacts
            .iter()
            .find(|artifact| {
                artifact.artifact_kind == BackendNativeProjectionArtifactKindV1::TypeQlSchema
            })
            .expect("TypeQL schema artifact");
        assert_eq!(
            typeql_schema.mutation_authority,
            ProjectionMutationAuthorityV1::AxiographOnly
        );
        assert!(typeql_schema.body.contains("reviewer sub person"));
        assert!(typeql_schema.body.contains("approval sub relation"));
        assert!(typeql_schema.body.contains("plays approval:role_approver"));
        assert!(typeql_schema
            .body
            .contains("axiograph_context_axis_ctx sub axiograph_object"));
        assert!(typeql_schema
            .caveats
            .iter()
            .any(|caveat| caveat.contains("not the accepted ontology source")));
        assert_eq!(
            typeql_schema.kernel_ref_labels,
            plan.core.compiled_ir_evidence.kernel_ref_labels
        );
        let typeql_reads = plan
            .core
            .native_artifacts
            .iter()
            .find(|artifact| {
                artifact.artifact_kind == BackendNativeProjectionArtifactKindV1::TypeQlReadQuery
            })
            .expect("TypeQL read artifact");
        assert!(typeql_reads.body.contains("match $approval"));
        assert!(typeql_reads
            .body
            .contains("role_approver: $approval_approver"));

        let approval = plan
            .core
            .relation_plans
            .iter()
            .find(|rel| rel.relation_name == "Approval")
            .expect("approval relation");
        assert_eq!(
            approval.tuple_encoding,
            ProjectionRelationTupleEncodingV1::RelationshipEntity
        );
        assert!(approval.carrier_edge.is_none());
        assert!(approval.preserved_losslessly);
        let approver = approval
            .role_plans
            .iter()
            .find(|role| role.role_name == "approver")
            .expect("approver role");
        assert_eq!(approver.scoped_role_name, "Approval:approver");
        assert_eq!(
            approver.admissible_player_types,
            vec!["Person".to_string(), "Reviewer".to_string()]
        );
        assert!(approval
            .role_plans
            .iter()
            .any(|role| role.role_kind == PushdownRoleKindV1::Context));
        assert!(approval
            .role_plans
            .iter()
            .any(|role| role.role_kind == PushdownRoleKindV1::Temporal));
    }

    #[test]
    fn typedb_operational_surface_exposes_transport_summary_and_reconciliation_boundary() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let plan = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::typedb_primary(),
            &typed_projection(),
        )
        .expect("typedb plan");

        let surface = plan.operational_surface();

        assert_eq!(surface.backend_label, "TypeDB");
        assert_eq!(
            surface.native_query_dialect,
            BackendNativeQueryDialectV1::TypeQl
        );
        assert_eq!(surface.trust.trust_class, "projected_read_surface");
        assert_eq!(surface.trust.completeness_claim, "not_claimed");
        assert_eq!(surface.trust.ontology_closure_claim, "not_claimed");
        assert!(surface
            .trust
            .notes
            .iter()
            .any(|note| note.contains("does not claim complete answers")));
        assert_eq!(
            surface.compiled_ir_evidence.source_surface,
            "compiled_schema_ir"
        );
        assert_eq!(
            surface.kernel_ref_labels,
            surface.compiled_ir_evidence.kernel_ref_labels
        );
        assert_eq!(
            surface.kernel_refs,
            surface.compiled_ir_evidence.kernel_refs
        );
        assert!(surface
            .capability_decisions
            .iter()
            .any(|decision| decision.capability == "mutation_authority" && decision.satisfied));
        assert!(surface
            .native_projection_notes
            .iter()
            .any(|note| note.surface_label == "typeql"
                && note.read_contract.contains("anchor-scoped")));
        assert!(surface.native_artifacts.iter().any(|artifact| {
            artifact.artifact_label == "typedb_typeql_read_queries"
                && artifact.read_contract.contains("anchor-scoped")
        }));
        assert_eq!(surface.relation_transport.total_relations, 2);
        assert_eq!(surface.relation_transport.lossless_relations, 2);
        assert_eq!(surface.relation_transport.reduced_relations, 0);
        assert_eq!(
            surface.context_transport.strategy,
            ProjectionContextMappingStrategyV1::SeparateNamespaces
        );
        assert!(surface
            .residual_obligations
            .iter()
            .any(|msg| msg.contains("lifted through the compiled IR")));
        assert!(surface
            .reconciliation_boundary
            .iter()
            .any(|msg| msg.contains("CQ gates")));
    }

    #[test]
    fn terminusdb_pushdown_plan_uses_named_graphs_and_reified_facts() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let plan = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::terminusdb_supported(),
            &typed_projection(),
        )
        .expect("terminus plan");

        let BackendPushdownPlanV1::TerminusDb(plan) = plan else {
            panic!("expected terminus plan");
        };
        assert_eq!(
            plan.core.native_query_dialect,
            BackendNativeQueryDialectV1::Woql
        );
        assert_eq!(
            plan.core.context_mapping.strategy,
            ProjectionContextMappingStrategyV1::NamedGraphs
        );
        assert!(plan
            .core
            .preserved_interfaces
            .iter()
            .any(|iface| iface.interface_kind == PreservedLowerTierInterfaceKindV1::RdfDataset));
        assert!(plan.core.preserved_interfaces.iter().any(
            |iface| iface.interface_kind == PreservedLowerTierInterfaceKindV1::ShaclValidation
        ));
        assert!(plan
            .core
            .lifting_contracts
            .iter()
            .any(|contract| contract.source_surface_label == "rdf_named_graph_dataset"));
        assert!(plan
            .core
            .trust_caveats
            .iter()
            .any(|msg| msg.contains("semantic refs")));
        assert_eq!(plan.core.compiled_ir_evidence.context_axis_count, 2);
        assert!(plan
            .core
            .compiled_ir_evidence
            .kernel_ref_labels
            .iter()
            .any(|label| label.contains("schema_arrow") && label.contains("from")));
        assert!(plan.core.capability_decisions.iter().any(|decision| {
            decision.profile == "backend"
                && decision.capability == "branching_and_merge"
                && decision.satisfied
        }));
        assert!(plan.core.native_projection_notes.iter().any(|note| {
            note.surface_label == "terminusdb_branch_history"
                && note.read_contract.contains("not semantic VCS authority")
        }));
        assert!(
            plan.rdf_vcs_projection_shape
                .rdf_named_graph_dataset_readable
        );
        assert!(
            plan.rdf_vcs_projection_shape
                .named_graph_context_world_mapping
        );
        assert!(
            plan.rdf_vcs_projection_shape
                .schema_instance_graph_separation
        );
        assert!(plan.rdf_vcs_projection_shape.branch_history_mirroring);
        let rdf_artifact = plan
            .core
            .native_artifacts
            .iter()
            .find(|artifact| {
                artifact.artifact_kind == BackendNativeProjectionArtifactKindV1::RdfTurtleSchema
            })
            .expect("RDF artifact");
        assert!(rdf_artifact
            .body
            .contains("@prefix ax: <https://axiograph.dev/projection/>"));
        assert_eq!(
            rdf_artifact.kernel_refs,
            plan.core.compiled_ir_evidence.kernel_refs
        );
        assert!(rdf_artifact
            .body
            .contains("ax:tupleEncoding \"reified_fact\""));
        assert!(rdf_artifact.body.contains("axgraph:schema_main"));
        assert!(rdf_artifact
            .caveats
            .iter()
            .any(|caveat| caveat.contains("not the semantic kernel")));
        let woql_artifact = plan
            .core
            .native_artifacts
            .iter()
            .find(|artifact| {
                artifact.artifact_kind == BackendNativeProjectionArtifactKindV1::WoqlReadQuery
            })
            .expect("WOQL artifact");
        assert!(woql_artifact.body.contains("WOQL.quad"));
        assert!(woql_artifact.body.contains("instance/main"));
        assert!(woql_artifact
            .caveats
            .iter()
            .any(|caveat| caveat.contains("cannot promote")));

        let escalates = plan
            .core
            .relation_plans
            .iter()
            .find(|rel| rel.relation_name == "EscalatesTo")
            .expect("escalates relation");
        assert_eq!(
            escalates.tuple_encoding,
            ProjectionRelationTupleEncodingV1::ReifiedFact
        );
        let carrier = escalates.carrier_edge.as_ref().expect("carrier edge");
        assert_eq!(carrier.source_role, "from");
        assert_eq!(carrier.target_role, "to");
    }

    #[test]
    fn reduced_terminus_operational_surface_reports_anchor_and_context_obligations() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let mut projection = typed_projection();
        projection.preserves_context_world_axes = false;
        projection.supports_anchor_scoped_query_pushdown = false;
        projection.supports_context_scoped_query_pushdown = false;
        projection.native_query_access = ProjectionNativeQueryAccessV1::ReadOnlyPartial;
        let plan = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::terminusdb_supported(),
            &projection,
        )
        .expect("terminus plan");

        let surface = plan.operational_surface();

        assert_eq!(surface.trust.trust_class, "projected_read_surface");
        assert_eq!(surface.trust.completeness_claim, "not_claimed");
        assert_eq!(surface.relation_transport.total_relations, 2);
        assert_eq!(surface.relation_transport.lossless_relations, 1);
        assert_eq!(surface.relation_transport.reduced_relations, 1);
        assert!(surface
            .residual_obligations
            .iter()
            .any(|msg| msg.contains("anchor-scoped rechecks")));
        assert!(surface
            .residual_obligations
            .iter()
            .any(|msg| msg.contains("Axiograph-side reindexing")));
        assert!(surface
            .reconciliation_boundary
            .iter()
            .any(|msg| msg.contains("stored validation reports")));
    }

    #[test]
    fn backend_pushdown_rejects_backend_mutation_authority() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let mut projection = typed_projection();
        projection.mutation_authority = ProjectionMutationAuthorityV1::BackendWritableMirror;
        let err = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::typedb_primary(),
            &projection,
        )
        .expect_err("writable mirror must be rejected");

        assert!(err
            .to_string()
            .contains("backend mutation authority is not allowed"));
    }

    #[test]
    fn typedb_pushdown_rejects_profiles_that_drop_relation_objects() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let mut projection = typed_projection();
        projection.preserves_relation_objects = false;
        let err = build_backend_pushdown_plan(
            &compiled_ir,
            &BackendCapabilityProfileV1::typedb_primary(),
            &projection,
        )
        .expect_err("typedb should reject lossy relation-object projection");

        assert!(err
            .to_string()
            .contains("typedb pushdown requires relation-object"));
    }

    #[test]
    fn typedb_pushdown_rejects_profiles_without_schema_constraint_capabilities() {
        let compiled_ir = compile_schema_ir(&sample_schema());
        let mut backend = BackendCapabilityProfileV1::typedb_primary();
        backend.constraints = false;
        let err = build_backend_pushdown_plan(&compiled_ir, &backend, &typed_projection())
            .expect_err("typedb should reject missing schema constraint support");

        assert!(err
            .to_string()
            .contains("typedb pushdown requires native constraints"));
    }
}
