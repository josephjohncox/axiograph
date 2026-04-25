use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use axiograph_pathdb::{
    kernel_ir::{
        compile_instance_functor_ir, compile_schema_category_ir, InstanceIr, KernelModuleIr,
        SchemaCategoryArrowRefIr, SchemaCategoryObjectRefIr, TheoryIr, TheorySubjectRefIr,
    },
    AcceptedSnapshotId, AxiDigest, ProposalDigest, WorldModelRunId,
};

pub const SEMANTIC_SLICE_MANIFEST_VERSION_V1: &str = "semantic_slice_manifest_v1";
pub const SEMANTIC_MERGE_LATTICE_VERSION_V1: &str = "semantic_merge_lattice_v1";
pub const SEMANTIC_MERGE_PLAN_VERSION_V1: &str = "semantic_merge_plan_v1";
pub const SEMANTIC_SLICE_DIFF_VERSION_V1: &str = "semantic_slice_diff_v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMergeOperationKindV1 {
    Merge,
    Rebase,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSliceTrustClassV1 {
    CertifiableFragment,
    RuntimeChecked,
    ReviewOnly,
    EvidenceBacked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSliceRefKindV1 {
    Commit,
    Module,
    SchemaObject,
    RelationObject,
    RoleProjection,
    SubtypeInclusion,
    TheoryObligation,
    InstanceFunctor,
    ContextWorld,
    CompetencyQuestion,
    BehaviorCase,
    ImplementationSurface,
    Evidence,
    ExplicitIrRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticSliceRefV1 {
    pub kind: SemanticSliceRefKindV1,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SemanticSliceSelectorV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_object_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory_obligation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competency_question_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behavior_case_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implementation_surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_model_run_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explicit_ir_refs: Vec<String>,
}

impl SemanticSliceSelectorV1 {
    pub fn is_empty(&self) -> bool {
        self.schema_ids.is_empty()
            && self.relation_object_ids.is_empty()
            && self.role_ids.is_empty()
            && self.theory_obligation_ids.is_empty()
            && self.context_ids.is_empty()
            && self.competency_question_names.is_empty()
            && self.behavior_case_ids.is_empty()
            && self.implementation_surface_ids.is_empty()
            && self.world_model_run_ids.is_empty()
            && self.explicit_ir_refs.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSliceManifestV1 {
    pub version: String,
    pub slice_id: AxiDigest,
    pub label: String,
    pub base_ref_name: String,
    pub commit_id: AxiDigest,
    pub accepted_snapshot_id: AcceptedSnapshotId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_ir_digest: Option<AxiDigest>,
    pub trust_class: SemanticSliceTrustClassV1,
    pub selector: SemanticSliceSelectorV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_refs: Vec<SemanticSliceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposal_digests: Vec<ProposalDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_model_run_ids: Vec<WorldModelRunId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSliceEdgeKindV1 {
    Inclusion,
    Overlap,
    Dependency,
    Conflict,
    MissingSupport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSliceEdgeV1 {
    pub source_slice_id: AxiDigest,
    pub target_slice_id: AxiDigest,
    pub kind: SemanticSliceEdgeKindV1,
    pub trust_class: SemanticSliceTrustClassV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_refs: Vec<SemanticSliceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticMergeLatticeV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<SemanticSliceManifestV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<SemanticSliceEdgeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticAutoJoinDecisionV1 {
    pub decision_id: String,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slice_ids: Vec<AxiDigest>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticMergeConflictV1 {
    pub conflict_id: String,
    pub artifact_kind: String,
    pub artifact_id: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_refs: Vec<SemanticSliceRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMergePlanV1 {
    pub version: String,
    pub operation: SemanticMergeOperationKindV1,
    pub plan_id: AxiDigest,
    pub source_ref_name: String,
    pub target_ref_name: String,
    pub base_commit_id: AxiDigest,
    pub source_commit_id: AxiDigest,
    pub target_commit_id: AxiDigest,
    pub policy: String,
    pub source_slice: SemanticSliceManifestV1,
    pub target_slice: SemanticSliceManifestV1,
    pub lattice: SemanticMergeLatticeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_join_decisions: Vec<SemanticAutoJoinDecisionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<SemanticMergeConflictV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolver_steps: Vec<crate::typed_refinement::RuntimeRefinementHandleV1>,
    pub can_materialize: bool,
    pub evolution_preview: crate::evolution_preview::EvolutionPreviewV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSliceDiffV1 {
    pub version: String,
    pub left_slice_id: AxiDigest,
    pub right_slice_id: AxiDigest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_refs: Vec<SemanticSliceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_refs: Vec<SemanticSliceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_refs: Vec<SemanticSliceRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticResolverStepsReportV1 {
    pub version: String,
    pub plan_id: AxiDigest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolver_steps: Vec<crate::typed_refinement::RuntimeRefinementHandleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

pub fn semantic_slice_from_ref_view(
    view: &crate::accepted_plane::SemRefViewV1,
    selector: SemanticSliceSelectorV1,
) -> SemanticSliceManifestV1 {
    let refs = semantic_refs_from_commit(&view.commit);
    let label = selector
        .label
        .clone()
        .unwrap_or_else(|| view.pointer.ref_name.clone());
    let refs = selected_refs_for_selector(&selector, refs);
    let slice_id = digest_semantic_slice(
        &label,
        &view.pointer.ref_name,
        &view.pointer.commit_id,
        &selector,
        &refs,
    );
    let world_model_run_ids = view
        .commit
        .world_model_run_id
        .clone()
        .into_iter()
        .chain(view.commit.delta.world_model_run_refs.iter().cloned())
        .collect();
    SemanticSliceManifestV1 {
        version: SEMANTIC_SLICE_MANIFEST_VERSION_V1.to_string(),
        slice_id,
        label,
        base_ref_name: view.pointer.ref_name.clone(),
        commit_id: view.pointer.commit_id.clone(),
        accepted_snapshot_id: view.commit.accepted_snapshot_id.clone(),
        kernel_ir_digest: view
            .commit
            .state
            .accepted_tree_digest
            .clone()
            .or_else(|| Some(view.commit.module_digest.clone())),
        trust_class: trust_class_for_commit(&view.commit),
        selector,
        selected_refs: refs,
        proposal_digests: view.commit.proposal_digests.clone(),
        world_model_run_ids,
        notes: vec![
            "semantic slice is a finite runtime restriction over accepted typed ontology refs; it is not a completeness claim".to_string(),
            "relation objects, role projections, subtype inclusions, theory obligations, and instance functor refs are preserved when present in the commit delta or compiled kernel IR".to_string(),
        ],
    }
}

pub fn enrich_semantic_slice_with_kernel_module_ir(
    mut manifest: SemanticSliceManifestV1,
    kernel: &KernelModuleIr,
) -> SemanticSliceManifestV1 {
    let mut refs = manifest.selected_refs;
    refs.extend(semantic_refs_from_kernel_module_ir(kernel));
    let refs = selected_refs_for_selector(&manifest.selector, refs);
    manifest.kernel_ir_digest = manifest
        .kernel_ir_digest
        .clone()
        .or_else(|| Some(kernel.module_digest.clone()));
    manifest.slice_id = digest_semantic_slice(
        &manifest.label,
        &manifest.base_ref_name,
        &manifest.commit_id,
        &manifest.selector,
        &refs,
    );
    manifest.selected_refs = refs;
    manifest.notes.push(
        "slice refs were enriched from compiled KernelModuleIr; schema/category, theory, and instance-functor ids are runtime-addressable handles, not proof certificates"
            .to_string(),
    );
    manifest.notes.sort();
    manifest.notes.dedup();
    manifest
}

pub fn semantic_refs_from_kernel_module_ir(kernel: &KernelModuleIr) -> Vec<SemanticSliceRefV1> {
    let mut refs = vec![SemanticSliceRefV1 {
        kind: SemanticSliceRefKindV1::Module,
        id: kernel.module_digest.to_string(),
        label: Some("kernel_module".to_string()),
        source: "kernel_ir.module_digest".to_string(),
    }];

    for schema in &kernel.schemas {
        refs.push(SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::ExplicitIrRef,
            id: format!("schema_category:{}", schema.schema_id),
            label: Some(schema.schema_id.to_string()),
            source: "kernel_ir.schema_category".to_string(),
        });
        let category = compile_schema_category_ir(schema);
        for object in category.objects {
            match object.object {
                SchemaCategoryObjectRefIr::ObjectType {
                    object_type_id,
                    name,
                } => refs.push(SemanticSliceRefV1 {
                    kind: SemanticSliceRefKindV1::SchemaObject,
                    id: object_type_id.to_string(),
                    label: Some(name),
                    source: "kernel_ir.schema_category.object".to_string(),
                }),
                SchemaCategoryObjectRefIr::RelationObject { relation_id, name } => {
                    refs.push(SemanticSliceRefV1 {
                        kind: SemanticSliceRefKindV1::RelationObject,
                        id: relation_id.to_string(),
                        label: Some(name),
                        source: "kernel_ir.schema_category.object".to_string(),
                    })
                }
            }
        }
        for arrow in category.arrows {
            match arrow.arrow_ref {
                SchemaCategoryArrowRefIr::RoleProjection {
                    role_id,
                    role_name: _,
                    ..
                } => refs.push(SemanticSliceRefV1 {
                    kind: SemanticSliceRefKindV1::RoleProjection,
                    id: role_id.to_string(),
                    label: Some(arrow.name),
                    source: "kernel_ir.schema_category.arrow.role_projection".to_string(),
                }),
                SchemaCategoryArrowRefIr::SubtypeInclusion {
                    schema_id,
                    subtype,
                    supertype,
                } => refs.push(SemanticSliceRefV1 {
                    kind: SemanticSliceRefKindV1::SubtypeInclusion,
                    id: subtype_inclusion_id(&schema_id.to_string(), &subtype, &supertype),
                    label: Some(format!("{subtype} <: {supertype}")),
                    source: format!(
                        "kernel_ir.schema_category.arrow.subtype_inclusion:{role_name}",
                        role_name = arrow.name
                    ),
                }),
            }
        }
    }

    for theory in &kernel.theories {
        refs.extend(semantic_refs_from_theory_ir(theory));
    }

    for instance in &kernel.instances {
        refs.extend(semantic_refs_from_instance_ir(kernel, instance));
    }

    normalize_refs(&mut refs);
    refs
}

fn semantic_refs_from_theory_ir(theory: &TheoryIr) -> Vec<SemanticSliceRefV1> {
    let mut refs = vec![SemanticSliceRefV1 {
        kind: SemanticSliceRefKindV1::ExplicitIrRef,
        id: theory.theory_id.to_string(),
        label: Some(theory.theory_id.to_string()),
        source: "kernel_ir.theory".to_string(),
    }];
    for subject in theory.subject_refs() {
        refs.push(ref_from_theory_subject(
            &subject,
            "kernel_ir.theory_subject",
        ));
    }
    for obligation in theory.obligation_refs() {
        refs.push(SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::TheoryObligation,
            id: obligation.stable_id(),
            label: Some(obligation.display_name()),
            source: "kernel_ir.theory_obligation".to_string(),
        });
    }
    refs
}

fn semantic_refs_from_instance_ir(
    kernel: &KernelModuleIr,
    instance: &InstanceIr,
) -> Vec<SemanticSliceRefV1> {
    let mut refs = vec![SemanticSliceRefV1 {
        kind: SemanticSliceRefKindV1::InstanceFunctor,
        id: instance.instance_id.to_string(),
        label: Some(instance.schema_id.to_string()),
        source: "kernel_ir.instance_functor".to_string(),
    }];
    if let Some(schema) = kernel
        .schemas
        .iter()
        .find(|schema| schema.schema_id == instance.schema_id)
    {
        if let Ok(functor) = compile_instance_functor_ir(schema, instance) {
            for object_image in functor.object_images {
                refs.push(match object_image.object {
                    SchemaCategoryObjectRefIr::ObjectType {
                        object_type_id,
                        name,
                    } => SemanticSliceRefV1 {
                        kind: SemanticSliceRefKindV1::SchemaObject,
                        id: object_type_id.to_string(),
                        label: Some(name),
                        source: "kernel_ir.instance_functor.object_image".to_string(),
                    },
                    SchemaCategoryObjectRefIr::RelationObject { relation_id, name } => {
                        SemanticSliceRefV1 {
                            kind: SemanticSliceRefKindV1::RelationObject,
                            id: relation_id.to_string(),
                            label: Some(name),
                            source: "kernel_ir.instance_functor.object_image".to_string(),
                        }
                    }
                });
            }
            for arrow_image in functor.arrow_images {
                refs.push(match arrow_image.arrow_ref {
                    SchemaCategoryArrowRefIr::RoleProjection {
                        role_id, role_name, ..
                    } => SemanticSliceRefV1 {
                        kind: SemanticSliceRefKindV1::RoleProjection,
                        id: role_id.to_string(),
                        label: Some(role_name),
                        source: "kernel_ir.instance_functor.arrow_image".to_string(),
                    },
                    SchemaCategoryArrowRefIr::SubtypeInclusion {
                        schema_id,
                        subtype,
                        supertype,
                    } => SemanticSliceRefV1 {
                        kind: SemanticSliceRefKindV1::SubtypeInclusion,
                        id: subtype_inclusion_id(&schema_id.to_string(), &subtype, &supertype),
                        label: Some(format!("{subtype} <: {supertype}")),
                        source: "kernel_ir.instance_functor.arrow_image".to_string(),
                    },
                });
            }
        }
    }
    refs
}

fn ref_from_theory_subject(subject: &TheorySubjectRefIr, source: &str) -> SemanticSliceRefV1 {
    match subject {
        TheorySubjectRefIr::Theory { theory_id } => SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::ExplicitIrRef,
            id: theory_id.to_string(),
            label: Some(theory_id.to_string()),
            source: source.to_string(),
        },
        TheorySubjectRefIr::Relation {
            relation_id,
            relation_name,
        } => SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::RelationObject,
            id: relation_id.to_string(),
            label: Some(relation_name.clone()),
            source: source.to_string(),
        },
        TheorySubjectRefIr::Role {
            role_id,
            relation_name,
            role_name,
            ..
        } => SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::RoleProjection,
            id: role_id.to_string(),
            label: Some(format!("{relation_name}.{role_name}")),
            source: source.to_string(),
        },
    }
}

fn subtype_inclusion_id(schema_id: &str, subtype: &str, supertype: &str) -> String {
    format!("subtype_inclusion:{schema_id}:{subtype}:{supertype}")
}

fn selected_refs_for_selector(
    selector: &SemanticSliceSelectorV1,
    mut refs: Vec<SemanticSliceRefV1>,
) -> Vec<SemanticSliceRefV1> {
    normalize_refs(&mut refs);
    if !selector.is_empty() {
        refs.retain(|reference| selector_matches_ref(selector, reference));
    }
    refs
}

fn normalize_refs(refs: &mut Vec<SemanticSliceRefV1>) {
    refs.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| ref_source_priority(&a.source).cmp(&ref_source_priority(&b.source)))
            .then_with(|| a.source.cmp(&b.source))
    });
    refs.dedup_by(|a, b| a.kind == b.kind && a.id == b.id);
}

fn ref_source_priority(source: &str) -> u8 {
    if source.contains("schema_category") {
        0
    } else if source.contains("theory_obligation") || source.contains("theory_subject") {
        1
    } else if source.contains("sem_delta") || source.contains("sem_commit") {
        2
    } else if source.contains("instance_functor") {
        3
    } else {
        4
    }
}

fn digest_semantic_slice(
    label: &str,
    base_ref_name: &str,
    commit_id: &AxiDigest,
    selector: &SemanticSliceSelectorV1,
    refs: &[SemanticSliceRefV1],
) -> AxiDigest {
    let label = selector.label.clone().unwrap_or_else(|| label.to_string());
    let digest_input = serde_json::json!({
        "version": SEMANTIC_SLICE_MANIFEST_VERSION_V1,
        "label": label,
        "ref": base_ref_name,
        "commit": commit_id,
        "selector": selector,
        "refs": refs,
    });
    digest_json(&digest_input)
}

pub fn build_semantic_merge_lattice_v1(
    slices: Vec<SemanticSliceManifestV1>,
) -> SemanticMergeLatticeV1 {
    let mut edges = Vec::new();
    for (left_index, left) in slices.iter().enumerate() {
        for right in slices.iter().skip(left_index + 1) {
            let shared_refs = shared_refs(left, right);
            if shared_refs.is_empty() {
                edges.push(SemanticSliceEdgeV1 {
                    source_slice_id: left.slice_id.clone(),
                    target_slice_id: right.slice_id.clone(),
                    kind: SemanticSliceEdgeKindV1::Dependency,
                    trust_class: weaker_trust(left.trust_class, right.trust_class),
                    shared_refs: Vec::new(),
                    notes: vec![
                        "slices are disjoint over currently selected typed refs and are conservative auto-join candidates".to_string(),
                    ],
                });
                continue;
            }

            let kind = if refs_subset(left, right) || refs_subset(right, left) {
                SemanticSliceEdgeKindV1::Inclusion
            } else if shared_refs.iter().any(is_conflict_sensitive_ref) {
                SemanticSliceEdgeKindV1::Conflict
            } else {
                SemanticSliceEdgeKindV1::Overlap
            };
            edges.push(SemanticSliceEdgeV1 {
                source_slice_id: left.slice_id.clone(),
                target_slice_id: right.slice_id.clone(),
                kind,
                trust_class: weaker_trust(left.trust_class, right.trust_class),
                shared_refs,
                notes: vec![
                    "slice relation is computed over finite selected runtime refs, not over full ontology closure".to_string(),
                ],
            });
        }
    }

    SemanticMergeLatticeV1 {
        version: SEMANTIC_MERGE_LATTICE_VERSION_V1.to_string(),
        slices,
        edges,
        notes: vec![
            "operational lattice is finite and runtime-scoped; arbitrary ontology merge completeness is not claimed".to_string(),
            "category/functor semantics inform the selected refs and conflict-sensitive overlap checks".to_string(),
        ],
    }
}

pub fn semantic_merge_plan_from_dry_run(
    dry_run: &crate::accepted_plane::SemMergeDryRunResultV1,
    operation: SemanticMergeOperationKindV1,
    source_selector: SemanticSliceSelectorV1,
    target_selector: SemanticSliceSelectorV1,
) -> SemanticMergePlanV1 {
    let source_slice = semantic_slice_from_ref_view(&dry_run.source, source_selector);
    let target_slice = semantic_slice_from_ref_view(&dry_run.target, target_selector);
    let lattice = build_semantic_merge_lattice_v1(vec![source_slice.clone(), target_slice.clone()]);
    let mut conflicts = conflicts_from_lattice(&lattice);
    conflicts.extend(conflicts_from_reconciliation(
        &dry_run.reconciliation.reconciliation,
    ));
    conflicts.sort_by(|a, b| a.conflict_id.cmp(&b.conflict_id));
    conflicts.dedup_by(|a, b| a.conflict_id == b.conflict_id);
    let resolver_steps = conflicts
        .iter()
        .map(|conflict| {
            crate::typed_refinement::RuntimeRefinementHandleV1::new_reconciliation(
                crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
                    reconciliation_id: dry_run
                        .reconciliation
                        .reconciliation
                        .reconciliation_id
                        .to_string(),
                    artifact_kind: conflict.artifact_kind.clone(),
                    artifact_id: conflict.artifact_id.clone(),
                    resolution: "review_required".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
            )
        })
        .collect::<Vec<_>>();
    let auto_join_decisions =
        auto_join_decisions(&source_slice, &target_slice, &lattice, conflicts.is_empty());
    let mut residual_obligations = dry_run
        .reconciliation
        .preview
        .evolution_preview
        .residual_obligations
        .clone();
    residual_obligations.extend(conflicts.iter().map(|conflict| {
        format!(
            "resolve {} `{}` before materializing semantic {}",
            conflict.artifact_kind,
            conflict.artifact_id,
            operation_label(operation)
        )
    }));
    let mut next_actions = dry_run
        .reconciliation
        .preview
        .evolution_preview
        .exploration_next_actions
        .clone();
    let runtime_theory_check = dry_run
        .reconciliation
        .preview
        .evolution_preview
        .runtime_theory_check
        .clone();
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            residual_obligations.push(format!(
                "runtime theory check has {} blocking error(s)",
                summary.blocking_errors
            ));
            next_actions.push(
                "resolve RuntimeTheoryCheckReportV1 blocking judgments before materializing merge or rebase".to_string(),
            );
        }
        for id in &summary.residual_obligation_ids {
            residual_obligations.push(format!("runtime theory residual obligation `{id}`"));
        }
        if !summary.residual_obligation_ids.is_empty() {
            next_actions.push(
                "review runtime theory residual obligations before treating the semantic merge as a strong ontology update".to_string(),
            );
        }
    }
    residual_obligations.sort();
    residual_obligations.dedup();
    if resolver_steps.is_empty() {
        next_actions.push(format!(
            "semantic {} plan is conservative-auto-joinable under policy `{}`; review CQ/trust/coverage summaries before materialization",
            operation_label(operation),
            dry_run.reconciliation.reconciliation.policy
        ));
    } else {
        next_actions.push(format!(
            "apply or review {} resolver step(s) before semantic {} materialization",
            resolver_steps.len(),
            operation_label(operation)
        ));
    }
    next_actions.sort();
    next_actions.dedup();
    let can_materialize = dry_run.reconciliation.preview.ok
        && resolver_steps.is_empty()
        && residual_obligations.is_empty();
    let digest_input = serde_json::json!({
        "version": SEMANTIC_MERGE_PLAN_VERSION_V1,
        "operation": operation,
        "source": dry_run.source.pointer.commit_id,
        "target": dry_run.target.pointer.commit_id,
        "base": dry_run.reconciliation.reconciliation.base_commit_id,
        "policy": dry_run.reconciliation.reconciliation.policy,
        "source_slice": source_slice.slice_id,
        "target_slice": target_slice.slice_id,
        "conflicts": conflicts,
    });
    SemanticMergePlanV1 {
        version: SEMANTIC_MERGE_PLAN_VERSION_V1.to_string(),
        operation,
        plan_id: digest_json(&digest_input),
        source_ref_name: dry_run.source.pointer.ref_name.clone(),
        target_ref_name: dry_run.target.pointer.ref_name.clone(),
        base_commit_id: dry_run.reconciliation.reconciliation.base_commit_id.clone(),
        source_commit_id: dry_run.source.pointer.commit_id.clone(),
        target_commit_id: dry_run.target.pointer.commit_id.clone(),
        policy: dry_run.reconciliation.reconciliation.policy.clone(),
        source_slice,
        target_slice,
        lattice,
        auto_join_decisions,
        conflicts,
        resolver_steps,
        can_materialize,
        evolution_preview: dry_run.reconciliation.preview.evolution_preview.clone(),
        runtime_theory_check,
        residual_obligations,
        next_actions,
        non_claims: vec![
            "runtime merge plan soundness means well-typed under stated anchors and gates; it does not claim complete ontology closure".to_string(),
            "HoTT/groupoid and Lean certification are future narrow-fragment checks, not implied by this runtime plan".to_string(),
            "backend-native graph history is a projected read lens and is not semantic merge authority".to_string(),
        ],
    }
}

pub fn semantic_slice_diff(
    left: &SemanticSliceManifestV1,
    right: &SemanticSliceManifestV1,
) -> SemanticSliceDiffV1 {
    let left_refs = left.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    let right_refs = right.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    SemanticSliceDiffV1 {
        version: SEMANTIC_SLICE_DIFF_VERSION_V1.to_string(),
        left_slice_id: left.slice_id.clone(),
        right_slice_id: right.slice_id.clone(),
        added_refs: right_refs.difference(&left_refs).cloned().collect(),
        removed_refs: left_refs.difference(&right_refs).cloned().collect(),
        shared_refs: left_refs.intersection(&right_refs).cloned().collect(),
    }
}

pub fn resolver_steps_report(plan: &SemanticMergePlanV1) -> SemanticResolverStepsReportV1 {
    SemanticResolverStepsReportV1 {
        version: "semantic_resolver_steps_v1".to_string(),
        plan_id: plan.plan_id.clone(),
        resolver_steps: plan.resolver_steps.clone(),
        residual_obligations: plan.residual_obligations.clone(),
        next_actions: plan.next_actions.clone(),
    }
}

fn semantic_refs_from_commit(
    commit: &crate::accepted_plane::SemCommitV1,
) -> Vec<SemanticSliceRefV1> {
    let mut refs = vec![
        SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::Commit,
            id: commit.commit_id.to_string(),
            label: Some(commit.action.clone()),
            source: "sem_commit".to_string(),
        },
        SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::Module,
            id: commit.module_digest.to_string(),
            label: Some(commit.module_name.clone()),
            source: "sem_commit.module_digest".to_string(),
        },
    ];
    if let Some(delta) = commit.delta.semantic_delta.as_ref() {
        refs.extend(delta.subject_refs.iter().map(|subject| SemanticSliceRefV1 {
            kind: classify_ref_kind(subject),
            id: subject.clone(),
            label: Some(subject.clone()),
            source: "sem_delta.subject_refs".to_string(),
        }));
        refs.extend(delta.primitives.iter().map(|primitive| {
            let json =
                serde_json::to_string(primitive).unwrap_or_else(|_| format!("{primitive:?}"));
            SemanticSliceRefV1 {
                kind: SemanticSliceRefKindV1::ExplicitIrRef,
                id: format!("primitive:{}", axiograph_dsl::digest::axi_digest_v1(&json)),
                label: Some(primitive_kind_label(primitive)),
                source: "sem_delta.primitives".to_string(),
            }
        }));
    }
    refs.extend(
        commit
            .proposal_digests
            .iter()
            .map(|digest| SemanticSliceRefV1 {
                kind: SemanticSliceRefKindV1::Evidence,
                id: digest.to_string(),
                label: Some("proposal_digest".to_string()),
                source: "sem_commit.proposal_digests".to_string(),
            }),
    );
    refs.extend(
        commit
            .delta
            .evidence_blobs_added
            .iter()
            .map(|digest| SemanticSliceRefV1 {
                kind: SemanticSliceRefKindV1::Evidence,
                id: digest.to_string(),
                label: Some("evidence_blob".to_string()),
                source: "sem_delta.evidence_blobs_added".to_string(),
            }),
    );
    refs.extend(
        commit
            .delta
            .lifecycle_events
            .iter()
            .map(|event| SemanticSliceRefV1 {
                kind: classify_ref_kind(&event.artifact.artifact_id),
                id: event.artifact.artifact_id.clone(),
                label: Some(event.artifact.artifact_kind.clone()),
                source: "sem_delta.lifecycle_events".to_string(),
            }),
    );
    refs
}

fn classify_ref_kind(value: &str) -> SemanticSliceRefKindV1 {
    let lower = value.to_ascii_lowercase();
    if lower.contains("behavior_case") || lower.contains("behaviorcase") {
        SemanticSliceRefKindV1::BehaviorCase
    } else if lower.contains("competency") || lower.contains("/cq/") {
        SemanticSliceRefKindV1::CompetencyQuestion
    } else if lower.contains("surface") || lower.contains("endpoint:") || lower.contains("job:") {
        SemanticSliceRefKindV1::ImplementationSurface
    } else if lower.contains("context") || lower.contains("world") {
        SemanticSliceRefKindV1::ContextWorld
    } else if lower.contains("theory")
        || lower.contains("constraint")
        || lower.contains("equation")
        || lower.contains("rewrite")
    {
        SemanticSliceRefKindV1::TheoryObligation
    } else if lower.contains("role") {
        SemanticSliceRefKindV1::RoleProjection
    } else if lower.contains("subtype") || lower.contains("inclusion") {
        SemanticSliceRefKindV1::SubtypeInclusion
    } else if lower.contains("relation") {
        SemanticSliceRefKindV1::RelationObject
    } else if lower.contains("schema") || lower.contains("object") {
        SemanticSliceRefKindV1::SchemaObject
    } else if lower.contains("instance") || lower.contains("fact") {
        SemanticSliceRefKindV1::InstanceFunctor
    } else {
        SemanticSliceRefKindV1::ExplicitIrRef
    }
}

fn primitive_kind_label(primitive: &crate::evolution_preview::EvolutionPrimitiveV1) -> String {
    serde_json::to_value(primitive)
        .ok()
        .and_then(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "evolution_primitive".to_string())
}

fn selector_matches_ref(
    selector: &SemanticSliceSelectorV1,
    reference: &SemanticSliceRefV1,
) -> bool {
    match reference.kind {
        SemanticSliceRefKindV1::SchemaObject => contains_ref(&selector.schema_ids, &reference.id),
        SemanticSliceRefKindV1::RelationObject => {
            contains_ref(&selector.relation_object_ids, &reference.id)
        }
        SemanticSliceRefKindV1::RoleProjection => contains_ref(&selector.role_ids, &reference.id),
        SemanticSliceRefKindV1::SubtypeInclusion => {
            contains_ref(&selector.explicit_ir_refs, &reference.id)
        }
        SemanticSliceRefKindV1::TheoryObligation => {
            contains_ref(&selector.theory_obligation_ids, &reference.id)
        }
        SemanticSliceRefKindV1::ContextWorld => contains_ref(&selector.context_ids, &reference.id),
        SemanticSliceRefKindV1::CompetencyQuestion => {
            contains_ref(&selector.competency_question_names, &reference.id)
        }
        SemanticSliceRefKindV1::BehaviorCase => {
            contains_ref(&selector.behavior_case_ids, &reference.id)
        }
        SemanticSliceRefKindV1::ImplementationSurface => {
            contains_ref(&selector.implementation_surface_ids, &reference.id)
        }
        SemanticSliceRefKindV1::Evidence => {
            contains_ref(&selector.world_model_run_ids, &reference.id)
                || contains_ref(&selector.explicit_ir_refs, &reference.id)
        }
        SemanticSliceRefKindV1::Commit
        | SemanticSliceRefKindV1::Module
        | SemanticSliceRefKindV1::InstanceFunctor
        | SemanticSliceRefKindV1::ExplicitIrRef => {
            contains_ref(&selector.explicit_ir_refs, &reference.id)
        }
    }
}

fn contains_ref(candidates: &[String], value: &str) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate == value || value.contains(candidate))
}

fn trust_class_for_commit(
    commit: &crate::accepted_plane::SemCommitV1,
) -> SemanticSliceTrustClassV1 {
    if commit.validation_ok == Some(false) {
        return SemanticSliceTrustClassV1::ReviewOnly;
    }
    match commit.kind {
        crate::accepted_plane::SemCommitKindV1::Promote
        | crate::accepted_plane::SemCommitKindV1::Merge
        | crate::accepted_plane::SemCommitKindV1::Validation => {
            SemanticSliceTrustClassV1::RuntimeChecked
        }
        crate::accepted_plane::SemCommitKindV1::EvidenceCommit
        | crate::accepted_plane::SemCommitKindV1::WorldModelRun => {
            SemanticSliceTrustClassV1::EvidenceBacked
        }
        crate::accepted_plane::SemCommitKindV1::ProjectionMaterialization
        | crate::accepted_plane::SemCommitKindV1::TagMove
        | crate::accepted_plane::SemCommitKindV1::Admin => SemanticSliceTrustClassV1::ReviewOnly,
    }
}

fn shared_refs(
    left: &SemanticSliceManifestV1,
    right: &SemanticSliceManifestV1,
) -> Vec<SemanticSliceRefV1> {
    let left_refs = left.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    let right_refs = right.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    left_refs.intersection(&right_refs).cloned().collect()
}

fn refs_subset(left: &SemanticSliceManifestV1, right: &SemanticSliceManifestV1) -> bool {
    let left_refs = left.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    let right_refs = right.selected_refs.iter().cloned().collect::<BTreeSet<_>>();
    left_refs.is_subset(&right_refs) || right_refs.is_subset(&left_refs)
}

fn is_conflict_sensitive_ref(reference: &SemanticSliceRefV1) -> bool {
    matches!(
        reference.kind,
        SemanticSliceRefKindV1::RelationObject
            | SemanticSliceRefKindV1::RoleProjection
            | SemanticSliceRefKindV1::SubtypeInclusion
            | SemanticSliceRefKindV1::TheoryObligation
            | SemanticSliceRefKindV1::ContextWorld
            | SemanticSliceRefKindV1::ImplementationSurface
    )
}

fn conflicts_from_lattice(lattice: &SemanticMergeLatticeV1) -> Vec<SemanticMergeConflictV1> {
    lattice
        .edges
        .iter()
        .filter(|edge| edge.kind == SemanticSliceEdgeKindV1::Conflict)
        .flat_map(|edge| {
            edge.shared_refs.iter().map(|reference| {
                let conflict_input = serde_json::json!({
                    "source": edge.source_slice_id,
                    "target": edge.target_slice_id,
                    "ref": reference,
                });
                SemanticMergeConflictV1 {
                    conflict_id: format!("semantic_conflict_v1:{}", digest_json(&conflict_input)),
                    artifact_kind: format!("{:?}", reference.kind).to_ascii_lowercase(),
                    artifact_id: reference.id.clone(),
                    detail: "overlapping conflict-sensitive typed refs require resolver review"
                        .to_string(),
                    shared_refs: vec![reference.clone()],
                }
            })
        })
        .collect()
}

fn conflicts_from_reconciliation(
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> Vec<SemanticMergeConflictV1> {
    reconciliation
        .conflicts
        .iter()
        .map(|conflict| SemanticMergeConflictV1 {
            conflict_id: format!(
                "semantic_conflict_v1:{}",
                axiograph_dsl::digest::axi_digest_v1(&format!(
                    "{}:{}:{}",
                    reconciliation.reconciliation_id,
                    conflict.artifact.artifact_kind,
                    conflict.artifact.artifact_id
                ))
            ),
            artifact_kind: conflict.artifact.artifact_kind.clone(),
            artifact_id: conflict.artifact.artifact_id.clone(),
            detail: conflict.detail.clone(),
            shared_refs: vec![SemanticSliceRefV1 {
                kind: classify_ref_kind(&conflict.artifact.artifact_id),
                id: conflict.artifact.artifact_id.clone(),
                label: Some(conflict.artifact.artifact_kind.clone()),
                source: "sem_reconciliation.conflicts".to_string(),
            }],
        })
        .collect()
}

fn auto_join_decisions(
    source: &SemanticSliceManifestV1,
    target: &SemanticSliceManifestV1,
    lattice: &SemanticMergeLatticeV1,
    no_conflicts: bool,
) -> Vec<SemanticAutoJoinDecisionV1> {
    if !no_conflicts {
        return Vec::new();
    }
    let shared = shared_refs(source, target);
    let decision = if source.commit_id == target.commit_id {
        "idempotent_commit"
    } else if shared.is_empty() {
        "disjoint_join"
    } else if lattice
        .edges
        .iter()
        .all(|edge| edge.kind != SemanticSliceEdgeKindV1::Conflict)
    {
        "non_conflicting_overlap"
    } else {
        return Vec::new();
    };
    vec![SemanticAutoJoinDecisionV1 {
        decision_id: format!(
            "auto_join_v1:{}",
            axiograph_dsl::digest::axi_digest_v1(&format!(
                "{}:{}:{}",
                decision, source.slice_id, target.slice_id
            ))
        ),
        decision: decision.to_string(),
        slice_ids: vec![source.slice_id.clone(), target.slice_id.clone()],
        reason: "conservative runtime analysis found no conflict-sensitive unresolved overlap"
            .to_string(),
    }]
}

fn weaker_trust(
    left: SemanticSliceTrustClassV1,
    right: SemanticSliceTrustClassV1,
) -> SemanticSliceTrustClassV1 {
    std::cmp::max(left, right)
}

fn operation_label(operation: SemanticMergeOperationKindV1) -> &'static str {
    match operation {
        SemanticMergeOperationKindV1::Merge => "merge",
        SemanticMergeOperationKindV1::Rebase => "rebase",
    }
}

fn digest_json(value: &serde_json::Value) -> AxiDigest {
    let text = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice_with_refs(id: &str, refs: Vec<SemanticSliceRefV1>) -> SemanticSliceManifestV1 {
        SemanticSliceManifestV1 {
            version: SEMANTIC_SLICE_MANIFEST_VERSION_V1.to_string(),
            slice_id: AxiDigest::new(format!("fnv1a64:{id}")),
            label: id.to_string(),
            base_ref_name: format!("heads/review/{id}"),
            commit_id: AxiDigest::new(format!("fnv1a64:commit-{id}")),
            accepted_snapshot_id: AcceptedSnapshotId::new(format!("accepted:{id}")),
            kernel_ir_digest: Some(AxiDigest::new(format!("fnv1a64:kernel-{id}"))),
            trust_class: SemanticSliceTrustClassV1::RuntimeChecked,
            selector: SemanticSliceSelectorV1::default(),
            selected_refs: refs,
            proposal_digests: Vec::new(),
            world_model_run_ids: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn relation_ref(id: &str) -> SemanticSliceRefV1 {
        SemanticSliceRefV1 {
            kind: SemanticSliceRefKindV1::RelationObject,
            id: id.to_string(),
            label: Some(id.to_string()),
            source: "test".to_string(),
        }
    }

    #[test]
    fn lattice_marks_disjoint_slices_as_dependency_join_candidates() {
        let left = slice_with_refs("left", vec![relation_ref("relation:S:Parent")]);
        let right = slice_with_refs("right", vec![relation_ref("relation:S:Child")]);
        let lattice = build_semantic_merge_lattice_v1(vec![left, right]);

        assert_eq!(lattice.edges.len(), 1);
        assert_eq!(lattice.edges[0].kind, SemanticSliceEdgeKindV1::Dependency);
        assert!(lattice.edges[0].shared_refs.is_empty());
    }

    #[test]
    fn lattice_marks_shared_relation_object_as_conflict_sensitive() {
        let left = slice_with_refs("left", vec![relation_ref("relation:S:Parent")]);
        let right = slice_with_refs("right", vec![relation_ref("relation:S:Parent")]);
        let lattice = build_semantic_merge_lattice_v1(vec![left, right]);

        assert_eq!(lattice.edges.len(), 1);
        assert_eq!(lattice.edges[0].kind, SemanticSliceEdgeKindV1::Inclusion);
        assert_eq!(lattice.edges[0].shared_refs.len(), 1);
    }

    #[test]
    fn slice_diff_reports_added_removed_and_shared_refs() {
        let left = slice_with_refs(
            "left",
            vec![
                relation_ref("relation:S:Parent"),
                relation_ref("relation:S:Child"),
            ],
        );
        let right = slice_with_refs(
            "right",
            vec![
                relation_ref("relation:S:Parent"),
                relation_ref("relation:S:Guardian"),
            ],
        );

        let diff = semantic_slice_diff(&left, &right);
        assert_eq!(diff.shared_refs.len(), 1);
        assert_eq!(diff.added_refs.len(), 1);
        assert_eq!(diff.removed_refs.len(), 1);
    }

    #[test]
    fn kernel_module_ir_refs_expose_category_theory_and_instance_functor_handles() {
        let text = r#"
module SliceDemo

schema Org:
  object Person
  object Employee
  object Department
  subtype Employee < Person
  relation WorksAt(worker: Employee, dept: Department)

theory OrgRules on Org:
  constraint key WorksAt(worker, dept)

instance TinyOrg of Org:
  Person = {Alice}
  Employee = {Alice}
  Department = {Ops}
  WorksAt = {(worker=Alice, dept=Ops)}
"#;
        let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
        let kernel =
            axiograph_pathdb::compile_kernel_module_ir(&module, text).expect("compile kernel ir");

        let refs = semantic_refs_from_kernel_module_ir(&kernel);

        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::SchemaObject
                && reference.label.as_deref() == Some("Employee")
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::RelationObject
                && reference.label.as_deref() == Some("WorksAt")
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::RoleProjection
                && reference
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains("WorksAt.worker"))
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::SubtypeInclusion
                && reference.id.contains("Employee")
                && reference.id.contains("Person")
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::TheoryObligation
                && reference
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains("WorksAt"))
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::InstanceFunctor
                && reference.id.contains("TinyOrg")
        }));
    }
}
