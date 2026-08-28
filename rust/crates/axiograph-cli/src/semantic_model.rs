//! Pure semantic review/report models.
//!
//! These types carry no filesystem layout, pointer, WAL, checkpoint, or
//! persistence behavior. Durable accepted state and lineage belong to AxiStore.

use axiograph_kernel::{MaterializationIdV2, ObjectBlobIdV2};
use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest, ProposalAdapterRunId, ProposalDigest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UntrustedReconciliationPreviewV2 {
    pub version: String,
    pub reconciliation_id: AxiDigest,
    pub base_commit_id: AxiDigest,
    pub left_commit_id: AxiDigest,
    pub right_commit_id: AxiDigest,
    pub policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_ref_name: Option<String>,
    pub evolution_preview: crate::evolution_preview::EvolutionPreviewV1,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_object_id: Option<ObjectBlobIdV2>,
}

/// Untrusted planning summary only. Accepted commit identity and parent-shape
/// validation belong to `axiograph_store::SemCommitV2`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UntrustedCommitSummaryV2 {
    pub version: String,
    pub commit_id: AxiDigest,
    #[serde(default)]
    pub ordered_parent_commit_ids: Vec<AxiDigest>,
    #[serde(default)]
    pub kind: UntrustedCommitKindV2,
    pub created_at_unix_secs: u64,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub action: String,
    #[serde(default)]
    pub provenance: SemCommitProvenanceV1,
    #[serde(default)]
    pub state: SemStateRefV1,
    #[serde(default)]
    pub delta: SemDeltaV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<crate::evolution_preview::SemGateSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_id: Option<AxiDigest>,
    pub accepted_snapshot_id: AcceptedSnapshotId,
    #[serde(default)]
    pub accepted_parent_snapshot_id: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization_id: Option<MaterializationIdV2>,
    #[serde(default)]
    pub proposal_digests: Vec<ProposalDigest>,
    pub policy: String,
    pub module_name: String,
    pub module_digest: AxiDigest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_report_id: Option<ObjectBlobIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_certificate_id: Option<ObjectBlobIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report_id: Option<ObjectBlobIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_ok: Option<bool>,
    #[serde(default)]
    pub proposal_adapter_run_id: Option<ProposalAdapterRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UntrustedCommitKindV2 {
    #[default]
    Promote,
    EvidenceCommit,
    ProjectionMaterialization,
    Merge,
    Validation,
    PredictiveProposalRun,
    TagMove,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemCommitProvenanceV1 {
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<AxiDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_adapter_run_id: Option<ProposalAdapterRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemStateRefV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id_before: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id_after: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization_id_before: Option<MaterializationIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization_id_after: Option<MaterializationIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_tree_digest: Option<AxiDigest>,
    #[serde(default)]
    pub evidence_digests: Vec<ProposalDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SemDeltaV1 {
    #[serde(default)]
    pub module_digests_added: Vec<AxiDigest>,
    #[serde(default)]
    pub module_digests_removed: Vec<AxiDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<crate::evolution_preview::SemGateSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_delta: Option<crate::evolution_preview::EvolutionSemanticDeltaV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_summary: Option<crate::evolution_preview::SemTrustSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_summary: Option<crate::evolution_preview::SemRuleSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_summary: Option<crate::evolution_preview::EvolutionCoverageSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub evidence_blobs_added: Vec<ProposalDigest>,
    #[serde(default)]
    pub certificate_refs_added: Vec<String>,
    #[serde(default)]
    pub quality_report_refs_added: Vec<String>,
    #[serde(default)]
    pub validation_report_refs_added: Vec<String>,
    #[serde(default)]
    pub projection_manifest_refs_added: Vec<AxiDigest>,
    #[serde(default)]
    pub lifecycle_events: Vec<SemLifecycleEventV1>,
    #[serde(default)]
    pub proposal_adapter_run_refs: Vec<ProposalAdapterRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemLifecycleEventV1 {
    pub artifact: ArtifactRefV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<LifecycleStageV1>,
    pub to: LifecycleStageV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRefV1 {
    pub artifact_kind: String,
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_subject_ref: Option<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStageV1 {
    Proposed,
    Validated,
    Reviewed,
    Accepted,
    Certified,
    Superseded,
    Retracted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemRefPointerV1 {
    pub version: String,
    pub ref_name: String,
    pub commit_id: AxiDigest,
    pub updated_at_unix_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<crate::evolution_preview::SemGateSummaryV1>,
}

/// Dry-run input for runtime merge analysis. This is deliberately not the
/// accepted reconciliation object and cannot be passed to AxiStore. Accepted
/// merges require `axiograph_store::SemReconciliationV2` reviewed candidates,
/// payload fingerprints, and typed decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UntrustedMergeReviewV2 {
    pub version: String,
    pub reconciliation_id: AxiDigest,
    #[serde(default)]
    pub created_at_unix_secs: u64,
    pub base_commit_id: AxiDigest,
    pub left_commit_id: AxiDigest,
    pub right_commit_id: AxiDigest,
    pub policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_commit_id: Option<AxiDigest>,
    #[serde(default)]
    pub conflicts: Vec<MergePreviewConflictV2>,
    #[serde(default)]
    pub decisions: Vec<MergePreviewDecisionV2>,
    #[serde(default)]
    pub certificate_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergePreviewConflictV2 {
    pub artifact: ArtifactRefV1,
    pub detail: String,
}

/// Human-readable preview choice. The string is explanatory only; it has no
/// accepted-state authority and is not a substitute for a typed V2 decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergePreviewDecisionV2 {
    pub artifact: ArtifactRefV1,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemRefViewV1 {
    pub pointer: SemRefPointerV1,
    pub commit: UntrustedCommitSummaryV2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UntrustedMergeReviewViewV2 {
    pub reconciliation: UntrustedMergeReviewV2,
    pub preview: UntrustedReconciliationPreviewV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_object_id: Option<ObjectBlobIdV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMergeDryRunV2 {
    pub source: SemRefViewV1,
    pub target: SemRefViewV1,
    pub reconciliation: UntrustedMergeReviewViewV2,
}
