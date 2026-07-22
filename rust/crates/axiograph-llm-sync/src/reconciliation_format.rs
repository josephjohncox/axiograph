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
const MAX_RECONCILIATION_SOURCES: usize = 10_000;
const MAX_RECONCILIATION_FACTS: usize = 100_000;
const MAX_RECONCILIATION_CONFLICTS: usize = 100_000;
const MAX_RECONCILIATION_NESTED_ITEMS: usize = 500_000;
const MAX_RECONCILIATION_COUNTER: u32 = 100_000_000;

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

impl Default for ReconciliationState {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconciliationState {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            facts: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    fn validate(&self) -> std::io::Result<()> {
        for (label, actual, limit) in [
            ("sources", self.sources.len(), MAX_RECONCILIATION_SOURCES),
            ("facts", self.facts.len(), MAX_RECONCILIATION_FACTS),
            (
                "conflicts",
                self.conflicts.len(),
                MAX_RECONCILIATION_CONFLICTS,
            ),
        ] {
            if actual > limit {
                return invalid_data(format!(
                    "reconciliation {label} count {actual} exceeds {limit}"
                ));
            }
        }

        let mut nested_items = 0_usize;
        for source in &self.sources {
            nested_items = nested_items
                .checked_add(source.domain_expertise.len())
                .ok_or_else(|| invalid_data_error("nested item count overflow"))?;
            validate_weight(source.base_credibility.value(), "source credibility")?;
            for weight in source.domain_expertise.values() {
                validate_weight(weight.value(), "domain expertise")?;
            }
            if source.track_record.correct > MAX_RECONCILIATION_COUNTER
                || source.track_record.incorrect > MAX_RECONCILIATION_COUNTER
            {
                return invalid_data("source track-record counter exceeds hard limit");
            }
        }
        for fact in &self.facts {
            nested_items = nested_items
                .checked_add(fact.evidence.len())
                .and_then(|count| count.checked_add(fact.sources.len()))
                .ok_or_else(|| invalid_data_error("nested item count overflow"))?;
            validate_weight(fact.weight.value(), "fact weight")?;
            for evidence in &fact.evidence {
                validate_weight(evidence.strength.value(), "evidence strength")?;
            }
            if fact.upvotes > MAX_RECONCILIATION_COUNTER
                || fact.downvotes > MAX_RECONCILIATION_COUNTER
            {
                return invalid_data("fact vote counter exceeds hard limit");
            }
        }
        for conflict in &self.conflicts {
            if let Resolution::Merge { weights } = &conflict.resolution {
                validate_weight(weights.0, "merge weight")?;
                validate_weight(weights.1, "merge weight")?;
            }
        }
        if nested_items > MAX_RECONCILIATION_NESTED_ITEMS {
            return invalid_data(format!(
                "reconciliation nested item count {nested_items} exceeds {MAX_RECONCILIATION_NESTED_ITEMS}"
            ));
        }
        Ok(())
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        self.validate()?;
        crate::format::serialize_verified(self, VERSION, 0)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let (state, header): (Self, _) = crate::format::deserialize_verified(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if header.schema_version != VERSION {
            return invalid_data(format!(
                "unsupported reconciliation schema version {}; expected {}",
                header.schema_version, VERSION
            ));
        }
        state.validate()?;
        Ok(state)
    }

    /// Save to file
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let bytes = self.to_bytes()?;
        axiograph_security::write_file_atomic_bounded(
            path,
            bytes,
            crate::format::MAX_VERIFIED_FORMAT_BYTES,
            "reconciliation state",
        )
        .map_err(|error| invalid_data_error(error.to_string()))
    }

    /// Load from a bounded regular file without following a final symlink.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = axiograph_security::read_file_bounded(
            path,
            crate::format::MAX_VERIFIED_FORMAT_BYTES,
            "reconciliation state",
        )
        .map_err(|error| invalid_data_error(error.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

fn validate_weight(value: f32, label: &str) -> std::io::Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return invalid_data(format!("{label} must be finite and in [0, 1]"));
    }
    Ok(())
}

fn invalid_data<T>(message: impl Into<String>) -> std::io::Result<T> {
    Err(invalid_data_error(message))
}

fn invalid_data_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
