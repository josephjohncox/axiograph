//! Accepted `.axi` plane management (append-only log + snapshot ids).
//!
//! Motivation
//! ----------
//! Axiograph intentionally separates:
//! - evidence-plane artifacts (`proposals.json`, doc chunks, heuristic edges), from
//! - the accepted/canonical `.axi` plane (reviewed modules).
//!
//! The accepted plane should behave like production code:
//! - changes are versioned,
//! - promotion is explicit,
//! - and builds are reproducible.
//!
//! This module implements a small, pragmatic first step:
//! - an append-only JSONL log of promotions
//! - content-derived snapshot ids (stable)
//! - and a reproducible “rebuild PathDB from snapshots” command.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_dsl::schema_v1::ConstraintV1;
use axiograph_pathdb::{
    AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest, PathdbSnapshotId, ProposalDigest,
    WorldModelRunId,
};

use crate::axi_input::require_canonical_axi_text;

const ACCEPTED_PLANE_VERSION_V1: &str = "accepted_plane_v1";
const ACCEPTED_PLANE_LOG_V1: &str = "accepted_plane.log.jsonl";
const ACCEPTED_PLANE_HEAD_FILE: &str = "HEAD";
const ACCEPTED_PLANE_SEM_HEAD_FILE: &str = "sem/HEAD";
const ACCEPTED_PLANE_MODULES_DIR: &str = "modules";
const ACCEPTED_PLANE_SNAPSHOTS_DIR: &str = "snapshots";
const ACCEPTED_PLANE_QUALITY_DIR: &str = "quality";
const ACCEPTED_PLANE_CERTS_DIR: &str = "certs";
const ACCEPTED_PLANE_SEM_DIR: &str = "sem";
const ACCEPTED_PLANE_SEM_COMMITS_DIR: &str = "sem/commits";
const ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR: &str = "sem/reconciliations";
const ACCEPTED_PLANE_SEM_SLICES_DIR: &str = "sem/slices";
#[allow(dead_code)]
const ACCEPTED_PLANE_SEM_HEADS_MAIN_FILE: &str = "sem/refs/heads/main";
const ACCEPTED_PLANE_SEM_HEADS_MAIN_REF: &str = "heads/main";
const ACCEPTED_PLANE_SEM_REFS_DIR: &str = "sem/refs";
const ACCEPTED_PLANE_SEM_HEADS_DIR: &str = "sem/refs/heads";
const ACCEPTED_PLANE_SEM_HEADS_REVIEW_DIR: &str = "sem/refs/heads/review";
const ACCEPTED_PLANE_SEM_HEADS_EVIDENCE_DIR: &str = "sem/refs/heads/evidence";
const ACCEPTED_PLANE_SEM_HEADS_WM_DIR: &str = "sem/refs/heads/wm";
const ACCEPTED_PLANE_SEM_TAGS_DIR: &str = "sem/refs/tags";
const ACCEPTED_PLANE_SEM_VALIDATIONS_DIR: &str = "sem/validations";
const ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR: &str = "sem/world_model_runs";
const ACCEPTED_PLANE_SEM_PROJECTIONS_DIR: &str = "sem/projections";

const ACCEPTED_PLANE_SNAPSHOT_VERSION_V1: &str = "accepted_plane_snapshot_v1";
const ACCEPTED_PLANE_EVENT_VERSION_V1: &str = "accepted_plane_event_v1";
const ACCEPTED_PLANE_PROMOTION_PREVIEW_VERSION_V1: &str = "accepted_plane_promotion_preview_v1";
#[cfg_attr(not(test), allow(dead_code))]
const ACCEPTED_PLANE_RECONCILIATION_PREVIEW_VERSION_V1: &str =
    "accepted_plane_reconciliation_preview_v1";
const ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1: &str = "accepted_plane_semantic_commit_v1";
const ACCEPTED_PLANE_SEM_REF_POINTER_VERSION_V1: &str = "accepted_plane_sem_ref_pointer_v1";
#[allow(dead_code)]
const ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1: &str = "accepted_plane_sem_reconciliation_v1";
#[cfg_attr(not(test), allow(dead_code))]
const WORLD_MODEL_RUN_RECORD_VERSION_V1: &str = "world_model_run_record_v1";
#[cfg_attr(not(test), allow(dead_code))]
const BACKEND_PROJECTION_MANIFEST_VERSION_V1: &str = "backend_projection_manifest_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneSnapshotV1 {
    pub version: String,
    pub snapshot_id: AcceptedSnapshotId,
    pub previous_snapshot_id: Option<AcceptedSnapshotId>,
    pub created_at_unix_secs: u64,
    /// Module name -> module digest.
    ///
    /// We keep this as a map so the snapshot meaning is stable regardless of
    /// promotion ordering.
    pub modules: BTreeMap<String, AcceptedModuleRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedModuleRefV1 {
    pub module_digest: AxiDigest,
    /// Path relative to the accepted-plane directory.
    pub stored_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneEventV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub action: String,
    pub snapshot_id: AcceptedSnapshotId,
    pub previous_snapshot_id: Option<AcceptedSnapshotId>,
    pub module_name: String,
    pub module_digest: AxiDigest,
    pub stored_module_path: String,
    #[serde(default)]
    pub message: Option<String>,
    /// Optional quality gate profile used during promotion (`off|fast|strict`).
    #[serde(default)]
    pub quality_profile: Option<String>,
    /// Optional path to a stored quality report (relative to the accepted-plane directory).
    #[serde(default)]
    pub quality_report_path: Option<String>,
    #[serde(default)]
    pub quality_error_count: Option<usize>,
    #[serde(default)]
    pub quality_warning_count: Option<usize>,
    #[serde(default)]
    pub quality_info_count: Option<usize>,
    /// Optional path to a stored constraints certificate (relative to the accepted-plane directory).
    #[serde(default)]
    pub constraints_cert_path: Option<String>,
    #[serde(default)]
    pub constraints_constraint_count: Option<u32>,
    #[serde(default)]
    pub constraints_instance_count: Option<u32>,
    #[serde(default)]
    pub constraints_check_count: Option<u32>,
    /// Optional path to a stored promotion preview/validation report (relative to the accepted-plane directory).
    #[serde(default)]
    pub validation_report_path: Option<String>,
    #[serde(default)]
    pub validation_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPreviewOptionsV1 {
    pub quality_profile: String,
    pub quality_plane: String,
    #[serde(default)]
    pub competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    #[serde(default)]
    pub competency_gate: crate::proposals_validate::CompetencyGatePolicyV1,
}

impl Default for PromotionPreviewOptionsV1 {
    fn default() -> Self {
        Self {
            quality_profile: "off".to_string(),
            quality_plane: "both".to_string(),
            competency_questions: Vec::new(),
            competency_gate: crate::proposals_validate::CompetencyGatePolicyV1::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPreviewReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_snapshot_id: Option<AcceptedSnapshotId>,
    pub candidate_module_name: String,
    pub candidate_axi_digest_v1: AxiDigest,
    pub import_summary: PromotionImportSummaryV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    pub quality_delta: crate::quality::QualityReportV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_gate: Option<crate::proposals_validate::CompetencyGateReportV1>,
    pub trust: crate::proposals_validate::ProposalValidationTrustContractV1,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_report_path: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationPreviewReportV1 {
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
    pub stored_report_path: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationReviewApplyResultV1 {
    pub handle: crate::typed_refinement::RuntimeRefinementHandleV1,
    pub base_reconciliation: SemReconciliationV1,
    pub updated_reconciliation: SemReconciliationV1,
    pub evolution_preview: crate::evolution_preview::EvolutionPreviewV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromotionImportSummaryV1 {
    pub meta_entities_added: usize,
    pub meta_relations_added: usize,
    pub instances_imported: usize,
    pub entities_added: usize,
    pub tuple_entities_added: usize,
    pub relations_added: usize,
    pub derived_edges_added: usize,
    pub entity_type_upgrades: usize,
}

impl From<axiograph_pathdb::axi_module_import::AxiSchemaV1ImportSummary>
    for PromotionImportSummaryV1
{
    fn from(value: axiograph_pathdb::axi_module_import::AxiSchemaV1ImportSummary) -> Self {
        Self {
            meta_entities_added: value.meta_entities_added,
            meta_relations_added: value.meta_relations_added,
            instances_imported: value.instances_imported,
            entities_added: value.entities_added,
            tuple_entities_added: value.tuple_entities_added,
            relations_added: value.relations_added,
            derived_edges_added: value.derived_edges_added,
            entity_type_upgrades: value.entity_type_upgrades,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteReviewedModuleOptionsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub quality_profile: String,
    pub quality_plane: String,
    #[serde(default)]
    pub competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    #[serde(default)]
    pub competency_gate: crate::proposals_validate::CompetencyGatePolicyV1,
    #[serde(default = "default_persist_validation_report")]
    pub persist_validation_report: bool,
}

fn default_persist_validation_report() -> bool {
    true
}

impl Default for PromoteReviewedModuleOptionsV1 {
    fn default() -> Self {
        Self {
            message: None,
            quality_profile: "off".to_string(),
            quality_plane: "both".to_string(),
            competency_questions: Vec::new(),
            competency_gate: crate::proposals_validate::CompetencyGatePolicyV1::default(),
            persist_validation_report: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteReviewedModuleResultV1 {
    pub snapshot_id: AcceptedSnapshotId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_report_path: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationSemanticCommitOptionsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default = "default_semantic_commit_author")]
    pub author: String,
    #[serde(default = "default_persist_validation_report")]
    pub persist_validation_report: bool,
    #[serde(default = "default_update_resolved_ref")]
    pub update_resolved_ref: bool,
    #[serde(default = "default_update_semantic_head")]
    pub update_semantic_head: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
fn default_update_resolved_ref() -> bool {
    true
}

#[cfg_attr(not(test), allow(dead_code))]
fn default_update_semantic_head() -> bool {
    true
}

impl Default for ReconciliationSemanticCommitOptionsV1 {
    fn default() -> Self {
        Self {
            message: None,
            author: default_semantic_commit_author(),
            persist_validation_report: true,
            update_resolved_ref: true,
            update_semantic_head: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemCommitV1 {
    pub version: String,
    pub commit_id: AxiDigest,
    #[serde(default)]
    pub parent_commit_id: Option<AxiDigest>,
    #[serde(default)]
    pub kind: SemCommitKindV1,
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
    pub pathdb_snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default)]
    pub proposal_digests: Vec<ProposalDigest>,
    pub policy: String,
    pub module_name: String,
    pub module_digest: AxiDigest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_cert_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_ok: Option<bool>,
    #[serde(default)]
    pub world_model_run_id: Option<WorldModelRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SemCommitKindV1 {
    #[default]
    Promote,
    EvidenceCommit,
    ProjectionMaterialization,
    Merge,
    Validation,
    WorldModelRun,
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
    pub world_model_run_id: Option<WorldModelRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemStateRefV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id_before: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id_after: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pathdb_snapshot_id_before: Option<PathdbSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pathdb_snapshot_id_after: Option<PathdbSnapshotId>,
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
    pub world_model_run_refs: Vec<WorldModelRunId>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemHeadV1 {
    Symbolic {
        ref_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit_id: Option<AxiDigest>,
    },
    Detached {
        commit_id: AxiDigest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemRefNameV1 {
    Main,
    WorldModel { name: String },
    Review { name: String },
    Evidence { name: String },
    Tag { name: String },
    Generic { ref_name: String },
}

impl SemRefNameV1 {
    pub fn main() -> Self {
        Self::Main
    }

    pub fn world_model(name: impl Into<String>) -> Result<Self> {
        Ok(Self::WorldModel {
            name: validate_sem_ref_suffix(name.into(), "world-model branch")?,
        })
    }

    pub fn review(name: impl Into<String>) -> Result<Self> {
        Ok(Self::Review {
            name: validate_sem_ref_suffix(name.into(), "review branch")?,
        })
    }

    pub fn evidence(name: impl Into<String>) -> Result<Self> {
        Ok(Self::Evidence {
            name: validate_sem_ref_suffix(name.into(), "evidence branch")?,
        })
    }

    pub fn tag(name: impl Into<String>) -> Result<Self> {
        Ok(Self::Tag {
            name: validate_sem_ref_suffix(name.into(), "semantic tag")?,
        })
    }

    pub fn parse(ref_name: &str) -> Result<Self> {
        let trimmed = ref_name.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("semantic ref name must not be empty"));
        }
        if trimmed == ACCEPTED_PLANE_SEM_HEADS_MAIN_REF {
            return Ok(Self::Main);
        }
        if let Some(name) = trimmed.strip_prefix("heads/wm/") {
            return Self::world_model(name);
        }
        if let Some(name) = trimmed.strip_prefix("heads/review/") {
            return Self::review(name);
        }
        if let Some(name) = trimmed.strip_prefix("heads/evidence/") {
            return Self::evidence(name);
        }
        if let Some(name) = trimmed.strip_prefix("tags/") {
            return Self::tag(name);
        }
        Ok(Self::Generic {
            ref_name: validate_sem_ref_suffix(trimmed.to_string(), "semantic ref")?,
        })
    }

    pub fn as_ref_name(&self) -> String {
        match self {
            Self::Main => ACCEPTED_PLANE_SEM_HEADS_MAIN_REF.to_string(),
            Self::WorldModel { name } => format!("heads/wm/{name}"),
            Self::Review { name } => format!("heads/review/{name}"),
            Self::Evidence { name } => format!("heads/evidence/{name}"),
            Self::Tag { name } => format!("tags/{name}"),
            Self::Generic { ref_name } => ref_name.clone(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_branch(&self) -> bool {
        matches!(
            self,
            Self::Main | Self::WorldModel { .. } | Self::Review { .. } | Self::Evidence { .. }
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_tag(&self) -> bool {
        matches!(self, Self::Tag { .. })
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemReconciliationV1 {
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
    pub conflicts: Vec<SemConflictRecordV1>,
    #[serde(default)]
    pub decisions: Vec<SemDecisionRecordV1>,
    #[serde(default)]
    pub certificate_refs: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemConflictRecordV1 {
    pub artifact: ArtifactRefV1,
    pub detail: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemDecisionRecordV1 {
    pub artifact: ArtifactRefV1,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PathdbSemanticCommitOptionsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub proposal_digests: Vec<ProposalDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<crate::evolution_preview::SemGateSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_model_run_id: Option<WorldModelRunId>,
    #[serde(default = "default_pathdb_semantic_commit_policy")]
    pub policy: String,
    #[serde(default = "default_semantic_commit_author")]
    pub author: String,
}

fn default_pathdb_semantic_commit_policy() -> String {
    "evidence_plane".to_string()
}

fn default_semantic_commit_author() -> String {
    "axiograph-cli".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldModelRunStatusV1 {
    Previewed,
    CommittedToPathdb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldModelRunRecordV1 {
    pub version: String,
    pub run_id: WorldModelRunId,
    pub trace_id: WorldModelRunId,
    pub created_at_unix_secs: u64,
    pub status: WorldModelRunStatusV1,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_digest_v1: Option<AxiDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: ProposalDigest,
    pub proposal_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_total_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_plane: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionBackendKindV1 {
    RdfQuadStore,
    PropertyGraph,
    ManagedSchemaGraph,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionBackendEngineV1 {
    TypeDb,
    TerminusDb,
    ApacheAge,
    Neo4j,
    Neptune,
    JanusGraph,
    Memgraph,
    Other,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionSupportTierV1 {
    Primary,
    Supported,
    Experimental,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendCapabilityProfileV1 {
    pub backend_engine: ProjectionBackendEngineV1,
    pub support_tier: ProjectionSupportTierV1,
    pub backend_kind: ProjectionBackendKindV1,
    pub backend_label: String,
    #[serde(default)]
    pub named_graphs: bool,
    #[serde(default)]
    pub transactions: bool,
    #[serde(default)]
    pub constraints: bool,
    #[serde(default)]
    pub schema_management: bool,
    #[serde(default)]
    pub native_type_system: bool,
    #[serde(default)]
    pub native_nary_relations: bool,
    #[serde(default)]
    pub typed_query_validation: bool,
    #[serde(default)]
    pub logic_programming_or_functions: bool,
    #[serde(default)]
    pub cypher_like_queries: bool,
    #[serde(default)]
    pub gremlin_like_traversals: bool,
    #[serde(default)]
    pub sparql_dataset_queries: bool,
    #[serde(default)]
    pub relationship_entities: bool,
    #[serde(default)]
    pub procedures_or_triggers: bool,
    #[serde(default)]
    pub multi_database_or_namespace_support: bool,
    #[serde(default)]
    pub immutable_history: bool,
    #[serde(default)]
    pub branching_and_merge: bool,
    #[serde(default)]
    pub diff_and_patch: bool,
    #[serde(default)]
    pub schema_instance_separation: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl BackendCapabilityProfileV1 {
    pub fn typedb_primary() -> Self {
        Self {
            backend_engine: ProjectionBackendEngineV1::TypeDb,
            support_tier: ProjectionSupportTierV1::Primary,
            backend_kind: ProjectionBackendKindV1::ManagedSchemaGraph,
            backend_label: "TypeDB".to_string(),
            named_graphs: false,
            transactions: true,
            constraints: true,
            schema_management: true,
            native_type_system: true,
            native_nary_relations: true,
            typed_query_validation: true,
            logic_programming_or_functions: true,
            cypher_like_queries: false,
            gremlin_like_traversals: false,
            sparql_dataset_queries: false,
            relationship_entities: true,
            procedures_or_triggers: false,
            multi_database_or_namespace_support: true,
            immutable_history: false,
            branching_and_merge: false,
            diff_and_patch: false,
            schema_instance_separation: false,
        }
    }

    pub fn terminusdb_supported() -> Self {
        Self {
            backend_engine: ProjectionBackendEngineV1::TerminusDb,
            support_tier: ProjectionSupportTierV1::Supported,
            backend_kind: ProjectionBackendKindV1::RdfQuadStore,
            backend_label: "TerminusDB".to_string(),
            named_graphs: true,
            transactions: true,
            constraints: true,
            schema_management: true,
            native_type_system: true,
            native_nary_relations: false,
            typed_query_validation: false,
            logic_programming_or_functions: true,
            cypher_like_queries: false,
            gremlin_like_traversals: false,
            sparql_dataset_queries: false,
            relationship_entities: true,
            procedures_or_triggers: false,
            multi_database_or_namespace_support: true,
            immutable_history: true,
            branching_and_merge: true,
            diff_and_patch: true,
            schema_instance_separation: true,
        }
    }

    pub fn apache_age_experimental() -> Self {
        Self {
            backend_engine: ProjectionBackendEngineV1::ApacheAge,
            support_tier: ProjectionSupportTierV1::Experimental,
            backend_kind: ProjectionBackendKindV1::PropertyGraph,
            backend_label: "Apache AGE".to_string(),
            named_graphs: false,
            transactions: true,
            constraints: false,
            schema_management: false,
            native_type_system: false,
            native_nary_relations: false,
            typed_query_validation: false,
            logic_programming_or_functions: false,
            cypher_like_queries: true,
            gremlin_like_traversals: false,
            sparql_dataset_queries: false,
            relationship_entities: false,
            procedures_or_triggers: false,
            multi_database_or_namespace_support: false,
            immutable_history: false,
            branching_and_merge: false,
            diff_and_patch: false,
            schema_instance_separation: false,
        }
    }

    pub fn can_mirror_semantic_vcs_workspace(&self) -> bool {
        self.immutable_history && self.branching_and_merge && self.diff_and_patch
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionNativeQueryAccessV1 {
    #[default]
    None,
    ReadOnlyPartial,
    ReadOnlyAnchorScoped,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMutationAuthorityV1 {
    #[default]
    AxiographOnly,
    BackendWritableMirror,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectionCapabilityProfileV1 {
    #[serde(default)]
    pub preserves_relation_objects: bool,
    #[serde(default)]
    pub binary_carrier_edges_only_when_lossless: bool,
    #[serde(default)]
    pub preserves_nary_relation_objects: bool,
    #[serde(default)]
    pub preserves_context_world_axes: bool,
    #[serde(default)]
    pub preserves_evidence_objects: bool,
    #[serde(default)]
    pub preserves_provenance_links: bool,
    #[serde(default)]
    pub supports_anchor_scoped_query_pushdown: bool,
    #[serde(default)]
    pub supports_context_scoped_query_pushdown: bool,
    #[serde(default)]
    pub native_query_access: ProjectionNativeQueryAccessV1,
    #[serde(default)]
    pub mutation_authority: ProjectionMutationAuthorityV1,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ProjectionCapabilityProfileV1 {
    pub fn allows_native_read_queries(&self) -> bool {
        !matches!(
            self.native_query_access,
            ProjectionNativeQueryAccessV1::None
        )
    }

    pub fn requires_axiograph_mutation_authority(&self) -> bool {
        matches!(
            self.mutation_authority,
            ProjectionMutationAuthorityV1::AxiographOnly
        )
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionObjectMappingV1 {
    pub object_name: String,
    pub backend_label: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionRoleMappingV1 {
    pub role_name: String,
    pub backend_slot: String,
    #[serde(default)]
    pub preserved_explicitly: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionCarrierEdgeMappingV1 {
    pub edge_label: String,
    pub source_role: String,
    pub target_role: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionRelationTupleEncodingV1 {
    RelationNode,
    ReifiedFact,
    RelationshipEntity,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionRelationMappingV1 {
    pub relation_name: String,
    pub tuple_encoding: ProjectionRelationTupleEncodingV1,
    pub tuple_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier_edge: Option<ProjectionCarrierEdgeMappingV1>,
    #[serde(default)]
    pub role_mappings: Vec<ProjectionRoleMappingV1>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionContextMappingStrategyV1 {
    NamedGraphs,
    SeparateNamespaces,
    SeparateDatabases,
    TupleProperties,
    SidecarIndex,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionContextAxisBindingV1 {
    pub axis_name: String,
    pub backend_slot: String,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionContextMappingV1 {
    pub strategy: ProjectionContextMappingStrategyV1,
    #[serde(default)]
    pub axis_bindings: Vec<ProjectionContextAxisBindingV1>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionManifestV1 {
    pub version: String,
    pub projection_id: AxiDigest,
    pub created_at_unix_secs: u64,
    pub accepted_snapshot_id: AcceptedSnapshotId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sem_ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sem_commit_id: Option<AxiDigest>,
    pub compiled_ir_digest: AxiDigest,
    pub materialization_ref: String,
    pub backend: BackendCapabilityProfileV1,
    pub projection: ProjectionCapabilityProfileV1,
    #[serde(default)]
    pub object_mappings: Vec<ProjectionObjectMappingV1>,
    #[serde(default)]
    pub relation_mappings: Vec<ProjectionRelationMappingV1>,
    pub context_mapping: ProjectionContextMappingV1,
    #[serde(default)]
    pub trust_caveats: Vec<String>,
    #[serde(default)]
    pub round_trip_limitations: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProjectionSemanticCommitOptionsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default = "default_projection_semantic_commit_policy")]
    pub policy: String,
    #[serde(default = "default_semantic_commit_author")]
    pub author: String,
}

#[cfg_attr(not(test), allow(dead_code))]
fn default_projection_semantic_commit_policy() -> String {
    "projection_materialization".to_string()
}

/// Initialize the accepted-plane directory layout.
///
/// This is idempotent and safe to run even if the directory already exists.
pub(crate) fn init_accepted_plane_dir(accepted_dir: &Path) -> Result<()> {
    ensure_layout(accepted_dir)
}

/// Resolve an accepted-plane snapshot id for CLI usage.
///
/// Supports:
/// - `head` / `latest`
/// - full ids (`fnv1a64:...`)
/// - unique prefixes (either of the full id, or of the digest suffix after `:`)
pub(crate) fn resolve_snapshot_id_for_cli(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedSnapshotId> {
    ensure_layout(accepted_dir)?;
    resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)
}

pub(crate) fn read_snapshot_for_cli(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedPlaneSnapshotV1> {
    ensure_layout(accepted_dir)?;
    let snapshot_id = resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    read_snapshot(accepted_dir, &snapshot_id)
}

#[allow(dead_code)]
pub fn promote_reviewed_module(
    candidate_axi: &Path,
    accepted_dir: &Path,
    message: Option<&str>,
    quality_profile: &str,
) -> Result<AcceptedSnapshotId> {
    Ok(promote_reviewed_module_with_options(
        candidate_axi,
        accepted_dir,
        &PromoteReviewedModuleOptionsV1 {
            message: message.map(str::to_owned),
            quality_profile: quality_profile.to_string(),
            ..PromoteReviewedModuleOptionsV1::default()
        },
    )?
    .snapshot_id)
}

pub fn promote_reviewed_module_with_options(
    candidate_axi: &Path,
    accepted_dir: &Path,
    options: &PromoteReviewedModuleOptionsV1,
) -> Result<PromoteReviewedModuleResultV1> {
    ensure_layout(accepted_dir)?;

    let text = fs::read_to_string(candidate_axi)?;
    // Conservative gate: accepted-plane promotion only accepts canonical,
    // well-typed `.axi` modules, never reversible PathDB snapshot exports.
    let validated = require_canonical_axi_text(&text)?.into_parts().1;
    let reviewed = axiograph_pathdb::axi_module_typecheck::review_axi_v1_module(
        validated,
        axiograph_pathdb::axi_module_typecheck::ReviewStamp {
            reviewer: None,
            note: options.message.clone(),
        },
    );

    // Hard gate: accepted/canonical modules must not contain unknown/opaque
    // constraints. If a constraint is not structured, it cannot participate in
    // certificate checking or schema-directed tooling, and we don't want silent
    // semantics drift in the accepted plane.
    //
    // If you want to keep richer (not yet executable/certifiable) content in a
    // canonical module, prefer a `constraint Name:` named-block.
    let mut unknown: Vec<(String, String)> = Vec::new();
    for th in &reviewed.module().theories {
        for c in &th.constraints {
            if let ConstraintV1::Unknown { text } = c {
                unknown.push((th.name.clone(), text.clone()));
            }
        }
    }
    if !unknown.is_empty() {
        let mut msg = String::new();
        msg.push_str("promotion blocked: unknown/unsupported theory constraints found in candidate module.\n");
        msg.push_str("Fix the module by rewriting constraints into canonical structured forms (or use a named-block constraint).\n");
        msg.push_str("Unknown constraints:\n");
        for (i, (th_name, text)) in unknown.iter().take(8).enumerate() {
            msg.push_str(&format!("  {i}: theory `{th_name}`: {text}\n"));
        }
        if unknown.len() > 8 {
            msg.push_str(&format!("  ... ({} more)\n", unknown.len() - 8));
        }
        return Err(anyhow!(msg.trim_end().to_string()));
    }

    let module_name = reviewed.module().module_name.clone();
    let module_digest = AxiDigest::from_axi_text(&text);

    // Hard gate: accepted-plane promotions must satisfy the conservative,
    // certificate-checkable constraint subset.
    //
    // This complements the quality/lint pass: it is the stable, semantics-driven
    // check we expect to keep in sync with the Lean trusted checker.
    let constraints_proof =
        axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1(&reviewed)?;
    let constraints_cert = axiograph_pathdb::certificate::CertificateV2::axi_constraints_ok_v1(
        constraints_proof.clone(),
    )
    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(
        module_digest.clone(),
    ));

    // Store the certificate once per module digest (idempotent across snapshots).
    let constraints_cert_rel_path = PathBuf::from(ACCEPTED_PLANE_CERTS_DIR).join(format!(
        "{}__{}__axi_constraints_ok_v1.json",
        sanitize_path_component(&module_name),
        digest_to_filename(&module_digest)
    ));
    let constraints_cert_abs_path = accepted_dir.join(&constraints_cert_rel_path);
    if !constraints_cert_abs_path.exists() {
        fs::write(
            &constraints_cert_abs_path,
            serde_json::to_string_pretty(&constraints_cert)?,
        )?;
    }

    // Optional quality gate (untrusted tooling). If enabled, we:
    // - import the module into an in-memory PathDB to get a uniform representation,
    // - run lints + constraint checks,
    // - and attach the resulting report to the accepted-plane event.
    let quality_profile = options.quality_profile.trim().to_ascii_lowercase();
    let quality_plane = if options.quality_plane.trim().is_empty() {
        "both".to_string()
    } else {
        options.quality_plane.trim().to_ascii_lowercase()
    };
    let quality_report = if quality_profile != "off" {
        if !matches!(quality_profile.as_str(), "fast" | "strict") {
            return Err(anyhow!(
                "unknown --quality `{}` (expected off|fast|strict)",
                quality_profile
            ));
        }
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &reviewed,
        )?;
        db.build_indexes();

        let report = crate::quality::run_quality_checks(
            &db,
            &candidate_axi.to_path_buf(),
            &quality_profile,
            &quality_plane,
        )?;
        if report.summary.error_count > 0 {
            return Err(anyhow!(
                "quality gate failed: {} error(s) found (run `axiograph check quality {}` for details)",
                report.summary.error_count,
                candidate_axi.display()
            ));
        }
        Some(report)
    } else {
        None
    };

    let previous_snapshot_id = read_head(accepted_dir)?;
    let previous_snapshot = if let Some(prev) = previous_snapshot_id.as_ref() {
        Some(read_snapshot(accepted_dir, prev)?)
    } else {
        None
    };

    let preview = preview_reviewed_module_from_reviewed(
        accepted_dir,
        previous_snapshot.as_ref(),
        &reviewed,
        &text,
        &module_digest,
        &PromotionPreviewOptionsV1 {
            quality_profile: quality_profile.clone(),
            quality_plane: quality_plane.clone(),
            competency_questions: options.competency_questions.clone(),
            competency_gate: options.competency_gate.clone(),
        },
    )?;
    if !preview.ok {
        let mut msg = format!(
            "promotion preview blocked for module `{}`",
            preview.candidate_module_name
        );
        if let Some(cq) = preview.competency_gate.as_ref() {
            msg.push_str(&format!(
                ": competency gate failed (regressions={}, satisfied_after={}/{})",
                cq.regressions, cq.satisfied_after, cq.total
            ));
        } else if preview.quality_delta.summary.error_count > 0 {
            msg.push_str(&format!(
                ": preview quality delta introduced {} error(s)",
                preview.quality_delta.summary.error_count
            ));
        }
        return Err(anyhow!(msg));
    }

    let stored_rel_path = store_module_if_needed(
        accepted_dir,
        &module_name,
        &module_digest,
        candidate_axi,
        &text,
    )?;

    let mut modules: BTreeMap<String, AcceptedModuleRefV1> = match previous_snapshot {
        Some(s) => s.modules,
        None => BTreeMap::new(),
    };
    modules.insert(
        module_name.clone(),
        AcceptedModuleRefV1 {
            module_digest: module_digest.clone(),
            stored_path: stored_rel_path.clone(),
        },
    );

    let snapshot_id = accepted_plane_snapshot_id_v1(previous_snapshot_id.as_ref(), &modules);
    let snapshot = AcceptedPlaneSnapshotV1 {
        version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id: previous_snapshot_id.clone(),
        created_at_unix_secs: now_unix_secs(),
        modules,
    };
    write_snapshot(accepted_dir, &snapshot)?;
    write_head(accepted_dir, &snapshot_id)?;

    let validation_report_path = if options.persist_validation_report {
        Some(persist_promotion_preview_report(
            accepted_dir,
            &snapshot_id,
            &preview,
        )?)
    } else {
        None
    };

    // Store the quality report (if present) in the accepted-plane directory.
    let (quality_report_path, quality_counts) = if let Some(report) = quality_report.as_ref() {
        let filename = format!(
            "{}__{}__{}.json",
            digest_to_filename(snapshot_id.as_str()),
            sanitize_path_component(&module_name),
            digest_to_filename(&module_digest)
        );
        let rel_path = PathBuf::from(ACCEPTED_PLANE_QUALITY_DIR).join(filename);
        let abs_path = accepted_dir.join(&rel_path);
        fs::write(&abs_path, serde_json::to_string_pretty(report)?)?;
        (
            Some(rel_path.to_string_lossy().to_string()),
            Some((
                report.summary.error_count,
                report.summary.warning_count,
                report.summary.info_count,
            )),
        )
    } else {
        (None, None)
    };

    let event = AcceptedPlaneEventV1 {
        version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
        created_at_unix_secs: now_unix_secs(),
        action: "promote".to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id,
        module_name,
        module_digest,
        stored_module_path: stored_rel_path,
        message: options.message.clone(),
        quality_profile: if quality_profile == "off" {
            None
        } else {
            Some(quality_profile.clone())
        },
        quality_report_path,
        quality_error_count: quality_counts.map(|(e, _, _)| e),
        quality_warning_count: quality_counts.map(|(_, w, _)| w),
        quality_info_count: quality_counts.map(|(_, _, i)| i),
        constraints_cert_path: Some(constraints_cert_rel_path.to_string_lossy().to_string()),
        constraints_constraint_count: Some(constraints_proof.constraint_count),
        constraints_instance_count: Some(constraints_proof.instance_count),
        constraints_check_count: Some(constraints_proof.check_count),
        validation_report_path: validation_report_path.clone(),
        validation_ok: Some(preview.ok),
    };
    append_event(accepted_dir, &event)?;

    let semantic_commit = semantic_commit_from_promotion(
        &event,
        read_sem_head_commit_id(accepted_dir)?,
        &snapshot,
        preview.evolution_preview.as_ref(),
    )?;
    write_semantic_commit(accepted_dir, &semantic_commit)?;
    write_sem_head_commit_id(accepted_dir, &semantic_commit.commit_id)?;
    write_sem_ref_pointer_for_main(accepted_dir, &semantic_commit.commit_id)?;

    Ok(PromoteReviewedModuleResultV1 {
        snapshot_id,
        validation_report_path: validation_report_path.clone(),
        stored_report_path: validation_report_path,
    })
}

pub fn build_pathdb_from_snapshot(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
    out_axpd: &Path,
) -> Result<()> {
    ensure_layout(accepted_dir)?;

    let snapshot_id = resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    let snapshot = read_snapshot(accepted_dir, &snapshot_id)?;

    let mut db = axiograph_pathdb::PathDB::new();

    let mut module_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    for (module_name, module_ref) in &snapshot.modules {
        let path = resolve_stored_module_path_under_dir(accepted_dir, &module_ref.stored_path)?;
        let text = fs::read_to_string(&path).map_err(|e| {
            anyhow!(
                "failed to read module `{}` at `{}`: {e}",
                module_name,
                path.display()
            )
        })?;

        let digest = AxiDigest::from_axi_text(&text);
        if digest != module_ref.module_digest {
            return Err(anyhow!(
                "module `{}` digest mismatch: manifest={} file={}",
                module_name,
                module_ref.module_digest,
                digest
            ));
        }

        let module = require_canonical_axi_text(&text)?.into_parts().1;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &module,
        )?;

        // Grounding always has evidence: embed the canonical `.axi` module text
        // as an untrusted DocChunk so LLM/UI workflows can cite and open it.
        module_chunks.push(crate::doc_chunks::chunk_from_axi_module_text(
            module_name,
            digest.as_str(),
            &text,
        ));
    }

    let _ = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &module_chunks);
    db.build_indexes();
    fs::write(out_axpd, db.to_bytes()?)?;
    Ok(())
}

#[allow(dead_code)]
pub fn preview_reviewed_module_v1(
    candidate_axi: &Path,
    accepted_dir: &Path,
    options: &PromotionPreviewOptionsV1,
) -> Result<PromotionPreviewReportV1> {
    ensure_layout(accepted_dir)?;
    let text = fs::read_to_string(candidate_axi)?;
    let validated = require_canonical_axi_text(&text)?.into_parts().1;
    let reviewed = axiograph_pathdb::axi_module_typecheck::review_axi_v1_module(
        validated,
        axiograph_pathdb::axi_module_typecheck::ReviewStamp::default(),
    );
    let module_digest = AxiDigest::from_axi_text(&text);
    let previous_snapshot_id = read_head(accepted_dir)?;
    let previous_snapshot = if let Some(prev) = previous_snapshot_id.as_ref() {
        Some(read_snapshot(accepted_dir, prev)?)
    } else {
        None
    };
    preview_reviewed_module_from_reviewed(
        accepted_dir,
        previous_snapshot.as_ref(),
        &reviewed,
        &text,
        &module_digest,
        options,
    )
}

fn preview_reviewed_module_from_reviewed(
    accepted_dir: &Path,
    previous_snapshot: Option<&AcceptedPlaneSnapshotV1>,
    reviewed: &axiograph_pathdb::Module<axiograph_pathdb::Reviewed>,
    text: &str,
    module_digest: &AxiDigest,
    options: &PromotionPreviewOptionsV1,
) -> Result<PromotionPreviewReportV1> {
    let before_db = build_pathdb_for_snapshot_state(accepted_dir, previous_snapshot, None)?.0;
    let (after_db, import_summary) =
        build_pathdb_for_snapshot_state(accepted_dir, previous_snapshot, Some(reviewed))?;

    let quality_profile = options.quality_profile.trim().to_ascii_lowercase();
    let quality_plane = if options.quality_plane.trim().is_empty() {
        "both".to_string()
    } else {
        options.quality_plane.trim().to_ascii_lowercase()
    };
    let quality_delta = if quality_profile == "off" {
        empty_quality_delta_report("accepted_plane:promotion_preview", &quality_plane)
    } else {
        crate::proposals_validate::quality_delta_report_v1(
            &before_db,
            &after_db,
            &quality_profile,
            &quality_plane,
        )?
    };
    let competency_gate = if options.competency_questions.is_empty() {
        None
    } else {
        Some(crate::proposals_validate::competency_gate_report_v1(
            &before_db,
            &after_db,
            &options.competency_questions,
            &options.competency_gate,
        )?)
    };
    let runtime_theory_check = runtime_theory_summary_for_promotion_text(text)?;
    let runtime_theory_ok = runtime_theory_check.as_ref().is_none_or(|summary| {
        summary.blocking_errors == 0
            && summary.residual_obligation_ids.is_empty()
            && summary.completeness_claim.starts_with("claimed_under_")
            && summary.ontology_closure_claim.starts_with("claimed_under_")
    });
    let ok = quality_delta.summary.error_count == 0
        && competency_gate
            .as_ref()
            .map(|gate| gate.gate_passed)
            .unwrap_or(true)
        && runtime_theory_ok;

    let mut reasons = vec![
        "preview compares the current accepted snapshot against the would-be accepted snapshot"
            .to_string(),
        "soundness covers Rust-side typing, conservative constraint gates, and preview checks; not completeness".to_string(),
    ];
    if let Some(base) = previous_snapshot {
        reasons.push(format!("base accepted snapshot: {}", base.snapshot_id));
    } else {
        reasons.push("base accepted snapshot: (none)".to_string());
    }
    reasons.push(format!(
        "candidate module `{}` with digest {}",
        reviewed.module().module_name,
        module_digest
    ));
    if !text.is_empty() {
        reasons
            .push("candidate module text was parsed as canonical .axi before preview".to_string());
    }
    if let Some(cq) = competency_gate.as_ref() {
        reasons.push(format!(
            "competency coverage compared {} question(s) before and after promotion preview",
            cq.total
        ));
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        reasons.push(format!(
            "runtime theory check compared {} compiled theor{} with completeness={} and ontology_closure={}",
            summary.theory_count,
            if summary.theory_count == 1 { "y" } else { "ies" },
            summary.completeness_claim,
            summary.ontology_closure_claim
        ));
        if summary.blocking_errors > 0 {
            reasons.push(format!(
                "promotion preview has {} blocking runtime theory judgment(s)",
                summary.blocking_errors
            ));
        }
    }

    let trust = crate::proposals_validate::ProposalValidationTrustContractV1 {
        trust_class: "accepted_promotion_preview".to_string(),
        soundness: "candidate_module_typechecked_constraints_checked_and_previewed".to_string(),
        coverage: if options.competency_questions.is_empty() {
            "accepted_snapshot_delta_only".to_string()
        } else {
            "accepted_snapshot_delta_plus_competency_questions".to_string()
        },
        scope: "accepted_snapshot_scoped_preview".to_string(),
        reasons,
    };
    let after_meta =
        axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&after_db).unwrap_or_default();
    let import_summary: PromotionImportSummaryV1 = import_summary.unwrap_or_default().into();
    let runtime_theory_residuals = runtime_theory_check
        .as_ref()
        .into_iter()
        .flat_map(|summary| {
            summary
                .residual_obligation_ids
                .iter()
                .map(|id| format!("runtime theory residual obligation `{id}`"))
        })
        .collect::<Vec<_>>();
    let mut evolution_preview = crate::evolution_preview::build_evolution_preview_v1(
        "accepted_promotion_preview",
        previous_snapshot.map(|s| s.snapshot_id.clone()),
        reviewed.module().module_name.clone(),
        crate::evolution_preview::promotion_typed_change_summary(
            &reviewed.module().module_name,
            module_digest,
            &import_summary,
        ),
        &quality_delta,
        competency_gate.as_ref(),
        &trust,
        Some(crate::semantic_claim::runtime_semantic_summary_for_preview(
            &after_meta,
            &trust,
            competency_gate.as_ref(),
            quality_delta.summary.error_count,
        )),
        runtime_theory_residuals,
        ok,
    );
    evolution_preview.runtime_theory_check = runtime_theory_check;

    Ok(PromotionPreviewReportV1 {
        version: ACCEPTED_PLANE_PROMOTION_PREVIEW_VERSION_V1.to_string(),
        base_snapshot_id: previous_snapshot.map(|s| s.snapshot_id.clone()),
        candidate_module_name: reviewed.module().module_name.clone(),
        candidate_axi_digest_v1: module_digest.clone(),
        import_summary,
        evolution_preview: Some(evolution_preview),
        quality_delta,
        competency_gate,
        trust,
        ok,
        stored_report_path: None,
    })
}

fn runtime_theory_summary_for_promotion_text(
    text: &str,
) -> Result<Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    match crate::runtime_theory_check::runtime_theory_check_reports_from_axi_text(
        text,
        None,
        axiograph_pathdb::RuntimeTheoryClosureTierV1::FiniteFragment,
    ) {
        Ok(report) => Ok(Some(report.summary)),
        Err(err) if err.to_string().contains("no compiled theories matched") => Ok(None),
        Err(err) => Err(err),
    }
}

fn empty_quality_delta_report(input: &str, plane: &str) -> crate::quality::QualityReportV1 {
    crate::quality::QualityReportV1 {
        version: "quality_report_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input.to_string(),
        profile: "off".to_string(),
        plane: plane.to_string(),
        summary: crate::quality::QualitySummaryV1::default(),
        findings: Vec::new(),
    }
}

fn build_pathdb_for_snapshot_state(
    accepted_dir: &Path,
    snapshot: Option<&AcceptedPlaneSnapshotV1>,
    candidate: Option<&axiograph_pathdb::Module<axiograph_pathdb::Reviewed>>,
) -> Result<(
    axiograph_pathdb::PathDB,
    Option<axiograph_pathdb::axi_module_import::AxiSchemaV1ImportSummary>,
)> {
    let mut db = axiograph_pathdb::PathDB::new();
    let candidate_name = candidate.map(|m| m.module().module_name.as_str());
    let mut candidate_summary = None;

    if let Some(snapshot) = snapshot {
        for (module_name, module_ref) in &snapshot.modules {
            if Some(module_name.as_str()) == candidate_name {
                continue;
            }
            let path = accepted_dir.join(&module_ref.stored_path);
            let text = fs::read_to_string(&path).map_err(|e| {
                anyhow!(
                    "failed to read module `{}` at `{}`: {e}",
                    module_name,
                    path.display()
                )
            })?;
            let digest = AxiDigest::from_axi_text(&text);
            if digest != module_ref.module_digest {
                return Err(anyhow!(
                    "module `{}` digest mismatch: manifest={} file={}",
                    module_name,
                    module_ref.module_digest,
                    digest
                ));
            }
            let module = require_canonical_axi_text(&text)?.into_parts().1;
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
                &mut db, &module,
            )?;
        }
    }

    if let Some(candidate) = candidate {
        candidate_summary = Some(
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
                &mut db, candidate,
            )?,
        );
    }

    db.build_indexes();
    Ok((db, candidate_summary))
}

fn promotion_preview_report_path(
    accepted_dir: &Path,
    snapshot_id: &AcceptedSnapshotId,
    module_name: &str,
    module_digest: &AxiDigest,
) -> PathBuf {
    let file = format!(
        "{}__{}__{}.json",
        digest_to_filename(snapshot_id.as_str()),
        sanitize_path_component(module_name),
        digest_to_filename(module_digest)
    );
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_VALIDATIONS_DIR)
        .join(file)
}

fn persist_promotion_preview_report(
    accepted_dir: &Path,
    snapshot_id: &AcceptedSnapshotId,
    report: &PromotionPreviewReportV1,
) -> Result<String> {
    let abs_path = promotion_preview_report_path(
        accepted_dir,
        snapshot_id,
        &report.candidate_module_name,
        &report.candidate_axi_digest_v1,
    );
    let rel_path = abs_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&abs_path)
        .to_string_lossy()
        .to_string();
    let mut stored = report.clone();
    stored.stored_report_path = Some(rel_path.clone());
    fs::write(&abs_path, serde_json::to_string_pretty(&stored)?)?;
    Ok(rel_path)
}

#[cfg_attr(not(test), allow(dead_code))]
fn reconciliation_preview_report_path(
    accepted_dir: &Path,
    reconciliation_id: &AxiDigest,
) -> PathBuf {
    let file = format!(
        "reconciliation__{}.json",
        digest_to_filename(reconciliation_id.as_str())
    );
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_VALIDATIONS_DIR)
        .join(file)
}

#[cfg_attr(not(test), allow(dead_code))]
fn persist_reconciliation_preview_report(
    accepted_dir: &Path,
    report: &ReconciliationPreviewReportV1,
) -> Result<String> {
    let abs_path = reconciliation_preview_report_path(accepted_dir, &report.reconciliation_id);
    let rel_path = abs_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&abs_path)
        .to_string_lossy()
        .to_string();
    let mut stored = report.clone();
    stored.stored_report_path = Some(rel_path.clone());
    fs::write(&abs_path, serde_json::to_string_pretty(&stored)?)?;
    Ok(rel_path)
}

pub fn persist_world_model_run_record(
    accepted_dir: &Path,
    record: &WorldModelRunRecordV1,
) -> Result<PathBuf> {
    ensure_layout(accepted_dir)?;
    let path = world_model_run_record_path(accepted_dir, &record.run_id);
    if path.exists() {
        let existing = read_world_model_run_record(accepted_dir, &record.run_id)?;
        if existing != *record {
            return Err(anyhow!(
                "world-model run id collision: `{}` already exists with different contents",
                record.run_id
            ));
        }
        return Ok(path);
    }

    let json = serde_json::to_string_pretty(record)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn persist_pathdb_semantic_commit(
    accepted_dir: &Path,
    accepted_snapshot_id: &AcceptedSnapshotId,
    pathdb_snapshot_id: &PathdbSnapshotId,
    options: &PathdbSemanticCommitOptionsV1,
) -> Result<SemCommitV1> {
    ensure_layout(accepted_dir)?;
    let accepted_snapshot = read_snapshot(accepted_dir, accepted_snapshot_id)?;
    let _pathdb_snapshot =
        crate::pathdb_wal::read_pathdb_snapshot_for_cli(accepted_dir, pathdb_snapshot_id.as_str())?;

    let parent_commit_id = read_sem_head_commit_id(accepted_dir)?;
    let commit = semantic_commit_from_pathdb_overlay(
        &accepted_snapshot,
        pathdb_snapshot_id,
        parent_commit_id,
        options,
    );
    write_semantic_commit(accepted_dir, &commit)?;
    write_sem_head_commit_id(accepted_dir, &commit.commit_id)?;
    Ok(commit)
}

pub fn read_world_model_run_record(
    accepted_dir: &Path,
    run_id: &WorldModelRunId,
) -> Result<WorldModelRunRecordV1> {
    let path = world_model_run_record_path(accepted_dir, run_id);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read world-model run manifest `{}`: {e}",
            path.display()
        )
    })?;
    let record: WorldModelRunRecordV1 = serde_json::from_str(&text)?;
    if record.run_id != *run_id {
        return Err(anyhow!(
            "world-model run manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            run_id,
            record.run_id
        ));
    }
    Ok(record)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn persist_projection_manifest(
    accepted_dir: &Path,
    manifest: &ProjectionManifestV1,
) -> Result<PathBuf> {
    ensure_layout(accepted_dir)?;
    read_snapshot(accepted_dir, &manifest.accepted_snapshot_id)?;

    let manifest = normalize_projection_manifest(manifest.clone())?;
    let path = projection_manifest_path(accepted_dir, &manifest.projection_id);
    if path.exists() {
        let existing = read_projection_manifest(accepted_dir, &manifest.projection_id)?;
        if existing != manifest {
            return Err(anyhow!(
                "backend projection id collision `{}`: existing manifest differs",
                manifest.projection_id
            ));
        }
        return Ok(path);
    }

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&path, json)?;
    Ok(path)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn read_projection_manifest(
    accepted_dir: &Path,
    projection_id: &AxiDigest,
) -> Result<ProjectionManifestV1> {
    let path = projection_manifest_path(accepted_dir, projection_id);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read projection manifest `{}`: {e}",
            path.display()
        )
    })?;
    let manifest = normalize_projection_manifest(serde_json::from_str(&text)?)?;
    if manifest.projection_id != *projection_id {
        return Err(anyhow!(
            "projection manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            projection_id,
            manifest.projection_id
        ));
    }
    Ok(manifest)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn persist_projection_semantic_commit(
    accepted_dir: &Path,
    manifest: &ProjectionManifestV1,
    options: &ProjectionSemanticCommitOptionsV1,
) -> Result<SemCommitV1> {
    ensure_layout(accepted_dir)?;
    let manifest = normalize_projection_manifest(manifest.clone())?;
    persist_projection_manifest(accepted_dir, &manifest)?;

    let parent_commit_id = read_sem_head_commit_id(accepted_dir)?;
    let commit = semantic_commit_from_projection_manifest(&manifest, parent_commit_id, options);
    write_semantic_commit(accepted_dir, &commit)?;
    write_sem_head_commit_id(accepted_dir, &commit.commit_id)?;
    Ok(commit)
}

fn ensure_layout(accepted_dir: &Path) -> Result<()> {
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_MODULES_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_QUALITY_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_CERTS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_COMMITS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_SLICES_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_REVIEW_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_EVIDENCE_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_WM_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_TAGS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_VALIDATIONS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_PROJECTIONS_DIR))?;
    // Log is append-only; create it if it doesn't exist.
    let log_path = accepted_dir.join(ACCEPTED_PLANE_LOG_V1);
    if !log_path.exists() {
        fs::write(&log_path, "")?;
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize_path_component(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn validate_sem_ref_suffix(name: String, label: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{label} name must not be empty"));
    }
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(anyhow!(
                "{label} name `{trimmed}` has an invalid path segment"
            ));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | '='))
        {
            return Err(anyhow!(
                "{label} name `{trimmed}` contains unsupported ref characters"
            ));
        }
    }
    Ok(trimmed.to_string())
}

fn runtime_theory_summary_blockers(
    summary: &crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1,
    label: &str,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if summary.blocking_errors > 0 {
        blockers.push(format!(
            "{label} has {} blocking runtime theory judgment(s)",
            summary.blocking_errors
        ));
    }
    if summary.blocked_obligations > 0 {
        blockers.push(format!(
            "{label} has {} blocked runtime theory obligation(s)",
            summary.blocked_obligations
        ));
    }
    if !summary.residual_obligation_ids.is_empty() {
        blockers.push(format!(
            "{label} has unresolved runtime theory residual obligation(s): {}",
            summary.residual_obligation_ids.join(", ")
        ));
    }
    blockers
}

fn gate_summary_blockers(
    summary: &crate::evolution_preview::SemGateSummaryV1,
    label: &str,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !summary.ok {
        blockers.push(format!("{label} gate summary is not ok"));
    }
    if summary.residual_obligation_count > 0 {
        blockers.push(format!(
            "{label} gate summary carries {} residual obligation(s)",
            summary.residual_obligation_count
        ));
    }
    if let Some(competency) = summary.competency.as_ref() {
        if !competency.gate_passed {
            blockers.push(format!(
                "{label} competency gate failed (regressions={}, satisfied_after={}/{})",
                competency.regressions, competency.satisfied_after, competency.total
            ));
        }
    }
    if let Some(runtime_theory) = summary.runtime_theory_check.as_ref() {
        blockers.extend(runtime_theory_summary_blockers(runtime_theory, label));
    }
    blockers
}

fn semantic_commit_gate_blockers(commit: &SemCommitV1) -> Vec<String> {
    let mut blockers = Vec::new();
    if commit.validation_ok == Some(false) {
        blockers.push(format!(
            "commit `{}` has validation_ok=false",
            commit.commit_id
        ));
    }
    if let Some(summary) = commit.gate_summary.as_ref() {
        blockers.extend(gate_summary_blockers(summary, "commit"));
    }
    if let Some(summary) = commit.delta.gate_summary.as_ref() {
        blockers.extend(gate_summary_blockers(summary, "delta"));
    }
    if let Some(runtime_theory) = commit.delta.runtime_theory_check.as_ref() {
        blockers.extend(runtime_theory_summary_blockers(runtime_theory, "delta"));
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

fn validate_semantic_ref_update(
    accepted_dir: &Path,
    target: &SemRefNameV1,
    commit: &SemCommitV1,
) -> Result<()> {
    let ref_name = target.as_ref_name();
    let gate_blockers = semantic_commit_gate_blockers(commit);
    match target {
        SemRefNameV1::Main => {
            if !matches!(
                commit.kind,
                SemCommitKindV1::Promote | SemCommitKindV1::Merge | SemCommitKindV1::Validation
            ) {
                return Err(anyhow!(
                    "semantic ref `heads/main` may only point at accepted promotion, reviewed merge, or validation commits; got {:?}",
                    commit.kind
                ));
            }
            if !gate_blockers.is_empty() {
                return Err(anyhow!(
                    "semantic ref `heads/main` cannot move to commit `{}` because materialization gates are blocked: {}",
                    commit.commit_id,
                    gate_blockers.join("; ")
                ));
            }
        }
        SemRefNameV1::WorldModel { .. } => {
            if commit.kind != SemCommitKindV1::WorldModelRun {
                return Err(anyhow!(
                    "world-model refs (`heads/wm/*`) require `WorldModelRun` commits; got {:?}",
                    commit.kind
                ));
            }
            let run_id = commit
                .world_model_run_id
                .as_ref()
                .or(commit.provenance.world_model_run_id.as_ref())
                .ok_or_else(|| {
                    anyhow!(
                        "world-model ref `{ref_name}` requires a commit with world_model_run_id provenance"
                    )
                })?;
            if !commit
                .delta
                .world_model_run_refs
                .iter()
                .any(|id| id == run_id)
            {
                return Err(anyhow!(
                    "world-model ref `{ref_name}` requires delta.world_model_run_refs to include `{run_id}`"
                ));
            }
            read_world_model_run_record(accepted_dir, run_id).map_err(|err| {
                anyhow!(
                    "world-model ref `{ref_name}` requires persisted WorldModelRunRecord `{run_id}`: {err}"
                )
            })?;
        }
        SemRefNameV1::Review { .. } => {
            if commit.kind == SemCommitKindV1::WorldModelRun && !gate_blockers.is_empty() {
                return Err(anyhow!(
                    "review ref `{ref_name}` cannot accept world-model commit `{}` because gates are blocked: {}",
                    commit.commit_id,
                    gate_blockers.join("; ")
                ));
            }
        }
        SemRefNameV1::Evidence { .. } => {
            if !matches!(
                commit.kind,
                SemCommitKindV1::EvidenceCommit
                    | SemCommitKindV1::WorldModelRun
                    | SemCommitKindV1::Validation
            ) {
                return Err(anyhow!(
                    "evidence refs (`heads/evidence/*`) may only point at evidence, world-model, or validation commits; got {:?}",
                    commit.kind
                ));
            }
        }
        SemRefNameV1::Tag { .. } => {
            if !matches!(
                commit.kind,
                SemCommitKindV1::Promote | SemCommitKindV1::Merge | SemCommitKindV1::Validation
            ) {
                return Err(anyhow!(
                    "semantic tags (`tags/*`) may only point at accepted promotion, reviewed merge, or validation commits; got {:?}",
                    commit.kind
                ));
            }
            if !gate_blockers.is_empty() {
                return Err(anyhow!(
                    "semantic tag `{ref_name}` cannot point at commit `{}` because materialization gates are blocked: {}",
                    commit.commit_id,
                    gate_blockers.join("; ")
                ));
            }
            if let Ok(existing) = read_sem_ref_pointer(accepted_dir, &ref_name) {
                if existing.commit_id != commit.commit_id {
                    return Err(anyhow!(
                        "semantic tag `{ref_name}` is immutable: existing commit={} attempted={}",
                        existing.commit_id,
                        commit.commit_id
                    ));
                }
            }
        }
        SemRefNameV1::Generic { .. } => {}
    }
    Ok(())
}

fn digest_to_filename(digest: impl AsRef<str>) -> String {
    digest.as_ref().replace(':', "_")
}

fn accepted_plane_snapshot_id_v1(
    previous_snapshot_id: Option<&AcceptedSnapshotId>,
    modules: &BTreeMap<String, AcceptedModuleRefV1>,
) -> AcceptedSnapshotId {
    use std::fmt::Write as _;

    let mut s = String::new();
    let _ = write!(&mut s, "{ACCEPTED_PLANE_VERSION_V1};");
    let _ = write!(
        &mut s,
        "prev={};",
        previous_snapshot_id
            .map(|id| id.as_str())
            .unwrap_or("(none)")
    );
    for (name, m) in modules {
        let _ = write!(
            &mut s,
            "module={name};digest={};path={};",
            m.module_digest.as_str(),
            m.stored_path
        );
    }
    AcceptedSnapshotId::new(axiograph_dsl::digest::axi_digest_v1(&s))
}

fn read_head(accepted_dir: &Path) -> Result<Option<AcceptedSnapshotId>> {
    let path = accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let id = text.trim().to_string();
    if id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(AcceptedSnapshotId::new(id)))
    }
}

fn write_head(accepted_dir: &Path, snapshot_id: &AcceptedSnapshotId) -> Result<()> {
    fs::write(
        accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE),
        format!("{snapshot_id}\n"),
    )?;
    Ok(())
}

fn resolve_snapshot_id(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedSnapshotId> {
    let s = snapshot_id_or_latest.trim();
    if s.eq_ignore_ascii_case("latest") || s.eq_ignore_ascii_case("head") {
        return read_head(accepted_dir)?
            .ok_or_else(|| anyhow!("accepted plane has no HEAD snapshot yet"));
    }

    // Fast path: full id (manifest exists).
    if snapshot_manifest_path(accepted_dir, s).exists() {
        return Ok(AcceptedSnapshotId::new(s));
    }

    // Prefix match against existing snapshot manifests.
    fn matches_snapshot_id(query: &str, id: &str) -> bool {
        if id.starts_with(query) {
            return true;
        }
        // Allow omitting the `<algo>:` prefix when matching.
        if let Some((_algo, rest)) = id.split_once(':') {
            if rest.starts_with(query) {
                return true;
            }
        }
        // Allow copying the filename form (colon replaced with underscore).
        if query.contains('_') && !query.contains(':') {
            let query2 = query.replacen('_', ":", 1);
            if id.starts_with(&query2) {
                return true;
            }
            if let Some((_algo, rest)) = id.split_once(':') {
                if rest.starts_with(&query2) {
                    return true;
                }
            }
        }
        false
    }

    let mut matches: Vec<AcceptedSnapshotId> = Vec::new();
    let snapshots_dir = accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR);
    let rd = fs::read_dir(&snapshots_dir).map_err(|e| {
        anyhow!(
            "failed to read accepted snapshots dir `{}`: {e}",
            snapshots_dir.display()
        )
    })?;
    for entry in rd {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(snap) = serde_json::from_str::<AcceptedPlaneSnapshotV1>(&text) else {
            continue;
        };
        if matches_snapshot_id(s, snap.snapshot_id.as_str()) {
            matches.push(snap.snapshot_id);
        }
    }
    matches.sort();
    matches.dedup();

    if matches.is_empty() {
        return Err(anyhow!(
            "unknown accepted-plane snapshot `{s}` (no matching manifest in `{}`)",
            snapshots_dir.display()
        ));
    }
    if matches.len() > 1 {
        let preview = matches
            .iter()
            .take(8)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!(
            "ambiguous accepted-plane snapshot `{s}` (matches {}): {preview}",
            matches.len()
        ));
    }

    Ok(matches[0].clone())
}

fn snapshot_manifest_path(accepted_dir: &Path, snapshot_id: &str) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(snapshot_id));
    accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR).join(file)
}

fn world_model_run_record_path(accepted_dir: &Path, run_id: &WorldModelRunId) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(run_id.as_str()));
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR)
        .join(file)
}

#[cfg_attr(not(test), allow(dead_code))]
fn projection_manifest_path(accepted_dir: &Path, projection_id: &AxiDigest) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(projection_id.as_str()));
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_PROJECTIONS_DIR)
        .join(file)
}

fn read_snapshot(
    accepted_dir: &Path,
    snapshot_id: &AcceptedSnapshotId,
) -> Result<AcceptedPlaneSnapshotV1> {
    let path = snapshot_manifest_path(accepted_dir, snapshot_id.as_str());
    let text = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read snapshot manifest `{}`: {e}", path.display()))?;
    let snapshot: AcceptedPlaneSnapshotV1 = serde_json::from_str(&text)?;
    if snapshot.snapshot_id != *snapshot_id {
        return Err(anyhow!(
            "snapshot manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            snapshot_id,
            snapshot.snapshot_id
        ));
    }
    Ok(snapshot)
}

fn resolve_stored_module_path_under_dir(accepted_dir: &Path, stored_path: &str) -> Result<PathBuf> {
    let stored = Path::new(stored_path);
    if stored.is_absolute() {
        return Err(anyhow!(
            "stored module path must be relative to accepted dir: `{stored_path}`"
        ));
    }
    if stored.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(anyhow!(
            "stored module path must not escape accepted dir: `{stored_path}`"
        ));
    }

    let accepted_dir = accepted_dir.canonicalize().map_err(|e| {
        anyhow!(
            "failed to canonicalize accepted dir `{}`: {e}",
            accepted_dir.display()
        )
    })?;
    let resolved = accepted_dir.join(stored);
    let canonical = resolved.canonicalize().map_err(|e| {
        anyhow!(
            "failed to canonicalize stored module path `{}`: {e}",
            resolved.display()
        )
    })?;
    if !canonical.starts_with(&accepted_dir) {
        return Err(anyhow!(
            "stored module path escapes accepted dir: `{}`",
            canonical.display()
        ));
    }
    Ok(canonical)
}

pub(crate) fn read_single_module_accepted_axi_anchor_and_text(
    accepted_dir: &Path,
    snapshot_id: &AcceptedSnapshotId,
) -> Result<Option<(AcceptedAxiAnchor, String)>> {
    let snapshot = read_snapshot(accepted_dir, snapshot_id)?;
    if snapshot.modules.len() != 1 {
        return Ok(None);
    }

    let (module_name, module_ref) = snapshot
        .modules
        .iter()
        .next()
        .expect("single-module snapshot must contain one module");
    let path = resolve_stored_module_path_under_dir(accepted_dir, &module_ref.stored_path)?;
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read module `{}` at `{}`: {e}",
            module_name,
            path.display()
        )
    })?;

    let digest = AxiDigest::from_axi_text(&text);
    if digest != module_ref.module_digest {
        return Err(anyhow!(
            "module `{}` digest mismatch: manifest={} file={}",
            module_name,
            module_ref.module_digest,
            digest
        ));
    }

    let _ = require_canonical_axi_text(&text)?;
    Ok(Some((
        AcceptedAxiAnchor::new(snapshot.snapshot_id.clone(), digest),
        text,
    )))
}

fn write_snapshot(accepted_dir: &Path, snapshot: &AcceptedPlaneSnapshotV1) -> Result<()> {
    let path = snapshot_manifest_path(accepted_dir, snapshot.snapshot_id.as_str());
    if path.exists() {
        // Idempotency: if the snapshot already exists, it must match.
        let existing = read_snapshot(accepted_dir, &snapshot.snapshot_id)?;
        if existing.modules != snapshot.modules {
            return Err(anyhow!(
                "snapshot id collision: `{}` already exists with different contents",
                snapshot.snapshot_id
            ));
        }
        return Ok(());
    }

    let json = serde_json::to_string_pretty(snapshot)?;
    fs::write(path, json)?;
    Ok(())
}

fn sem_ref_pointer_path(accepted_dir: &Path, ref_name: &str) -> PathBuf {
    let mut ref_path = PathBuf::from(ACCEPTED_PLANE_SEM_REFS_DIR);
    for segment in ref_name.split('/') {
        ref_path.push(segment);
    }
    accepted_dir.join(ref_path)
}

fn sem_head_path(accepted_dir: &Path) -> PathBuf {
    accepted_dir.join(ACCEPTED_PLANE_SEM_HEAD_FILE)
}

fn write_sem_head_symbolic_ref_name(accepted_dir: &Path, ref_name: &str) -> Result<()> {
    let target = SemRefNameV1::parse(ref_name)?;
    let path = sem_head_path(accepted_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("ref: {}\n", target.as_ref_name()))?;
    Ok(())
}

fn parse_sem_head_text(text: &str) -> Result<Option<SemHeadV1>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some(ref_name) = trimmed.strip_prefix("ref:") {
        let target = SemRefNameV1::parse(ref_name.trim())?;
        return Ok(Some(SemHeadV1::Symbolic {
            ref_name: target.as_ref_name(),
            commit_id: None,
        }));
    }
    Ok(Some(SemHeadV1::Detached {
        commit_id: AxiDigest::new(trimmed.to_string()),
    }))
}

fn read_semantic_head_raw(accepted_dir: &Path) -> Result<Option<SemHeadV1>> {
    let path = sem_head_path(accepted_dir);
    if !path.exists() {
        return Ok(None);
    }
    parse_sem_head_text(&fs::read_to_string(&path)?)
}

fn semantic_ref_name_for_commit_id(
    accepted_dir: &Path,
    commit_id: &AxiDigest,
) -> Result<Option<String>> {
    let mut matches = Vec::new();
    if let Some(pointer) = read_sem_ref_pointer_main(accepted_dir)? {
        if pointer.commit_id == *commit_id {
            matches.push(pointer.ref_name);
        }
    }

    let refs_root = accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR);
    if refs_root.exists() {
        for entry in walkdir::WalkDir::new(&refs_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            let rel = entry
                .path()
                .strip_prefix(&refs_root)
                .map_err(|err| anyhow!("failed to relativize semantic ref path: {err}"))?;
            let ref_name = rel.to_string_lossy().replace('\\', "/");
            if ref_name == ACCEPTED_PLANE_SEM_HEADS_MAIN_REF {
                continue;
            }
            let Ok(pointer) = read_sem_ref_pointer(accepted_dir, &ref_name) else {
                continue;
            };
            if pointer.commit_id == *commit_id {
                matches.push(pointer.ref_name);
            }
        }
    }

    matches.sort_by(|left, right| {
        sem_ref_head_migration_priority(left)
            .cmp(&sem_ref_head_migration_priority(right))
            .then_with(|| left.cmp(right))
    });
    matches.dedup();
    Ok(matches.into_iter().next())
}

fn sem_ref_head_migration_priority(ref_name: &str) -> u8 {
    if ref_name == ACCEPTED_PLANE_SEM_HEADS_MAIN_REF {
        0
    } else if ref_name.starts_with("heads/review/") {
        1
    } else if ref_name.starts_with("heads/wm/") {
        2
    } else if ref_name.starts_with("heads/evidence/") {
        3
    } else if ref_name.starts_with("heads/") {
        4
    } else if ref_name.starts_with("tags/") {
        5
    } else {
        6
    }
}

pub fn read_semantic_head(accepted_dir: &Path) -> Result<Option<SemHeadV1>> {
    let Some(head) = read_semantic_head_raw(accepted_dir)? else {
        return Ok(None);
    };
    match head {
        SemHeadV1::Symbolic { ref_name, .. } => {
            let pointer = read_sem_ref_pointer(accepted_dir, &ref_name)?;
            Ok(Some(SemHeadV1::Symbolic {
                ref_name: pointer.ref_name,
                commit_id: Some(pointer.commit_id),
            }))
        }
        SemHeadV1::Detached { commit_id } => {
            if let Some(ref_name) = semantic_ref_name_for_commit_id(accepted_dir, &commit_id)? {
                write_sem_head_symbolic_ref_name(accepted_dir, &ref_name)?;
                Ok(Some(SemHeadV1::Symbolic {
                    ref_name,
                    commit_id: Some(commit_id),
                }))
            } else {
                Ok(Some(SemHeadV1::Detached { commit_id }))
            }
        }
    }
}

#[allow(dead_code)]
fn sem_main_ref_path(accepted_dir: &Path) -> PathBuf {
    accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_MAIN_FILE)
}

fn sem_commit_path(accepted_dir: &Path, commit_id: &AxiDigest) -> PathBuf {
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_COMMITS_DIR)
        .join(format!("{}.json", digest_to_filename(commit_id.as_str())))
}

#[allow(dead_code)]
#[allow(dead_code)]
fn sem_reconciliation_path(accepted_dir: &Path, reconciliation_id: &AxiDigest) -> PathBuf {
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR)
        .join(format!(
            "{}.json",
            digest_to_filename(reconciliation_id.as_str())
        ))
}

fn sem_commit_id_v1(
    parent_commit_id: Option<&AxiDigest>,
    event: &AcceptedPlaneEventV1,
) -> AxiDigest {
    use std::fmt::Write as _;
    let mut material = String::new();
    let _ = write!(
        &mut material,
        "{};snapshot={};parent={};action={};module={};module_digest={};",
        ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1,
        event.snapshot_id,
        parent_commit_id.map(|id| id.as_str()).unwrap_or("(none)"),
        event.action,
        event.module_name,
        event.module_digest
    );
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&material))
}

#[cfg_attr(not(test), allow(dead_code))]
fn sem_merge_commit_id_v1(
    parent_commit_id: Option<&AxiDigest>,
    reconciliation: &SemReconciliationV1,
    validation_report_path: Option<&str>,
) -> AxiDigest {
    use std::fmt::Write as _;

    let mut material = String::new();
    let _ = write!(
        &mut material,
        "{};kind=merge;parent={};reconciliation={};base={};left={};right={};policy={};validation={};",
        ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1,
        parent_commit_id.map(|id| id.as_str()).unwrap_or("(none)"),
        reconciliation.reconciliation_id,
        reconciliation.base_commit_id,
        reconciliation.left_commit_id,
        reconciliation.right_commit_id,
        reconciliation.policy,
        validation_report_path.unwrap_or("(none)")
    );
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&material))
}

#[cfg_attr(not(test), allow(dead_code))]
fn semantic_commit_snapshot_for_reconciliation(
    accepted_dir: &Path,
    commit_id: &AxiDigest,
) -> Result<(AcceptedSnapshotId, Option<PathdbSnapshotId>)> {
    let commit = read_semantic_commit(accepted_dir, commit_id)?;
    Ok((commit.accepted_snapshot_id, commit.pathdb_snapshot_id))
}

#[cfg_attr(not(test), allow(dead_code))]
fn reconciliation_parent_commit_id(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Option<AxiDigest> {
    let _ = accepted_dir;
    Some(reconciliation.right_commit_id.clone())
}

#[cfg_attr(not(test), allow(dead_code))]
fn verify_reconciliation_target_head(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Result<()> {
    if let Some(ref_name) = reconciliation.target_ref_name.as_ref() {
        let pointer = read_sem_ref_pointer(accepted_dir, ref_name)?;
        if pointer.commit_id != reconciliation.right_commit_id {
            return Err(anyhow!(
                "semantic reconciliation target ref `{}` moved after reconciliation was recorded: expected right_commit_id={} got={}",
                ref_name,
                reconciliation.right_commit_id,
                pointer.commit_id
            ));
        }
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
fn reconciliation_with_outcome_commit(
    reconciliation: &SemReconciliationV1,
    commit_id: &AxiDigest,
) -> SemReconciliationV1 {
    let mut updated = reconciliation.clone();
    updated.outcome_commit_id = Some(commit_id.clone());
    updated
}

fn reconciliation_with_current_refs(
    reconciliation: &SemReconciliationV1,
    source_ref_name: &str,
    target_ref_name: &str,
) -> SemReconciliationV1 {
    let mut updated = reconciliation.clone();
    updated.source_ref_name = Some(source_ref_name.to_string());
    updated.target_ref_name = Some(target_ref_name.to_string());
    updated.resolved_ref_name = Some(target_ref_name.to_string());
    updated
}

fn reconciliation_decision_materializes(resolution: &str) -> bool {
    let normalized = resolution.trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "" | "manual_review" | "review_required" | "unresolved" | "todo" | "defer"
    )
}

fn unresolved_reconciliation_decision_blockers(
    reconciliation: &SemReconciliationV1,
) -> Vec<String> {
    let mut blockers = Vec::new();
    for conflict in &reconciliation.conflicts {
        let decision = reconciliation.decisions.iter().find(|decision| {
            decision.artifact.artifact_kind == conflict.artifact.artifact_kind
                && decision.artifact.artifact_id == conflict.artifact.artifact_id
        });
        match decision {
            None => blockers.push(format!(
                "{} `{}` has no recorded resolver decision",
                conflict.artifact.artifact_kind, conflict.artifact.artifact_id
            )),
            Some(decision) if !reconciliation_decision_materializes(&decision.resolution) => {
                blockers.push(format!(
                    "{} `{}` has non-materializing resolver decision `{}`",
                    conflict.artifact.artifact_kind,
                    conflict.artifact.artifact_id,
                    decision.resolution
                ));
            }
            Some(_) => {}
        }
    }
    blockers
}

fn evolution_preview_materialization_blockers(
    preview: &crate::evolution_preview::EvolutionPreviewV1,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !preview.ok {
        blockers.push(format!(
            "{} preview is not ok for `{}`",
            preview.kind, preview.candidate_label
        ));
    }
    if preview.quality_delta.summary.error_count > 0 {
        blockers.push(format!(
            "quality gate has {} error(s)",
            preview.quality_delta.summary.error_count
        ));
    }
    if let Some(gate) = preview.competency_gate.as_ref() {
        if !gate.gate_passed {
            blockers.push(format!(
                "competency gate failed (regressions={}, satisfied_after={}/{})",
                gate.regressions, gate.satisfied_after, gate.total
            ));
        }
    }
    if preview.trust_delta.regressions > 0 {
        blockers.push(format!(
            "trust gate has {} regression(s)",
            preview.trust_delta.regressions
        ));
    }
    if preview.coverage_summary.regressions > 0 {
        blockers.push(format!(
            "coverage gate has {} regression(s)",
            preview.coverage_summary.regressions
        ));
    }
    if let Some(runtime_theory) = preview.runtime_theory_check.as_ref() {
        blockers.extend(runtime_theory_summary_blockers(
            runtime_theory,
            "runtime theory check",
        ));
    }
    blockers.extend(
        preview
            .residual_obligations
            .iter()
            .map(|obligation| format!("residual obligation: {obligation}")),
    );
    blockers.sort();
    blockers.dedup();
    blockers
}

fn reconciliation_runtime_refinement_handle_by_id(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    handle_id: &str,
) -> Result<crate::typed_refinement::RuntimeRefinementHandleV1> {
    reconciliation_preview_report(accepted_dir, reconciliation)
        .evolution_preview
        .refinement_candidates
        .into_iter()
        .find(|candidate| candidate.handle.id == handle_id)
        .map(|candidate| candidate.handle)
        .ok_or_else(|| anyhow!("unknown reconciliation runtime refinement handle `{handle_id}`"))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_runtime_refinement_by_id_to_reconciliation(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    handle_id: &str,
) -> Result<SemReconciliationV1> {
    Ok(
        apply_runtime_refinement_by_id_to_reconciliation_with_available_compiled_theory(
            accepted_dir,
            reconciliation,
            handle_id,
        )?
        .updated_reconciliation,
    )
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_reconciliation_with_available_compiled_theory(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> Result<ReconciliationReviewApplyResultV1> {
    let updated_reconciliation =
        apply_runtime_refinement_handle_to_reconciliation(reconciliation, handle)?;
    let base_snapshot_id =
        semantic_commit_snapshot_for_reconciliation(accepted_dir, &reconciliation.base_commit_id)
            .ok()
            .map(|(snapshot_id, _)| snapshot_id);
    let evolution_preview =
        build_reconciliation_evolution_preview_from_available_compiled_theory_v1(
            accepted_dir,
            base_snapshot_id,
            &updated_reconciliation,
        );
    Ok(ReconciliationReviewApplyResultV1 {
        handle: handle.clone(),
        base_reconciliation: reconciliation.clone(),
        updated_reconciliation,
        evolution_preview,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_by_id_to_reconciliation_with_available_compiled_theory(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    handle_id: &str,
) -> Result<ReconciliationReviewApplyResultV1> {
    let handle =
        reconciliation_runtime_refinement_handle_by_id(accepted_dir, reconciliation, handle_id)?;
    apply_runtime_refinement_handle_to_reconciliation_with_available_compiled_theory(
        accepted_dir,
        reconciliation,
        &handle,
    )
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_reconciliation(
    reconciliation: &SemReconciliationV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> Result<SemReconciliationV1> {
    handle.validate()?;
    let crate::typed_refinement::RuntimeRefinementPayloadV1::ReconciliationReview { op } =
        &handle.payload
    else {
        return Err(anyhow!(
            "runtime refinement handle `{}` is not a reconciliation refinement",
            handle.id
        ));
    };
    let crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
        reconciliation_id,
        artifact_kind,
        artifact_id,
        resolution,
        theory_obligation_ref,
        theory_subject_ref,
        theory_subject_refs,
    } = op;
    if reconciliation.reconciliation_id.to_string() != *reconciliation_id {
        return Err(anyhow!(
            "reconciliation refinement handle `{}` targets reconciliation `{}` but current reconciliation is `{}`",
            handle.id,
            reconciliation_id,
            reconciliation.reconciliation_id
        ));
    }

    let mut updated = reconciliation.clone();
    let artifact = ArtifactRefV1 {
        artifact_kind: artifact_kind.clone(),
        artifact_id: artifact_id.clone(),
        theory_obligation_ref: theory_obligation_ref.clone(),
        theory_subject_ref: theory_subject_ref
            .clone()
            .or_else(|| primary_theory_subject_ref(theory_subject_refs)),
        theory_subject_refs: theory_subject_refs.clone(),
    };
    if let Some(existing) = updated.decisions.iter_mut().find(|decision| {
        decision.artifact.artifact_kind == artifact.artifact_kind
            && decision.artifact.artifact_id == artifact.artifact_id
    }) {
        existing.resolution = resolution.clone();
        if artifact.theory_obligation_ref.is_some() || !artifact.theory_subject_refs.is_empty() {
            existing.artifact.theory_obligation_ref = artifact.theory_obligation_ref.clone();
            existing.artifact.theory_subject_ref = artifact.theory_subject_ref.clone();
            existing.artifact.theory_subject_refs = artifact.theory_subject_refs.clone();
        }
    } else {
        updated.decisions.push(SemDecisionRecordV1 {
            artifact,
            resolution: resolution.clone(),
        });
    }
    Ok(updated)
}

#[cfg_attr(not(test), allow(dead_code))]
fn reconciliation_preview_report(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> ReconciliationPreviewReportV1 {
    let base_snapshot_id =
        semantic_commit_snapshot_for_reconciliation(accepted_dir, &reconciliation.base_commit_id)
            .ok()
            .map(|(snapshot_id, _)| snapshot_id);
    let evolution_preview =
        build_reconciliation_evolution_preview_from_available_compiled_theory_v1(
            accepted_dir,
            base_snapshot_id.clone(),
            reconciliation,
        );
    ReconciliationPreviewReportV1 {
        version: ACCEPTED_PLANE_RECONCILIATION_PREVIEW_VERSION_V1.to_string(),
        reconciliation_id: reconciliation.reconciliation_id.clone(),
        base_commit_id: reconciliation.base_commit_id.clone(),
        left_commit_id: reconciliation.left_commit_id.clone(),
        right_commit_id: reconciliation.right_commit_id.clone(),
        policy: reconciliation.policy.clone(),
        source_ref_name: reconciliation.source_ref_name.clone(),
        target_ref_name: reconciliation.target_ref_name.clone(),
        resolved_ref_name: reconciliation.resolved_ref_name.clone(),
        ok: evolution_preview.ok,
        evolution_preview,
        stored_report_path: None,
    }
}

fn artifact_has_missing_theory_handles(artifact: &ArtifactRefV1) -> bool {
    artifact.theory_obligation_ref.is_none()
        && artifact.theory_subject_ref.is_none()
        && artifact.theory_subject_refs.is_empty()
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

fn reconciliation_has_missing_theory_handles(reconciliation: &SemReconciliationV1) -> bool {
    reconciliation
        .conflicts
        .iter()
        .any(|conflict| artifact_has_missing_theory_handles(&conflict.artifact))
        || reconciliation
            .decisions
            .iter()
            .any(|decision| artifact_has_missing_theory_handles(&decision.artifact))
}

fn reconciliation_snapshot_ids(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Vec<AcceptedSnapshotId> {
    let mut snapshot_ids = Vec::new();
    let mut seen = BTreeSet::new();
    for commit_id in [
        &reconciliation.base_commit_id,
        &reconciliation.left_commit_id,
        &reconciliation.right_commit_id,
    ] {
        let Ok((snapshot_id, _)) =
            semantic_commit_snapshot_for_reconciliation(accepted_dir, commit_id)
        else {
            continue;
        };
        if seen.insert(snapshot_id.to_string()) {
            snapshot_ids.push(snapshot_id);
        }
    }
    snapshot_ids
}

fn reconciliation_compiled_theory_contexts(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Vec<(
    axiograph_pathdb::kernel_ir::CompiledSchemaIr,
    Vec<axiograph_pathdb::kernel_ir::TheoryIr>,
)> {
    let mut contexts = Vec::new();
    for snapshot_id in reconciliation_snapshot_ids(accepted_dir, reconciliation) {
        let Ok(snapshot) = read_snapshot(accepted_dir, &snapshot_id) else {
            continue;
        };
        for module_ref in snapshot.modules.values() {
            let path = accepted_dir.join(&module_ref.stored_path);
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if AxiDigest::from_axi_text(&text) != module_ref.module_digest {
                continue;
            }
            let Ok(validated) =
                require_canonical_axi_text(&text).map(|canonical| canonical.into_parts().1)
            else {
                continue;
            };
            for schema in &validated.module().schemas {
                let Ok(Some(compiled_schema)) = validated.compiled_schema_ir(&schema.name) else {
                    continue;
                };
                let Ok(theories) = validated.compiled_theories_for_schema(&schema.name) else {
                    continue;
                };
                contexts.push((compiled_schema, theories));
            }
        }
    }
    contexts
}

fn build_reconciliation_evolution_preview_from_available_compiled_theory_v1(
    accepted_dir: &Path,
    base_snapshot_id: Option<AcceptedSnapshotId>,
    reconciliation: &SemReconciliationV1,
) -> crate::evolution_preview::EvolutionPreviewV1 {
    if !reconciliation_has_missing_theory_handles(reconciliation) {
        return crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            base_snapshot_id,
            reconciliation,
        );
    }

    let mut contexts = reconciliation_compiled_theory_contexts(accepted_dir, reconciliation);
    let Some((compiled_schema, theories)) = contexts.pop() else {
        return crate::evolution_preview::build_reconciliation_evolution_preview_v1(
            base_snapshot_id,
            reconciliation,
        );
    };

    let enriched = contexts
        .into_iter()
        .fold(reconciliation.clone(), |current, context| {
            crate::evolution_preview::enrich_reconciliation_with_compiled_theory_v1(
                &context.0, &context.1, &current,
            )
        });
    crate::evolution_preview::build_reconciliation_evolution_preview_from_compiled_theory_v1(
        base_snapshot_id,
        &compiled_schema,
        &theories,
        &enriched,
    )
}

fn promotion_gate_summary(
    event: &AcceptedPlaneEventV1,
    preview: Option<&crate::evolution_preview::EvolutionPreviewV1>,
) -> Option<crate::evolution_preview::SemGateSummaryV1> {
    let summary = preview.map(crate::evolution_preview::sem_gate_summary_from_evolution_preview)?;
    let rule = crate::evolution_preview::SemRuleSummaryV1 {
        constraint_count: event.constraints_constraint_count.unwrap_or(0),
        instance_count: event.constraints_instance_count.unwrap_or(0),
        check_count: event.constraints_check_count.unwrap_or(0),
        ..Default::default()
    };
    Some(summary.with_rule_summary(rule))
}

fn semantic_commit_from_promotion(
    event: &AcceptedPlaneEventV1,
    parent_commit_id: Option<AxiDigest>,
    snapshot: &AcceptedPlaneSnapshotV1,
    preview: Option<&crate::evolution_preview::EvolutionPreviewV1>,
) -> Result<SemCommitV1> {
    let commit_id = sem_commit_id_v1(parent_commit_id.as_ref(), event);
    let gate_summary = promotion_gate_summary(event, preview);
    Ok(SemCommitV1 {
        version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
        commit_id,
        parent_commit_id,
        kind: SemCommitKindV1::Promote,
        created_at_unix_secs: now_unix_secs(),
        author: "axiograph-cli".to_string(),
        message: event.message.clone(),
        action: event.action.clone(),
        provenance: SemCommitProvenanceV1 {
            source: "accepted_plane".to_string(),
            command: Some("axiograph db accept promote".to_string()),
            source_commit: None,
            world_model_run_id: None,
        },
        state: SemStateRefV1 {
            accepted_snapshot_id_before: snapshot.previous_snapshot_id.clone(),
            accepted_snapshot_id_after: Some(snapshot.snapshot_id.clone()),
            pathdb_snapshot_id_before: None,
            pathdb_snapshot_id_after: None,
            accepted_tree_digest: None,
            evidence_digests: Vec::new(),
        },
        delta: SemDeltaV1 {
            module_digests_added: vec![event.module_digest.clone()],
            module_digests_removed: Vec::new(),
            gate_summary: gate_summary.clone(),
            semantic_delta: preview.map(|value| value.semantic_delta.clone()),
            trust_summary: preview.map(|value| value.trust_summary.clone()),
            rule_summary: preview.map(|value| value.rule_summary.clone()),
            coverage_summary: preview.map(|value| value.coverage_summary.clone()),
            runtime_theory_check: preview.and_then(|value| value.runtime_theory_check.clone()),
            evidence_blobs_added: Vec::new(),
            certificate_refs_added: event.constraints_cert_path.clone().into_iter().collect(),
            quality_report_refs_added: event.quality_report_path.clone().into_iter().collect(),
            validation_report_refs_added: event
                .validation_report_path
                .clone()
                .into_iter()
                .collect(),
            projection_manifest_refs_added: Vec::new(),
            lifecycle_events: vec![SemLifecycleEventV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "module".to_string(),
                    artifact_id: event.module_digest.to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                from: Some(LifecycleStageV1::Reviewed),
                to: LifecycleStageV1::Accepted,
                reason: event.message.clone(),
            }],
            world_model_run_refs: Vec::new(),
        },
        gate_summary,
        reconciliation_id: None,
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        accepted_parent_snapshot_id: snapshot.previous_snapshot_id.clone(),
        pathdb_snapshot_id: None,
        proposal_digests: Vec::new(),
        policy: "conservative".to_string(),
        module_name: event.module_name.clone(),
        module_digest: event.module_digest.clone(),
        quality_report_path: event.quality_report_path.clone(),
        constraints_cert_path: event.constraints_cert_path.clone(),
        validation_report_path: event.validation_report_path.clone(),
        validation_ok: event.validation_ok,
        world_model_run_id: None,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn semantic_commit_from_reconciliation(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    parent_commit_id: Option<AxiDigest>,
    preview: &crate::evolution_preview::EvolutionPreviewV1,
    validation_report_path: Option<String>,
    options: &ReconciliationSemanticCommitOptionsV1,
) -> Result<SemCommitV1> {
    let (before_snapshot_id, before_pathdb_snapshot_id) =
        semantic_commit_snapshot_for_reconciliation(accepted_dir, &reconciliation.right_commit_id)?;
    let after_snapshot_id = before_snapshot_id.clone();
    let after_pathdb_snapshot_id = before_pathdb_snapshot_id.clone();
    let commit_id = sem_merge_commit_id_v1(
        parent_commit_id.as_ref(),
        reconciliation,
        validation_report_path.as_deref(),
    );
    let message = options.message.clone().or_else(|| {
        Some(format!(
            "reconcile {} conflict(s) under `{}`",
            reconciliation.conflicts.len(),
            reconciliation.policy
        ))
    });
    let gate_summary = crate::evolution_preview::sem_gate_summary_from_evolution_preview(preview);
    Ok(SemCommitV1 {
        version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
        commit_id,
        parent_commit_id,
        kind: SemCommitKindV1::Merge,
        created_at_unix_secs: now_unix_secs(),
        author: options.author.clone(),
        message: message.clone(),
        action: "semantic_reconciliation".to_string(),
        provenance: SemCommitProvenanceV1 {
            source: "semantic_vcs".to_string(),
            command: Some("axiograph sem merge".to_string()),
            source_commit: Some(reconciliation.left_commit_id.clone()),
            world_model_run_id: None,
        },
        state: SemStateRefV1 {
            accepted_snapshot_id_before: Some(before_snapshot_id.clone()),
            accepted_snapshot_id_after: Some(after_snapshot_id.clone()),
            pathdb_snapshot_id_before: before_pathdb_snapshot_id.clone(),
            pathdb_snapshot_id_after: after_pathdb_snapshot_id.clone(),
            accepted_tree_digest: None,
            evidence_digests: Vec::new(),
        },
        delta: SemDeltaV1 {
            module_digests_added: Vec::new(),
            module_digests_removed: Vec::new(),
            gate_summary: Some(gate_summary.clone()),
            semantic_delta: Some(preview.semantic_delta.clone()),
            trust_summary: Some(preview.trust_summary.clone()),
            rule_summary: Some(preview.rule_summary.clone()),
            coverage_summary: Some(preview.coverage_summary.clone()),
            runtime_theory_check: preview.runtime_theory_check.clone(),
            evidence_blobs_added: Vec::new(),
            certificate_refs_added: reconciliation.certificate_refs.clone(),
            quality_report_refs_added: Vec::new(),
            validation_report_refs_added: validation_report_path.clone().into_iter().collect(),
            projection_manifest_refs_added: Vec::new(),
            lifecycle_events: Vec::new(),
            world_model_run_refs: Vec::new(),
        },
        gate_summary: Some(gate_summary),
        reconciliation_id: Some(reconciliation.reconciliation_id.clone()),
        accepted_snapshot_id: after_snapshot_id.clone(),
        accepted_parent_snapshot_id: Some(before_snapshot_id),
        pathdb_snapshot_id: after_pathdb_snapshot_id,
        proposal_digests: Vec::new(),
        policy: reconciliation.policy.clone(),
        module_name: "(semantic_reconciliation)".to_string(),
        module_digest: reconciliation.reconciliation_id.clone(),
        quality_report_path: None,
        constraints_cert_path: None,
        validation_report_path,
        validation_ok: Some(preview.ok),
        world_model_run_id: None,
    })
}

fn pathdb_overlay_digest_v1(
    accepted_snapshot_id: &AcceptedSnapshotId,
    pathdb_snapshot_id: &PathdbSnapshotId,
    proposal_digests: &[ProposalDigest],
    world_model_run_id: Option<&WorldModelRunId>,
) -> AxiDigest {
    use std::fmt::Write as _;

    let mut material = String::new();
    let _ = write!(
        &mut material,
        "{};accepted={};pathdb={};",
        ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1, accepted_snapshot_id, pathdb_snapshot_id
    );
    for digest in proposal_digests {
        let _ = write!(&mut material, "proposal={};", digest);
    }
    let _ = write!(
        &mut material,
        "wm_run={};",
        world_model_run_id.map(|id| id.as_str()).unwrap_or("(none)")
    );
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&material))
}

fn sem_commit_id_for_pathdb_overlay_v1(
    parent_commit_id: Option<&AxiDigest>,
    accepted_snapshot_id: &AcceptedSnapshotId,
    pathdb_snapshot_id: &PathdbSnapshotId,
    proposal_digests: &[ProposalDigest],
    world_model_run_id: Option<&WorldModelRunId>,
) -> AxiDigest {
    use std::fmt::Write as _;

    let mut material = String::new();
    let _ = write!(
        &mut material,
        "{};action=pathdb_commit;accepted={};pathdb={};parent={};",
        ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1,
        accepted_snapshot_id,
        pathdb_snapshot_id,
        parent_commit_id.map(|id| id.as_str()).unwrap_or("(none)")
    );
    for digest in proposal_digests {
        let _ = write!(&mut material, "proposal={};", digest);
    }
    let _ = write!(
        &mut material,
        "wm_run={};",
        world_model_run_id.map(|id| id.as_str()).unwrap_or("(none)")
    );
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&material))
}

#[cfg_attr(not(test), allow(dead_code))]
fn projection_manifest_id_v1(manifest: &ProjectionManifestV1) -> Result<AxiDigest> {
    #[derive(Serialize)]
    struct ProjectionManifestIdentityMaterial<'a> {
        accepted_snapshot_id: &'a AcceptedSnapshotId,
        source_sem_ref_name: &'a Option<String>,
        source_sem_commit_id: &'a Option<AxiDigest>,
        compiled_ir_digest: &'a AxiDigest,
        materialization_ref: &'a String,
        backend: &'a BackendCapabilityProfileV1,
        projection: &'a ProjectionCapabilityProfileV1,
        object_mappings: &'a [ProjectionObjectMappingV1],
        relation_mappings: &'a [ProjectionRelationMappingV1],
        context_mapping: &'a ProjectionContextMappingV1,
        trust_caveats: &'a [String],
        round_trip_limitations: &'a [String],
    }

    let material = ProjectionManifestIdentityMaterial {
        accepted_snapshot_id: &manifest.accepted_snapshot_id,
        source_sem_ref_name: &manifest.source_sem_ref_name,
        source_sem_commit_id: &manifest.source_sem_commit_id,
        compiled_ir_digest: &manifest.compiled_ir_digest,
        materialization_ref: &manifest.materialization_ref,
        backend: &manifest.backend,
        projection: &manifest.projection,
        object_mappings: &manifest.object_mappings,
        relation_mappings: &manifest.relation_mappings,
        context_mapping: &manifest.context_mapping,
        trust_caveats: &manifest.trust_caveats,
        round_trip_limitations: &manifest.round_trip_limitations,
    };
    let json = serde_json::to_string(&material)?;
    Ok(AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(
        &format!("{BACKEND_PROJECTION_MANIFEST_VERSION_V1};{json}"),
    )))
}

#[cfg_attr(not(test), allow(dead_code))]
fn normalize_projection_manifest(
    mut manifest: ProjectionManifestV1,
) -> Result<ProjectionManifestV1> {
    if manifest.materialization_ref.trim().is_empty() {
        return Err(anyhow!(
            "backend projection materialization_ref must not be empty"
        ));
    }
    if let Some(ref_name) = manifest.source_sem_ref_name.as_deref() {
        let _ = SemRefNameV1::parse(ref_name)?;
    }
    manifest.version = BACKEND_PROJECTION_MANIFEST_VERSION_V1.to_string();
    if manifest.created_at_unix_secs == 0 {
        manifest.created_at_unix_secs = now_unix_secs();
    }
    manifest.projection_id = projection_manifest_id_v1(&manifest)?;
    Ok(manifest)
}

#[cfg_attr(not(test), allow(dead_code))]
fn sem_commit_id_for_projection_manifest_v1(
    parent_commit_id: Option<&AxiDigest>,
    manifest: &ProjectionManifestV1,
) -> AxiDigest {
    use std::fmt::Write as _;

    let mut material = String::new();
    let _ = write!(
        &mut material,
        "{};action=backend_projection;accepted={};projection={};parent={};",
        ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1,
        manifest.accepted_snapshot_id,
        manifest.projection_id,
        parent_commit_id.map(|id| id.as_str()).unwrap_or("(none)")
    );
    let _ = write!(
        &mut material,
        "engine={:?};support_tier={:?};materialization={};",
        manifest.backend.backend_engine,
        manifest.backend.support_tier,
        manifest.materialization_ref
    );
    AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&material))
}

#[cfg_attr(not(test), allow(dead_code))]
fn semantic_commit_from_projection_manifest(
    manifest: &ProjectionManifestV1,
    parent_commit_id: Option<AxiDigest>,
    options: &ProjectionSemanticCommitOptionsV1,
) -> SemCommitV1 {
    let commit_id = sem_commit_id_for_projection_manifest_v1(parent_commit_id.as_ref(), manifest);
    SemCommitV1 {
        version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
        commit_id,
        parent_commit_id,
        kind: SemCommitKindV1::ProjectionMaterialization,
        created_at_unix_secs: now_unix_secs(),
        author: options.author.clone(),
        message: options.message.clone(),
        action: "backend_projection".to_string(),
        provenance: SemCommitProvenanceV1 {
            source: "backend_projection".to_string(),
            command: Some("axiograph sem project-backend".to_string()),
            source_commit: manifest.source_sem_commit_id.clone(),
            world_model_run_id: None,
        },
        state: SemStateRefV1 {
            accepted_snapshot_id_before: Some(manifest.accepted_snapshot_id.clone()),
            accepted_snapshot_id_after: Some(manifest.accepted_snapshot_id.clone()),
            pathdb_snapshot_id_before: None,
            pathdb_snapshot_id_after: None,
            accepted_tree_digest: Some(manifest.compiled_ir_digest.clone()),
            evidence_digests: Vec::new(),
        },
        delta: SemDeltaV1 {
            module_digests_added: Vec::new(),
            module_digests_removed: Vec::new(),
            gate_summary: None,
            semantic_delta: None,
            trust_summary: None,
            rule_summary: None,
            coverage_summary: None,
            runtime_theory_check: None,
            evidence_blobs_added: Vec::new(),
            certificate_refs_added: Vec::new(),
            quality_report_refs_added: Vec::new(),
            validation_report_refs_added: Vec::new(),
            projection_manifest_refs_added: vec![manifest.projection_id.clone()],
            lifecycle_events: Vec::new(),
            world_model_run_refs: Vec::new(),
        },
        gate_summary: None,
        reconciliation_id: None,
        accepted_snapshot_id: manifest.accepted_snapshot_id.clone(),
        accepted_parent_snapshot_id: None,
        pathdb_snapshot_id: None,
        proposal_digests: Vec::new(),
        policy: options.policy.clone(),
        module_name: "(backend_projection)".to_string(),
        module_digest: manifest.projection_id.clone(),
        quality_report_path: None,
        constraints_cert_path: None,
        validation_report_path: None,
        validation_ok: None,
        world_model_run_id: None,
    }
}

fn semantic_commit_from_pathdb_overlay(
    accepted_snapshot: &AcceptedPlaneSnapshotV1,
    pathdb_snapshot_id: &PathdbSnapshotId,
    parent_commit_id: Option<AxiDigest>,
    options: &PathdbSemanticCommitOptionsV1,
) -> SemCommitV1 {
    let commit_id = sem_commit_id_for_pathdb_overlay_v1(
        parent_commit_id.as_ref(),
        &accepted_snapshot.snapshot_id,
        pathdb_snapshot_id,
        &options.proposal_digests,
        options.world_model_run_id.as_ref(),
    );
    SemCommitV1 {
        version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
        commit_id,
        parent_commit_id,
        kind: if options.world_model_run_id.is_some() {
            SemCommitKindV1::WorldModelRun
        } else {
            SemCommitKindV1::EvidenceCommit
        },
        created_at_unix_secs: now_unix_secs(),
        author: options.author.clone(),
        message: options.message.clone(),
        action: "pathdb_commit".to_string(),
        provenance: SemCommitProvenanceV1 {
            source: if options.world_model_run_id.is_some() {
                "world_model".to_string()
            } else {
                "pathdb_wal".to_string()
            },
            command: Some("axiograph db accept pathdb-commit".to_string()),
            source_commit: None,
            world_model_run_id: options.world_model_run_id.clone(),
        },
        state: SemStateRefV1 {
            accepted_snapshot_id_before: Some(accepted_snapshot.snapshot_id.clone()),
            accepted_snapshot_id_after: Some(accepted_snapshot.snapshot_id.clone()),
            pathdb_snapshot_id_before: None,
            pathdb_snapshot_id_after: Some(pathdb_snapshot_id.clone()),
            accepted_tree_digest: None,
            evidence_digests: options.proposal_digests.clone(),
        },
        delta: SemDeltaV1 {
            module_digests_added: Vec::new(),
            module_digests_removed: Vec::new(),
            gate_summary: options.gate_summary.clone(),
            semantic_delta: None,
            trust_summary: None,
            rule_summary: None,
            coverage_summary: None,
            runtime_theory_check: options
                .gate_summary
                .as_ref()
                .and_then(|summary| summary.runtime_theory_check.clone()),
            evidence_blobs_added: options.proposal_digests.clone(),
            certificate_refs_added: Vec::new(),
            quality_report_refs_added: Vec::new(),
            validation_report_refs_added: Vec::new(),
            projection_manifest_refs_added: Vec::new(),
            lifecycle_events: options
                .proposal_digests
                .iter()
                .map(|digest| SemLifecycleEventV1 {
                    artifact: ArtifactRefV1 {
                        artifact_kind: "proposal_set".to_string(),
                        artifact_id: digest.to_string(),
                        theory_obligation_ref: None,
                        theory_subject_ref: None,
                        theory_subject_refs: Vec::new(),
                    },
                    from: Some(LifecycleStageV1::Proposed),
                    to: LifecycleStageV1::Validated,
                    reason: options.message.clone(),
                })
                .collect(),
            world_model_run_refs: options.world_model_run_id.clone().into_iter().collect(),
        },
        gate_summary: options.gate_summary.clone(),
        reconciliation_id: None,
        accepted_snapshot_id: accepted_snapshot.snapshot_id.clone(),
        accepted_parent_snapshot_id: accepted_snapshot.previous_snapshot_id.clone(),
        pathdb_snapshot_id: Some(pathdb_snapshot_id.clone()),
        proposal_digests: options.proposal_digests.clone(),
        policy: options.policy.clone(),
        module_name: "(pathdb_overlay)".to_string(),
        module_digest: pathdb_overlay_digest_v1(
            &accepted_snapshot.snapshot_id,
            pathdb_snapshot_id,
            &options.proposal_digests,
            options.world_model_run_id.as_ref(),
        ),
        quality_report_path: None,
        constraints_cert_path: None,
        validation_report_path: None,
        validation_ok: None,
        world_model_run_id: options.world_model_run_id.clone(),
    }
}

fn write_sem_head_commit_id(accepted_dir: &Path, commit_id: &AxiDigest) -> Result<()> {
    let path = sem_head_path(accepted_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{commit_id}\n"))?;
    Ok(())
}

fn read_sem_head_commit_id(accepted_dir: &Path) -> Result<Option<AxiDigest>> {
    match read_semantic_head(accepted_dir)? {
        Some(SemHeadV1::Symbolic {
            commit_id: Some(commit_id),
            ..
        })
        | Some(SemHeadV1::Detached { commit_id }) => Ok(Some(commit_id)),
        Some(SemHeadV1::Symbolic {
            commit_id: None, ..
        }) => Ok(None),
        None => Ok(None),
    }
}

fn write_sem_ref_pointer_for_main(accepted_dir: &Path, commit_id: &AxiDigest) -> Result<PathBuf> {
    write_sem_ref_pointer_for_target(accepted_dir, &SemRefNameV1::main(), commit_id)
}

fn sem_gate_summary_for_commit(
    accepted_dir: &Path,
    commit_id: &AxiDigest,
) -> Option<crate::evolution_preview::SemGateSummaryV1> {
    read_semantic_commit(accepted_dir, commit_id)
        .ok()
        .and_then(|commit| commit.gate_summary)
}

fn write_sem_ref_pointer_for_target(
    accepted_dir: &Path,
    target: &SemRefNameV1,
    commit_id: &AxiDigest,
) -> Result<PathBuf> {
    let ref_name = target.as_ref_name();
    let path = write_sem_ref_pointer(
        accepted_dir,
        &ref_name,
        SemRefPointerV1 {
            version: ACCEPTED_PLANE_SEM_REF_POINTER_VERSION_V1.to_string(),
            ref_name: ref_name.clone(),
            commit_id: commit_id.clone(),
            updated_at_unix_secs: now_unix_secs(),
            gate_summary: sem_gate_summary_for_commit(accepted_dir, commit_id),
        },
    )?;
    if matches!(target, SemRefNameV1::Main) {
        write_sem_head_symbolic_ref_name(accepted_dir, &ref_name)?;
    }
    Ok(path)
}

pub fn write_sem_ref_pointer(
    accepted_dir: &Path,
    ref_name: &str,
    pointer: SemRefPointerV1,
) -> Result<PathBuf> {
    let target = SemRefNameV1::parse(ref_name)?;
    let ref_name = target.as_ref_name();
    if pointer.ref_name != ref_name {
        return Err(anyhow!(
            "semantic ref pointer mismatch: path ref={} pointer ref={}",
            ref_name,
            pointer.ref_name
        ));
    }
    let path = sem_ref_pointer_path(accepted_dir, &ref_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&pointer)?;
    fs::write(&path, json)?;
    Ok(path)
}

#[allow(dead_code)]
pub fn read_sem_ref_pointer(accepted_dir: &Path, ref_name: &str) -> Result<SemRefPointerV1> {
    let target = SemRefNameV1::parse(ref_name)?;
    let ref_name = target.as_ref_name();
    let path = sem_ref_pointer_path(accepted_dir, &ref_name);
    let text = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read semantic ref `{}`: {e}", path.display()))?;
    let pointer: SemRefPointerV1 = serde_json::from_str(&text)?;
    if pointer.ref_name != ref_name {
        return Err(anyhow!(
            "malformed semantic ref pointer `{}`: expected={} got={}",
            path.display(),
            ref_name,
            pointer.ref_name
        ));
    }
    Ok(pointer)
}

pub fn persist_semantic_ref(
    accepted_dir: &Path,
    ref_name: &str,
    commit_id: &AxiDigest,
) -> Result<SemRefPointerV1> {
    let target = SemRefNameV1::parse(ref_name)?;
    persist_semantic_ref_target(accepted_dir, &target, commit_id)
}

pub fn persist_semantic_ref_target(
    accepted_dir: &Path,
    target: &SemRefNameV1,
    commit_id: &AxiDigest,
) -> Result<SemRefPointerV1> {
    ensure_layout(accepted_dir)?;
    let commit = read_semantic_commit(accepted_dir, commit_id)?;
    validate_semantic_ref_update(accepted_dir, target, &commit)?;
    let ref_name = target.as_ref_name();
    let pointer = SemRefPointerV1 {
        version: ACCEPTED_PLANE_SEM_REF_POINTER_VERSION_V1.to_string(),
        ref_name: ref_name.clone(),
        commit_id: commit_id.clone(),
        updated_at_unix_secs: now_unix_secs(),
        gate_summary: sem_gate_summary_for_commit(accepted_dir, commit_id),
    };
    write_sem_ref_pointer(accepted_dir, &ref_name, pointer.clone())?;
    if matches!(target, SemRefNameV1::Main) {
        write_sem_head_symbolic_ref_name(accepted_dir, &ref_name)?;
    }
    Ok(pointer)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn read_sem_ref_pointer_target(
    accepted_dir: &Path,
    target: &SemRefNameV1,
) -> Result<SemRefPointerV1> {
    read_sem_ref_pointer(accepted_dir, &target.as_ref_name())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn persist_semantic_branch_ref(
    accepted_dir: &Path,
    branch: &SemRefNameV1,
    commit_id: &AxiDigest,
) -> Result<SemRefPointerV1> {
    if !branch.is_branch() {
        return Err(anyhow!(
            "semantic branch helper requires a branch ref, got `{}`",
            branch.as_ref_name()
        ));
    }
    persist_semantic_ref_target(accepted_dir, branch, commit_id)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn persist_semantic_tag_ref(
    accepted_dir: &Path,
    tag: &SemRefNameV1,
    commit_id: &AxiDigest,
) -> Result<SemRefPointerV1> {
    if !tag.is_tag() {
        return Err(anyhow!(
            "semantic tag helper requires a tag ref, got `{}`",
            tag.as_ref_name()
        ));
    }
    persist_semantic_ref_target(accepted_dir, tag, commit_id)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn checkout_semantic_ref(accepted_dir: &Path, ref_name: &str) -> Result<SemRefViewV1> {
    ensure_layout(accepted_dir)?;
    let target = SemRefNameV1::parse(ref_name)?;
    let normalized_ref = target.as_ref_name();
    let view = read_sem_ref_view(accepted_dir, &normalized_ref)?;
    write_sem_head_symbolic_ref_name(accepted_dir, &normalized_ref)?;
    Ok(view)
}

#[allow(dead_code)]
pub fn detach_semantic_head(accepted_dir: &Path, commit_id: &AxiDigest) -> Result<SemCommitV1> {
    ensure_layout(accepted_dir)?;
    let commit = read_semantic_commit(accepted_dir, commit_id)?;
    write_sem_head_commit_id(accepted_dir, commit_id)?;
    Ok(commit)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemStatusV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sem_head: Option<SemHeadV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sem_head_commit_id: Option<AxiDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_ref: Option<SemRefPointerV1>,
    #[serde(default)]
    pub review_refs: Vec<SemRefPointerV1>,
    #[serde(default)]
    pub evidence_refs: Vec<SemRefPointerV1>,
    #[serde(default)]
    pub world_model_refs: Vec<SemRefPointerV1>,
    #[serde(default)]
    pub tag_refs: Vec<SemRefPointerV1>,
    #[serde(default)]
    pub reconciliation_ids: Vec<AxiDigest>,
    #[serde(default)]
    pub world_model_run_ids: Vec<WorldModelRunId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemRefViewV1 {
    pub pointer: SemRefPointerV1,
    pub commit: SemCommitV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemReconciliationViewV1 {
    pub reconciliation: SemReconciliationV1,
    pub preview: ReconciliationPreviewReportV1,
    pub stored_reconciliation_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemMergeDryRunResultV1 {
    pub source: SemRefViewV1,
    pub target: SemRefViewV1,
    pub reconciliation: SemReconciliationViewV1,
}

pub fn sem_status(accepted_dir: &Path) -> Result<SemStatusV1> {
    ensure_layout(accepted_dir)?;

    let sem_head = read_semantic_head(accepted_dir)?;
    let sem_head_commit_id = match sem_head.as_ref() {
        Some(SemHeadV1::Symbolic {
            commit_id: Some(commit_id),
            ..
        })
        | Some(SemHeadV1::Detached { commit_id }) => Some(commit_id.clone()),
        Some(SemHeadV1::Symbolic {
            commit_id: None, ..
        })
        | None => None,
    };
    let main_ref = read_sem_ref_pointer_main(accepted_dir)?;

    let refs_root = accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR).join("heads");
    let mut review_refs = Vec::new();
    let mut evidence_refs = Vec::new();
    let mut world_model_refs = Vec::new();
    if refs_root.exists() {
        let review_root = refs_root.join("review");
        if review_root.exists() {
            for entry in walkdir::WalkDir::new(&review_root)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let rel = entry
                    .path()
                    .strip_prefix(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))
                    .map_err(|e| anyhow!("failed to relativize semantic ref path: {e}"))?;
                let ref_name = rel.to_string_lossy().replace('\\', "/");
                review_refs.push(read_sem_ref_pointer(accepted_dir, &ref_name)?);
            }
        }
        let evidence_root = refs_root.join("evidence");
        if evidence_root.exists() {
            for entry in walkdir::WalkDir::new(&evidence_root)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let rel = entry
                    .path()
                    .strip_prefix(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))
                    .map_err(|e| anyhow!("failed to relativize semantic ref path: {e}"))?;
                let ref_name = rel.to_string_lossy().replace('\\', "/");
                evidence_refs.push(read_sem_ref_pointer(accepted_dir, &ref_name)?);
            }
        }
        let wm_root = refs_root.join("wm");
        if wm_root.exists() {
            for entry in walkdir::WalkDir::new(&wm_root)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let rel = entry
                    .path()
                    .strip_prefix(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))
                    .map_err(|e| anyhow!("failed to relativize semantic ref path: {e}"))?;
                let ref_name = rel.to_string_lossy().replace('\\', "/");
                world_model_refs.push(read_sem_ref_pointer(accepted_dir, &ref_name)?);
            }
        }
    }
    review_refs.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));
    evidence_refs.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));
    world_model_refs.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));

    let tags_root = accepted_dir.join(ACCEPTED_PLANE_SEM_TAGS_DIR);
    let mut tag_refs = Vec::new();
    if tags_root.exists() {
        for entry in walkdir::WalkDir::new(&tags_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = entry
                .path()
                .strip_prefix(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))
                .map_err(|e| anyhow!("failed to relativize semantic tag path: {e}"))?;
            let ref_name = rel.to_string_lossy().replace('\\', "/");
            tag_refs.push(read_sem_ref_pointer(accepted_dir, &ref_name)?);
        }
    }
    tag_refs.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));

    let reconciliations_root = accepted_dir.join(ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR);
    let mut reconciliation_ids = Vec::new();
    if reconciliations_root.exists() {
        for entry in fs::read_dir(&reconciliations_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let reconciliation: SemReconciliationV1 = serde_json::from_str(&text)?;
            reconciliation_ids.push(reconciliation.reconciliation_id);
        }
    }
    reconciliation_ids.sort();

    let world_model_runs_root = accepted_dir.join(ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR);
    let mut world_model_run_ids = Vec::new();
    if world_model_runs_root.exists() {
        for entry in fs::read_dir(&world_model_runs_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let run: WorldModelRunRecordV1 = serde_json::from_str(&text)?;
            world_model_run_ids.push(run.run_id);
        }
    }
    world_model_run_ids.sort();

    Ok(SemStatusV1 {
        version: "accepted_plane_sem_status_v1".to_string(),
        sem_head,
        sem_head_commit_id,
        main_ref,
        review_refs,
        evidence_refs,
        world_model_refs,
        tag_refs,
        reconciliation_ids,
        world_model_run_ids,
    })
}

fn path_relative_to_accepted_dir(accepted_dir: &Path, path: &Path) -> String {
    path.strip_prefix(accepted_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn read_sem_ref_view(accepted_dir: &Path, ref_name: &str) -> Result<SemRefViewV1> {
    let pointer = read_sem_ref_pointer(accepted_dir, ref_name)?;
    let commit = read_semantic_commit(accepted_dir, &pointer.commit_id)?;
    Ok(SemRefViewV1 { pointer, commit })
}

pub fn persist_semantic_slice_manifest(
    accepted_dir: &Path,
    manifest: &crate::semantic_merge_lattice::SemanticSliceManifestV1,
) -> Result<String> {
    ensure_layout(accepted_dir)?;
    let path = semantic_slice_manifest_path(accepted_dir, &manifest.slice_id);
    fs::write(&path, serde_json::to_string_pretty(manifest)?)?;
    Ok(path_relative_to_accepted_dir(accepted_dir, &path))
}

pub fn read_semantic_slice_manifest(
    accepted_dir: &Path,
    slice_id_or_path: &str,
) -> Result<crate::semantic_merge_lattice::SemanticSliceManifestV1> {
    ensure_layout(accepted_dir)?;
    let input_path = Path::new(slice_id_or_path);
    let accepted_relative_path = accepted_dir.join(input_path);
    let path = if input_path.exists() {
        input_path.to_path_buf()
    } else if accepted_relative_path.exists() {
        accepted_relative_path
    } else {
        semantic_slice_manifest_path(accepted_dir, &AxiDigest::new(slice_id_or_path))
    };
    let text = fs::read_to_string(&path).map_err(|err| {
        anyhow!(
            "failed to read semantic slice manifest `{}`: {err}",
            path.display()
        )
    })?;
    let manifest: crate::semantic_merge_lattice::SemanticSliceManifestV1 =
        serde_json::from_str(&text).map_err(|err| {
            anyhow!(
                "failed to parse semantic slice manifest `{}`: {err}",
                path.display()
            )
        })?;
    if manifest.version != crate::semantic_merge_lattice::SEMANTIC_SLICE_MANIFEST_VERSION_V1 {
        return Err(anyhow!(
            "unsupported semantic slice manifest version `{}` in `{}`",
            manifest.version,
            path.display()
        ));
    }
    Ok(manifest)
}

fn semantic_slice_manifest_path(accepted_dir: &Path, slice_id: &AxiDigest) -> PathBuf {
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_SLICES_DIR)
        .join(format!("{}.json", digest_to_filename(slice_id)))
}

pub fn read_sem_reconciliation_view(
    accepted_dir: &Path,
    reconciliation_id: &AxiDigest,
) -> Result<SemReconciliationViewV1> {
    let reconciliation = read_reconciliation(accepted_dir, reconciliation_id)?;
    let preview = preview_reconciliation_object(accepted_dir, &reconciliation)?;
    Ok(SemReconciliationViewV1 {
        reconciliation,
        preview,
        stored_reconciliation_path: path_relative_to_accepted_dir(
            accepted_dir,
            &sem_reconciliation_path(accepted_dir, reconciliation_id),
        ),
    })
}

fn semantic_commit_ancestor_distances(
    accepted_dir: &Path,
    start_commit_id: &AxiDigest,
) -> Result<BTreeMap<String, usize>> {
    let mut distances = BTreeMap::new();
    let mut next_commit_id = Some(start_commit_id.clone());
    let mut distance = 0usize;
    while let Some(commit_id) = next_commit_id {
        if distances.insert(commit_id.to_string(), distance).is_some() {
            break;
        }
        let commit = read_semantic_commit(accepted_dir, &commit_id)?;
        next_commit_id = commit.parent_commit_id;
        distance += 1;
    }
    Ok(distances)
}

fn sem_merge_base_commit_id(
    accepted_dir: &Path,
    source_ref_name: &str,
    source_commit_id: &AxiDigest,
    target_ref_name: &str,
    target_commit_id: &AxiDigest,
) -> Result<AxiDigest> {
    let source_ancestors = semantic_commit_ancestor_distances(accepted_dir, source_commit_id)?;
    let mut next_commit_id = Some(target_commit_id.clone());
    while let Some(commit_id) = next_commit_id {
        if source_ancestors.contains_key(commit_id.as_str()) {
            return Ok(commit_id);
        }
        let commit = read_semantic_commit(accepted_dir, &commit_id)?;
        next_commit_id = commit.parent_commit_id;
    }

    Err(anyhow!(
        "semantic merge dry-run requires `{}` and `{}` to share a common persisted semantic ancestor",
        source_ref_name,
        target_ref_name
    ))
}

#[allow(dead_code)]
fn read_sem_ref_pointer_main(accepted_dir: &Path) -> Result<Option<SemRefPointerV1>> {
    let path = sem_main_ref_path(accepted_dir);
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        return Err(anyhow!("semantic ref `{}` is not a file", path.display()));
    }
    let text = fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let pointer: SemRefPointerV1 = serde_json::from_str(&text)?;
    Ok(Some(pointer))
}

fn sem_delta_is_empty(delta: &SemDeltaV1) -> bool {
    delta.module_digests_added.is_empty()
        && delta.module_digests_removed.is_empty()
        && delta.gate_summary.is_none()
        && delta.semantic_delta.is_none()
        && delta.trust_summary.is_none()
        && delta.rule_summary.is_none()
        && delta.coverage_summary.is_none()
        && delta.runtime_theory_check.is_none()
        && delta.evidence_blobs_added.is_empty()
        && delta.certificate_refs_added.is_empty()
        && delta.quality_report_refs_added.is_empty()
        && delta.validation_report_refs_added.is_empty()
        && delta.projection_manifest_refs_added.is_empty()
        && delta.lifecycle_events.is_empty()
        && delta.world_model_run_refs.is_empty()
}

fn normalize_semantic_commit(mut commit: SemCommitV1) -> SemCommitV1 {
    if commit.action == "pathdb_commit" {
        commit.kind = if commit.world_model_run_id.is_some() {
            SemCommitKindV1::WorldModelRun
        } else {
            SemCommitKindV1::EvidenceCommit
        };
    } else if commit.action == "backend_projection" {
        commit.kind = SemCommitKindV1::ProjectionMaterialization;
    }

    if commit.provenance.source.is_empty() {
        commit.provenance = SemCommitProvenanceV1 {
            source: if commit.action == "pathdb_commit" {
                if commit.world_model_run_id.is_some() {
                    "world_model".to_string()
                } else {
                    "pathdb_wal".to_string()
                }
            } else if commit.action == "backend_projection" {
                "backend_projection".to_string()
            } else {
                "accepted_plane".to_string()
            },
            command: if commit.action == "pathdb_commit" {
                Some("axiograph db accept pathdb-commit".to_string())
            } else if commit.action == "backend_projection" {
                Some("axiograph sem project-backend".to_string())
            } else {
                Some("axiograph db accept promote".to_string())
            },
            source_commit: None,
            world_model_run_id: commit.world_model_run_id.clone(),
        };
    }

    if commit.state.accepted_snapshot_id_before.is_none() {
        commit.state.accepted_snapshot_id_before = commit.accepted_parent_snapshot_id.clone();
    }
    if commit.state.accepted_snapshot_id_after.is_none() {
        commit.state.accepted_snapshot_id_after = Some(commit.accepted_snapshot_id.clone());
    }
    if commit.state.pathdb_snapshot_id_after.is_none() {
        commit.state.pathdb_snapshot_id_after = commit.pathdb_snapshot_id.clone();
    }
    if commit.state.evidence_digests.is_empty() && !commit.proposal_digests.is_empty() {
        commit.state.evidence_digests = commit.proposal_digests.clone();
    }

    if sem_delta_is_empty(&commit.delta) {
        commit.delta = if commit.action == "pathdb_commit" {
            SemDeltaV1 {
                module_digests_added: Vec::new(),
                module_digests_removed: Vec::new(),
                gate_summary: commit.gate_summary.clone(),
                semantic_delta: None,
                trust_summary: None,
                rule_summary: None,
                coverage_summary: None,
                runtime_theory_check: commit
                    .gate_summary
                    .as_ref()
                    .and_then(|summary| summary.runtime_theory_check.clone()),
                evidence_blobs_added: commit.proposal_digests.clone(),
                certificate_refs_added: Vec::new(),
                quality_report_refs_added: Vec::new(),
                validation_report_refs_added: Vec::new(),
                projection_manifest_refs_added: Vec::new(),
                lifecycle_events: commit
                    .proposal_digests
                    .iter()
                    .map(|digest| SemLifecycleEventV1 {
                        artifact: ArtifactRefV1 {
                            artifact_kind: "proposal_set".to_string(),
                            artifact_id: digest.to_string(),
                            theory_obligation_ref: None,
                            theory_subject_ref: None,
                            theory_subject_refs: Vec::new(),
                        },
                        from: Some(LifecycleStageV1::Proposed),
                        to: LifecycleStageV1::Validated,
                        reason: commit.message.clone(),
                    })
                    .collect(),
                world_model_run_refs: commit.world_model_run_id.clone().into_iter().collect(),
            }
        } else if commit.action == "backend_projection" {
            SemDeltaV1 {
                module_digests_added: Vec::new(),
                module_digests_removed: Vec::new(),
                gate_summary: None,
                semantic_delta: None,
                trust_summary: None,
                rule_summary: None,
                coverage_summary: None,
                runtime_theory_check: None,
                evidence_blobs_added: Vec::new(),
                certificate_refs_added: Vec::new(),
                quality_report_refs_added: Vec::new(),
                validation_report_refs_added: Vec::new(),
                projection_manifest_refs_added: vec![commit.module_digest.clone()],
                lifecycle_events: Vec::new(),
                world_model_run_refs: Vec::new(),
            }
        } else {
            SemDeltaV1 {
                module_digests_added: vec![commit.module_digest.clone()],
                module_digests_removed: Vec::new(),
                gate_summary: commit.gate_summary.clone(),
                semantic_delta: None,
                trust_summary: None,
                rule_summary: None,
                coverage_summary: None,
                runtime_theory_check: commit
                    .gate_summary
                    .as_ref()
                    .and_then(|summary| summary.runtime_theory_check.clone()),
                evidence_blobs_added: Vec::new(),
                certificate_refs_added: commit.constraints_cert_path.clone().into_iter().collect(),
                quality_report_refs_added: commit.quality_report_path.clone().into_iter().collect(),
                validation_report_refs_added: commit
                    .validation_report_path
                    .clone()
                    .into_iter()
                    .collect(),
                projection_manifest_refs_added: Vec::new(),
                lifecycle_events: vec![SemLifecycleEventV1 {
                    artifact: ArtifactRefV1 {
                        artifact_kind: "module".to_string(),
                        artifact_id: commit.module_digest.to_string(),
                        theory_obligation_ref: None,
                        theory_subject_ref: None,
                        theory_subject_refs: Vec::new(),
                    },
                    from: Some(LifecycleStageV1::Reviewed),
                    to: LifecycleStageV1::Accepted,
                    reason: commit.message.clone(),
                }],
                world_model_run_refs: Vec::new(),
            }
        };
    }

    commit
}

fn read_semantic_commit(accepted_dir: &Path, commit_id: &AxiDigest) -> Result<SemCommitV1> {
    let path = sem_commit_path(accepted_dir, commit_id);
    let text = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read semantic commit `{}`: {e}", path.display()))?;
    let commit = normalize_semantic_commit(serde_json::from_str(&text)?);
    if commit.commit_id != *commit_id {
        return Err(anyhow!(
            "semantic commit `{}` has mismatched id: expected={} got={}",
            path.display(),
            commit_id,
            commit.commit_id
        ));
    }
    Ok(commit)
}

pub fn read_semantic_commit_for_cli(
    accepted_dir: &Path,
    commit_id: &AxiDigest,
) -> Result<SemCommitV1> {
    read_semantic_commit(accepted_dir, commit_id)
}

pub fn sem_log(accepted_dir: &Path, limit: usize) -> Result<Vec<SemCommitV1>> {
    ensure_layout(accepted_dir)?;
    let mut out = Vec::new();
    let mut current = read_sem_head_commit_id(accepted_dir)?;
    let mut seen = BTreeSet::new();
    let limit = limit.max(1);

    while let Some(commit_id) = current {
        if !seen.insert(commit_id.clone()) {
            return Err(anyhow!(
                "semantic commit ancestry cycle detected at `{}`",
                commit_id
            ));
        }
        let commit = read_semantic_commit(accepted_dir, &commit_id)?;
        current = commit.parent_commit_id.clone();
        out.push(commit);
        if out.len() >= limit {
            break;
        }
    }

    Ok(out)
}

fn write_semantic_commit(accepted_dir: &Path, commit: &SemCommitV1) -> Result<PathBuf> {
    let commit = normalize_semantic_commit(commit.clone());
    let path = sem_commit_path(accepted_dir, &commit.commit_id);
    if path.exists() {
        let existing = read_semantic_commit(accepted_dir, &commit.commit_id)?;
        if existing != commit {
            return Err(anyhow!(
                "semantic commit id collision `{}`: existing commit differs",
                commit.commit_id
            ));
        }
        return Ok(path);
    }
    let json = serde_json::to_string_pretty(&commit)?;
    fs::write(&path, json)?;
    Ok(path)
}

#[allow(dead_code)]
#[allow(dead_code)]
pub fn persist_reconciliation(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Result<PathBuf> {
    write_reconciliation_record(accepted_dir, reconciliation, false)
}

fn write_reconciliation_record(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    allow_update: bool,
) -> Result<PathBuf> {
    ensure_layout(accepted_dir)?;
    let path = sem_reconciliation_path(accepted_dir, &reconciliation.reconciliation_id);
    if path.exists() {
        let existing = read_reconciliation(accepted_dir, &reconciliation.reconciliation_id)?;
        if existing != *reconciliation && !allow_update {
            return Err(anyhow!(
                "semantic reconciliation id collision `{}`: existing reconciliation differs",
                reconciliation.reconciliation_id
            ));
        }
        if existing == *reconciliation {
            return Ok(path);
        }
    }
    fs::write(&path, serde_json::to_string_pretty(reconciliation)?)?;
    Ok(path)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn persist_reconciliation_semantic_commit(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
    options: &ReconciliationSemanticCommitOptionsV1,
) -> Result<SemCommitV1> {
    ensure_layout(accepted_dir)?;
    verify_reconciliation_target_head(accepted_dir, reconciliation)?;
    persist_reconciliation(accepted_dir, reconciliation)?;

    let preview_report = reconciliation_preview_report(accepted_dir, reconciliation);
    let validation_report_path = if options.persist_validation_report {
        Some(persist_reconciliation_preview_report(
            accepted_dir,
            &preview_report,
        )?)
    } else {
        None
    };
    let mut materialization_blockers =
        evolution_preview_materialization_blockers(&preview_report.evolution_preview);
    materialization_blockers.extend(unresolved_reconciliation_decision_blockers(reconciliation));
    materialization_blockers.sort();
    materialization_blockers.dedup();
    if !materialization_blockers.is_empty() {
        return Err(anyhow!(
            "semantic reconciliation `{}` still has unresolved obligations; inspect `{}` before materializing a merge commit: {}",
            reconciliation.reconciliation_id,
            validation_report_path
                .as_deref()
                .unwrap_or("the in-memory reconciliation preview"),
            materialization_blockers.join("; ")
        ));
    }

    let parent_commit_id = reconciliation_parent_commit_id(accepted_dir, reconciliation);
    let commit = semantic_commit_from_reconciliation(
        accepted_dir,
        reconciliation,
        parent_commit_id,
        &preview_report.evolution_preview,
        validation_report_path,
        options,
    )?;
    write_semantic_commit(accepted_dir, &commit)?;
    write_reconciliation_record(
        accepted_dir,
        &reconciliation_with_outcome_commit(reconciliation, &commit.commit_id),
        true,
    )?;

    if options.update_resolved_ref {
        if let Some(ref_name) = reconciliation.resolved_ref_name.as_ref() {
            persist_semantic_ref(accepted_dir, ref_name, &commit.commit_id)?;
        }
    }
    if options.update_semantic_head {
        if options.update_resolved_ref {
            if let Some(ref_name) = reconciliation.resolved_ref_name.as_ref() {
                write_sem_head_symbolic_ref_name(accepted_dir, ref_name)?;
            } else {
                write_sem_head_commit_id(accepted_dir, &commit.commit_id)?;
            }
        } else {
            write_sem_head_commit_id(accepted_dir, &commit.commit_id)?;
        }
    }

    Ok(commit)
}

#[allow(dead_code)]
#[allow(dead_code)]
pub fn read_reconciliation(
    accepted_dir: &Path,
    reconciliation_id: &AxiDigest,
) -> Result<SemReconciliationV1> {
    let path = sem_reconciliation_path(accepted_dir, reconciliation_id);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read semantic reconciliation `{}`: {e}",
            path.display()
        )
    })?;
    let reconciliation: SemReconciliationV1 = serde_json::from_str(&text)?;
    if reconciliation.reconciliation_id != *reconciliation_id {
        return Err(anyhow!(
            "semantic reconciliation `{}` has mismatched id: expected={} got={}",
            path.display(),
            reconciliation_id,
            reconciliation.reconciliation_id
        ));
    }
    Ok(reconciliation)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn preview_reconciliation(
    accepted_dir: &Path,
    reconciliation_id: &AxiDigest,
) -> Result<ReconciliationPreviewReportV1> {
    let reconciliation = read_reconciliation(accepted_dir, reconciliation_id)?;
    Ok(reconciliation_preview_report(accepted_dir, &reconciliation))
}

pub fn preview_reconciliation_object(
    accepted_dir: &Path,
    reconciliation: &SemReconciliationV1,
) -> Result<ReconciliationPreviewReportV1> {
    ensure_layout(accepted_dir)?;
    Ok(reconciliation_preview_report(accepted_dir, reconciliation))
}

pub fn sem_merge_dry_run(
    accepted_dir: &Path,
    source_ref: &str,
    target_ref: &str,
    policy: &str,
) -> Result<SemMergeDryRunResultV1> {
    ensure_layout(accepted_dir)?;
    let source = read_sem_ref_view(accepted_dir, source_ref)?;
    let target = read_sem_ref_view(accepted_dir, target_ref)?;
    let merge_base_commit_id = sem_merge_base_commit_id(
        accepted_dir,
        &source.pointer.ref_name,
        &source.pointer.commit_id,
        &target.pointer.ref_name,
        &target.pointer.commit_id,
    )?;
    let reconciliation_id = AxiDigest::new(axiograph_dsl::digest::axi_digest_v1(&format!(
        "merge-dry-run|{}|{}|{}",
        source.pointer.commit_id, target.pointer.commit_id, policy
    )));
    let reconciliation_path = sem_reconciliation_path(accepted_dir, &reconciliation_id);
    let reconciliation = if reconciliation_path.exists() {
        let existing = read_reconciliation(accepted_dir, &reconciliation_id)?;
        let refreshed = reconciliation_with_current_refs(
            &existing,
            &source.pointer.ref_name,
            &target.pointer.ref_name,
        );
        if refreshed != existing {
            write_reconciliation_record(accepted_dir, &refreshed, true)?;
        }
        refreshed
    } else {
        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: reconciliation_id.clone(),
            created_at_unix_secs: now_unix_secs(),
            base_commit_id: merge_base_commit_id,
            left_commit_id: source.pointer.commit_id.clone(),
            right_commit_id: target.pointer.commit_id.clone(),
            policy: policy.to_string(),
            source_ref_name: Some(source.pointer.ref_name.clone()),
            target_ref_name: Some(target.pointer.ref_name.clone()),
            resolved_ref_name: Some(target.pointer.ref_name.clone()),
            outcome_commit_id: None,
            conflicts: Vec::new(),
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };
        persist_reconciliation(accepted_dir, &reconciliation)?;
        reconciliation
    };
    let reconciliation =
        read_sem_reconciliation_view(accepted_dir, &reconciliation.reconciliation_id)?;
    Ok(SemMergeDryRunResultV1 {
        source,
        target,
        reconciliation,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_reconciliation_refinement_by_id(
    accepted_dir: &Path,
    reconciliation_id: &AxiDigest,
    handle_id: &str,
) -> Result<ReconciliationReviewApplyResultV1> {
    let reconciliation = read_reconciliation(accepted_dir, reconciliation_id)?;
    let applied = apply_runtime_refinement_by_id_to_reconciliation_with_available_compiled_theory(
        accepted_dir,
        &reconciliation,
        handle_id,
    )?;
    write_reconciliation_record(accepted_dir, &applied.updated_reconciliation, true)?;
    Ok(applied)
}

fn store_module_if_needed(
    accepted_dir: &Path,
    module_name: &str,
    module_digest: &AxiDigest,
    candidate_path: &Path,
    text: &str,
) -> Result<String> {
    let module_dir = accepted_dir
        .join(ACCEPTED_PLANE_MODULES_DIR)
        .join(sanitize_path_component(module_name));
    fs::create_dir_all(&module_dir)?;

    let file_name = format!("{}.axi", digest_to_filename(module_digest));
    let stored_path = module_dir.join(file_name);

    if stored_path.exists() {
        // Ensure it matches the expected digest (basic corruption guard).
        let existing_text = fs::read_to_string(&stored_path)?;
        let existing_digest = AxiDigest::from_axi_text(&existing_text);
        if existing_digest != *module_digest {
            return Err(anyhow!(
                "accepted module path collision: `{}` exists but digest mismatches (expected {module_digest}, got {existing_digest})",
                stored_path.display()
            ));
        }
    } else {
        fs::write(&stored_path, text)?;
    }

    let rel = stored_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&stored_path)
        .to_string_lossy()
        .to_string();

    // Tiny UX guard: keep a backpointer to the candidate path in the log only,
    // but refuse to store a module outside the accepted dir by accident.
    if candidate_path.starts_with(accepted_dir) {
        // ok
    }

    Ok(rel)
}

fn append_event(accepted_dir: &Path, event: &AcceptedPlaneEventV1) -> Result<()> {
    let log_path = accepted_dir.join(ACCEPTED_PLANE_LOG_V1);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            anyhow!(
                "failed to open accepted plane log `{}`: {e}",
                log_path.display()
            )
        })?;

    let line = serde_json::to_string(event)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-accepted-plane-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    fn sample_runtime_semantics() -> crate::semantic_claim::RuntimeSemanticSummaryV1 {
        crate::semantic_claim::RuntimeSemanticSummaryV1 {
            version: crate::semantic_claim::RUNTIME_SEMANTIC_SUMMARY_VERSION_V1.to_string(),
            trust_class: "accepted_promotion_preview".to_string(),
            soundness: "candidate_module_typechecked_constraints_checked_and_previewed".to_string(),
            coverage: "accepted_snapshot_delta_plus_competency_questions".to_string(),
            scope: "accepted_snapshot_scoped_preview".to_string(),
            completeness_claim: "not_claimed".to_string(),
            ontology_closure_claim: "not_claimed".to_string(),
            rule_inventory: crate::semantic_claim::SemanticRuleInventoryV1 {
                total_rules: 4,
                relation_constraints: 3,
                rewrite_rules: 1,
                named_block_constraints: 0,
                runtime_checkable_rules: 3,
                runtime_visible_rules: 4,
                review_only_rules: 0,
                relations_with_rules: 1,
                theories_with_rules: 1,
                relation_names: vec!["Edge".to_string()],
                theory_names: vec!["DemoTheory".to_string()],
                notes: Vec::new(),
            },
            semantic_coverage: crate::semantic_claim::SemanticCoverageSummaryV1 {
                typed_fact_surface: "schema_scoped_fact_typing_present".to_string(),
                structured_constraint_surface: "partial_runtime_enforced_structured_constraints"
                    .to_string(),
                rewrite_surface: "declared_runtime_visible".to_string(),
                named_block_surface: "none_declared".to_string(),
                competency_surface: "cq_gated_preview".to_string(),
                quality_surface: "preview_quality_delta".to_string(),
                gaps: vec![
                    "rewrite rules still need broader runtime/certificate alignment".to_string(),
                ],
            },
            notes: vec!["runtime semantic claims remain soundness-scoped".to_string()],
        }
    }

    fn sample_runtime_theory_summary() -> crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
        crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
            version: "runtime_theory_check_summary_v1".to_string(),
            report_version: "runtime_theory_check_report_v1".to_string(),
            module_digest: "fnv1a64:promotion-module".to_string(),
            theory_count: 1,
            checked_obligations: 1,
            review_only_obligations: 0,
            residual_obligations: 0,
            blocked_obligations: 0,
            excluded_by_evidence: 0,
            blocking_errors: 0,
            closure_tiers: vec!["finite_fragment".to_string()],
            closure_trace: Default::default(),
            transport_summary: Default::default(),
            completeness_claim: "claimed_under_finite_fragment".to_string(),
            ontology_closure_claim: "claimed_under_finite_fragment".to_string(),
            residual_obligation_ids: Vec::new(),
            notes: vec!["test promotion theory summary".to_string()],
        }
    }

    fn sample_evolution_preview() -> crate::evolution_preview::EvolutionPreviewV1 {
        let mut preview = crate::evolution_preview::build_evolution_preview_v1(
            "accepted_promotion_preview",
            Some(AcceptedSnapshotId::new("fnv1a64:promotion-parent")),
            "PromotionDemo".to_string(),
            crate::evolution_preview::TypedChangeSummaryV1 {
                kind: "accepted_module_delta".to_string(),
                subjects: vec!["PromotionDemo".to_string()],
                primitives: Vec::new(),
                counts: BTreeMap::from([("relations_added".to_string(), 1usize)]),
                notes: Vec::new(),
                schema: crate::evolution_preview::TypedChangeBucketV1 {
                    added: 1,
                    ..crate::evolution_preview::TypedChangeBucketV1::default()
                },
                theory: crate::evolution_preview::TypedChangeBucketV1::default(),
                instance: crate::evolution_preview::TypedChangeBucketV1 {
                    added: 2,
                    ..crate::evolution_preview::TypedChangeBucketV1::default()
                },
                context: crate::evolution_preview::TypedChangeBucketV1::default(),
            },
            &crate::quality::QualityReportV1 {
                version: "quality_report_v1".to_string(),
                generated_at_unix_secs: 1,
                input: "PromotionDemo".to_string(),
                profile: "strict".to_string(),
                plane: "both".to_string(),
                summary: crate::quality::QualitySummaryV1 {
                    error_count: 0,
                    warning_count: 1,
                    info_count: 0,
                },
                findings: Vec::new(),
            },
            Some(&crate::proposals_validate::CompetencyGateReportV1 {
                total: 2,
                satisfied_before: 1,
                satisfied_after: 2,
                coverage_before: 0.5,
                coverage_after: 1.0,
                cost_before: 1.0,
                cost_after: 0.0,
                regressions: 0,
                improvements: 1,
                gate_passed: true,
                policy: crate::proposals_validate::CompetencyGatePolicyV1 {
                    fail_on_regression: true,
                    fail_on_unsatisfied_after: true,
                },
                questions: Vec::new(),
            }),
            &crate::proposals_validate::ProposalValidationTrustContractV1 {
                trust_class: "accepted_promotion_preview".to_string(),
                soundness: "candidate_module_typechecked_constraints_checked_and_previewed"
                    .to_string(),
                coverage: "accepted_snapshot_delta_plus_competency_questions".to_string(),
                scope: "accepted_snapshot_scoped_preview".to_string(),
                reasons: vec!["preview is scoped to the accepted snapshot delta".to_string()],
            },
            Some(sample_runtime_semantics()),
            std::iter::empty::<String>(),
            true,
        );
        preview.runtime_theory_check = Some(sample_runtime_theory_summary());
        preview
    }

    fn seed_semantic_commit(
        accepted_dir: &Path,
        snapshot_id: &str,
        previous_snapshot_id: Option<&str>,
        module_name: &str,
        module_digest: &str,
        message: &str,
    ) -> SemCommitV1 {
        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 11,
            action: "promote".to_string(),
            snapshot_id: AcceptedSnapshotId::new(snapshot_id),
            previous_snapshot_id: previous_snapshot_id.map(AcceptedSnapshotId::new),
            module_name: module_name.to_string(),
            module_digest: AxiDigest::new(module_digest),
            stored_module_path: format!(
                "modules/{module_name}/{}.axi",
                module_digest.replace(':', "_")
            ),
            message: Some(message.to_string()),
            quality_profile: None,
            quality_report_path: None,
            quality_error_count: None,
            quality_warning_count: None,
            quality_info_count: None,
            constraints_cert_path: None,
            constraints_constraint_count: None,
            constraints_instance_count: None,
            constraints_check_count: None,
            validation_report_path: None,
            validation_ok: Some(true),
        };
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: event.snapshot_id.clone(),
            previous_snapshot_id: event.previous_snapshot_id.clone(),
            created_at_unix_secs: 12,
            modules: BTreeMap::new(),
        };
        let parent_commit_id = event
            .previous_snapshot_id
            .as_ref()
            .map(|_| read_sem_head_commit_id(accepted_dir).expect("read head"))
            .flatten();
        let commit = semantic_commit_from_promotion(&event, parent_commit_id, &snapshot, None)
            .expect("seed promotion commit");
        write_semantic_commit(accepted_dir, &commit).expect("write seed commit");
        write_sem_head_commit_id(accepted_dir, &commit.commit_id).expect("write seed head");
        commit
    }

    fn sample_world_model_run_record(
        run_id: WorldModelRunId,
        accepted_snapshot_id: AcceptedSnapshotId,
    ) -> WorldModelRunRecordV1 {
        WorldModelRunRecordV1 {
            version: WORLD_MODEL_RUN_RECORD_VERSION_V1.to_string(),
            trace_id: run_id.clone(),
            run_id,
            created_at_unix_secs: 1_700_000_111,
            status: WorldModelRunStatusV1::CommittedToPathdb,
            backend: "test".to_string(),
            model: Some("test-world-model".to_string()),
            axi_digest_v1: None,
            input_pathdb_snapshot_id: None,
            input_accepted_snapshot_id: Some(accepted_snapshot_id),
            proposals_digest: ProposalDigest::new("fnv1a64:wm-proposals"),
            proposal_count: 1,
            committed_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:wm-pathdb")),
            committed_accepted_snapshot_id: None,
            guardrail_total_cost: None,
            guardrail_profile: None,
            guardrail_plane: None,
            notes: Vec::new(),
        }
    }

    fn seed_pathdb_semantic_commit(
        accepted_dir: &Path,
        snapshot_id: &str,
        parent_commit_id: Option<AxiDigest>,
        run_id: Option<WorldModelRunId>,
    ) -> SemCommitV1 {
        let accepted_snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: AcceptedSnapshotId::new(snapshot_id),
            previous_snapshot_id: None,
            created_at_unix_secs: 12,
            modules: BTreeMap::new(),
        };
        let options = PathdbSemanticCommitOptionsV1 {
            message: Some("pathdb semantic commit".to_string()),
            proposal_digests: vec![ProposalDigest::new("fnv1a64:proposal-ref")],
            world_model_run_id: run_id,
            ..PathdbSemanticCommitOptionsV1::default()
        };
        let commit = semantic_commit_from_pathdb_overlay(
            &accepted_snapshot,
            &PathdbSnapshotId::new("fnv1a64:pathdb-ref"),
            parent_commit_id,
            &options,
        );
        write_semantic_commit(accepted_dir, &commit).expect("write pathdb semantic commit");
        commit
    }

    fn seed_semantic_commit_with_module_text(
        accepted_dir: &Path,
        snapshot_id: &str,
        previous_snapshot_id: Option<&str>,
        module_name: &str,
        module_text: &str,
        message: &str,
    ) -> SemCommitV1 {
        let module_digest = AxiDigest::from_axi_text(module_text);
        let candidate_path = accepted_dir.join(format!(
            "{}-{}.axi",
            sanitize_path_component(module_name),
            digest_to_filename(module_digest.as_str())
        ));
        fs::write(&candidate_path, module_text).expect("write seed module");
        let stored_module_path = store_module_if_needed(
            accepted_dir,
            module_name,
            &module_digest,
            &candidate_path,
            module_text,
        )
        .expect("store seed module");

        let previous_snapshot_id = previous_snapshot_id.map(AcceptedSnapshotId::new);
        let mut modules = previous_snapshot_id
            .as_ref()
            .map(|snapshot_id| {
                read_snapshot(accepted_dir, snapshot_id)
                    .expect("read previous seed snapshot")
                    .modules
            })
            .unwrap_or_default();
        modules.insert(
            module_name.to_string(),
            AcceptedModuleRefV1 {
                module_digest: module_digest.clone(),
                stored_path: stored_module_path.clone(),
            },
        );

        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: AcceptedSnapshotId::new(snapshot_id),
            previous_snapshot_id: previous_snapshot_id.clone(),
            created_at_unix_secs: 12,
            modules,
        };
        write_snapshot(accepted_dir, &snapshot).expect("write seed snapshot");
        write_head(accepted_dir, &snapshot.snapshot_id).expect("write accepted head");

        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 11,
            action: "promote".to_string(),
            snapshot_id: snapshot.snapshot_id.clone(),
            previous_snapshot_id,
            module_name: module_name.to_string(),
            module_digest,
            stored_module_path,
            message: Some(message.to_string()),
            quality_profile: None,
            quality_report_path: None,
            quality_error_count: None,
            quality_warning_count: None,
            quality_info_count: None,
            constraints_cert_path: None,
            constraints_constraint_count: None,
            constraints_instance_count: None,
            constraints_check_count: None,
            validation_report_path: None,
            validation_ok: Some(true),
        };
        let parent_commit_id = event
            .previous_snapshot_id
            .as_ref()
            .map(|_| read_sem_head_commit_id(accepted_dir).expect("read head"))
            .flatten();
        let commit = semantic_commit_from_promotion(&event, parent_commit_id, &snapshot, None)
            .expect("seed promotion commit");
        write_semantic_commit(accepted_dir, &commit).expect("write seed commit");
        write_sem_head_commit_id(accepted_dir, &commit.commit_id).expect("write seed head");
        commit
    }

    fn sample_projection_manifest(
        accepted_snapshot_id: AcceptedSnapshotId,
        source_sem_commit_id: Option<AxiDigest>,
    ) -> ProjectionManifestV1 {
        ProjectionManifestV1 {
            version: String::new(),
            projection_id: AxiDigest::new("fnv1a64:placeholder"),
            created_at_unix_secs: 0,
            accepted_snapshot_id,
            source_sem_ref_name: Some("heads/review/backend-projection".to_string()),
            source_sem_commit_id,
            compiled_ir_digest: AxiDigest::new("fnv1a64:compiled-ir"),
            materialization_ref: "typedb://axiograph/ontology_main".to_string(),
            backend: BackendCapabilityProfileV1::typedb_primary(),
            projection: ProjectionCapabilityProfileV1 {
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
            },
            object_mappings: vec![
                ProjectionObjectMappingV1 {
                    object_name: "Node".to_string(),
                    backend_label: "node".to_string(),
                },
                ProjectionObjectMappingV1 {
                    object_name: "EdgeFact".to_string(),
                    backend_label: "edge_fact".to_string(),
                },
            ],
            relation_mappings: vec![ProjectionRelationMappingV1 {
                relation_name: "Edge".to_string(),
                tuple_encoding: ProjectionRelationTupleEncodingV1::RelationNode,
                tuple_label: "edge_fact".to_string(),
                carrier_edge: Some(ProjectionCarrierEdgeMappingV1 {
                    edge_label: "edge_carrier".to_string(),
                    source_role: "from".to_string(),
                    target_role: "to".to_string(),
                }),
                role_mappings: vec![
                    ProjectionRoleMappingV1 {
                        role_name: "from".to_string(),
                        backend_slot: "role:from".to_string(),
                        preserved_explicitly: true,
                    },
                    ProjectionRoleMappingV1 {
                        role_name: "to".to_string(),
                        backend_slot: "role:to".to_string(),
                        preserved_explicitly: true,
                    },
                ],
            }],
            context_mapping: ProjectionContextMappingV1 {
                strategy: ProjectionContextMappingStrategyV1::TupleProperties,
                axis_bindings: vec![ProjectionContextAxisBindingV1 {
                    axis_name: "world".to_string(),
                    backend_slot: "property:world".to_string(),
                }],
            },
            trust_caveats: vec![
                "query pushdown remains soundness-scoped and does not imply completeness"
                    .to_string(),
            ],
            round_trip_limitations: vec![
                "backend labels are adapter-level choices and do not define ontology identity"
                    .to_string(),
            ],
        }
    }

    #[test]
    fn snapshot_manifest_round_trips_typed_module_digest() {
        let accepted_dir = temp_test_dir("snapshot-roundtrip");
        ensure_layout(&accepted_dir).expect("layout");
        let snapshot_id = AcceptedSnapshotId::new("fnv1a64:accepted00000001");
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: snapshot_id.clone(),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted00000000")),
            created_at_unix_secs: 17,
            modules: BTreeMap::from([(
                "Demo".to_string(),
                AcceptedModuleRefV1 {
                    module_digest: AxiDigest::new("fnv1a64:0123456789abcdef"),
                    stored_path: "modules/Demo/fnv1a64_0123456789abcdef.axi".to_string(),
                },
            )]),
        };

        write_snapshot(&accepted_dir, &snapshot).expect("write snapshot");
        write_head(&accepted_dir, &snapshot_id).expect("write head");

        let round_trip =
            read_snapshot_for_cli(&accepted_dir, "head").expect("read snapshot through cli seam");
        assert_eq!(
            round_trip.modules["Demo"].module_digest,
            AxiDigest::new("fnv1a64:0123456789abcdef")
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn accepted_plane_event_json_round_trips_typed_digest() {
        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 99,
            action: "promote".to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:snapshot"),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:prev")),
            module_name: "Demo".to_string(),
            module_digest: AxiDigest::new("fnv1a64:feedfacecafebeef"),
            stored_module_path: "modules/Demo/demo.axi".to_string(),
            message: Some("test".to_string()),
            quality_profile: Some("strict".to_string()),
            quality_report_path: Some("quality/report.json".to_string()),
            quality_error_count: Some(0),
            quality_warning_count: Some(1),
            quality_info_count: Some(2),
            constraints_cert_path: Some("certs/demo.json".to_string()),
            constraints_constraint_count: Some(3),
            constraints_instance_count: Some(4),
            constraints_check_count: Some(5),
            validation_report_path: Some("sem/validations/report.json".to_string()),
            validation_ok: Some(true),
        };

        let json = serde_json::to_string(&event).expect("event should serialize");
        let round_trip: AcceptedPlaneEventV1 =
            serde_json::from_str(&json).expect("event should deserialize");

        assert_eq!(
            round_trip.module_digest,
            AxiDigest::new("fnv1a64:feedfacecafebeef")
        );
    }

    #[test]
    fn promote_reviewed_module_rejects_unknown_constraints_even_after_validation() {
        let accepted_dir = temp_test_dir("reject-unknown-constraints");
        ensure_layout(&accepted_dir).expect("layout");

        let axi_path = accepted_dir.join("UnknownConstraint.axi");
        fs::write(
            &axi_path,
            r#"module UnknownConstraint

schema S:
  object A
  relation R(from: A, to: A)

constraints S
  R
    unsupported(foo)

instance I of S:
  A = {x}
  R = {(from=x, to=x)}
"#,
        )
        .expect("write candidate module");

        let err = promote_reviewed_module(&axi_path, &accepted_dir, Some("test"), "off")
            .expect_err("promotion should reject unknown constraints");
        let msg = err.to_string();
        assert!(
            !msg.trim().is_empty(),
            "expected a meaningful rejection message"
        );
        assert!(
            read_head(&accepted_dir)
                .expect("read head after failed promotion")
                .is_none(),
            "rejected promotion must not advance accepted HEAD"
        );
        let snapshots_dir = accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR);
        let snapshot_count = fs::read_dir(&snapshots_dir)
            .expect("read snapshots dir")
            .filter(|entry| {
                entry
                    .as_ref()
                    .ok()
                    .and_then(|e| e.path().extension().map(|ext| ext == "json"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            snapshot_count, 0,
            "rejected promotion must not write snapshots"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn build_pathdb_from_snapshot_rejects_stored_pathdb_export_modules() {
        let accepted_dir = temp_test_dir("reject-stored-pathdb-export");
        ensure_layout(&accepted_dir).expect("layout");

        let canonical = r#"module Demo

schema S:
  object A
  relation R(from: A, to: A)

instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;

        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, canonical)
            .expect("import canonical module");
        db.build_indexes();
        let snapshot_export = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)
            .expect("export pathdb snapshot");
        let digest = AxiDigest::from_axi_text(&snapshot_export);

        let stored_rel = format!("modules/Forged/{}.axi", digest_to_filename(digest.as_str()));
        let stored_abs = accepted_dir.join(&stored_rel);
        fs::create_dir_all(stored_abs.parent().expect("stored parent")).expect("mkdir");
        fs::write(&stored_abs, &snapshot_export).expect("write forged stored module");

        let snapshot_id = AcceptedSnapshotId::new("fnv1a64:forgedsnapshot");
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: snapshot_id.clone(),
            previous_snapshot_id: None,
            created_at_unix_secs: 1,
            modules: BTreeMap::from([(
                "Forged".to_string(),
                AcceptedModuleRefV1 {
                    module_digest: digest,
                    stored_path: stored_rel,
                },
            )]),
        };
        write_snapshot(&accepted_dir, &snapshot).expect("write forged snapshot manifest");
        write_head(&accepted_dir, &snapshot_id).expect("write head");

        let out_axpd = accepted_dir.join("out.axpd");
        let err = build_pathdb_from_snapshot(&accepted_dir, "head", &out_axpd)
            .expect_err("stored PathDBExportV1 module must be rejected");
        assert!(err
            .to_string()
            .contains("expected a canonical .axi module, but input is a PathDBExportV1 snapshot"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn single_module_anchor_reader_rejects_paths_outside_accepted_dir() {
        let accepted_dir = temp_test_dir("anchor-path-escape");
        ensure_layout(&accepted_dir).expect("layout");

        let snapshot_id = AcceptedSnapshotId::new("accepted:path-escape");
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: snapshot_id.clone(),
            previous_snapshot_id: None,
            created_at_unix_secs: 1,
            modules: BTreeMap::from([(
                "Escape".to_string(),
                AcceptedModuleRefV1 {
                    module_digest: AxiDigest::new("fnv1a64:deadbeef"),
                    stored_path: "../escape.axi".to_string(),
                },
            )]),
        };
        write_snapshot(&accepted_dir, &snapshot).expect("write snapshot manifest");

        let err = read_single_module_accepted_axi_anchor_and_text(&accepted_dir, &snapshot_id)
            .expect_err("stored path escape must be rejected");
        assert!(err
            .to_string()
            .contains("stored module path must not escape accepted dir"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn world_model_run_record_round_trips_typed_anchors() {
        let accepted_dir = temp_test_dir("world-model-run-record");
        ensure_layout(&accepted_dir).expect("layout");

        let record = WorldModelRunRecordV1 {
            version: WORLD_MODEL_RUN_RECORD_VERSION_V1.to_string(),
            run_id: WorldModelRunId::new("wm::run"),
            trace_id: WorldModelRunId::new("wm::trace"),
            created_at_unix_secs: 123,
            status: WorldModelRunStatusV1::CommittedToPathdb,
            backend: "plugin".to_string(),
            model: Some("deterministic".to_string()),
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:axi")),
            input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-in")),
            input_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-in")),
            proposals_digest: ProposalDigest::new("fnv1a64:proposals"),
            proposal_count: 2,
            committed_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-out")),
            committed_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-out")),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
            notes: vec!["typed".to_string(), "anchored".to_string()],
        };

        let path = persist_world_model_run_record(&accepted_dir, &record)
            .expect("persist world-model run");
        assert!(
            path.ends_with("sem/world_model_runs/wm__run.json"),
            "unexpected persisted path: {}",
            path.display()
        );

        let round_trip =
            read_world_model_run_record(&accepted_dir, &record.run_id).expect("read run record");
        assert_eq!(round_trip, record);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn world_model_run_record_rejects_run_id_collision_with_different_typed_lineage() {
        let accepted_dir = temp_test_dir("world-model-run-record-collision");
        ensure_layout(&accepted_dir).expect("layout");

        let record = WorldModelRunRecordV1 {
            version: WORLD_MODEL_RUN_RECORD_VERSION_V1.to_string(),
            run_id: WorldModelRunId::new("wm::shared"),
            trace_id: WorldModelRunId::new("wm::trace"),
            created_at_unix_secs: 123,
            status: WorldModelRunStatusV1::Previewed,
            backend: "plugin".to_string(),
            model: None,
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:axi")),
            input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-a")),
            input_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-a")),
            proposals_digest: ProposalDigest::new("fnv1a64:proposals-a"),
            proposal_count: 1,
            committed_pathdb_snapshot_id: None,
            committed_accepted_snapshot_id: None,
            guardrail_total_cost: None,
            guardrail_profile: None,
            guardrail_plane: None,
            notes: vec!["preview".to_string()],
        };
        persist_world_model_run_record(&accepted_dir, &record).expect("persist first record");

        let err = persist_world_model_run_record(
            &accepted_dir,
            &WorldModelRunRecordV1 {
                proposals_digest: ProposalDigest::new("fnv1a64:proposals-b"),
                input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-b")),
                ..record
            },
        )
        .expect_err("mismatched run id must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("world-model run id collision"));
        assert!(msg.contains("wm::shared"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn promote_with_options_blocks_competency_regressions() {
        let accepted_dir = temp_test_dir("promotion-cq-regression");
        ensure_layout(&accepted_dir).expect("layout");

        let baseline_axi = accepted_dir.join("PromoBaseline.axi");
        fs::write(
            &baseline_axi,
            r#"module Promo

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

theory FamRules on Fam:
  constraint functional Parent.child -> Parent.parent

instance I of Fam:
  Person = {Alice, Bob, Carol}
  Parent = {(child=Carol, parent=Bob)}
"#,
        )
        .expect("write baseline");
        let baseline_snapshot =
            promote_reviewed_module(&baseline_axi, &accepted_dir, Some("baseline"), "off")
                .expect("promote baseline");

        let candidate_axi = accepted_dir.join("PromoCandidate.axi");
        fs::write(
            &candidate_axi,
            r#"module Promo

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of Fam:
  Person = {Alice, Bob, Carol}
"#,
        )
        .expect("write candidate");

        let err = promote_reviewed_module_with_options(
            &candidate_axi,
            &accepted_dir,
            &PromoteReviewedModuleOptionsV1 {
                message: Some("candidate".to_string()),
                quality_profile: "off".to_string(),
                quality_plane: "both".to_string(),
                competency_questions: vec![crate::world_model::CompetencyQuestionV1 {
                    name: "carol_parent".to_string(),
                    question: Some("Carol should still have Bob as a parent".to_string()),
                    authoring: None,
                    query: "select ?f where ?f = Fam.Parent(child=Carol, parent=Bob) limit 1"
                        .to_string(),
                    min_rows: 1,
                    weight: 1.0,
                    contexts: Vec::new(),
                }],
                competency_gate: crate::proposals_validate::CompetencyGatePolicyV1 {
                    fail_on_regression: true,
                    fail_on_unsatisfied_after: false,
                },
                persist_validation_report: true,
            },
        )
        .expect_err("promotion should fail on CQ regression");
        assert!(
            err.to_string().contains("competency gate failed"),
            "unexpected error: {err}"
        );
        assert_eq!(
            read_head(&accepted_dir)
                .expect("read head")
                .expect("head exists after failed promotion"),
            baseline_snapshot,
            "failed promotion must not advance accepted HEAD"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn promote_with_options_persists_validation_report() {
        let accepted_dir = temp_test_dir("promotion-preview-report");
        ensure_layout(&accepted_dir).expect("layout");

        let baseline_axi = accepted_dir.join("PromoBaselineEmpty.axi");
        fs::write(
            &baseline_axi,
            r#"module Promo

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of Fam:
  Person = {Alice, Bob, Carol}
"#,
        )
        .expect("write baseline");
        promote_reviewed_module(&baseline_axi, &accepted_dir, Some("baseline"), "off")
            .expect("promote baseline");

        let candidate_axi = accepted_dir.join("PromoCandidateFilled.axi");
        fs::write(
            &candidate_axi,
            r#"module Promo

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

theory FamRules on Fam:
  constraint functional Parent.child -> Parent.parent

instance I of Fam:
  Person = {Alice, Bob, Carol}
  Parent = {(child=Carol, parent=Bob)}
"#,
        )
        .expect("write candidate");

        let result = promote_reviewed_module_with_options(
            &candidate_axi,
            &accepted_dir,
            &PromoteReviewedModuleOptionsV1 {
                message: Some("candidate".to_string()),
                quality_profile: "off".to_string(),
                quality_plane: "both".to_string(),
                competency_questions: vec![crate::world_model::CompetencyQuestionV1 {
                    name: "carol_parent".to_string(),
                    question: Some("Carol should gain Bob as a parent".to_string()),
                    authoring: None,
                    query: "select ?f where ?f = Fam.Parent(child=Carol, parent=Bob) limit 1"
                        .to_string(),
                    min_rows: 1,
                    weight: 1.0,
                    contexts: Vec::new(),
                }],
                competency_gate: crate::proposals_validate::CompetencyGatePolicyV1 {
                    fail_on_regression: true,
                    fail_on_unsatisfied_after: true,
                },
                persist_validation_report: true,
            },
        )
        .expect("promotion should pass");

        let rel_path = result
            .validation_report_path
            .clone()
            .expect("validation report should be persisted");
        let abs_path = accepted_dir.join(&rel_path);
        assert!(
            abs_path.exists(),
            "expected persisted report at {}",
            abs_path.display()
        );

        let report_text = fs::read_to_string(&abs_path).expect("read validation report");
        let report: PromotionPreviewReportV1 =
            serde_json::from_str(&report_text).expect("parse validation report");
        assert!(report.ok);
        assert_eq!(
            report.stored_report_path.as_deref(),
            Some(rel_path.as_str())
        );
        let gate = report
            .competency_gate
            .as_ref()
            .expect("competency gate report");
        assert_eq!(gate.total, 1);
        assert_eq!(gate.improvements, 1);
        assert_eq!(gate.regressions, 0);
        let evolution = report
            .evolution_preview
            .as_ref()
            .expect("shared evolution preview");
        assert_eq!(evolution.kind, "accepted_promotion_preview");
        assert_eq!(evolution.typed_change.kind, "accepted_module_delta");
        assert!(evolution.runtime_theory_check.is_some());
        assert_eq!(
            evolution
                .runtime_theory_check
                .as_ref()
                .expect("runtime theory summary")
                .completeness_claim,
            "claimed_under_finite_fragment"
        );
        assert!(evolution.typed_change.schema.added > 0);
        assert!(evolution.typed_change.instance.added > 0);
        assert_eq!(
            evolution
                .runtime_semantics
                .as_ref()
                .expect("runtime semantic summary")
                .completeness_claim,
            "not_claimed"
        );
        assert_eq!(evolution.trust_delta.competency_coverage_before, Some(0.0));
        assert_eq!(evolution.trust_delta.competency_coverage_after, Some(1.0));
        assert_eq!(evolution.trust_delta.improvements, 1);
        assert_eq!(evolution.trust_delta.regressions, 0);
        assert_eq!(evolution.trust_delta.changed_questions, 0);
        assert_eq!(evolution.trust_delta.questions.len(), 1);
        assert!(
            evolution.residual_obligations.is_empty(),
            "successful promotion preview should not leave residual obligations: {:?}",
            evolution.residual_obligations
        );
        let sem_head = read_sem_head_commit_id(&accepted_dir)
            .expect("read sem head")
            .expect("promotion should write semantic head");
        let commit = read_semantic_commit(&accepted_dir, &sem_head).expect("read semantic commit");
        let gate_summary = commit.gate_summary.as_ref().expect("commit gate summary");
        assert_eq!(gate_summary.kind, "accepted_promotion_preview");
        assert_eq!(gate_summary.candidate_label, "Promo");
        assert!(gate_summary.runtime_theory_check.is_some());
        assert_eq!(
            gate_summary
                .runtime_semantics
                .as_ref()
                .expect("runtime semantics")
                .completeness_claim,
            "not_claimed"
        );
        assert!(
            gate_summary.rule.is_some(),
            "promotion gate summary should carry persisted rule-count metadata"
        );
        let main_pointer = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer")
            .expect("main pointer exists");
        assert_eq!(main_pointer.gate_summary, commit.gate_summary);

        let log_text = fs::read_to_string(accepted_dir.join(ACCEPTED_PLANE_LOG_V1))
            .expect("read accepted plane log");
        let last_line = log_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .last()
            .expect("last event line");
        let event: AcceptedPlaneEventV1 =
            serde_json::from_str(last_line).expect("parse last event");
        assert_eq!(event.snapshot_id, result.snapshot_id);
        assert_eq!(
            event.validation_report_path.as_deref(),
            Some(rel_path.as_str())
        );
        assert_eq!(event.validation_ok, Some(true));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_ref_and_commit_round_trip() {
        let accepted_dir = temp_test_dir("sem-commit-roundtrip");
        ensure_layout(&accepted_dir).expect("layout");

        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 1_700_000_000,
            action: "promote".to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:sem-snapshot-a"),
            previous_snapshot_id: None,
            module_name: "Axi".to_string(),
            module_digest: AxiDigest::new("fnv1a64:module-a"),
            stored_module_path: "modules/Axi/demo.axi".to_string(),
            message: Some("seed".to_string()),
            quality_profile: Some("off".to_string()),
            quality_report_path: None,
            quality_error_count: None,
            quality_warning_count: None,
            quality_info_count: None,
            constraints_cert_path: Some("certs/module-a.json".to_string()),
            constraints_constraint_count: Some(1),
            constraints_instance_count: Some(2),
            constraints_check_count: Some(3),
            validation_report_path: None,
            validation_ok: Some(true),
        };
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: event.snapshot_id.clone(),
            previous_snapshot_id: None,
            created_at_unix_secs: 1_700_000_000,
            modules: BTreeMap::new(),
        };

        let commit =
            semantic_commit_from_promotion(&event, None, &snapshot, None).expect("build commit");
        let path = write_semantic_commit(&accepted_dir, &commit).expect("persist commit");
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(&digest_to_filename(commit.commit_id.as_str())),
            "unexpected commit path {}",
            path.display()
        );

        let round_trip =
            read_semantic_commit(&accepted_dir, &commit.commit_id).expect("read commit");
        assert_eq!(round_trip, commit);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_commit_parent_chain_round_trip() {
        let accepted_dir = temp_test_dir("sem-commit-parent-chain");
        ensure_layout(&accepted_dir).expect("layout");

        let snapshot_a = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:sem-snapshot-a"),
            previous_snapshot_id: None,
            created_at_unix_secs: 1,
            modules: BTreeMap::new(),
        };
        let event_a = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            action: "promote".to_string(),
            snapshot_id: snapshot_a.snapshot_id.clone(),
            previous_snapshot_id: None,
            module_name: "Axi".to_string(),
            module_digest: AxiDigest::new("fnv1a64:module-a"),
            stored_module_path: "modules/Axi/a.axi".to_string(),
            message: Some("a".to_string()),
            quality_profile: Some("off".to_string()),
            quality_report_path: None,
            quality_error_count: None,
            quality_warning_count: None,
            quality_info_count: None,
            constraints_cert_path: None,
            constraints_constraint_count: None,
            constraints_instance_count: None,
            constraints_check_count: None,
            validation_report_path: None,
            validation_ok: Some(true),
        };
        let commit_a =
            semantic_commit_from_promotion(&event_a, None, &snapshot_a, None).expect("seed commit");
        write_semantic_commit(&accepted_dir, &commit_a).expect("write seed commit");
        write_sem_head_commit_id(&accepted_dir, &commit_a.commit_id).expect("write sem head");
        write_sem_ref_pointer_for_main(&accepted_dir, &commit_a.commit_id).expect("write main ref");

        let snapshot_b = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:sem-snapshot-b"),
            previous_snapshot_id: Some(snapshot_a.snapshot_id.clone()),
            created_at_unix_secs: 2,
            modules: BTreeMap::new(),
        };
        let event_b = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 2,
            action: "promote".to_string(),
            snapshot_id: snapshot_b.snapshot_id.clone(),
            previous_snapshot_id: Some(snapshot_a.snapshot_id),
            module_name: "Axi".to_string(),
            module_digest: AxiDigest::new("fnv1a64:module-b"),
            stored_module_path: "modules/Axi/b.axi".to_string(),
            message: Some("b".to_string()),
            quality_profile: Some("off".to_string()),
            quality_report_path: None,
            quality_error_count: None,
            quality_warning_count: None,
            quality_info_count: None,
            constraints_cert_path: None,
            constraints_constraint_count: None,
            constraints_instance_count: None,
            constraints_check_count: None,
            validation_report_path: None,
            validation_ok: Some(true),
        };

        let commit_b = semantic_commit_from_promotion(
            &event_b,
            Some(commit_a.commit_id.clone()),
            &snapshot_b,
            None,
        )
        .expect("child commit");
        write_semantic_commit(&accepted_dir, &commit_b).expect("write child commit");

        let round_trip_b =
            read_semantic_commit(&accepted_dir, &commit_b.commit_id).expect("read child");
        assert_eq!(
            round_trip_b.parent_commit_id,
            Some(commit_a.commit_id.clone())
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_head_and_main_ref_persist() {
        let accepted_dir = temp_test_dir("sem-head-main-ref");
        ensure_layout(&accepted_dir).expect("layout");

        let commit_id = AxiDigest::new("fnv1a64:manual-commit");
        write_sem_head_commit_id(&accepted_dir, &commit_id).expect("write sem head");
        write_sem_ref_pointer_for_main(&accepted_dir, &commit_id).expect("write main ref");

        let head = read_sem_head_commit_id(&accepted_dir).expect("read sem head");
        assert_eq!(head, Some(commit_id.clone()));

        let pointer = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main ref")
            .expect("main ref exists");
        assert_eq!(pointer.ref_name, ACCEPTED_PLANE_SEM_HEADS_MAIN_REF);
        assert_eq!(pointer.commit_id, commit_id);
        assert_eq!(
            sem_main_ref_path(&accepted_dir),
            accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_MAIN_FILE)
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_branch_refs_round_trip_for_wm_and_review() {
        let accepted_dir = temp_test_dir("sem-branch-refs");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base-ref-commit",
            None,
            "BaseRefModule",
            "fnv1a64:module-base-ref-commit",
            "base",
        );
        let run_id = WorldModelRunId::new("wmrun:demo-run");
        persist_world_model_run_record(
            &accepted_dir,
            &sample_world_model_run_record(
                run_id.clone(),
                AcceptedSnapshotId::new("fnv1a64:snap-wm-commit"),
            ),
        )
        .expect("persist wm run");
        let wm_commit = seed_pathdb_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-wm-commit",
            Some(base_commit.commit_id.clone()),
            Some(run_id),
        )
        .commit_id;
        let review_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-review-commit",
            Some("fnv1a64:snap-base-ref-commit"),
            "ReviewModule",
            "fnv1a64:module-review-commit",
            "review",
        )
        .commit_id;
        persist_semantic_ref(&accepted_dir, "heads/wm/demo_run", &wm_commit).expect("wm ref");
        persist_semantic_ref(&accepted_dir, "heads/review/fam-parent", &review_commit)
            .expect("review ref");

        assert_eq!(
            read_sem_ref_pointer(&accepted_dir, "heads/wm/demo_run")
                .expect("read wm ref")
                .commit_id,
            wm_commit
        );
        assert_eq!(
            read_sem_ref_pointer(&accepted_dir, "heads/review/fam-parent")
                .expect("read review ref")
                .commit_id,
            review_commit
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_reconciliation_round_trips() {
        let accepted_dir = temp_test_dir("sem-reconciliation");
        ensure_layout(&accepted_dir).expect("layout");

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconciliation"),
            created_at_unix_secs: 42,
            base_commit_id: AxiDigest::new("fnv1a64:base"),
            left_commit_id: AxiDigest::new("fnv1a64:left"),
            right_commit_id: AxiDigest::new("fnv1a64:right"),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/fam-parent".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/main".to_string()),
            outcome_commit_id: Some(AxiDigest::new("fnv1a64:merge")),
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "module".to_string(),
                    artifact_id: "fnv1a64:module".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "parent relation changed incompatibly".to_string(),
            }],
            decisions: vec![SemDecisionRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "module".to_string(),
                    artifact_id: "fnv1a64:module".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                resolution: "prefer_left".to_string(),
            }],
            certificate_refs: vec!["certs/family_merge.json".to_string()],
        };

        let path = persist_reconciliation(&accepted_dir, &reconciliation).expect("persist");
        assert!(path.exists(), "reconciliation should be written");
        let round_trip =
            read_reconciliation(&accepted_dir, &reconciliation.reconciliation_id).expect("read");
        assert_eq!(round_trip, reconciliation);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_runtime_refinement_handle_upserts_decision() {
        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconciliation"),
            created_at_unix_secs: 42,
            base_commit_id: AxiDigest::new("fnv1a64:base"),
            left_commit_id: AxiDigest::new("fnv1a64:left"),
            right_commit_id: AxiDigest::new("fnv1a64:right"),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/fam-parent".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "rewrite_rule".to_string(),
                    artifact_id: "normalize_parent".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "parent relation changed incompatibly".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };

        let handle = crate::typed_refinement::RuntimeRefinementHandleV1::new_reconciliation(
            crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
                reconciliation_id: reconciliation.reconciliation_id.to_string(),
                artifact_kind: "rewrite_rule".to_string(),
                artifact_id: "normalize_parent".to_string(),
                resolution: "prefer_right".to_string(),
                theory_obligation_ref: None,
                theory_subject_ref: None,
                theory_subject_refs: Vec::new(),
            },
        );
        let updated = apply_runtime_refinement_handle_to_reconciliation(&reconciliation, &handle)
            .expect("apply reconciliation refinement handle");
        assert_eq!(updated.decisions.len(), 1);
        assert_eq!(updated.decisions[0].artifact.artifact_kind, "rewrite_rule");
        assert_eq!(
            updated.decisions[0].artifact.artifact_id,
            "normalize_parent"
        );
        assert_eq!(updated.decisions[0].resolution, "prefer_right");
    }

    #[test]
    fn reconciliation_runtime_refinement_by_id_uses_enriched_preview_candidates() {
        let accepted_dir = temp_test_dir("sem-reconciliation-apply-by-id-theory");
        ensure_layout(&accepted_dir).expect("layout");

        let module_text = r#"
module RefundPolicy

schema Refund:
  object RefundRequest
  object Approver
  relation RefundApproval(request: RefundRequest, approver: Approver)

theory RefundRules on Refund:
  constraint key RefundApproval(request, approver)
"#;

        let base_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-base-apply-by-id",
            None,
            "RefundPolicy",
            module_text,
            "base",
        );
        let left_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-left-apply-by-id",
            Some("fnv1a64:snap-base-apply-by-id"),
            "RefundPolicy",
            module_text,
            "left",
        );
        let right_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-right-apply-by-id",
            Some("fnv1a64:snap-left-apply-by-id"),
            "RefundPolicy",
            module_text,
            "right",
        );

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-apply-by-id"),
            created_at_unix_secs: 1_700_000_444,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left and right both change approval routing".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };

        let handle_id = reconciliation_preview_report(&accepted_dir, &reconciliation)
            .evolution_preview
            .refinement_candidates
            .into_iter()
            .find(|candidate| {
                candidate.artifact_id.as_deref() == Some("RefundApproval")
                    && candidate.resolution.as_deref() == Some("prefer_right")
            })
            .map(|candidate| candidate.handle.id)
            .expect("expected prefer_right reconciliation refinement candidate");

        let updated = apply_runtime_refinement_by_id_to_reconciliation(
            &accepted_dir,
            &reconciliation,
            &handle_id,
        )
        .expect("apply reconciliation refinement by id");

        assert_eq!(updated.decisions.len(), 1);
        assert_eq!(updated.decisions[0].resolution, "prefer_right");
        assert!(matches!(
            updated.decisions[0].artifact.theory_obligation_ref.as_ref(),
            Some(
                axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                    relation_name,
                    ..
                }
            ) if relation_name.as_deref() == Some("RefundApproval")
        ));
        assert!(updated.decisions[0]
            .artifact
            .theory_subject_refs
            .iter()
            .any(|subject| matches!(
                subject,
                axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                    relation_name,
                    ..
                } if relation_name == "RefundApproval"
            )));
        assert!(matches!(
            updated.decisions[0].artifact.theory_subject_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                relation_name,
                ..
            }) if relation_name == "RefundApproval"
        ));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_runtime_refinement_handle_refreshes_existing_decision_theory_refs() {
        let accepted_dir = temp_test_dir("sem-reconciliation-refresh-theory");
        ensure_layout(&accepted_dir).expect("layout");

        let module_text = r#"
module RefundPolicy

schema Refund:
  object RefundRequest
  object Approver
  relation RefundApproval(request: RefundRequest, approver: Approver)

theory RefundRules on Refund:
  constraint key RefundApproval(request, approver)
"#;

        let base_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-base-refresh-theory",
            None,
            "RefundPolicy",
            module_text,
            "base",
        );
        let left_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-left-refresh-theory",
            Some("fnv1a64:snap-base-refresh-theory"),
            "RefundPolicy",
            module_text,
            "left",
        );
        let right_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-right-refresh-theory",
            Some("fnv1a64:snap-left-refresh-theory"),
            "RefundPolicy",
            module_text,
            "right",
        );

        let unresolved = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-refresh-theory"),
            created_at_unix_secs: 1_700_000_445,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left and right both change approval routing".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };

        let handle = reconciliation_preview_report(&accepted_dir, &unresolved)
            .evolution_preview
            .refinement_candidates
            .into_iter()
            .find(|candidate| {
                candidate.artifact_id.as_deref() == Some("RefundApproval")
                    && candidate.resolution.as_deref() == Some("prefer_right")
            })
            .map(|candidate| candidate.handle)
            .expect("expected prefer_right reconciliation refinement candidate");

        let mut with_existing_decision = unresolved.clone();
        with_existing_decision.decisions.push(SemDecisionRecordV1 {
            artifact: ArtifactRefV1 {
                artifact_kind: "schema_relation".to_string(),
                artifact_id: "RefundApproval".to_string(),
                theory_obligation_ref: None,
                theory_subject_ref: None,
                theory_subject_refs: Vec::new(),
            },
            resolution: "manual_review".to_string(),
        });

        let updated =
            apply_runtime_refinement_handle_to_reconciliation(&with_existing_decision, &handle)
                .expect("apply reconciliation refinement handle");

        assert_eq!(updated.decisions.len(), 1);
        assert_eq!(updated.decisions[0].resolution, "prefer_right");
        assert!(matches!(
            updated.decisions[0].artifact.theory_obligation_ref.as_ref(),
            Some(
                axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                    relation_name,
                    ..
                }
            ) if relation_name.as_deref() == Some("RefundApproval")
        ));
        assert!(updated.decisions[0]
            .artifact
            .theory_subject_refs
            .iter()
            .any(|subject| matches!(
                subject,
                axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                    relation_name,
                    ..
                } if relation_name == "RefundApproval"
            )));
        assert!(matches!(
            updated.decisions[0].artifact.theory_subject_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                relation_name,
                ..
            }) if relation_name == "RefundApproval"
        ));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_ref_targets_round_trip_for_main_wm_and_review() {
        let accepted_dir = temp_test_dir("sem-ref-targets");
        ensure_layout(&accepted_dir).expect("layout");

        let commit_id = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-targets",
            None,
            "RefTargetModule",
            "fnv1a64:module-ref-targets",
            "ref-targets",
        )
        .commit_id;
        let run_id = WorldModelRunId::new("wmrun:ref-targets");
        persist_world_model_run_record(
            &accepted_dir,
            &sample_world_model_run_record(
                run_id.clone(),
                AcceptedSnapshotId::new("fnv1a64:snap-ref-targets-wm"),
            ),
        )
        .expect("persist wm run");
        let wm_commit_id = seed_pathdb_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-targets-wm",
            Some(commit_id.clone()),
            Some(run_id),
        )
        .commit_id;
        let main = SemRefNameV1::parse("heads/main").expect("parse main");
        let wm = SemRefNameV1::parse("heads/wm/demo-run").expect("parse wm");
        let review = SemRefNameV1::parse("heads/review/schema-a").expect("parse review");
        let evidence = SemRefNameV1::parse("heads/evidence/source-a").expect("parse evidence");
        let tag = SemRefNameV1::parse("tags/v1.0.0").expect("parse tag");

        assert_eq!(main, SemRefNameV1::main());
        assert_eq!(
            wm,
            SemRefNameV1::world_model("demo-run").expect("wm target")
        );
        assert_eq!(
            review,
            SemRefNameV1::review("schema-a").expect("review target")
        );
        assert_eq!(
            evidence,
            SemRefNameV1::evidence("source-a").expect("evidence target")
        );
        assert_eq!(tag, SemRefNameV1::tag("v1.0.0").expect("tag target"));

        let main_pointer =
            persist_semantic_ref_target(&accepted_dir, &main, &commit_id).expect("write main");
        let wm_pointer =
            persist_semantic_ref_target(&accepted_dir, &wm, &wm_commit_id).expect("write wm");
        let review_pointer =
            persist_semantic_ref_target(&accepted_dir, &review, &commit_id).expect("write review");
        let evidence_pointer = persist_semantic_ref_target(&accepted_dir, &evidence, &wm_commit_id)
            .expect("write evidence");
        let tag_pointer =
            persist_semantic_ref_target(&accepted_dir, &tag, &commit_id).expect("write tag");

        assert_eq!(
            read_sem_ref_pointer_target(&accepted_dir, &main).expect("read main"),
            main_pointer
        );
        assert_eq!(
            read_sem_ref_pointer_target(&accepted_dir, &wm).expect("read wm"),
            wm_pointer
        );
        assert_eq!(
            read_sem_ref_pointer_target(&accepted_dir, &review).expect("read review"),
            review_pointer
        );
        assert_eq!(
            read_sem_ref_pointer_target(&accepted_dir, &evidence).expect("read evidence"),
            evidence_pointer
        );
        assert_eq!(
            read_sem_ref_pointer_target(&accepted_dir, &tag).expect("read tag"),
            tag_pointer
        );
        assert!(
            accepted_dir.join("sem/refs/heads/wm/demo-run").exists(),
            "wm branch ref should persist at sem/refs/heads/wm/<name>"
        );
        assert!(
            accepted_dir.join("sem/refs/heads/review/schema-a").exists(),
            "review branch ref should persist at sem/refs/heads/review/<name>"
        );
        assert!(
            accepted_dir
                .join("sem/refs/heads/evidence/source-a")
                .exists(),
            "evidence branch ref should persist at sem/refs/heads/evidence/<name>"
        );
        assert!(
            accepted_dir.join("sem/refs/tags/v1.0.0").exists(),
            "semantic tag ref should persist at sem/refs/tags/<name>"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_head_migrates_direct_commit_to_symbolic_ref() {
        let accepted_dir = temp_test_dir("sem-head-symbolic-migration");
        ensure_layout(&accepted_dir).expect("layout");

        let commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-head-migration",
            None,
            "HeadMigrationModule",
            "fnv1a64:module-head-migration",
            "head-migration",
        );
        write_sem_ref_pointer_for_main(&accepted_dir, &commit.commit_id).expect("write main ref");
        write_sem_head_commit_id(&accepted_dir, &commit.commit_id).expect("detached head");

        let resolved = read_sem_head_commit_id(&accepted_dir).expect("read migrated head");
        assert_eq!(resolved, Some(commit.commit_id.clone()));
        let head = read_semantic_head(&accepted_dir)
            .expect("read symbolic head")
            .expect("head exists");
        assert_eq!(
            head,
            SemHeadV1::Symbolic {
                ref_name: "heads/main".to_string(),
                commit_id: Some(commit.commit_id.clone()),
            }
        );
        let head_text =
            fs::read_to_string(sem_head_path(&accepted_dir)).expect("read migrated sem head");
        assert_eq!(head_text.trim(), "ref: heads/main");

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_branch_checkout_and_tag_helpers_round_trip() {
        let accepted_dir = temp_test_dir("sem-branch-checkout-tag");
        ensure_layout(&accepted_dir).expect("layout");

        let main_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-helper-main",
            None,
            "HelperMainModule",
            "fnv1a64:module-helper-main",
            "helper-main",
        );
        let review_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-helper-review",
            Some("fnv1a64:snap-helper-main"),
            "HelperReviewModule",
            "fnv1a64:module-helper-review",
            "helper-review",
        );

        let review = SemRefNameV1::review("refund-policy").expect("review ref");
        let review_pointer =
            persist_semantic_branch_ref(&accepted_dir, &review, &review_commit.commit_id)
                .expect("persist review branch");
        assert_eq!(review_pointer.ref_name, "heads/review/refund-policy");

        let checked_out =
            checkout_semantic_ref(&accepted_dir, "heads/review/refund-policy").expect("checkout");
        assert_eq!(checked_out.pointer.commit_id, review_commit.commit_id);
        assert_eq!(
            read_semantic_head(&accepted_dir).expect("read head"),
            Some(SemHeadV1::Symbolic {
                ref_name: "heads/review/refund-policy".to_string(),
                commit_id: Some(review_commit.commit_id.clone()),
            })
        );

        let tag = SemRefNameV1::tag("v1.0.0").expect("tag ref");
        let tag_pointer =
            persist_semantic_tag_ref(&accepted_dir, &tag, &main_commit.commit_id).expect("tag");
        assert_eq!(tag_pointer.ref_name, "tags/v1.0.0");

        let err = persist_semantic_branch_ref(&accepted_dir, &tag, &main_commit.commit_id)
            .expect_err("tag is not a branch");
        assert!(err.to_string().contains("branch helper"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_generic_ref_rejects_path_traversal_segments() {
        let accepted_dir = temp_test_dir("sem-ref-invalid-path");
        ensure_layout(&accepted_dir).expect("layout");

        let commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-invalid-ref",
            None,
            "RefModule",
            "fnv1a64:module-invalid-ref",
            "invalid-ref",
        );

        let err = persist_semantic_ref(&accepted_dir, "heads/custom/../escape", &commit.commit_id)
            .expect_err("path traversal ref should be rejected");
        assert!(err.to_string().contains("invalid path segment"));
        assert!(!accepted_dir
            .join("sem/refs/heads/custom/../escape")
            .exists());

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn persist_semantic_ref_rejects_nonexistent_commit_id() {
        let accepted_dir = temp_test_dir("sem-ref-missing-commit");
        ensure_layout(&accepted_dir).expect("layout");

        let err = persist_semantic_ref(
            &accepted_dir,
            "heads/custom/demo",
            &AxiDigest::new("fnv1a64:missing-commit"),
        )
        .expect_err("missing commit should be rejected");
        assert!(err.to_string().contains("failed to read semantic commit"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_ref_validation_rejects_invalid_branch_family_transitions() {
        let accepted_dir = temp_test_dir("sem-ref-invalid-transitions");
        ensure_layout(&accepted_dir).expect("layout");

        let main_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-validation-main",
            None,
            "MainRefModule",
            "fnv1a64:module-ref-validation-main",
            "main",
        );
        let evidence_commit = seed_pathdb_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-validation-evidence",
            Some(main_commit.commit_id.clone()),
            None,
        );

        let err = persist_semantic_ref(&accepted_dir, "heads/main", &evidence_commit.commit_id)
            .expect_err("evidence commit must not move main");
        assert!(
            err.to_string().contains("heads/main"),
            "unexpected error: {err:#}"
        );

        let err = persist_semantic_ref(&accepted_dir, "heads/wm/no-run", &main_commit.commit_id)
            .expect_err("promote commit must not move wm ref");
        assert!(
            err.to_string().contains("WorldModelRun"),
            "unexpected error: {err:#}"
        );

        let run_id = WorldModelRunId::new("wmrun:validation-run");
        let wm_commit_missing_record = seed_pathdb_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-validation-wm",
            Some(main_commit.commit_id.clone()),
            Some(run_id.clone()),
        );
        let err = persist_semantic_ref(
            &accepted_dir,
            "heads/wm/missing-record",
            &wm_commit_missing_record.commit_id,
        )
        .expect_err("wm ref requires persisted run record");
        assert!(
            err.to_string().contains("WorldModelRunRecord"),
            "unexpected error: {err:#}"
        );

        persist_world_model_run_record(
            &accepted_dir,
            &sample_world_model_run_record(
                run_id,
                AcceptedSnapshotId::new("fnv1a64:snap-ref-validation-wm"),
            ),
        )
        .expect("persist wm run");
        persist_semantic_ref(
            &accepted_dir,
            "heads/wm/valid-record",
            &wm_commit_missing_record.commit_id,
        )
        .expect("wm ref with run record");

        let err = persist_semantic_ref(
            &accepted_dir,
            "heads/evidence/promote",
            &main_commit.commit_id,
        )
        .expect_err("promote commit must not move evidence ref");
        assert!(
            err.to_string().contains("evidence refs"),
            "unexpected error: {err:#}"
        );

        let err = persist_semantic_ref(
            &accepted_dir,
            "tags/unreviewed-evidence",
            &evidence_commit.commit_id,
        )
        .expect_err("evidence commit must not be taggable");
        assert!(
            err.to_string().contains("semantic tags"),
            "unexpected error: {err:#}"
        );

        persist_semantic_ref(&accepted_dir, "tags/v1", &main_commit.commit_id)
            .expect("initial tag");
        let other_main_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-ref-validation-main-2",
            Some("fnv1a64:snap-ref-validation-main"),
            "MainRefModule2",
            "fnv1a64:module-ref-validation-main-2",
            "main-2",
        );
        let err = persist_semantic_ref(&accepted_dir, "tags/v1", &other_main_commit.commit_id)
            .expect_err("semantic tags are immutable");
        assert!(
            err.to_string().contains("immutable"),
            "unexpected error: {err:#}"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn persist_semantic_ref_for_main_updates_semantic_head() {
        let accepted_dir = temp_test_dir("sem-ref-main-updates-head");
        ensure_layout(&accepted_dir).expect("layout");

        let commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-main-update",
            None,
            "MainModule",
            "fnv1a64:module-main-update",
            "main-update",
        );

        persist_semantic_ref(&accepted_dir, "heads/main", &commit.commit_id)
            .expect("persist main ref");

        assert_eq!(
            read_sem_head_commit_id(&accepted_dir).expect("read sem head"),
            Some(commit.commit_id.clone())
        );
        assert_eq!(
            read_sem_ref_pointer(&accepted_dir, "heads/main")
                .expect("read main ref")
                .commit_id,
            commit.commit_id
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_reconciliation_round_trips_from_sem_directory() {
        let accepted_dir = temp_test_dir("sem-reconciliation");
        ensure_layout(&accepted_dir).expect("layout");

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-a"),
            created_at_unix_secs: 1_700_000_123,
            base_commit_id: AxiDigest::new("fnv1a64:base"),
            left_commit_id: AxiDigest::new("fnv1a64:left"),
            right_commit_id: AxiDigest::new("fnv1a64:right"),
            policy: "prefer_review".to_string(),
            source_ref_name: Some("heads/wm/demo-run".to_string()),
            target_ref_name: Some("heads/review/schema-a".to_string()),
            resolved_ref_name: Some("heads/main".to_string()),
            outcome_commit_id: Some(AxiDigest::new("fnv1a64:merged")),
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "module".to_string(),
                    artifact_id: "fnv1a64:module-a".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "schema conflict".to_string(),
            }],
            decisions: vec![SemDecisionRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "module".to_string(),
                    artifact_id: "fnv1a64:module-a".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                resolution: "take review branch".to_string(),
            }],
            certificate_refs: vec!["certs/reconcile-a.json".to_string()],
        };

        let path =
            persist_reconciliation(&accepted_dir, &reconciliation).expect("persist reconcile");
        assert!(
            path.ends_with("sem/reconciliations/fnv1a64_reconcile-a.json"),
            "unexpected reconciliation path: {}",
            path.display()
        );

        let round_trip = read_reconciliation(&accepted_dir, &reconciliation.reconciliation_id)
            .expect("read reconciliation");
        assert_eq!(round_trip, reconciliation);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn semantic_slice_manifest_round_trips_under_sem_slices() {
        let accepted_dir = temp_test_dir("sem-slice-manifest");
        ensure_layout(&accepted_dir).expect("layout");

        let commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-slice",
            None,
            "SliceModule",
            "fnv1a64:module-slice",
            "slice",
        );
        persist_semantic_ref(&accepted_dir, "heads/main", &commit.commit_id)
            .expect("persist main ref");
        let view = read_sem_ref_view(&accepted_dir, "heads/main").expect("read ref view");
        let manifest = crate::semantic_merge_lattice::semantic_slice_from_ref_view(
            &view,
            crate::semantic_merge_lattice::SemanticSliceSelectorV1::default(),
        );

        let stored_path =
            persist_semantic_slice_manifest(&accepted_dir, &manifest).expect("persist slice");
        assert!(
            stored_path.starts_with("sem/slices/"),
            "unexpected stored path: {stored_path}"
        );
        let round_trip = read_semantic_slice_manifest(&accepted_dir, manifest.slice_id.as_str())
            .expect("read slice by id");
        assert_eq!(round_trip, manifest);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_semantic_commit_persists_preview_and_updates_ref() {
        let accepted_dir = temp_test_dir("sem-reconciliation-commit");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base",
            None,
            "BaseModule",
            "fnv1a64:module-base",
            "base",
        );
        let left_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-left",
            Some("fnv1a64:snap-base"),
            "LeftModule",
            "fnv1a64:module-left",
            "left",
        );
        let right_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-right",
            Some("fnv1a64:snap-left"),
            "RightModule",
            "fnv1a64:module-right",
            "right",
        );
        persist_semantic_ref(&accepted_dir, "heads/main", &right_commit.commit_id)
            .expect("seed main ref");

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-commit"),
            created_at_unix_secs: 1_700_000_777,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/review/refund-policy-merged".to_string()),
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left introduces a stricter approval path than right".to_string(),
            }],
            decisions: vec![SemDecisionRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                resolution: "prefer_review_branch".to_string(),
            }],
            certificate_refs: vec!["certs/reconcile-commit.json".to_string()],
        };

        let commit = persist_reconciliation_semantic_commit(
            &accepted_dir,
            &reconciliation,
            &ReconciliationSemanticCommitOptionsV1::default(),
        )
        .expect("persist reconciliation commit");

        assert_eq!(commit.kind, SemCommitKindV1::Merge);
        assert_eq!(
            commit.reconciliation_id,
            Some(AxiDigest::new("fnv1a64:reconcile-commit"))
        );
        assert_eq!(
            commit.parent_commit_id,
            Some(right_commit.commit_id.clone()),
            "target branch commit should be used as the linearized parent"
        );
        assert_eq!(
            commit
                .delta
                .semantic_delta
                .as_ref()
                .expect("semantic delta")
                .changed_layers,
            vec!["schema".to_string()]
        );
        assert_eq!(
            commit
                .delta
                .trust_summary
                .as_ref()
                .expect("trust summary")
                .trust_class,
            "runtime_checked_reconciliation_preview"
        );
        assert_eq!(
            commit.delta.certificate_refs_added,
            vec!["certs/reconcile-commit.json".to_string()]
        );
        assert_eq!(
            commit
                .validation_report_path
                .as_deref()
                .expect("validation report path"),
            "sem/validations/reconciliation__fnv1a64_reconcile-commit.json"
        );
        assert_eq!(
            commit.delta.validation_report_refs_added,
            vec!["sem/validations/reconciliation__fnv1a64_reconcile-commit.json".to_string()]
        );

        let report_text = fs::read_to_string(
            accepted_dir.join(
                commit
                    .validation_report_path
                    .as_ref()
                    .expect("validation report path"),
            ),
        )
        .expect("read validation report");
        let report: ReconciliationPreviewReportV1 =
            serde_json::from_str(&report_text).expect("parse reconciliation preview report");
        assert_eq!(
            report.evolution_preview.kind,
            "semantic_reconciliation_preview"
        );
        assert_eq!(
            report
                .stored_report_path
                .as_deref()
                .expect("stored report path"),
            "sem/validations/reconciliation__fnv1a64_reconcile-commit.json"
        );

        let ref_pointer = read_sem_ref_pointer(&accepted_dir, "heads/review/refund-policy-merged")
            .expect("read merged review ref");
        assert_eq!(ref_pointer.commit_id, commit.commit_id);
        assert_eq!(
            read_sem_head_commit_id(&accepted_dir).expect("read sem head"),
            Some(commit.commit_id.clone())
        );

        let round_trip =
            read_semantic_commit(&accepted_dir, &commit.commit_id).expect("read merge commit");
        assert_eq!(round_trip.kind, SemCommitKindV1::Merge);
        assert_eq!(
            round_trip
                .gate_summary
                .as_ref()
                .expect("gate summary")
                .trust
                .trust_class,
            "runtime_checked_reconciliation_preview"
        );
        let updated_reconciliation =
            read_reconciliation(&accepted_dir, &reconciliation.reconciliation_id)
                .expect("read updated reconciliation");
        assert_eq!(
            updated_reconciliation.outcome_commit_id,
            Some(commit.commit_id.clone())
        );
        assert_eq!(
            round_trip.provenance.source_commit,
            Some(left_commit.commit_id.clone())
        );
        assert_eq!(
            round_trip.state.accepted_snapshot_id_before,
            round_trip.state.accepted_snapshot_id_after,
            "first merge slice should keep accepted-plane state stable until promotion"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_preview_report_enriches_theory_refs_from_snapshot_modules() {
        let accepted_dir = temp_test_dir("sem-reconciliation-preview-theory");
        ensure_layout(&accepted_dir).expect("layout");

        let module_text = r#"
module RefundPolicy

schema Refund:
  object RefundRequest
  object Approver
  relation RefundApproval(request: RefundRequest, approver: Approver)

theory RefundRules on Refund:
  constraint key RefundApproval(request, approver)
"#;

        let base_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-base-theory",
            None,
            "RefundPolicy",
            module_text,
            "base",
        );
        let left_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-left-theory",
            Some("fnv1a64:snap-base-theory"),
            "RefundPolicy",
            module_text,
            "left",
        );
        let right_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-right-theory",
            Some("fnv1a64:snap-left-theory"),
            "RefundPolicy",
            module_text,
            "right",
        );

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-preview-theory"),
            created_at_unix_secs: 1_700_000_999,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left and right both change approval routing".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };

        let report = reconciliation_preview_report(&accepted_dir, &reconciliation);
        assert_eq!(
            report.evolution_preview.kind,
            "semantic_reconciliation_preview"
        );
        assert_eq!(report.evolution_preview.refinement_candidates.len(), 3);
        for candidate in &report.evolution_preview.refinement_candidates {
            assert!(
                matches!(
                    candidate.theory_obligation_ref.as_ref(),
                    Some(
                        axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                            relation_name: Some(relation_name),
                            ..
                        }
                    ) if relation_name == "RefundApproval"
                ),
                "expected a compiled-theory obligation for RefundApproval, got {:?}",
                candidate.theory_obligation_ref
            );
            assert!(
                candidate.theory_subject_refs.iter().any(|subject| matches!(
                    subject,
                    axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                        relation_name,
                        ..
                    } if relation_name == "RefundApproval"
                )),
                "expected compiled-theory relation subject refs for RefundApproval, got {:?}",
                candidate.theory_subject_refs
            );
        }

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn preview_reconciliation_reads_persisted_record() {
        let accepted_dir = temp_test_dir("sem-reconciliation-preview-public");
        ensure_layout(&accepted_dir).expect("layout");

        let module_text = r#"
module RefundPolicy

schema Refund:
  object RefundRequest
  object Approver
  relation RefundApproval(request: RefundRequest, approver: Approver)

theory RefundRules on Refund:
  constraint key RefundApproval(request, approver)
"#;

        let base_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-base-preview-public",
            None,
            "RefundPolicy",
            module_text,
            "base",
        );
        let left_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-left-preview-public",
            Some("fnv1a64:snap-base-preview-public"),
            "RefundPolicy",
            module_text,
            "left",
        );
        let right_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-right-preview-public",
            Some("fnv1a64:snap-left-preview-public"),
            "RefundPolicy",
            module_text,
            "right",
        );

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-preview-public"),
            created_at_unix_secs: 1_700_001_111,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left and right both change approval routing".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };
        persist_reconciliation(&accepted_dir, &reconciliation).expect("persist reconciliation");

        let report = preview_reconciliation(&accepted_dir, &reconciliation.reconciliation_id)
            .expect("preview persisted reconciliation");
        assert_eq!(report.reconciliation_id, reconciliation.reconciliation_id);
        assert_eq!(
            report.evolution_preview.kind,
            "semantic_reconciliation_preview"
        );
        assert!(!report.evolution_preview.refinement_candidates.is_empty());

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn apply_reconciliation_refinement_by_id_persists_updated_record() {
        let accepted_dir = temp_test_dir("sem-reconciliation-apply-public");
        ensure_layout(&accepted_dir).expect("layout");

        let module_text = r#"
module RefundPolicy

schema Refund:
  object RefundRequest
  object Approver
  relation RefundApproval(request: RefundRequest, approver: Approver)

theory RefundRules on Refund:
  constraint key RefundApproval(request, approver)
"#;

        let base_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-base-apply-public",
            None,
            "RefundPolicy",
            module_text,
            "base",
        );
        let left_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-left-apply-public",
            Some("fnv1a64:snap-base-apply-public"),
            "RefundPolicy",
            module_text,
            "left",
        );
        let right_commit = seed_semantic_commit_with_module_text(
            &accepted_dir,
            "fnv1a64:snap-right-apply-public",
            Some("fnv1a64:snap-left-apply-public"),
            "RefundPolicy",
            module_text,
            "right",
        );

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-apply-public"),
            created_at_unix_secs: 1_700_001_222,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "left and right both change approval routing".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };
        persist_reconciliation(&accepted_dir, &reconciliation).expect("persist reconciliation");

        let handle_id = preview_reconciliation(&accepted_dir, &reconciliation.reconciliation_id)
            .expect("preview persisted reconciliation")
            .evolution_preview
            .refinement_candidates
            .into_iter()
            .find(|candidate| {
                candidate.artifact_id.as_deref() == Some("RefundApproval")
                    && candidate.resolution.as_deref() == Some("prefer_right")
            })
            .map(|candidate| candidate.handle.id)
            .expect("expected prefer_right refinement candidate");

        let applied = apply_reconciliation_refinement_by_id(
            &accepted_dir,
            &reconciliation.reconciliation_id,
            &handle_id,
        )
        .expect("apply persisted reconciliation refinement");
        assert_eq!(applied.updated_reconciliation.decisions.len(), 1);
        assert_eq!(
            applied.updated_reconciliation.decisions[0].resolution,
            "prefer_right"
        );

        let stored = read_reconciliation(&accepted_dir, &reconciliation.reconciliation_id)
            .expect("read updated reconciliation");
        assert_eq!(stored.decisions.len(), 1);
        assert_eq!(stored.decisions[0].resolution, "prefer_right");
        assert!(matches!(
            stored.decisions[0].artifact.theory_subject_ref.as_ref(),
            Some(axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                relation_name,
                ..
            }) if relation_name == "RefundApproval"
        ));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_semantic_commit_fails_closed_on_unresolved_conflicts() {
        let accepted_dir = temp_test_dir("sem-reconciliation-unresolved");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base",
            None,
            "BaseModule",
            "fnv1a64:module-base",
            "base",
        );
        let left_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-left",
            Some("fnv1a64:snap-base"),
            "LeftModule",
            "fnv1a64:module-left",
            "left",
        );
        let right_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-right",
            Some("fnv1a64:snap-left"),
            "RightModule",
            "fnv1a64:module-right",
            "right",
        );
        persist_semantic_ref(&accepted_dir, "heads/main", &right_commit.commit_id)
            .expect("seed main ref");

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-unresolved"),
            created_at_unix_secs: 1_700_000_888,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/review/refund-policy-merged".to_string()),
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: ArtifactRefV1 {
                    artifact_kind: "schema_relation".to_string(),
                    artifact_id: "RefundApproval".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                detail: "conflict remains unresolved".to_string(),
            }],
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };

        let err = persist_reconciliation_semantic_commit(
            &accepted_dir,
            &reconciliation,
            &ReconciliationSemanticCommitOptionsV1::default(),
        )
        .expect_err("unresolved reconciliation should not materialize merge commit");
        assert!(
            err.to_string().contains("still has unresolved obligations"),
            "unexpected error: {err:#}"
        );
        assert!(
            accepted_dir
                .join("sem/validations/reconciliation__fnv1a64_reconcile-unresolved.json")
                .exists(),
            "preview report should still be persisted for review"
        );
        assert!(
            read_sem_ref_pointer(&accepted_dir, "heads/review/refund-policy-merged").is_err(),
            "resolved ref should not move on a failed preview"
        );
        assert_eq!(
            read_sem_head_commit_id(&accepted_dir).expect("read sem head"),
            Some(right_commit.commit_id.clone()),
            "semantic head should remain unchanged when merge materialization fails"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn reconciliation_semantic_commit_fails_closed_on_non_materializing_decision() {
        let accepted_dir = temp_test_dir("sem-reconciliation-manual-review");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base-manual-review",
            None,
            "BaseModule",
            "fnv1a64:module-base-manual-review",
            "base",
        );
        let left_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-left-manual-review",
            Some("fnv1a64:snap-base-manual-review"),
            "LeftModule",
            "fnv1a64:module-left-manual-review",
            "left",
        );
        write_sem_head_commit_id(&accepted_dir, &base_commit.commit_id).expect("reset sem head");
        let right_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-right-manual-review",
            Some("fnv1a64:snap-base-manual-review"),
            "RightModule",
            "fnv1a64:module-right-manual-review",
            "right",
        );
        persist_semantic_ref(&accepted_dir, "heads/main", &right_commit.commit_id)
            .expect("seed main ref");

        let artifact = ArtifactRefV1 {
            artifact_kind: "schema_relation".to_string(),
            artifact_id: "RefundApproval".to_string(),
            theory_obligation_ref: None,
            theory_subject_ref: None,
            theory_subject_refs: Vec::new(),
        };
        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-manual-review"),
            created_at_unix_secs: 1_700_000_889,
            base_commit_id: base_commit.commit_id.clone(),
            left_commit_id: left_commit.commit_id.clone(),
            right_commit_id: right_commit.commit_id.clone(),
            policy: "cq_gate".to_string(),
            source_ref_name: Some("heads/review/refund-policy".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: Some("heads/main".to_string()),
            outcome_commit_id: None,
            conflicts: vec![SemConflictRecordV1 {
                artifact: artifact.clone(),
                detail: "conflict remains unresolved".to_string(),
            }],
            decisions: vec![SemDecisionRecordV1 {
                artifact,
                resolution: "manual_review".to_string(),
            }],
            certificate_refs: Vec::new(),
        };

        let err = persist_reconciliation_semantic_commit(
            &accepted_dir,
            &reconciliation,
            &ReconciliationSemanticCommitOptionsV1::default(),
        )
        .expect_err("manual_review decision should not materialize merge commit");
        assert!(
            err.to_string()
                .contains("non-materializing resolver decision"),
            "unexpected error: {err:#}"
        );
        assert_eq!(
            read_sem_head_commit_id(&accepted_dir).expect("read sem head"),
            Some(right_commit.commit_id),
            "semantic head should remain unchanged when resolver decision is non-materializing"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn promotion_semantic_commit_populates_state_and_delta() {
        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 11,
            action: "promote".to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:promotion-snapshot"),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:promotion-parent")),
            module_name: "PromotionDemo".to_string(),
            module_digest: AxiDigest::new("fnv1a64:promotion-module"),
            stored_module_path: "modules/PromotionDemo/demo.axi".to_string(),
            message: Some("promote it".to_string()),
            quality_profile: Some("strict".to_string()),
            quality_report_path: Some("quality/promotion.json".to_string()),
            quality_error_count: Some(0),
            quality_warning_count: Some(1),
            quality_info_count: Some(2),
            constraints_cert_path: Some("certs/promotion.json".to_string()),
            constraints_constraint_count: Some(3),
            constraints_instance_count: Some(4),
            constraints_check_count: Some(5),
            validation_report_path: Some("sem/validations/promotion.json".to_string()),
            validation_ok: Some(true),
        };
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: event.snapshot_id.clone(),
            previous_snapshot_id: event.previous_snapshot_id.clone(),
            created_at_unix_secs: 12,
            modules: BTreeMap::new(),
        };

        let preview = sample_evolution_preview();
        let commit = semantic_commit_from_promotion(&event, None, &snapshot, Some(&preview))
            .expect("promotion");
        assert_eq!(commit.kind, SemCommitKindV1::Promote);
        assert_eq!(commit.provenance.source, "accepted_plane");
        assert_eq!(
            commit.state.accepted_snapshot_id_before,
            Some(AcceptedSnapshotId::new("fnv1a64:promotion-parent"))
        );
        assert_eq!(
            commit.state.accepted_snapshot_id_after,
            Some(AcceptedSnapshotId::new("fnv1a64:promotion-snapshot"))
        );
        assert!(commit.state.pathdb_snapshot_id_after.is_none());
        assert_eq!(
            commit.delta.module_digests_added,
            vec![AxiDigest::new("fnv1a64:promotion-module")]
        );
        assert_eq!(
            commit.delta.certificate_refs_added,
            vec!["certs/promotion.json".to_string()]
        );
        assert_eq!(
            commit
                .delta
                .semantic_delta
                .as_ref()
                .expect("semantic delta")
                .changed_layers,
            vec!["schema".to_string(), "instance".to_string()]
        );
        assert_eq!(
            commit
                .delta
                .trust_summary
                .as_ref()
                .expect("trust summary")
                .trust_class,
            "accepted_promotion_preview"
        );
        assert_eq!(
            commit
                .delta
                .rule_summary
                .as_ref()
                .expect("rule summary")
                .total_rules,
            4
        );
        assert_eq!(
            commit
                .delta
                .coverage_summary
                .as_ref()
                .expect("coverage summary")
                .competency_questions_total,
            2
        );
        assert_eq!(
            commit
                .delta
                .runtime_theory_check
                .as_ref()
                .expect("runtime theory check")
                .completeness_claim,
            "claimed_under_finite_fragment"
        );
        assert_eq!(
            commit.delta.quality_report_refs_added,
            vec!["quality/promotion.json".to_string()]
        );
        assert_eq!(
            commit.delta.validation_report_refs_added,
            vec!["sem/validations/promotion.json".to_string()]
        );
        assert_eq!(commit.delta.lifecycle_events.len(), 1);
        assert_eq!(
            commit.delta.lifecycle_events[0].to,
            LifecycleStageV1::Accepted
        );
        let gate_summary = commit.gate_summary.as_ref().expect("gate summary");
        assert_eq!(gate_summary.trust.trust_class, "accepted_promotion_preview");
        assert!(gate_summary.runtime_theory_check.is_some());
        assert_eq!(
            gate_summary
                .competency
                .as_ref()
                .expect("competency")
                .improvements,
            1
        );
        assert_eq!(
            gate_summary
                .rule
                .as_ref()
                .expect("rule summary")
                .constraint_count,
            3
        );
        assert_eq!(
            gate_summary
                .runtime_semantics
                .as_ref()
                .expect("runtime semantics")
                .coverage,
            "accepted_snapshot_delta_plus_competency_questions"
        );
    }

    #[test]
    fn pathdb_semantic_commit_populates_state_and_delta() {
        let accepted_snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:accepted-overlay"),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-parent")),
            created_at_unix_secs: 20,
            modules: BTreeMap::new(),
        };
        let commit = semantic_commit_from_pathdb_overlay(
            &accepted_snapshot,
            &PathdbSnapshotId::new("fnv1a64:pathdb-overlay"),
            Some(AxiDigest::new("fnv1a64:parent-commit")),
            &PathdbSemanticCommitOptionsV1 {
                message: Some("overlay".to_string()),
                proposal_digests: vec![ProposalDigest::new("fnv1a64:proposal-a")],
                gate_summary: Some(
                    crate::evolution_preview::sem_gate_summary_from_evolution_preview(
                        &sample_evolution_preview(),
                    ),
                ),
                world_model_run_id: Some(WorldModelRunId::new("wm::run-a")),
                ..PathdbSemanticCommitOptionsV1::default()
            },
        );

        assert_eq!(commit.kind, SemCommitKindV1::WorldModelRun);
        assert_eq!(commit.provenance.source, "world_model");
        assert_eq!(
            commit.state.accepted_snapshot_id_before,
            Some(AcceptedSnapshotId::new("fnv1a64:accepted-overlay"))
        );
        assert_eq!(
            commit.state.accepted_snapshot_id_after,
            Some(AcceptedSnapshotId::new("fnv1a64:accepted-overlay"))
        );
        assert_eq!(
            commit.state.pathdb_snapshot_id_after,
            Some(PathdbSnapshotId::new("fnv1a64:pathdb-overlay"))
        );
        assert_eq!(
            commit.state.evidence_digests,
            vec![ProposalDigest::new("fnv1a64:proposal-a")]
        );
        assert!(commit.delta.module_digests_added.is_empty());
        assert_eq!(
            commit.delta.evidence_blobs_added,
            vec![ProposalDigest::new("fnv1a64:proposal-a")]
        );
        assert_eq!(
            commit.delta.world_model_run_refs,
            vec![WorldModelRunId::new("wm::run-a")]
        );
        assert!(commit.delta.semantic_delta.is_none());
        assert!(commit.delta.trust_summary.is_none());
        assert!(commit.delta.rule_summary.is_none());
        assert!(commit.delta.coverage_summary.is_none());
        assert_eq!(commit.delta.lifecycle_events.len(), 1);
        assert_eq!(
            commit.delta.lifecycle_events[0].artifact.artifact_kind,
            "proposal_set"
        );
        assert_eq!(
            commit
                .gate_summary
                .as_ref()
                .expect("gate summary")
                .trust
                .trust_class,
            "accepted_promotion_preview"
        );
    }

    #[test]
    fn promote_reviewed_module_emits_semantic_commit() {
        let accepted_dir = temp_test_dir("sem-promotion-emits-commit");
        ensure_layout(&accepted_dir).expect("layout");

        let base_axi = accepted_dir.join("Baseline.axi");
        fs::write(
            &base_axi,
            r#"module WorldModelDemo

schema Demo:
  object Node
  relation Edge(from: Node, to: Node)

instance I of Demo:
  Node = {a, b}
  Edge = {(from=a, to=b)}
"#,
        )
        .expect("write baseline module");

        let result_snapshot =
            promote_reviewed_module(&base_axi, &accepted_dir, Some("semantic-promotion"), "off")
                .expect("promote baseline");

        let sem_head = read_sem_head_commit_id(&accepted_dir).expect("read sem head");
        assert!(sem_head.is_some(), "promotion must write sem/HEAD");

        let main_pointer = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer")
            .expect("main pointer exists");

        let sem_commit_id = sem_head.expect("sem head present");
        assert_eq!(main_pointer.commit_id, sem_commit_id);

        let commit =
            read_semantic_commit(&accepted_dir, &sem_commit_id).expect("read semantic commit");
        assert_eq!(commit.accepted_snapshot_id, result_snapshot);
        assert_eq!(commit.action, "promote");
        assert_eq!(commit.kind, SemCommitKindV1::Promote);
        assert_eq!(commit.module_name, "WorldModelDemo");
        assert_eq!(commit.parent_commit_id, None);
        assert_eq!(
            commit.state.accepted_snapshot_id_after,
            Some(result_snapshot.clone())
        );
        assert_eq!(commit.delta.module_digests_added.len(), 1);
        assert!(
            commit.gate_summary.is_some(),
            "promotion commits should persist gate summary"
        );
        assert_eq!(main_pointer.gate_summary, commit.gate_summary);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn pathdb_semantic_commit_advances_sem_head_without_moving_main() {
        let accepted_dir = temp_test_dir("sem-pathdb-overlay-commit");
        ensure_layout(&accepted_dir).expect("layout");

        let base_axi = accepted_dir.join("Baseline.axi");
        fs::write(
            &base_axi,
            r#"module EvidenceWorld

schema Demo:
  object Node
  relation Edge(from: Node, to: Node)

instance I of Demo:
  Node = {a, b}
  Edge = {(from=a, to=b)}
"#,
        )
        .expect("write baseline module");

        let accepted_snapshot_id =
            promote_reviewed_module(&base_axi, &accepted_dir, Some("semantic-promotion"), "off")
                .expect("promote baseline");

        let promote_head = read_sem_head_commit_id(&accepted_dir)
            .expect("read sem head after promotion")
            .expect("promotion should seed sem head");
        let main_pointer_before = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer")
            .expect("main pointer exists");
        assert_eq!(main_pointer_before.commit_id, promote_head);

        let pathdb_commit =
            crate::pathdb_wal::commit_pathdb_snapshot_on_accepted_snapshot_with_overlays(
                &accepted_dir,
                &accepted_snapshot_id,
                &[],
                &[],
                Some("evidence overlay"),
            )
            .expect("commit pathdb overlay");

        let overlay_commit = persist_pathdb_semantic_commit(
            &accepted_dir,
            &accepted_snapshot_id,
            &pathdb_commit.snapshot_id,
            &PathdbSemanticCommitOptionsV1 {
                message: Some("overlay".to_string()),
                proposal_digests: vec![ProposalDigest::new("fnv1a64:proposal-a")],
                world_model_run_id: Some(WorldModelRunId::new("wm::overlay-run")),
                ..PathdbSemanticCommitOptionsV1::default()
            },
        )
        .expect("persist semantic overlay commit");

        let sem_head_after = read_sem_head_commit_id(&accepted_dir)
            .expect("read sem head after overlay")
            .expect("overlay must update sem head");
        assert_eq!(sem_head_after, overlay_commit.commit_id);
        assert_eq!(overlay_commit.parent_commit_id, Some(promote_head.clone()));
        assert_eq!(overlay_commit.action, "pathdb_commit");
        assert_eq!(overlay_commit.kind, SemCommitKindV1::WorldModelRun);
        assert_eq!(
            overlay_commit.pathdb_snapshot_id,
            Some(pathdb_commit.snapshot_id.clone())
        );
        assert_eq!(
            overlay_commit.proposal_digests,
            vec![ProposalDigest::new("fnv1a64:proposal-a")]
        );
        assert_eq!(
            overlay_commit.world_model_run_id,
            Some(WorldModelRunId::new("wm::overlay-run"))
        );
        assert_eq!(
            overlay_commit.state.pathdb_snapshot_id_after,
            Some(pathdb_commit.snapshot_id.clone())
        );
        assert_eq!(
            overlay_commit.delta.world_model_run_refs,
            vec![WorldModelRunId::new("wm::overlay-run")]
        );
        assert!(overlay_commit.gate_summary.is_none());

        let main_pointer_after = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer after overlay")
            .expect("main pointer exists");
        assert_eq!(
            main_pointer_after.commit_id, promote_head,
            "evidence-plane pathdb commits must not advance refs/heads/main"
        );
        assert_eq!(
            main_pointer_after.gate_summary, main_pointer_before.gate_summary,
            "evidence-plane commits must not rewrite main's promotion gate summary"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn projection_manifest_round_trips_with_typed_backend_profile() {
        let accepted_dir = temp_test_dir("sem-projection-manifest");
        ensure_layout(&accepted_dir).expect("layout");

        let base_axi = accepted_dir.join("ProjectionDemo.axi");
        fs::write(
            &base_axi,
            r#"module ProjectionDemo

schema Demo:
  object Node
  relation Edge(from: Node, to: Node)

instance I of Demo:
  Node = {a, b}
  Edge = {(from=a, to=b)}
"#,
        )
        .expect("write projection module");

        let accepted_snapshot_id =
            promote_reviewed_module(&base_axi, &accepted_dir, Some("projection-base"), "off")
                .expect("promote baseline");
        let manifest = sample_projection_manifest(accepted_snapshot_id.clone(), None);
        let path = persist_projection_manifest(&accepted_dir, &manifest).expect("persist manifest");

        assert!(
            path.starts_with(accepted_dir.join("sem/projections")),
            "projection manifests must live under sem/projections"
        );

        let stored = read_projection_manifest(
            &accepted_dir,
            &normalize_projection_manifest(manifest)
                .expect("normalize")
                .projection_id,
        )
        .expect("read manifest");
        assert_eq!(stored.version, BACKEND_PROJECTION_MANIFEST_VERSION_V1);
        assert_eq!(stored.accepted_snapshot_id, accepted_snapshot_id);
        assert_eq!(
            stored.backend.backend_engine,
            ProjectionBackendEngineV1::TypeDb
        );
        assert_eq!(
            stored.backend.support_tier,
            ProjectionSupportTierV1::Primary
        );
        assert!(stored.backend.constraints);
        assert!(stored.projection.preserves_relation_objects);
        assert_eq!(stored.relation_mappings.len(), 1);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn backend_capability_presets_encode_supported_backend_priority() {
        let typedb = BackendCapabilityProfileV1::typedb_primary();
        let terminus = BackendCapabilityProfileV1::terminusdb_supported();
        let age = BackendCapabilityProfileV1::apache_age_experimental();

        assert_eq!(typedb.backend_engine, ProjectionBackendEngineV1::TypeDb);
        assert_eq!(typedb.support_tier, ProjectionSupportTierV1::Primary);
        assert!(typedb.constraints);
        assert!(typedb.native_type_system);
        assert!(typedb.native_nary_relations);
        assert!(typedb.typed_query_validation);
        assert!(typedb.relationship_entities);
        assert!(
            !typedb.can_mirror_semantic_vcs_workspace(),
            "TypeDB should be treated as the primary typed execution target, not as the semantic VCS authority"
        );

        assert_eq!(
            terminus.backend_engine,
            ProjectionBackendEngineV1::TerminusDb
        );
        assert_eq!(terminus.support_tier, ProjectionSupportTierV1::Supported);
        assert!(terminus.named_graphs);
        assert!(terminus.native_type_system);
        assert!(terminus.immutable_history);
        assert!(terminus.branching_and_merge);
        assert!(terminus.diff_and_patch);
        assert!(terminus.schema_instance_separation);
        assert!(terminus.can_mirror_semantic_vcs_workspace());

        assert_eq!(age.backend_engine, ProjectionBackendEngineV1::ApacheAge);
        assert_eq!(age.support_tier, ProjectionSupportTierV1::Experimental);
        assert!(
            !age.constraints,
            "Apache AGE should not be elevated to supported until the schema/constraint story is stronger"
        );
    }

    #[test]
    fn projection_capability_profile_can_expose_read_only_native_queries_without_granting_mutation()
    {
        let typed_projection = ProjectionCapabilityProfileV1 {
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
        };
        assert!(typed_projection.allows_native_read_queries());
        assert!(typed_projection.requires_axiograph_mutation_authority());

        let passive_projection = ProjectionCapabilityProfileV1::default();
        assert!(!passive_projection.allows_native_read_queries());
        assert!(passive_projection.requires_axiograph_mutation_authority());
    }

    #[test]
    fn projection_semantic_commit_advances_sem_head_without_moving_main() {
        let accepted_dir = temp_test_dir("sem-projection-commit");
        ensure_layout(&accepted_dir).expect("layout");

        let base_axi = accepted_dir.join("ProjectionCommit.axi");
        fs::write(
            &base_axi,
            r#"module ProjectionCommit

schema Demo:
  object Node
  relation Edge(from: Node, to: Node)

instance I of Demo:
  Node = {a, b}
  Edge = {(from=a, to=b)}
"#,
        )
        .expect("write projection module");

        let accepted_snapshot_id =
            promote_reviewed_module(&base_axi, &accepted_dir, Some("projection-base"), "off")
                .expect("promote baseline");
        let promote_head = read_sem_head_commit_id(&accepted_dir)
            .expect("read sem head")
            .expect("promotion should seed sem head");
        let main_pointer_before = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer")
            .expect("main pointer exists");

        let manifest =
            sample_projection_manifest(accepted_snapshot_id.clone(), Some(promote_head.clone()));
        let normalized_manifest =
            normalize_projection_manifest(manifest.clone()).expect("normalize manifest");
        let commit = persist_projection_semantic_commit(
            &accepted_dir,
            &manifest,
            &ProjectionSemanticCommitOptionsV1 {
                message: Some("project typed backend".to_string()),
                ..ProjectionSemanticCommitOptionsV1::default()
            },
        )
        .expect("persist projection commit");

        assert_eq!(commit.kind, SemCommitKindV1::ProjectionMaterialization);
        assert_eq!(commit.action, "backend_projection");
        assert_eq!(
            commit.state.accepted_snapshot_id_after,
            Some(accepted_snapshot_id.clone())
        );
        assert_eq!(
            commit.state.accepted_tree_digest,
            Some(AxiDigest::new("fnv1a64:compiled-ir"))
        );
        assert_eq!(
            commit.delta.projection_manifest_refs_added,
            vec![normalized_manifest.projection_id.clone()]
        );
        assert_eq!(commit.parent_commit_id, Some(promote_head.clone()));

        let sem_head_after = read_sem_head_commit_id(&accepted_dir)
            .expect("read sem head after projection")
            .expect("projection commit should advance sem head");
        assert_eq!(sem_head_after, commit.commit_id);

        let main_pointer_after = read_sem_ref_pointer_main(&accepted_dir)
            .expect("read main pointer after projection")
            .expect("main pointer exists");
        assert_eq!(
            main_pointer_after.commit_id, main_pointer_before.commit_id,
            "projection materialization must not advance refs/heads/main"
        );

        let stored_manifest =
            read_projection_manifest(&accepted_dir, &normalized_manifest.projection_id)
                .expect("read stored projection manifest");
        assert_eq!(
            stored_manifest.projection_id,
            normalized_manifest.projection_id
        );
        assert_eq!(
            stored_manifest.source_sem_commit_id,
            Some(promote_head),
            "projection manifest should preserve semantic source lineage"
        );
        assert!(stored_manifest.projection.allows_native_read_queries());
        assert!(stored_manifest
            .projection
            .requires_axiograph_mutation_authority());

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_status_reports_refs_reconciliations_and_world_model_runs() {
        let accepted_dir = temp_test_dir("sem-status");
        ensure_layout(&accepted_dir).expect("layout");

        let commit_id = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-status-commit",
            None,
            "StatusModule",
            "fnv1a64:module-status-commit",
            "status",
        )
        .commit_id;
        write_sem_head_commit_id(&accepted_dir, &commit_id).expect("write sem head");
        let run = sample_world_model_run_record(
            WorldModelRunId::new("wmrun:demo"),
            AcceptedSnapshotId::new("fnv1a64:snap-status-wm"),
        );
        persist_world_model_run_record(&accepted_dir, &run).expect("persist world model run");
        let wm_commit_id = seed_pathdb_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-status-wm",
            Some(commit_id.clone()),
            Some(run.run_id.clone()),
        )
        .commit_id;

        persist_semantic_ref(&accepted_dir, "heads/main", &commit_id).expect("write main ref");
        persist_semantic_ref(&accepted_dir, "heads/review/demo", &commit_id)
            .expect("write review ref");
        persist_semantic_ref(&accepted_dir, "heads/wm/demo-run", &wm_commit_id)
            .expect("write wm ref");
        persist_semantic_ref(&accepted_dir, "heads/evidence/demo-source", &wm_commit_id)
            .expect("write evidence ref");
        persist_semantic_ref(&accepted_dir, "tags/status-v1", &commit_id).expect("write tag");

        let reconciliation = SemReconciliationV1 {
            version: ACCEPTED_PLANE_SEM_RECONCILIATION_VERSION_V1.to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile-status"),
            created_at_unix_secs: 1,
            base_commit_id: commit_id.clone(),
            left_commit_id: commit_id.clone(),
            right_commit_id: commit_id.clone(),
            policy: "demo".to_string(),
            source_ref_name: Some("heads/review/demo".to_string()),
            target_ref_name: Some("heads/main".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: Vec::new(),
            decisions: Vec::new(),
            certificate_refs: Vec::new(),
        };
        persist_reconciliation(&accepted_dir, &reconciliation).expect("persist reconciliation");

        let status = sem_status(&accepted_dir).expect("sem status");
        assert_eq!(status.sem_head_commit_id, Some(commit_id.clone()));
        assert_eq!(
            status
                .main_ref
                .as_ref()
                .map(|pointer| pointer.commit_id.clone()),
            Some(commit_id)
        );
        assert_eq!(status.review_refs.len(), 1);
        assert_eq!(status.evidence_refs.len(), 1);
        assert_eq!(status.world_model_refs.len(), 1);
        assert_eq!(status.tag_refs.len(), 1);
        assert_eq!(
            status.reconciliation_ids,
            vec![AxiDigest::new("fnv1a64:reconcile-status")]
        );
        assert_eq!(
            status.world_model_run_ids,
            vec![WorldModelRunId::new("wmrun:demo")]
        );
    }

    #[test]
    fn sem_merge_dry_run_builds_candidate_reconciliation_from_refs() {
        let accepted_dir = temp_test_dir("sem-merge-dry-run");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base-merge-dry-run",
            None,
            "BaseModule",
            "fnv1a64:module-base-merge-dry-run",
            "base",
        );
        let source_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-source-merge-dry-run",
            Some("fnv1a64:snap-base-merge-dry-run"),
            "SourceModule",
            "fnv1a64:module-source-merge-dry-run",
            "source",
        );
        write_sem_head_commit_id(&accepted_dir, &base_commit.commit_id).expect("reset sem head");
        let target_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-target-merge-dry-run",
            Some("fnv1a64:snap-base-merge-dry-run"),
            "TargetModule",
            "fnv1a64:module-target-merge-dry-run",
            "target",
        );

        persist_semantic_ref(&accepted_dir, "heads/review/demo", &source_commit.commit_id)
            .expect("write source ref");
        persist_semantic_ref(&accepted_dir, "heads/main", &target_commit.commit_id)
            .expect("write target ref");

        let result = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/demo",
            "heads/main",
            "semantic_merge_dry_run",
        )
        .expect("sem merge dry-run");

        assert_eq!(result.source.pointer.ref_name, "heads/review/demo");
        assert_eq!(result.source.commit.commit_id, source_commit.commit_id);
        assert_eq!(result.target.pointer.ref_name, "heads/main");
        assert_eq!(result.target.commit.commit_id, target_commit.commit_id);
        assert_eq!(
            result.reconciliation.reconciliation.left_commit_id,
            source_commit.commit_id
        );
        assert_eq!(
            result.reconciliation.reconciliation.right_commit_id,
            target_commit.commit_id
        );
        assert_eq!(
            result.reconciliation.reconciliation.base_commit_id,
            base_commit.commit_id
        );
        assert_eq!(
            result
                .reconciliation
                .reconciliation
                .source_ref_name
                .as_deref(),
            Some("heads/review/demo")
        );
        assert_eq!(
            result
                .reconciliation
                .reconciliation
                .target_ref_name
                .as_deref(),
            Some("heads/main")
        );
        assert_eq!(
            result.reconciliation.preview.reconciliation_id,
            result.reconciliation.reconciliation.reconciliation_id
        );
        assert_eq!(
            result.reconciliation.preview.evolution_preview.kind,
            "semantic_reconciliation_preview"
        );
        assert_eq!(
            result.reconciliation.stored_reconciliation_path,
            format!(
                "sem/reconciliations/{}.json",
                digest_to_filename(
                    result
                        .reconciliation
                        .reconciliation
                        .reconciliation_id
                        .as_str()
                )
            )
        );
        assert!(
            accepted_dir
                .join(&result.reconciliation.stored_reconciliation_path)
                .exists(),
            "expected persisted reconciliation at {}",
            result.reconciliation.stored_reconciliation_path
        );

        let reread = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/demo",
            "heads/main",
            "semantic_merge_dry_run",
        )
        .expect("repeat sem merge dry-run");
        assert_eq!(
            reread.reconciliation.reconciliation, result.reconciliation.reconciliation,
            "re-running the same dry-run should reuse the persisted reconciliation state"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_merge_dry_run_requires_common_semantic_ancestor() {
        let accepted_dir = temp_test_dir("sem-merge-dry-run-no-common-ancestor");
        ensure_layout(&accepted_dir).expect("layout");

        let left_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-left-no-common-ancestor",
            None,
            "LeftModule",
            "fnv1a64:module-left-no-common-ancestor",
            "left",
        );
        let right_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-right-no-common-ancestor",
            None,
            "RightModule",
            "fnv1a64:module-right-no-common-ancestor",
            "right",
        );

        persist_semantic_ref(&accepted_dir, "heads/review/demo", &left_commit.commit_id)
            .expect("write source ref");
        persist_semantic_ref(&accepted_dir, "heads/main", &right_commit.commit_id)
            .expect("write target ref");

        let err = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/demo",
            "heads/main",
            "semantic_merge_dry_run",
        )
        .expect_err("merge without common ancestor should fail");
        assert!(
            err.to_string()
                .contains("share a common persisted semantic ancestor"),
            "unexpected error: {err:#}"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_merge_materialization_persists_merge_commit_from_dry_run_reconciliation() {
        let accepted_dir = temp_test_dir("sem-merge-materialize");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base-merge-materialize",
            None,
            "BaseModule",
            "fnv1a64:module-base-merge-materialize",
            "base",
        );
        let source_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-source-merge-materialize",
            Some("fnv1a64:snap-base-merge-materialize"),
            "SourceModule",
            "fnv1a64:module-source-merge-materialize",
            "source",
        );
        write_sem_head_commit_id(&accepted_dir, &base_commit.commit_id).expect("reset sem head");
        let target_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-target-merge-materialize",
            Some("fnv1a64:snap-base-merge-materialize"),
            "TargetModule",
            "fnv1a64:module-target-merge-materialize",
            "target",
        );

        persist_semantic_ref(&accepted_dir, "heads/review/demo", &source_commit.commit_id)
            .expect("write source ref");
        persist_semantic_ref(&accepted_dir, "heads/main", &target_commit.commit_id)
            .expect("write target ref");

        let dry_run = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/demo",
            "heads/main",
            "semantic_merge_materialize",
        )
        .expect("sem merge dry-run before materialization");

        let commit = persist_reconciliation_semantic_commit(
            &accepted_dir,
            &dry_run.reconciliation.reconciliation,
            &ReconciliationSemanticCommitOptionsV1::default(),
        )
        .expect("materialize merge commit from dry-run reconciliation");

        assert_eq!(commit.kind, SemCommitKindV1::Merge);
        assert_eq!(
            commit.parent_commit_id,
            Some(target_commit.commit_id.clone())
        );
        assert_eq!(
            commit.reconciliation_id,
            Some(
                dry_run
                    .reconciliation
                    .reconciliation
                    .reconciliation_id
                    .clone()
            )
        );
        assert_eq!(
            read_sem_head_commit_id(&accepted_dir).expect("read sem head after merge"),
            Some(commit.commit_id.clone())
        );
        assert_eq!(
            read_sem_ref_pointer(&accepted_dir, "heads/main")
                .expect("read updated main ref")
                .commit_id,
            commit.commit_id
        );
        assert_eq!(
            read_reconciliation(
                &accepted_dir,
                &dry_run.reconciliation.reconciliation.reconciliation_id,
            )
            .expect("read updated reconciliation")
            .outcome_commit_id,
            Some(commit.commit_id.clone())
        );
        assert!(
            accepted_dir
                .join(
                    commit
                        .validation_report_path
                        .as_ref()
                        .expect("validation report path")
                )
                .exists(),
            "materialized merge should persist its reconciliation preview report"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_merge_dry_run_refreshes_reused_reconciliation_ref_aliases() {
        let accepted_dir = temp_test_dir("sem-merge-refresh-aliases");
        ensure_layout(&accepted_dir).expect("layout");

        let base_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-base-refresh-aliases",
            None,
            "BaseModule",
            "fnv1a64:module-base-refresh-aliases",
            "base",
        );
        let source_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-source-refresh-aliases",
            Some("fnv1a64:snap-base-refresh-aliases"),
            "SourceModule",
            "fnv1a64:module-source-refresh-aliases",
            "source",
        );
        write_sem_head_commit_id(&accepted_dir, &base_commit.commit_id).expect("reset sem head");
        let target_commit = seed_semantic_commit(
            &accepted_dir,
            "fnv1a64:snap-target-refresh-aliases",
            Some("fnv1a64:snap-base-refresh-aliases"),
            "TargetModule",
            "fnv1a64:module-target-refresh-aliases",
            "target",
        );

        persist_semantic_ref(
            &accepted_dir,
            "heads/review/first",
            &source_commit.commit_id,
        )
        .expect("write first source ref");
        persist_semantic_ref(
            &accepted_dir,
            "heads/review/second",
            &source_commit.commit_id,
        )
        .expect("write second source ref");
        persist_semantic_ref(&accepted_dir, "heads/main", &target_commit.commit_id)
            .expect("write main target ref");
        persist_semantic_ref(
            &accepted_dir,
            "heads/custom/target",
            &target_commit.commit_id,
        )
        .expect("write custom target ref");

        let first = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/first",
            "heads/main",
            "semantic_merge_alias_refresh",
        )
        .expect("first sem merge dry-run");
        assert_eq!(
            first
                .reconciliation
                .reconciliation
                .source_ref_name
                .as_deref(),
            Some("heads/review/first")
        );
        assert_eq!(
            first
                .reconciliation
                .reconciliation
                .target_ref_name
                .as_deref(),
            Some("heads/main")
        );

        let second = sem_merge_dry_run(
            &accepted_dir,
            "heads/review/second",
            "heads/custom/target",
            "semantic_merge_alias_refresh",
        )
        .expect("second sem merge dry-run");
        assert_eq!(
            second
                .reconciliation
                .reconciliation
                .source_ref_name
                .as_deref(),
            Some("heads/review/second")
        );
        assert_eq!(
            second
                .reconciliation
                .reconciliation
                .target_ref_name
                .as_deref(),
            Some("heads/custom/target")
        );
        assert_eq!(
            second
                .reconciliation
                .reconciliation
                .resolved_ref_name
                .as_deref(),
            Some("heads/custom/target")
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sem_log_follows_parent_commit_chain_from_sem_head() {
        let accepted_dir = temp_test_dir("sem-log");
        ensure_layout(&accepted_dir).expect("layout");

        let parent = SemCommitV1 {
            version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
            commit_id: AxiDigest::new("fnv1a64:parent"),
            parent_commit_id: None,
            kind: SemCommitKindV1::Promote,
            created_at_unix_secs: 1,
            author: "test".to_string(),
            message: Some("parent".to_string()),
            action: "promote".to_string(),
            provenance: SemCommitProvenanceV1::default(),
            state: SemStateRefV1::default(),
            delta: SemDeltaV1::default(),
            gate_summary: None,
            reconciliation_id: None,
            accepted_snapshot_id: AcceptedSnapshotId::new("accepted:parent"),
            accepted_parent_snapshot_id: None,
            pathdb_snapshot_id: None,
            proposal_digests: Vec::new(),
            policy: "test".to_string(),
            module_name: "ParentModule".to_string(),
            module_digest: AxiDigest::new("fnv1a64:module-parent"),
            quality_report_path: None,
            constraints_cert_path: None,
            validation_report_path: None,
            validation_ok: None,
            world_model_run_id: None,
        };
        let child = SemCommitV1 {
            version: ACCEPTED_PLANE_SEM_COMMIT_VERSION_V1.to_string(),
            commit_id: AxiDigest::new("fnv1a64:child"),
            parent_commit_id: Some(parent.commit_id.clone()),
            kind: SemCommitKindV1::Promote,
            created_at_unix_secs: 2,
            author: "test".to_string(),
            message: Some("child".to_string()),
            action: "promote".to_string(),
            provenance: SemCommitProvenanceV1::default(),
            state: SemStateRefV1::default(),
            delta: SemDeltaV1::default(),
            gate_summary: None,
            reconciliation_id: None,
            accepted_snapshot_id: AcceptedSnapshotId::new("accepted:child"),
            accepted_parent_snapshot_id: Some(AcceptedSnapshotId::new("accepted:parent")),
            pathdb_snapshot_id: None,
            proposal_digests: Vec::new(),
            policy: "test".to_string(),
            module_name: "ChildModule".to_string(),
            module_digest: AxiDigest::new("fnv1a64:module-child"),
            quality_report_path: None,
            constraints_cert_path: None,
            validation_report_path: None,
            validation_ok: None,
            world_model_run_id: None,
        };

        write_semantic_commit(&accepted_dir, &parent).expect("write parent commit");
        write_semantic_commit(&accepted_dir, &child).expect("write child commit");
        write_sem_head_commit_id(&accepted_dir, &child.commit_id).expect("write sem head");

        let commits = sem_log(&accepted_dir, 10).expect("sem log");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].commit_id, child.commit_id);
        assert_eq!(commits[1].commit_id, parent.commit_id);
    }
}
