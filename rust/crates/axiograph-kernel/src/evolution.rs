//! Explicit semantic comparability across exact module revisions.

use crate::{IdentityError, ModuleIdV2, RevisionDigestV2, SemanticKeyV2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(deny_unknown_fields)]
pub struct RevisionScopedSemanticKeyV2 {
    pub module_id: ModuleIdV2,
    pub revision: RevisionDigestV2,
    pub semantic_key: SemanticKeyV2,
}

impl RevisionScopedSemanticKeyV2 {
    pub fn new(
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        semantic_key: SemanticKeyV2,
    ) -> Self {
        Self {
            module_id,
            revision,
            semantic_key,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReindexOperationV2 {
    Preserve,
    Rename,
    Split,
    Merge,
    Drop,
}

impl ReindexOperationV2 {
    const fn label(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Rename => "rename",
            Self::Split => "split",
            Self::Merge => "merge",
            Self::Drop => "drop",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReindexEntryV2 {
    pub operation: ReindexOperationV2,
    pub sources: Vec<SemanticKeyV2>,
    pub targets: Vec<SemanticKeyV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevisionReindexMapV2 {
    pub module_id: ModuleIdV2,
    pub source_revision: RevisionDigestV2,
    pub target_revision: RevisionDigestV2,
    pub entries: Vec<ReindexEntryV2>,
}

impl RevisionReindexMapV2 {
    pub fn checked(
        module_id: ModuleIdV2,
        source_revision: RevisionDigestV2,
        target_revision: RevisionDigestV2,
        entries: Vec<ReindexEntryV2>,
    ) -> Result<Self, IdentityError> {
        if source_revision == target_revision {
            return Err(IdentityError::SameRevision);
        }

        let mut source_keys = BTreeSet::new();
        let mut target_keys = BTreeSet::new();
        for entry in &entries {
            validate_cardinality(entry)?;
            for key in &entry.sources {
                if !source_keys.insert(key.clone()) {
                    return Err(IdentityError::DuplicateEvolutionSource(key.to_string()));
                }
            }
            for key in &entry.targets {
                if !target_keys.insert(key.clone()) {
                    return Err(IdentityError::DuplicateEvolutionTarget(key.to_string()));
                }
            }
        }

        Ok(Self {
            module_id,
            source_revision,
            target_revision,
            entries,
        })
    }

    pub fn source_ref(&self, key: SemanticKeyV2) -> RevisionScopedSemanticKeyV2 {
        RevisionScopedSemanticKeyV2::new(self.module_id.clone(), self.source_revision.clone(), key)
    }

    pub fn target_ref(&self, key: SemanticKeyV2) -> RevisionScopedSemanticKeyV2 {
        RevisionScopedSemanticKeyV2::new(self.module_id.clone(), self.target_revision.clone(), key)
    }
}

fn validate_cardinality(entry: &ReindexEntryV2) -> Result<(), IdentityError> {
    let sources = entry.sources.len();
    let targets = entry.targets.len();
    let valid = match entry.operation {
        ReindexOperationV2::Preserve | ReindexOperationV2::Rename => sources == 1 && targets == 1,
        ReindexOperationV2::Split => sources == 1 && targets >= 2,
        ReindexOperationV2::Merge => sources >= 2 && targets == 1,
        ReindexOperationV2::Drop => sources == 1 && targets == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(IdentityError::InvalidEvolutionCardinality {
            operation: entry.operation.label(),
            sources,
            targets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RepositoryIdV2, RevisionDigestV2};

    fn fixture() -> (ModuleIdV2, RevisionDigestV2, RevisionDigestV2) {
        let repository = RepositoryIdV2::from_descriptor_bytes(b"repo");
        let module = ModuleIdV2::derive(&repository, "M");
        (
            module,
            RevisionDigestV2::from_accepted_text("module M\n"),
            RevisionDigestV2::from_accepted_text("module M\n\n"),
        )
    }

    #[test]
    fn checked_map_rejects_ambiguous_reuse_and_bad_cardinality() {
        let (module, source, target) = fixture();
        let a = SemanticKeyV2::derive(&module, "object", "A");
        let b = SemanticKeyV2::derive(&module, "object", "B");
        let duplicate = vec![
            ReindexEntryV2 {
                operation: ReindexOperationV2::Preserve,
                sources: vec![a.clone()],
                targets: vec![a.clone()],
            },
            ReindexEntryV2 {
                operation: ReindexOperationV2::Rename,
                sources: vec![a],
                targets: vec![b],
            },
        ];
        assert!(matches!(
            RevisionReindexMapV2::checked(module, source, target, duplicate),
            Err(IdentityError::DuplicateEvolutionSource(_))
        ));
    }

    #[test]
    fn scoped_refs_reject_cross_revision_equality() {
        let (module, source, target) = fixture();
        let key = SemanticKeyV2::derive(&module, "object", "A");
        let map = RevisionReindexMapV2::checked(
            module,
            source,
            target,
            vec![ReindexEntryV2 {
                operation: ReindexOperationV2::Preserve,
                sources: vec![key.clone()],
                targets: vec![key.clone()],
            }],
        )
        .expect("valid preserve map");
        assert_ne!(map.source_ref(key.clone()), map.target_ref(key));
    }
}
