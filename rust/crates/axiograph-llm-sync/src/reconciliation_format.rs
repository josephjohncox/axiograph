//! Reconciliation persistence format (Rust runtime)
//!
//! Reconciliation state is persisted only through the shared verified CBOR
//! envelope from `format`, so runtime state gets the same header/checksum
//! validation as the other LLM-sync verified artifacts.
//!
//! ## State Format
//!
//! ```text
//! +-------------------------+
//! | VerifiedHeader (CBOR)   |
//! +-------------------------+
//! | ReconciliationState     |
//! | content (CBOR)          |
//! +-------------------------+
//! ```

use crate::reconciliation::{ResolvedConflict, SourceCredibility, WeightedFact};
use crate::{ConflictType, Resolution};
use serde::{Deserialize, Serialize};

/// Current reconciliation state schema version.
pub const VERSION: u32 = 1;

// ============================================================================
// Type Tags
// ============================================================================

pub(crate) fn conflict_type_to_byte(ct: &ConflictType) -> u8 {
    match ct {
        ConflictType::Contradiction => 0,
        ConflictType::AttributeMismatch => 1,
        ConflictType::ConfidenceConflict => 2,
        ConflictType::SchemaViolation => 3,
    }
}

pub(crate) fn resolution_to_byte(r: &Resolution) -> u8 {
    match r {
        Resolution::ReplaceOld => 0,
        Resolution::KeepOld => 1,
        Resolution::Merge { .. } => 2,
        Resolution::HumanReview => 3,
    }
}

// ============================================================================
// Full State Serialization
// ============================================================================

/// Complete reconciliation state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationState {
    pub sources: Vec<SourceCredibility>,
    pub facts: Vec<WeightedFact>,
    pub conflicts: Vec<ResolvedConflict>,
}

impl ReconciliationState {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            facts: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::format::serialize_verified(self, VERSION, 0)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let (state, header): (Self, _) = crate::format::deserialize_verified(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if header.schema_version != VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "unsupported reconciliation schema version {}; expected {}",
                    header.schema_version, VERSION
                ),
            ));
        }
        Ok(state)
    }

    /// Save to file
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)
    }

    /// Load from file
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }
}
