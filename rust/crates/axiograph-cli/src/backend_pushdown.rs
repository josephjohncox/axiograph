#![allow(dead_code)]

use anyhow::{bail, Result};
use axiograph_pathdb::kernel_ir::{CompiledSchemaIr, RelationSemanticsIr, RoleKind, WitnessViewIr};
use serde::{Deserialize, Serialize};

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
pub struct BackendPushdownCoreV1 {
    pub version: String,
    pub backend: BackendCapabilityProfileV1,
    pub projection: ProjectionCapabilityProfileV1,
    pub native_query_dialect: BackendNativeQueryDialectV1,
    #[serde(default)]
    pub object_plans: Vec<ProjectionObjectMappingV1>,
    #[serde(default)]
    pub relation_plans: Vec<TypedPushdownRelationPlanV1>,
    pub context_mapping: ProjectionContextMappingV1,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminusDbPushdownPlanV1 {
    pub core: BackendPushdownCoreV1,
    pub schema_graph_label: String,
    pub instance_graph_label: String,
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
    if !projection.preserves_relation_objects || !projection.preserves_nary_relation_objects {
        bail!("typedb pushdown requires relation-object and n-ary preservation");
    }

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
    let preserved_interfaces = typedb_preserved_interfaces(projection);
    let lifting_contracts = typedb_lifting_contracts();

    Ok(TypeDbPushdownPlanV1 {
        core: BackendPushdownCoreV1 {
            version: BACKEND_PUSHDOWN_PLAN_VERSION_V1.to_string(),
            backend: backend.clone(),
            projection: projection.clone(),
            native_query_dialect: BackendNativeQueryDialectV1::TypeQl,
            object_plans,
            relation_plans,
            context_mapping,
            preserved_interfaces,
            lifting_contracts,
            trust_caveats,
            semantic_losses,
        },
        typeql_relation_kind: "relation".to_string(),
        typeql_role_prefix: "role".to_string(),
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
    if !projection.preserves_relation_objects {
        bail!("terminusdb pushdown requires relation-object preservation");
    }

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

    Ok(TerminusDbPushdownPlanV1 {
        core: BackendPushdownCoreV1 {
            version: BACKEND_PUSHDOWN_PLAN_VERSION_V1.to_string(),
            backend: backend.clone(),
            projection: projection.clone(),
            native_query_dialect: BackendNativeQueryDialectV1::Woql,
            object_plans,
            relation_plans,
            context_mapping,
            preserved_interfaces,
            lifting_contracts,
            trust_caveats,
            semantic_losses,
        },
        schema_graph_label: "schema/main".to_string(),
        instance_graph_label: "instance/main".to_string(),
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
}
