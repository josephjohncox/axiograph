use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use axiograph_kernel::SchemaGeneratorKindIr;
use axiograph_pathdb::{
    kernel_ir::RuntimeIrRef, AcceptedSnapshotId, AxiDigest, ProposalAdapterRunId, ProposalDigest,
};
#[cfg(test)]
use axiograph_pathdb::{KernelRefV2, RuntimeModuleIndex};

pub const SEMANTIC_SLICE_MANIFEST_VERSION_V1: &str = "semantic_slice_manifest_v1";
pub const SEMANTIC_MERGE_LATTICE_VERSION_V1: &str = "semantic_merge_lattice_v1";
pub const SEMANTIC_MERGE_PLAN_VERSION_V1: &str = "semantic_merge_plan_v1";
#[cfg_attr(not(test), allow(dead_code))]
pub const SEMANTIC_REBASE_PLAN_VERSION_V1: &str = "semantic_rebase_plan_v1";
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
    InstanceModel,
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
    pub proposal_adapter_run_ids: Vec<String>,
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
            && self.proposal_adapter_run_ids.is_empty()
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
    pub kernel_refs: Vec<RuntimeIrRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposal_digests: Vec<ProposalDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposal_adapter_run_ids: Vec<ProposalAdapterRunId>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMergeBlockerKindV1 {
    Conflict,
    ResolverStep,
    QualityGate,
    CompetencyGate,
    TrustRegression,
    CoverageRegression,
    RuntimeTheory,
    ResidualObligation,
    PreviewNotOk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticMergeBlockerV1 {
    pub kind: SemanticMergeBlockerKindV1,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<SemanticMergeBlockerV1>,
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

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTransportStatusV1 {
    Preserved,
    Transported,
    Failed,
    ResolverRequired,
    Opaque,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticTransportRefV1 {
    pub source_ref: SemanticSliceRefV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<SemanticSliceRefV1>,
    pub status: SemanticTransportStatusV1,
    pub basis: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRebasePlanV1 {
    pub version: String,
    pub plan_id: AxiDigest,
    pub source_ref_name: String,
    pub onto_ref_name: String,
    pub base_commit_id: AxiDigest,
    pub source_commit_id: AxiDigest,
    pub onto_commit_id: AxiDigest,
    pub policy: String,
    pub source_slice: SemanticSliceManifestV1,
    pub onto_slice: SemanticSliceManifestV1,
    pub lattice: SemanticMergeLatticeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transport_basis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transported_refs: Vec<SemanticTransportRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_transports: Vec<SemanticTransportRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolver_steps: Vec<crate::typed_refinement::RuntimeRefinementHandleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<SemanticMergeBlockerV1>,
    pub can_materialize: bool,
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
    view: &crate::semantic_model::SemRefViewV1,
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
    let proposal_adapter_run_ids = view
        .commit
        .proposal_adapter_run_id
        .clone()
        .into_iter()
        .chain(view.commit.delta.proposal_adapter_run_refs.iter().cloned())
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
        kernel_refs: Vec::new(),
        proposal_digests: view.commit.proposal_digests.clone(),
        proposal_adapter_run_ids,
        notes: vec![
            "semantic slice is a finite runtime restriction over accepted typed ontology refs; it is not a completeness claim".to_string(),
            "canonical relation, role, generator, theory-obligation, instance-model, and fact refs are preserved when present in the compiled kernel IR".to_string(),
        ],
    }
}

#[cfg(test)]
pub fn enrich_semantic_slice_with_kernel_module_ir(
    mut manifest: SemanticSliceManifestV1,
    kernel: &RuntimeModuleIndex,
) -> SemanticSliceManifestV1 {
    let mut refs = manifest.selected_refs;
    refs.extend(semantic_refs_from_kernel_module_ir(kernel));
    let refs = selected_refs_for_selector(&manifest.selector, refs);
    let surface = kernel.runtime_semantic_index();
    manifest.kernel_refs = surface.refs;
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
        "slice refs were enriched from the derived RuntimeModuleIndex; its ids are runtime-addressable citations, not canonical handles or proof certificates"
            .to_string(),
    );
    manifest.notes.sort();
    manifest.notes.dedup();
    manifest
}

#[cfg(test)]
pub fn semantic_refs_from_kernel_module_ir(kernel: &RuntimeModuleIndex) -> Vec<SemanticSliceRefV1> {
    let mut refs = Vec::new();
    for surface_ref in kernel.runtime_semantic_index().refs {
        let RuntimeIrRef::Canonical { citation } = &surface_ref else {
            continue;
        };
        let (kind, id) = match &citation.reference {
            KernelRefV2::Module { module_id, .. } => {
                (SemanticSliceRefKindV1::Module, module_id.to_string())
            }
            KernelRefV2::Schema { schema_id, .. } => {
                (SemanticSliceRefKindV1::ExplicitIrRef, schema_id.to_string())
            }
            KernelRefV2::ObjectType { object_type_id, .. } => (
                SemanticSliceRefKindV1::SchemaObject,
                object_type_id.to_string(),
            ),
            KernelRefV2::Relation { relation_id, .. } => (
                SemanticSliceRefKindV1::RelationObject,
                relation_id.to_string(),
            ),
            KernelRefV2::Role { role_id, .. } => {
                (SemanticSliceRefKindV1::RoleProjection, role_id.to_string())
            }
            KernelRefV2::Generator {
                schema_id,
                semantic_key,
                ..
            } => {
                let kind = kernel
                    .canonical_snapshot()
                    .and_then(|snapshot| {
                        snapshot
                            .ir()
                            .schemas()
                            .iter()
                            .find(|schema| &schema.schema_id == schema_id)
                    })
                    .and_then(|schema| {
                        schema
                            .generators
                            .iter()
                            .find(|generator| &generator.semantic_key == semantic_key)
                    })
                    .map_or(SemanticSliceRefKindV1::ExplicitIrRef, |generator| {
                        if generator.kind == SchemaGeneratorKindIr::SubtypeInclusion {
                            SemanticSliceRefKindV1::SubtypeInclusion
                        } else {
                            SemanticSliceRefKindV1::ExplicitIrRef
                        }
                    });
                (kind, semantic_key.to_string())
            }
            KernelRefV2::Theory { theory_id, .. } => {
                (SemanticSliceRefKindV1::ExplicitIrRef, theory_id.to_string())
            }
            KernelRefV2::Instance { instance_id, .. } => (
                SemanticSliceRefKindV1::InstanceModel,
                instance_id.to_string(),
            ),
            KernelRefV2::Constraint { constraint_id, .. } => (
                SemanticSliceRefKindV1::TheoryObligation,
                constraint_id.to_string(),
            ),
            KernelRefV2::Equation { equation_id, .. } => (
                SemanticSliceRefKindV1::TheoryObligation,
                equation_id.to_string(),
            ),
            KernelRefV2::RewriteRule {
                rewrite_rule_id, ..
            } => (
                SemanticSliceRefKindV1::TheoryObligation,
                rewrite_rule_id.to_string(),
            ),
            KernelRefV2::Fact { fact_id, .. } => {
                (SemanticSliceRefKindV1::ExplicitIrRef, fact_id.to_string())
            }
        };
        refs.push(SemanticSliceRefV1 {
            kind,
            id,
            label: Some(citation.label.clone()),
            source: "kernel_snapshot.canonical_ref".to_string(),
        });
    }
    normalize_refs(&mut refs);
    refs
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
    } else if source.contains("instance_model") {
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
            "canonical category and finite-model refs inform selected refs and conflict-sensitive overlap checks".to_string(),
        ],
    }
}

pub fn semantic_merge_plan_from_dry_run(
    dry_run: &crate::semantic_model::SemanticMergeDryRunV2,
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
    let blockers = semantic_merge_blockers_from_preview(
        &dry_run.reconciliation.preview.evolution_preview,
        &conflicts,
        &resolver_steps,
    );
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
    residual_obligations.extend(blockers.iter().map(|blocker| blocker.detail.clone()));
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
        && blockers.is_empty()
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
    fail_closed_semantic_merge_plan(SemanticMergePlanV1 {
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
        blockers,
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
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn semantic_rebase_plan_from_dry_run(
    dry_run: &crate::semantic_model::SemanticMergeDryRunV2,
    source_selector: SemanticSliceSelectorV1,
    onto_selector: SemanticSliceSelectorV1,
) -> SemanticRebasePlanV1 {
    let merge_plan = semantic_merge_plan_from_dry_run(
        dry_run,
        SemanticMergeOperationKindV1::Rebase,
        source_selector,
        onto_selector,
    );
    let (mut transport_basis, transported_refs, failed_transports) =
        rebase_transport_refs(&merge_plan.source_slice, &merge_plan.target_slice);
    let mut blockers = merge_plan.blockers.clone();
    let mut residual_obligations = merge_plan.residual_obligations.clone();

    if let Some(summary) = merge_plan.runtime_theory_check.as_ref() {
        transport_basis.extend(runtime_theory_transport_basis(summary));
        for residual in runtime_theory_transport_residuals(summary) {
            residual_obligations.push(residual.clone());
            blockers.push(blocker(
                SemanticMergeBlockerKindV1::RuntimeTheory,
                residual,
                summary.residual_obligation_ids.clone(),
            ));
        }
    }

    for failed in &failed_transports {
        residual_obligations.extend(failed.residual_obligations.clone());
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::ResidualObligation,
            format!(
                "failed transport for {:?} `{}`",
                failed.source_ref.kind, failed.source_ref.id
            ),
            vec![failed.source_ref.id.clone()],
        ));
    }

    transport_basis.sort();
    transport_basis.dedup();
    residual_obligations.sort();
    residual_obligations.dedup();
    blockers.sort();
    blockers.dedup();

    let mut next_actions = merge_plan.next_actions.clone();
    if failed_transports.is_empty() {
        next_actions.push(
            "review transported refs and gate summaries before semantic rebase materialization"
                .to_string(),
        );
    } else {
        next_actions.push(format!(
            "resolve {} failed transport(s) before semantic rebase materialization",
            failed_transports.len()
        ));
    }
    next_actions.sort();
    next_actions.dedup();

    let digest_input = serde_json::json!({
        "version": SEMANTIC_REBASE_PLAN_VERSION_V1,
        "source": merge_plan.source_commit_id,
        "onto": merge_plan.target_commit_id,
        "base": merge_plan.base_commit_id,
        "policy": merge_plan.policy,
        "source_slice": merge_plan.source_slice.slice_id,
        "onto_slice": merge_plan.target_slice.slice_id,
        "transported": transported_refs,
        "failed": failed_transports,
        "residuals": residual_obligations,
    });
    let can_materialize = merge_plan.can_materialize
        && blockers.is_empty()
        && failed_transports.is_empty()
        && residual_obligations.is_empty()
        && merge_plan.resolver_steps.is_empty();

    fail_closed_semantic_rebase_plan(SemanticRebasePlanV1 {
        version: SEMANTIC_REBASE_PLAN_VERSION_V1.to_string(),
        plan_id: digest_json(&digest_input),
        source_ref_name: merge_plan.source_ref_name,
        onto_ref_name: merge_plan.target_ref_name,
        base_commit_id: merge_plan.base_commit_id,
        source_commit_id: merge_plan.source_commit_id,
        onto_commit_id: merge_plan.target_commit_id,
        policy: merge_plan.policy,
        source_slice: merge_plan.source_slice,
        onto_slice: merge_plan.target_slice,
        lattice: merge_plan.lattice,
        transport_basis,
        transported_refs,
        failed_transports,
        resolver_steps: merge_plan.resolver_steps,
        blockers,
        can_materialize,
        runtime_theory_check: merge_plan.runtime_theory_check,
        residual_obligations,
        next_actions,
        non_claims: vec![
            "semantic rebase is typed runtime transport across selected refs, not a proof of global ontology equivalence".to_string(),
            "failed transports are residual review obligations and must not be materialized silently".to_string(),
            "backend-native branch transport is not semantic authority".to_string(),
        ],
    })
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn semantic_merge_plan_lean_json_v1(plan: &SemanticMergePlanV1) -> serde_json::Value {
    let result_refs = joined_refs(&plan.source_slice, &plan.target_slice);
    serde_json::json!({
        "version": "semantic_vcs_lean_merge_plan_v1",
        "base": lean_slice_manifest_json(&plan.source_slice, &plan.source_slice.selected_refs),
        "left": lean_slice_manifest_json(&plan.source_slice, &plan.source_slice.selected_refs),
        "right": lean_slice_manifest_json(&plan.target_slice, &plan.target_slice.selected_refs),
        "result": lean_slice_manifest_json(&plan.source_slice, &result_refs),
        "blockers": plan.blockers.iter().map(|blocker| lean_blocker_kind(blocker.kind)).collect::<Vec<_>>(),
        "resolver_steps": plan.resolver_steps.iter().map(lean_resolver_step_json).collect::<Vec<_>>(),
        "residual_obligations": plan.residual_obligations.iter().map(|obligation| {
            lean_ref_json("theory_obligation", obligation)
        }).collect::<Vec<_>>(),
        "trust_class": lean_trust_class(weaker_trust(
            plan.source_slice.trust_class,
            plan.target_slice.trust_class,
        )),
        "non_claims": [
            "Lean checks finite ref-join preservation and fail-closed gates, not a complete ontology closure",
            "slice anchors assert branch lineage but do not prove repository ancestry"
        ],
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn semantic_rebase_plan_lean_json_v1(plan: &SemanticRebasePlanV1) -> serde_json::Value {
    let result_refs = joined_refs(&plan.source_slice, &plan.onto_slice);
    let mut transport_items = plan
        .transported_refs
        .iter()
        .map(lean_transport_item_json)
        .collect::<Vec<_>>();
    transport_items.extend(
        plan.failed_transports
            .iter()
            .map(lean_transport_item_json)
            .collect::<Vec<_>>(),
    );
    serde_json::json!({
        "version": "semantic_vcs_lean_rebase_plan_v1",
        "source": lean_slice_manifest_json(&plan.source_slice, &plan.source_slice.selected_refs),
        "onto": lean_slice_manifest_json(&plan.onto_slice, &plan.onto_slice.selected_refs),
        "result": lean_slice_manifest_json(&plan.source_slice, &result_refs),
        "transport_items": transport_items,
        "blockers": plan.blockers.iter().map(|blocker| lean_blocker_kind(blocker.kind)).collect::<Vec<_>>(),
        "resolver_steps": plan.resolver_steps.iter().map(lean_resolver_step_json).collect::<Vec<_>>(),
        "residual_obligations": plan.residual_obligations.iter().map(|obligation| {
            lean_ref_json("theory_obligation", obligation)
        }).collect::<Vec<_>>(),
        "non_claims": [
            "Lean checks finite transport coverage, result membership, and fail-closed gates",
            "semantic rebase transport is selected-ref scoped, not global ontology equivalence"
        ],
    })
}

fn lean_slice_manifest_json(
    slice: &SemanticSliceManifestV1,
    refs: &[SemanticSliceRefV1],
) -> serde_json::Value {
    serde_json::json!({
        "anchor": {
            "accepted_ref": slice.base_ref_name,
            "accepted_snapshot_id": slice.accepted_snapshot_id.to_string(),
            "kernel_ir_digest": slice.kernel_ir_digest.as_ref().map(ToString::to_string).unwrap_or_else(|| "missing_kernel_ir_digest".to_string()),
        },
        "refs": refs.iter().map(lean_slice_ref_json).collect::<Vec<_>>(),
    })
}

fn lean_slice_ref_json(reference: &SemanticSliceRefV1) -> serde_json::Value {
    serde_json::json!({
        "kind": lean_ref_kind(reference.kind),
        "id": reference.id,
        "label": reference.label,
        "source": reference.source,
    })
}

fn lean_ref_json(kind: &str, id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "id": id,
    })
}

fn lean_resolver_step_json(
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> serde_json::Value {
    serde_json::json!({
        "handle_id": handle.id,
        "touched_refs": [],
        "required": true,
    })
}

fn lean_transport_item_json(transport: &SemanticTransportRefV1) -> serde_json::Value {
    let mut value = serde_json::json!({
        "source_ref": lean_slice_ref_json(&transport.source_ref),
        "status": lean_transport_status(transport.status),
        "required": true,
    });
    if let Some(target_ref) = transport.target_ref.as_ref() {
        value["target_ref"] = lean_slice_ref_json(target_ref);
    }
    value
}

fn joined_refs(
    left: &SemanticSliceManifestV1,
    right: &SemanticSliceManifestV1,
) -> Vec<SemanticSliceRefV1> {
    let mut refs = left.selected_refs.clone();
    refs.extend(right.selected_refs.clone());
    normalize_refs(&mut refs);
    refs
}

fn lean_ref_kind(kind: SemanticSliceRefKindV1) -> &'static str {
    match kind {
        SemanticSliceRefKindV1::Commit => "commit",
        SemanticSliceRefKindV1::Module => "module_ref",
        SemanticSliceRefKindV1::SchemaObject => "schema_object",
        SemanticSliceRefKindV1::RelationObject => "relation_object",
        SemanticSliceRefKindV1::RoleProjection => "role_projection",
        SemanticSliceRefKindV1::SubtypeInclusion => "subtype_inclusion",
        SemanticSliceRefKindV1::TheoryObligation => "theory_obligation",
        SemanticSliceRefKindV1::InstanceModel => "instance_model",
        SemanticSliceRefKindV1::ContextWorld => "context_world",
        SemanticSliceRefKindV1::CompetencyQuestion => "competency_question",
        SemanticSliceRefKindV1::BehaviorCase => "behavior_case",
        SemanticSliceRefKindV1::ImplementationSurface => "implementation_surface",
        SemanticSliceRefKindV1::Evidence => "evidence",
        SemanticSliceRefKindV1::ExplicitIrRef => "explicit_ir_ref",
    }
}

fn lean_blocker_kind(kind: SemanticMergeBlockerKindV1) -> &'static str {
    match kind {
        SemanticMergeBlockerKindV1::Conflict => "conflict",
        SemanticMergeBlockerKindV1::ResolverStep => "resolver_step",
        SemanticMergeBlockerKindV1::QualityGate => "quality_gate",
        SemanticMergeBlockerKindV1::CompetencyGate => "competency_gate",
        SemanticMergeBlockerKindV1::TrustRegression => "trust_regression",
        SemanticMergeBlockerKindV1::CoverageRegression => "coverage_regression",
        SemanticMergeBlockerKindV1::RuntimeTheory => "runtime_theory",
        SemanticMergeBlockerKindV1::ResidualObligation => "residual_obligation",
        SemanticMergeBlockerKindV1::PreviewNotOk => "preview_not_ok",
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn lean_trust_class(trust: SemanticSliceTrustClassV1) -> &'static str {
    match trust {
        SemanticSliceTrustClassV1::CertifiableFragment => "certifiable_fragment",
        SemanticSliceTrustClassV1::RuntimeChecked => "runtime_checked",
        SemanticSliceTrustClassV1::ReviewOnly => "review_only",
        SemanticSliceTrustClassV1::EvidenceBacked => "evidence_backed",
    }
}

fn lean_transport_status(status: SemanticTransportStatusV1) -> &'static str {
    match status {
        SemanticTransportStatusV1::Preserved => "preserved",
        SemanticTransportStatusV1::Transported => "transported",
        SemanticTransportStatusV1::Failed | SemanticTransportStatusV1::ResolverRequired => {
            "blocked"
        }
        SemanticTransportStatusV1::Opaque => "opaque_or_out_of_fragment",
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn validate_semantic_merge_plan_for_materialization(plan: &SemanticMergePlanV1) -> Result<()> {
    let checked = fail_closed_semantic_merge_plan(plan.clone());
    if checked.can_materialize {
        return Ok(());
    }
    Err(anyhow!(
        "semantic {} plan `{}` is not materializable: {}",
        operation_label(checked.operation),
        checked.plan_id,
        plan_blocker_summary(&checked.blockers, &checked.residual_obligations)
    ))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn validate_semantic_rebase_plan_for_materialization(
    plan: &SemanticRebasePlanV1,
) -> Result<()> {
    let checked = fail_closed_semantic_rebase_plan(plan.clone());
    if checked.can_materialize {
        return Ok(());
    }
    Err(anyhow!(
        "semantic rebase plan `{}` is not materializable: {}",
        checked.plan_id,
        plan_blocker_summary(&checked.blockers, &checked.residual_obligations)
    ))
}

fn fail_closed_semantic_merge_plan(mut plan: SemanticMergePlanV1) -> SemanticMergePlanV1 {
    plan.blockers
        .extend(semantic_merge_plan_invariant_blockers(&plan));
    plan.blockers.sort();
    plan.blockers.dedup();
    for detail in plan.blockers.iter().map(|blocker| blocker.detail.clone()) {
        if !plan.residual_obligations.contains(&detail) {
            plan.residual_obligations.push(detail);
        }
    }
    plan.residual_obligations.sort();
    plan.residual_obligations.dedup();
    plan.can_materialize = plan.evolution_preview.ok
        && plan.blockers.is_empty()
        && plan.conflicts.is_empty()
        && plan.resolver_steps.is_empty()
        && plan.residual_obligations.is_empty();
    plan
}

#[cfg_attr(not(test), allow(dead_code))]
fn fail_closed_semantic_rebase_plan(mut plan: SemanticRebasePlanV1) -> SemanticRebasePlanV1 {
    if plan.version != SEMANTIC_REBASE_PLAN_VERSION_V1 {
        plan.blockers.push(blocker(
            SemanticMergeBlockerKindV1::PreviewNotOk,
            format!(
                "unsupported semantic rebase plan version `{}`",
                plan.version
            ),
            Vec::new(),
        ));
    }
    if !plan.failed_transports.is_empty() {
        for failed in &plan.failed_transports {
            plan.blockers.push(blocker(
                SemanticMergeBlockerKindV1::ResidualObligation,
                format!(
                    "failed transport for {:?} `{}`",
                    failed.source_ref.kind, failed.source_ref.id
                ),
                vec![failed.source_ref.id.clone()],
            ));
        }
    }
    if !plan.resolver_steps.is_empty() {
        for step in &plan.resolver_steps {
            plan.blockers.push(blocker(
                SemanticMergeBlockerKindV1::ResolverStep,
                format!(
                    "resolver step `{}` must be resolved before rebase materialization",
                    step.id
                ),
                vec![step.id.clone()],
            ));
        }
    }
    if let Some(summary) = plan.runtime_theory_check.as_ref() {
        plan.blockers.extend(runtime_theory_blockers(summary));
    }
    plan.blockers.sort();
    plan.blockers.dedup();
    for detail in plan.blockers.iter().map(|blocker| blocker.detail.clone()) {
        if !plan.residual_obligations.contains(&detail) {
            plan.residual_obligations.push(detail);
        }
    }
    plan.residual_obligations.sort();
    plan.residual_obligations.dedup();
    plan.can_materialize = plan.blockers.is_empty()
        && plan.failed_transports.is_empty()
        && plan.resolver_steps.is_empty()
        && plan.residual_obligations.is_empty();
    plan
}

fn semantic_merge_plan_invariant_blockers(
    plan: &SemanticMergePlanV1,
) -> Vec<SemanticMergeBlockerV1> {
    let mut blockers = Vec::new();
    if plan.version != SEMANTIC_MERGE_PLAN_VERSION_V1 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::PreviewNotOk,
            format!("unsupported semantic merge plan version `{}`", plan.version),
            Vec::new(),
        ));
    }
    if !plan.evolution_preview.ok {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::PreviewNotOk,
            format!(
                "{} preview is not materializable for `{}`",
                plan.evolution_preview.kind, plan.evolution_preview.candidate_label
            ),
            Vec::new(),
        ));
    }
    if !plan.conflicts.is_empty()
        && !plan
            .blockers
            .iter()
            .any(|blocker| blocker.kind == SemanticMergeBlockerKindV1::Conflict)
    {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::Conflict,
            format!("{} unresolved semantic conflict(s)", plan.conflicts.len()),
            plan.conflicts
                .iter()
                .map(|conflict| conflict.artifact_id.clone())
                .collect(),
        ));
    }
    if !plan.resolver_steps.is_empty()
        && !plan
            .blockers
            .iter()
            .any(|blocker| blocker.kind == SemanticMergeBlockerKindV1::ResolverStep)
    {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::ResolverStep,
            format!(
                "{} resolver step(s) remain before materialization",
                plan.resolver_steps.len()
            ),
            plan.resolver_steps
                .iter()
                .map(|step| step.id.clone())
                .collect(),
        ));
    }
    if let Some(summary) = plan.runtime_theory_check.as_ref() {
        blockers.extend(runtime_theory_blockers(summary));
    }
    for residual in &plan.residual_obligations {
        let detail = if residual.starts_with("residual obligation:") {
            residual.clone()
        } else {
            format!("residual obligation: {residual}")
        };
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::ResidualObligation,
            detail,
            Vec::new(),
        ));
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

#[cfg_attr(not(test), allow(dead_code))]
fn plan_blocker_summary(
    blockers: &[SemanticMergeBlockerV1],
    residual_obligations: &[String],
) -> String {
    if !blockers.is_empty() {
        return blockers
            .iter()
            .map(|blocker| format!("{:?}: {}", blocker.kind, blocker.detail))
            .collect::<Vec<_>>()
            .join("; ");
    }
    if !residual_obligations.is_empty() {
        return residual_obligations.join("; ");
    }
    "plan failed closed without a materialization proof".to_string()
}

#[cfg_attr(not(test), allow(dead_code))]
fn rebase_transport_refs(
    source: &SemanticSliceManifestV1,
    target: &SemanticSliceManifestV1,
) -> (
    Vec<String>,
    Vec<SemanticTransportRefV1>,
    Vec<SemanticTransportRefV1>,
) {
    let mut basis = vec![
        format!("source_ref={}", source.base_ref_name),
        format!("onto_ref={}", target.base_ref_name),
        "transport matches exact stable typed refs first, then same-kind labels; missing images fail closed".to_string(),
    ];
    let mut transported = Vec::new();
    let mut failed = Vec::new();
    for source_ref in &source.selected_refs {
        let transport = transport_source_ref(source_ref, &target.selected_refs);
        match transport.status {
            SemanticTransportStatusV1::Failed
            | SemanticTransportStatusV1::ResolverRequired
            | SemanticTransportStatusV1::Opaque => failed.push(transport),
            SemanticTransportStatusV1::Preserved | SemanticTransportStatusV1::Transported => {
                transported.push(transport)
            }
        }
    }
    basis.sort();
    basis.dedup();
    transported.sort();
    failed.sort();
    (basis, transported, failed)
}

#[cfg_attr(not(test), allow(dead_code))]
fn transport_source_ref(
    source_ref: &SemanticSliceRefV1,
    target_refs: &[SemanticSliceRefV1],
) -> SemanticTransportRefV1 {
    if !ref_requires_transport(source_ref) {
        return SemanticTransportRefV1 {
            source_ref: source_ref.clone(),
            target_ref: None,
            status: SemanticTransportStatusV1::Preserved,
            basis: "ref kind is provenance or evidence metadata and does not require schema/category transport".to_string(),
            residual_obligations: Vec::new(),
        };
    }
    if let Some(target_ref) = target_refs
        .iter()
        .find(|target_ref| target_ref.kind == source_ref.kind && target_ref.id == source_ref.id)
    {
        return SemanticTransportRefV1 {
            source_ref: source_ref.clone(),
            target_ref: Some(target_ref.clone()),
            status: SemanticTransportStatusV1::Preserved,
            basis: "exact stable typed ref is present in source and onto slices".to_string(),
            residual_obligations: Vec::new(),
        };
    }
    if let Some(target_ref) = source_ref.label.as_ref().and_then(|label| {
        target_refs.iter().find(|target_ref| {
            target_ref.kind == source_ref.kind && target_ref.label.as_ref() == Some(label)
        })
    }) {
        return SemanticTransportRefV1 {
            source_ref: source_ref.clone(),
            target_ref: Some(target_ref.clone()),
            status: SemanticTransportStatusV1::Transported,
            basis: "same-kind label match supplied a conservative runtime transport candidate"
                .to_string(),
            residual_obligations: Vec::new(),
        };
    }
    SemanticTransportRefV1 {
        source_ref: source_ref.clone(),
        target_ref: None,
        status: SemanticTransportStatusV1::Failed,
        basis: "no exact stable typed ref or same-kind label image exists in the onto slice"
            .to_string(),
        residual_obligations: vec![format!(
            "transport missing target image for {:?} `{}`",
            source_ref.kind, source_ref.id
        )],
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn ref_requires_transport(reference: &SemanticSliceRefV1) -> bool {
    matches!(
        reference.kind,
        SemanticSliceRefKindV1::SchemaObject
            | SemanticSliceRefKindV1::RelationObject
            | SemanticSliceRefKindV1::RoleProjection
            | SemanticSliceRefKindV1::SubtypeInclusion
            | SemanticSliceRefKindV1::TheoryObligation
            | SemanticSliceRefKindV1::InstanceModel
            | SemanticSliceRefKindV1::ContextWorld
            | SemanticSliceRefKindV1::CompetencyQuestion
            | SemanticSliceRefKindV1::BehaviorCase
            | SemanticSliceRefKindV1::ImplementationSurface
            | SemanticSliceRefKindV1::ExplicitIrRef
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn runtime_theory_transport_basis(
    summary: &crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1,
) -> Vec<String> {
    let transport = &summary.transport_summary;
    vec![format!(
        "runtime_theory_transport preserved={} transported={} missing_object={} missing_arrow={} opaque={} resolver_required={}",
        transport.preserved_obligations,
        transport.transported_obligations,
        transport.missing_object_image_obligations,
        transport.missing_arrow_image_obligations,
        transport.opaque_or_out_of_fragment_obligations,
        transport.resolver_required_obligations
    )]
}

#[cfg_attr(not(test), allow(dead_code))]
fn runtime_theory_transport_residuals(
    summary: &crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1,
) -> Vec<String> {
    let transport = &summary.transport_summary;
    let mut residuals = Vec::new();
    if transport.missing_object_image_obligations > 0 {
        residuals.push(format!(
            "runtime theory transport has {} missing object-image obligation(s)",
            transport.missing_object_image_obligations
        ));
    }
    if transport.missing_arrow_image_obligations > 0 {
        residuals.push(format!(
            "runtime theory transport has {} missing arrow-image obligation(s)",
            transport.missing_arrow_image_obligations
        ));
    }
    if transport.opaque_or_out_of_fragment_obligations > 0 {
        residuals.push(format!(
            "runtime theory transport has {} opaque/out-of-fragment obligation(s)",
            transport.opaque_or_out_of_fragment_obligations
        ));
    }
    if transport.resolver_required_obligations > 0 {
        residuals.push(format!(
            "runtime theory transport requires resolver decisions for {} obligation(s)",
            transport.resolver_required_obligations
        ));
    }
    residuals
}

fn blocker(
    kind: SemanticMergeBlockerKindV1,
    detail: impl Into<String>,
    refs: Vec<String>,
) -> SemanticMergeBlockerV1 {
    SemanticMergeBlockerV1 {
        kind,
        detail: detail.into(),
        refs,
    }
}

fn runtime_theory_blockers(
    summary: &crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1,
) -> Vec<SemanticMergeBlockerV1> {
    let mut blockers = Vec::new();
    if summary.blocking_errors > 0 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::RuntimeTheory,
            format!(
                "runtime theory check has {} blocking judgment(s)",
                summary.blocking_errors
            ),
            Vec::new(),
        ));
    }
    if summary.blocked_obligations > 0 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::RuntimeTheory,
            format!(
                "runtime theory check has {} blocked obligation(s)",
                summary.blocked_obligations
            ),
            Vec::new(),
        ));
    }
    if !summary.residual_obligation_ids.is_empty() {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::RuntimeTheory,
            format!(
                "runtime theory residual obligation(s): {}",
                summary.residual_obligation_ids.join(", ")
            ),
            summary.residual_obligation_ids.clone(),
        ));
    }
    blockers
}

fn semantic_merge_blockers_from_preview(
    preview: &crate::evolution_preview::EvolutionPreviewV1,
    conflicts: &[SemanticMergeConflictV1],
    resolver_steps: &[crate::typed_refinement::RuntimeRefinementHandleV1],
) -> Vec<SemanticMergeBlockerV1> {
    let mut blockers = Vec::new();
    if !preview.ok {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::PreviewNotOk,
            format!(
                "{} preview is not materializable for `{}`",
                preview.kind, preview.candidate_label
            ),
            Vec::new(),
        ));
    }
    if preview.quality_delta.summary.error_count > 0 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::QualityGate,
            format!(
                "quality gate has {} error(s)",
                preview.quality_delta.summary.error_count
            ),
            Vec::new(),
        ));
    }
    if let Some(gate) = preview.competency_gate.as_ref() {
        if !gate.gate_passed {
            blockers.push(blocker(
                SemanticMergeBlockerKindV1::CompetencyGate,
                format!(
                    "competency gate failed (regressions={}, satisfied_after={}/{})",
                    gate.regressions, gate.satisfied_after, gate.total
                ),
                gate.questions
                    .iter()
                    .filter(|question| question.regression || !question.after_satisfied)
                    .map(|question| question.name.clone())
                    .collect(),
            ));
        }
    }
    if preview.trust_delta.regressions > 0 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::TrustRegression,
            format!(
                "trust summary has {} regression(s)",
                preview.trust_delta.regressions
            ),
            preview
                .trust_delta
                .questions
                .iter()
                .filter(|question| question.trust_changed)
                .map(|question| question.name.clone())
                .collect(),
        ));
    }
    if preview.coverage_summary.regressions > 0 {
        blockers.push(blocker(
            SemanticMergeBlockerKindV1::CoverageRegression,
            format!(
                "coverage summary has {} regression(s)",
                preview.coverage_summary.regressions
            ),
            Vec::new(),
        ));
    }
    if let Some(summary) = preview.runtime_theory_check.as_ref() {
        blockers.extend(runtime_theory_blockers(summary));
    }
    blockers.extend(preview.residual_obligations.iter().map(|obligation| {
        blocker(
            SemanticMergeBlockerKindV1::ResidualObligation,
            format!("residual obligation: {obligation}"),
            Vec::new(),
        )
    }));
    blockers.extend(conflicts.iter().map(|conflict| {
        blocker(
            SemanticMergeBlockerKindV1::Conflict,
            format!(
                "unresolved {} `{}`: {}",
                conflict.artifact_kind, conflict.artifact_id, conflict.detail
            ),
            conflict.shared_refs.iter().map(|r| r.id.clone()).collect(),
        )
    }));
    blockers.extend(resolver_steps.iter().map(|step| {
        blocker(
            SemanticMergeBlockerKindV1::ResolverStep,
            format!(
                "resolver step `{}` must be resolved before materialization",
                step.id
            ),
            vec![step.id.clone()],
        )
    }));
    blockers.sort();
    blockers.dedup();
    blockers
}

fn semantic_refs_from_commit(
    commit: &crate::semantic_model::UntrustedCommitSummaryV2,
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
            id: (*subject).to_string(),
            label: Some((*subject).to_string()),
            source: "sem_delta.subject_refs".to_string(),
        }));
        refs.extend(delta.primitives.iter().map(|primitive| {
            let json =
                serde_json::to_string(primitive).unwrap_or_else(|_| format!("{primitive:?}"));
            SemanticSliceRefV1 {
                kind: SemanticSliceRefKindV1::ExplicitIrRef,
                id: format!("primitive:{}", axiograph_kernel::revision_digest_v2(&json)),
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
        SemanticSliceRefKindV1::InstanceModel
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
            contains_ref(&selector.proposal_adapter_run_ids, &reference.id)
                || contains_ref(&selector.explicit_ir_refs, &reference.id)
        }
        SemanticSliceRefKindV1::Commit
        | SemanticSliceRefKindV1::Module
        | SemanticSliceRefKindV1::InstanceModel
        | SemanticSliceRefKindV1::ExplicitIrRef => {
            contains_ref(&selector.explicit_ir_refs, &reference.id)
        }
    }
}

fn contains_ref(candidates: &[String], value: &str) -> bool {
    candidates.iter().any(|candidate| {
        candidate == value || value.split([':', '/', '#']).any(|part| part == candidate)
    })
}

fn trust_class_for_commit(
    commit: &crate::semantic_model::UntrustedCommitSummaryV2,
) -> SemanticSliceTrustClassV1 {
    if commit.validation_ok == Some(false) {
        return SemanticSliceTrustClassV1::ReviewOnly;
    }
    match commit.kind {
        crate::semantic_model::UntrustedCommitKindV2::Promote
        | crate::semantic_model::UntrustedCommitKindV2::Merge
        | crate::semantic_model::UntrustedCommitKindV2::Validation => {
            SemanticSliceTrustClassV1::RuntimeChecked
        }
        crate::semantic_model::UntrustedCommitKindV2::EvidenceCommit
        | crate::semantic_model::UntrustedCommitKindV2::PredictiveProposalRun => {
            SemanticSliceTrustClassV1::EvidenceBacked
        }
        crate::semantic_model::UntrustedCommitKindV2::ProjectionMaterialization
        | crate::semantic_model::UntrustedCommitKindV2::TagMove
        | crate::semantic_model::UntrustedCommitKindV2::Admin => {
            SemanticSliceTrustClassV1::ReviewOnly
        }
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
    reconciliation: &crate::semantic_model::UntrustedMergeReviewV2,
) -> Vec<SemanticMergeConflictV1> {
    reconciliation
        .conflicts
        .iter()
        .filter_map(|conflict| {
            // Preview decisions are untrusted explanatory strings. They can
            // never authorize replacing a divergent canonical module slot;
            // accepted materialization requires SemReconciliationV2 typed
            // payload decisions and exact candidate anchors.
            if conflict.artifact.artifact_kind == "module_slot" {
                return Some((conflict, None));
            }
            let decision = reconciliation.decisions.iter().find(|decision| {
                decision.artifact.artifact_kind == conflict.artifact.artifact_kind
                    && decision.artifact.artifact_id == conflict.artifact.artifact_id
            });
            match decision {
                Some(decision) if reconciliation_decision_materializes(&decision.resolution) => {
                    None
                }
                Some(decision) => Some((conflict, Some(decision.resolution.clone()))),
                None => Some((conflict, None)),
            }
        })
        .map(|(conflict, non_materializing_resolution)| {
            let detail = non_materializing_resolution
                .map(|resolution| {
                    format!(
                        "{}; non-materializing resolver decision `{resolution}` still requires review",
                        conflict.detail
                    )
                })
                .unwrap_or_else(|| conflict.detail.clone());
            SemanticMergeConflictV1 {
                conflict_id: format!(
                    "semantic_conflict_v2:{}",
                    axiograph_kernel::revision_digest_v2(&format!(
                        "{}:{}:{}",
                        reconciliation.reconciliation_id,
                        conflict.artifact.artifact_kind,
                        conflict.artifact.artifact_id
                    ))
                ),
                artifact_kind: conflict.artifact.artifact_kind.clone(),
                artifact_id: conflict.artifact.artifact_id.clone(),
                detail,
                shared_refs: vec![SemanticSliceRefV1 {
                    kind: if conflict.artifact.artifact_kind == "module_slot" {
                        SemanticSliceRefKindV1::Module
                    } else {
                        classify_ref_kind(&conflict.artifact.artifact_id)
                    },
                    id: conflict.artifact.artifact_id.clone(),
                    label: Some(conflict.artifact.artifact_kind.clone()),
                    source: "sem_reconciliation.conflicts".to_string(),
                }],
            }
        })
        .collect()
}

fn reconciliation_decision_materializes(resolution: &str) -> bool {
    let normalized = resolution.trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "" | "manual_review" | "review_required" | "unresolved" | "todo" | "defer"
    )
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
            axiograph_kernel::revision_digest_v2(&format!(
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
    AxiDigest::new(axiograph_kernel::revision_digest_v2(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice_with_refs(id: &str, refs: Vec<SemanticSliceRefV1>) -> SemanticSliceManifestV1 {
        SemanticSliceManifestV1 {
            version: SEMANTIC_SLICE_MANIFEST_VERSION_V1.to_string(),
            slice_id: digest_json(&serde_json::json!({"slice": id})),
            label: id.to_string(),
            base_ref_name: format!("heads/review/{id}"),
            commit_id: digest_json(&serde_json::json!({"commit": id})),
            accepted_snapshot_id: AcceptedSnapshotId::new(
                digest_json(&serde_json::json!({"snapshot": id})).to_string(),
            ),
            kernel_ir_digest: Some(digest_json(&serde_json::json!({"kernel": id}))),
            trust_class: SemanticSliceTrustClassV1::RuntimeChecked,
            selector: SemanticSliceSelectorV1::default(),
            selected_refs: refs,
            kernel_refs: Vec::new(),
            proposal_digests: Vec::new(),
            proposal_adapter_run_ids: Vec::new(),
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
    fn selector_matching_uses_exact_ref_or_exact_component_not_substring() {
        let mut selector = SemanticSliceSelectorV1 {
            relation_object_ids: vec!["relation:S:Parent".to_string()],
            ..Default::default()
        };
        assert!(selector_matches_ref(
            &selector,
            &relation_ref("relation:S:Parent")
        ));
        assert!(!selector_matches_ref(
            &selector,
            &relation_ref("relation:S:ParentExtra")
        ));

        selector.relation_object_ids = vec!["Parent".to_string()];
        assert!(selector_matches_ref(
            &selector,
            &relation_ref("relation:S:Parent")
        ));
        assert!(!selector_matches_ref(
            &selector,
            &relation_ref("relation:S:ParentExtra")
        ));
    }

    fn commit_for_plan(id: &str) -> crate::semantic_model::UntrustedCommitSummaryV2 {
        crate::semantic_model::UntrustedCommitSummaryV2 {
            version: "untrusted_commit_summary_v2".to_string(),
            commit_id: digest_json(&serde_json::json!({"commit": id})),
            ordered_parent_commit_ids: Vec::new(),
            kind: crate::semantic_model::UntrustedCommitKindV2::Promote,
            created_at_unix_secs: 1,
            author: "test".to_string(),
            message: Some(id.to_string()),
            action: "promote".to_string(),
            provenance: crate::semantic_model::SemCommitProvenanceV1::default(),
            state: crate::semantic_model::SemStateRefV1::default(),
            delta: crate::semantic_model::SemDeltaV1::default(),
            gate_summary: None,
            reconciliation_id: None,
            accepted_snapshot_id: AcceptedSnapshotId::new(
                digest_json(&serde_json::json!({"snapshot": id})).to_string(),
            ),
            accepted_parent_snapshot_id: None,
            materialization_id: None,
            proposal_digests: Vec::new(),
            policy: "test".to_string(),
            module_name: format!("Module{id}"),
            module_digest: digest_json(&serde_json::json!({"module": id})),
            quality_report_id: None,
            constraints_certificate_id: None,
            validation_report_id: None,
            validation_ok: Some(true),
            proposal_adapter_run_id: None,
        }
    }

    fn ref_view(
        ref_name: &str,
        commit: crate::semantic_model::UntrustedCommitSummaryV2,
    ) -> crate::semantic_model::SemRefViewV1 {
        crate::semantic_model::SemRefViewV1 {
            pointer: crate::semantic_model::SemRefPointerV1 {
                version: "semantic_ref_pointer_v1".to_string(),
                ref_name: ref_name.to_string(),
                commit_id: commit.commit_id.clone(),
                updated_at_unix_secs: 1,
                gate_summary: None,
            },
            commit,
        }
    }

    fn runtime_theory_summary_with_blockers(
    ) -> crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
        crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
            version: "runtime_theory_check_summary_v1".to_string(),
            report_version: "runtime_theory_check_report_v1".to_string(),
            module_digest: digest_json(&serde_json::json!({"module": "source"})).to_string(),
            theory_count: 1,
            checked_obligations: 1,
            review_only_obligations: 0,
            residual_obligations: 1,
            blocked_obligations: 1,
            excluded_by_evidence: 0,
            blocking_errors: 1,
            admissibility_scopes: vec!["finite_fragment".to_string()],
            admissibility_trace: Default::default(),
            transport_summary: Default::default(),
            completeness_claim: "not_claimed_for_all_obligations".to_string(),
            ontology_closure_claim: "not_claimed_for_all_obligations".to_string(),
            residual_obligation_ids: vec!["theory:refund:coverage".to_string()],
            notes: Vec::new(),
        }
    }

    fn dry_run_for_reconciliation(
        source_ref_name: &str,
        target_ref_name: &str,
        source: crate::semantic_model::UntrustedCommitSummaryV2,
        target: crate::semantic_model::UntrustedCommitSummaryV2,
        reconciliation: crate::semantic_model::UntrustedMergeReviewV2,
        preview: crate::evolution_preview::EvolutionPreviewV1,
    ) -> crate::semantic_model::SemanticMergeDryRunV2 {
        crate::semantic_model::SemanticMergeDryRunV2 {
            source: ref_view(source_ref_name, source),
            target: ref_view(target_ref_name, target),
            reconciliation: crate::semantic_model::UntrustedMergeReviewViewV2 {
                preview: crate::semantic_model::UntrustedReconciliationPreviewV2 {
                    version: "untrusted_reconciliation_preview_v2".to_string(),
                    reconciliation_id: reconciliation.reconciliation_id.clone(),
                    base_commit_id: reconciliation.base_commit_id.clone(),
                    left_commit_id: reconciliation.left_commit_id.clone(),
                    right_commit_id: reconciliation.right_commit_id.clone(),
                    policy: reconciliation.policy.clone(),
                    source_ref_name: Some(source_ref_name.to_string()),
                    target_ref_name: Some(target_ref_name.to_string()),
                    resolved_ref_name: Some(target_ref_name.to_string()),
                    ok: preview.ok,
                    evolution_preview: preview,
                    report_object_id: None,
                },
                reconciliation,
                reconciliation_object_id: None,
            },
        }
    }

    fn empty_reconciliation(
        id: &str,
        source: &crate::semantic_model::UntrustedCommitSummaryV2,
        target: &crate::semantic_model::UntrustedCommitSummaryV2,
    ) -> crate::semantic_model::UntrustedMergeReviewV2 {
        crate::semantic_model::UntrustedMergeReviewV2 {
            version: "untrusted_merge_review_v2".to_string(),
            reconciliation_id: digest_json(&serde_json::json!({"reconciliation": id})),
            created_at_unix_secs: 1,
            base_commit_id: digest_json(&serde_json::json!({"commit": "base"})),
            left_commit_id: source.commit_id.clone(),
            right_commit_id: target.commit_id.clone(),
            policy: "test_policy".to_string(),
            source_ref_name: Some("heads/review/source".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/main".to_string()),
            outcome_commit_id: None,
            conflicts: Vec::new(),
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
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
    fn enriched_slice_carries_declared_kernel_refs_from_compiled_ir() {
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)

instance I of S:
  Person = {Alice, Bob}
  Parent = {
    (child=Alice, parent=Bob)
  }
"#;
        let module = axiograph_dsl::axi_v1::parse_axi_v1(axi).expect("parse fixture");
        let kernel =
            axiograph_pathdb::derive_runtime_module_index(&module, axi).expect("compile fixture");
        let slice = slice_with_refs("demo", Vec::new());

        let enriched = enrich_semantic_slice_with_kernel_module_ir(slice, &kernel);

        assert!(!enriched.selected_refs.is_empty());
        assert!(!enriched.kernel_refs.is_empty());
        kernel
            .runtime_semantic_index()
            .validate_refs(&enriched.kernel_refs)
            .expect("enriched slice kernel refs are declared by the compiled surface");
        assert!(enriched.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                RuntimeIrRef::Canonical {
                    citation: axiograph_pathdb::CanonicalKernelCitationIr {
                        reference: KernelRefV2::Constraint { .. },
                        ..
                    }
                }
            )
        }));
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
    fn dry_run_plan_surfaces_gate_conflict_and_runtime_theory_blockers() {
        let source = commit_for_plan("source");
        let target = commit_for_plan("target");
        let base_commit_id = digest_json(&serde_json::json!({"commit": "base"}));
        let reconciliation = crate::semantic_model::UntrustedMergeReviewV2 {
            version: "untrusted_merge_review_v2".to_string(),
            reconciliation_id: digest_json(&serde_json::json!({"reconciliation": "blocker"})),
            created_at_unix_secs: 1,
            base_commit_id: base_commit_id.clone(),
            left_commit_id: source.commit_id.clone(),
            right_commit_id: target.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/source".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/main".to_string()),
            outcome_commit_id: None,
            conflicts: vec![crate::semantic_model::MergePreviewConflictV2 {
                artifact: crate::semantic_model::ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "relation roles diverged".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };
        let mut preview = crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            None,
            &reconciliation,
        );
        preview.quality_delta.summary.error_count = 1;
        preview.competency_gate = Some(crate::proposals_validate::CompetencyGateReportV1 {
            total: 1,
            satisfied_before: 1,
            satisfied_after: 0,
            coverage_before: 1.0,
            coverage_after: 0.0,
            cost_before: 0.0,
            cost_after: 1.0,
            regressions: 1,
            improvements: 0,
            gate_passed: false,
            policy: crate::proposals_validate::CompetencyGatePolicyV1 {
                fail_on_regression: true,
                fail_on_unsatisfied_after: true,
            },
            questions: vec![crate::proposals_validate::CompetencyQuestionDeltaV1 {
                name: "refund approvals remain decidable".to_string(),
                min_rows: 1,
                weight: 1.0,
                before_rows: 1,
                after_rows: 0,
                before_satisfied: true,
                after_satisfied: false,
                regression: true,
                improvement: false,
                before_trust_class: "runtime_checked".to_string(),
                after_trust_class: "review_required".to_string(),
                trust_changed: true,
                before_trust_reasons: Vec::new(),
                after_trust_reasons: vec!["coverage regressed".to_string()],
            }],
        });
        preview.trust_delta.regressions = 1;
        preview.coverage_summary.regressions = 1;
        preview.runtime_theory_check = Some(runtime_theory_summary_with_blockers());
        preview.ok = false;
        let dry_run = crate::semantic_model::SemanticMergeDryRunV2 {
            source: ref_view("heads/review/source", source),
            target: ref_view("heads/main", target),
            reconciliation: crate::semantic_model::UntrustedMergeReviewViewV2 {
                reconciliation,
                preview: crate::semantic_model::UntrustedReconciliationPreviewV2 {
                    version: "untrusted_reconciliation_preview_v2".to_string(),
                    reconciliation_id: digest_json(
                        &serde_json::json!({"reconciliation": "blocker"}),
                    ),
                    base_commit_id,
                    left_commit_id: digest_json(&serde_json::json!({"commit": "source"})),
                    right_commit_id: digest_json(&serde_json::json!({"commit": "target"})),
                    policy: "cq_gate".to_string(),
                    source_ref_name: Some("heads/review/source".to_string()),
                    target_ref_name: Some("heads/main".to_string()),
                    resolved_ref_name: Some("heads/main".to_string()),
                    evolution_preview: preview,
                    ok: false,
                    report_object_id: None,
                },
                reconciliation_object_id: None,
            },
        };

        let plan = semantic_merge_plan_from_dry_run(
            &dry_run,
            SemanticMergeOperationKindV1::Merge,
            SemanticSliceSelectorV1::default(),
            SemanticSliceSelectorV1::default(),
        );
        assert!(!plan.can_materialize);
        assert!(!plan.resolver_steps.is_empty());
        for kind in [
            SemanticMergeBlockerKindV1::Conflict,
            SemanticMergeBlockerKindV1::ResolverStep,
            SemanticMergeBlockerKindV1::QualityGate,
            SemanticMergeBlockerKindV1::CompetencyGate,
            SemanticMergeBlockerKindV1::TrustRegression,
            SemanticMergeBlockerKindV1::CoverageRegression,
            SemanticMergeBlockerKindV1::RuntimeTheory,
        ] {
            assert!(
                plan.blockers.iter().any(|blocker| blocker.kind == kind),
                "expected blocker kind {kind:?} in {:#?}",
                plan.blockers
            );
        }
        assert!(plan
            .residual_obligations
            .iter()
            .any(|obligation| obligation.contains("runtime theory residual obligation")));

        let lean_json = semantic_merge_plan_lean_json_v1(&plan);
        let lean_blockers = lean_json["blockers"].as_array().unwrap();
        for kind in [
            "conflict",
            "resolver_step",
            "quality_gate",
            "competency_gate",
            "trust_regression",
            "coverage_regression",
            "runtime_theory",
            "residual_obligation",
            "preview_not_ok",
        ] {
            assert!(
                lean_blockers.iter().any(|blocker| blocker == kind),
                "expected Lean blocker kind `{kind}` in {lean_blockers:#?}",
            );
        }
        assert!(lean_json["resolver_steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| {
                step["handle_id"].as_str().is_some_and(|id| !id.is_empty())
                    && step["required"] == true
            }));
        assert!(lean_json["residual_obligations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|obligation| {
                obligation["kind"] == "theory_obligation"
                    && obligation["id"]
                        .as_str()
                        .is_some_and(|id| id.contains("runtime theory residual obligation"))
            }));
    }

    #[test]
    fn merge_plan_validation_fails_closed_on_late_residuals() {
        let source = commit_for_plan("source-clean");
        let target = commit_for_plan("target-clean");
        let reconciliation = empty_reconciliation("clean-reconciliation", &source, &target);
        let preview = crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            None,
            &reconciliation,
        );
        assert!(preview.ok);
        let dry_run = dry_run_for_reconciliation(
            "heads/review/source",
            "heads/main",
            source,
            target,
            reconciliation,
            preview,
        );
        let mut plan = semantic_merge_plan_from_dry_run(
            &dry_run,
            SemanticMergeOperationKindV1::Merge,
            SemanticSliceSelectorV1::default(),
            SemanticSliceSelectorV1::default(),
        );
        assert!(
            plan.can_materialize,
            "clean dry-run should be materializable before mutation: {:#?}",
            plan.blockers
        );

        plan.can_materialize = true;
        plan.residual_obligations
            .push("late residual injected by caller".to_string());
        let err = validate_semantic_merge_plan_for_materialization(&plan)
            .expect_err("late residual must fail closed");
        assert!(
            err.to_string().contains("late residual"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn untrusted_string_preview_cannot_suppress_typed_module_slot_conflict() {
        let source = commit_for_plan("source-module-slot-conflict");
        let target = commit_for_plan("target-module-slot-conflict");
        let mut reconciliation = empty_reconciliation("module-slot-conflict", &source, &target);
        let artifact = crate::semantic_model::ArtifactRefV1 {
            artifact_kind: "module_slot".to_string(),
            artifact_id: "SharedModule".to_string(),
            theory_obligation_ref: None,
            theory_subject_ref: None,
            theory_subject_refs: Vec::new(),
        };
        reconciliation
            .conflicts
            .push(crate::semantic_model::MergePreviewConflictV2 {
                artifact: artifact.clone(),
                detail: "divergent edit/edit module slot".to_string(),
            });
        reconciliation
            .decisions
            .push(crate::semantic_model::MergePreviewDecisionV2 {
                artifact,
                resolution: "use_left".to_string(),
            });
        let preview = crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            None,
            &reconciliation,
        );
        let dry_run = dry_run_for_reconciliation(
            "heads/review/source",
            "heads/main",
            source,
            target,
            reconciliation,
            preview,
        );
        let mut plan = semantic_merge_plan_from_dry_run(
            &dry_run,
            SemanticMergeOperationKindV1::Merge,
            SemanticSliceSelectorV1::default(),
            SemanticSliceSelectorV1::default(),
        );

        assert!(!plan.can_materialize);
        assert!(plan.conflicts.iter().any(|conflict| {
            conflict.artifact_kind == "module_slot" && conflict.artifact_id == "SharedModule"
        }));
        assert!(!plan.resolver_steps.is_empty());
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| { blocker.kind == SemanticMergeBlockerKindV1::Conflict }));

        plan.can_materialize = true;
        let err = validate_semantic_merge_plan_for_materialization(&plan)
            .expect_err("forcing can_materialize must not bypass a module-slot conflict");
        assert!(err.to_string().contains("module_slot"));

        let lean_json = semantic_merge_plan_lean_json_v1(&plan);
        assert!(lean_json["blockers"]
            .as_array()
            .expect("Lean blocker array")
            .iter()
            .any(|blocker| blocker == "conflict"));
        assert!(!lean_json["resolver_steps"]
            .as_array()
            .expect("Lean resolver step array")
            .is_empty());
    }

    #[test]
    fn merge_plan_exports_lean_readable_json_shape() {
        let source = commit_for_plan("source-lean");
        let target = commit_for_plan("target-lean");
        let reconciliation = empty_reconciliation("lean-reconciliation", &source, &target);
        let preview = crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            None,
            &reconciliation,
        );
        let dry_run = dry_run_for_reconciliation(
            "heads/review/source",
            "heads/main",
            source,
            target,
            reconciliation,
            preview,
        );
        let plan = semantic_merge_plan_from_dry_run(
            &dry_run,
            SemanticMergeOperationKindV1::Merge,
            SemanticSliceSelectorV1::default(),
            SemanticSliceSelectorV1::default(),
        );

        let lean_json = semantic_merge_plan_lean_json_v1(&plan);
        assert_eq!(lean_json["version"], "semantic_vcs_lean_merge_plan_v1");
        assert_eq!(
            lean_json["left"]["anchor"]["accepted_ref"],
            "heads/review/source"
        );
        assert_eq!(lean_json["right"]["anchor"]["accepted_ref"], "heads/main");
        assert!(lean_json["blockers"].as_array().unwrap().is_empty());
        assert!(lean_json["resolver_steps"].as_array().unwrap().is_empty());
        assert!(lean_json["residual_obligations"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(lean_json["trust_class"], "runtime_checked");
    }

    #[test]
    fn rebase_plan_surfaces_failed_transport_residuals() {
        let mut source = commit_for_plan("source-rebase");
        source.delta.semantic_delta = Some(crate::evolution_preview::EvolutionSemanticDeltaV1 {
            delta_kind: "source_relation_change".to_string(),
            subject_refs: vec!["relation:RefundApproval".to_string()],
            changed_layers: vec!["schema".to_string()],
            schema: crate::evolution_preview::TypedChangeBucketV1 {
                added: 1,
                ..crate::evolution_preview::TypedChangeBucketV1::default()
            },
            total_added: 1,
            ..crate::evolution_preview::EvolutionSemanticDeltaV1::default()
        });
        let target = commit_for_plan("target-rebase");
        let reconciliation = empty_reconciliation("rebase-transport", &source, &target);
        let preview = crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            None,
            &reconciliation,
        );
        let dry_run = dry_run_for_reconciliation(
            "heads/review/source",
            "heads/main",
            source,
            target,
            reconciliation,
            preview,
        );

        let plan = semantic_rebase_plan_from_dry_run(
            &dry_run,
            SemanticSliceSelectorV1::default(),
            SemanticSliceSelectorV1::default(),
        );

        assert_eq!(plan.version, SEMANTIC_REBASE_PLAN_VERSION_V1);
        assert!(!plan.can_materialize);
        assert!(plan.failed_transports.iter().any(|transport| {
            transport.source_ref.kind == SemanticSliceRefKindV1::RelationObject
                && transport.source_ref.id == "relation:RefundApproval"
                && transport.status == SemanticTransportStatusV1::Failed
        }));
        assert!(plan
            .residual_obligations
            .iter()
            .any(|obligation| obligation.contains("transport missing target image")));
        let err = validate_semantic_rebase_plan_for_materialization(&plan)
            .expect_err("failed transport must fail closed");
        assert!(
            err.to_string().contains("failed transport"),
            "unexpected error: {err:#}"
        );

        let lean_json = semantic_rebase_plan_lean_json_v1(&plan);
        assert_eq!(lean_json["version"], "semantic_vcs_lean_rebase_plan_v1");
        assert_eq!(
            lean_json["source"]["anchor"]["accepted_ref"],
            "heads/review/source"
        );
        assert_eq!(lean_json["onto"]["anchor"]["accepted_ref"], "heads/main");
        assert!(lean_json["transport_items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["status"] == "blocked"
                    && item["required"] == true
                    && item["source_ref"]["kind"] == "relation_object"
                    && item["source_ref"]["id"] == "relation:RefundApproval"
            }));
        assert!(lean_json["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker == "residual_obligation"));
        assert!(lean_json["residual_obligations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|obligation| {
                obligation["kind"] == "theory_obligation"
                    && obligation["id"]
                        .as_str()
                        .is_some_and(|id| id.contains("transport missing target image"))
            }));
    }

    #[test]
    fn kernel_module_ir_refs_expose_canonical_category_theory_and_instance_model_handles() {
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
        let kernel = axiograph_pathdb::derive_runtime_module_index(&module, text)
            .expect("compile kernel ir");

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
                && reference
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains("Employee") && label.contains("Person"))
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::TheoryObligation
                && reference
                    .label
                    .as_deref()
                    .is_some_and(|label| !label.is_empty())
        }));
        assert!(refs.iter().any(|reference| {
            reference.kind == SemanticSliceRefKindV1::InstanceModel
                && reference.label.as_deref() == Some("TinyOrg")
        }));
    }
}
