//! Typed anchors binding semantic artifacts to repository and accepted state.

use crate::{
    CommitIdV2, ModuleIdV2, RepositoryIdV2, RevisionDigestV2, SemanticKeyV2, SnapshotIdV2, TreeIdV2,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct AcceptedSnapshotAnchorV2 {
    repository_id: RepositoryIdV2,
    snapshot_id: SnapshotIdV2,
    tree_id: TreeIdV2,
    commit_id: CommitIdV2,
}

impl AcceptedSnapshotAnchorV2 {
    pub fn new(
        repository_id: RepositoryIdV2,
        snapshot_id: SnapshotIdV2,
        tree_id: TreeIdV2,
        commit_id: CommitIdV2,
    ) -> Self {
        Self {
            repository_id,
            snapshot_id,
            tree_id,
            commit_id,
        }
    }

    pub fn repository_id(&self) -> &RepositoryIdV2 {
        &self.repository_id
    }

    pub fn snapshot_id(&self) -> &SnapshotIdV2 {
        &self.snapshot_id
    }

    pub fn tree_id(&self) -> &TreeIdV2 {
        &self.tree_id
    }

    pub fn commit_id(&self) -> &CommitIdV2 {
        &self.commit_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct AcceptedModuleAnchorV2 {
    accepted: AcceptedSnapshotAnchorV2,
    module_id: ModuleIdV2,
    revision_digest: RevisionDigestV2,
}

impl AcceptedModuleAnchorV2 {
    pub fn new(
        accepted: AcceptedSnapshotAnchorV2,
        module_id: ModuleIdV2,
        revision_digest: RevisionDigestV2,
    ) -> Self {
        Self {
            accepted,
            module_id,
            revision_digest,
        }
    }

    pub fn accepted(&self) -> &AcceptedSnapshotAnchorV2 {
        &self.accepted
    }

    pub fn module_id(&self) -> &ModuleIdV2 {
        &self.module_id
    }

    pub fn revision_digest(&self) -> &RevisionDigestV2 {
        &self.revision_digest
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct AcceptedSemanticAnchorV2 {
    module: AcceptedModuleAnchorV2,
    semantic_key: SemanticKeyV2,
}

impl AcceptedSemanticAnchorV2 {
    pub fn new(module: AcceptedModuleAnchorV2, semantic_key: SemanticKeyV2) -> Self {
        Self {
            module,
            semantic_key,
        }
    }

    pub fn module(&self) -> &AcceptedModuleAnchorV2 {
        &self.module
    }

    pub fn semantic_key(&self) -> &SemanticKeyV2 {
        &self.semantic_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_any_accepted_anchor_field_changes_serialized_anchor() {
        let repository = RepositoryIdV2::from_descriptor_bytes(b"repo");
        let module = ModuleIdV2::derive(&repository, "M");
        let revision = RevisionDigestV2::from_accepted_text("module M\n");
        let tree = TreeIdV2::from_canonical_fields(&[revision.as_str().as_bytes()]);
        let snapshot = SnapshotIdV2::from_canonical_fields(&[tree.as_str().as_bytes()]);
        let commit = CommitIdV2::from_canonical_fields(&[snapshot.as_str().as_bytes()]);
        let accepted = AcceptedSnapshotAnchorV2::new(repository, snapshot, tree, commit);
        let anchor = AcceptedModuleAnchorV2::new(accepted, module, revision);
        let json = serde_json::to_string(&anchor).expect("serialize anchor");
        assert!(json.contains("axi:repository:v2:sha256:"));
        assert!(json.contains("axi:revision:v2:sha256:"));
    }
}
