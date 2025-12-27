//! Verified Binary Format with CBOR, Schemas, and Checksums
//!
//! This module provides a robust serialization format that:
//! 1. Uses CBOR for compact, schema-aware encoding
//! 2. Includes checksums for integrity verification
//! 3. Supports schema evolution with version negotiation
//! 4. Validates data against the formal spec invariants (Lean-checked semantics)

#![allow(unused_imports)]

use crate::reconciliation::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

// ============================================================================
// Format Header with Checksum
// ============================================================================

/// Magic bytes for Axiograph Verified Format
pub const MAGIC: [u8; 4] = [0x41, 0x58, 0x56, 0x46]; // "AXVF"

/// Current format version (semantic versioning packed)
pub const VERSION: u32 = 0x00_01_00_00; // 1.0.0

/// Header with integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub schema_version: u32,
    pub flags: u64,
    pub content_length: u64,
    pub content_checksum: [u8; 32], // SHA-256
    pub header_checksum: [u8; 32],  // SHA-256 of header (excluding this field)
}

impl VerifiedHeader {
    pub fn new(content: &[u8], schema_version: u32, flags: u64) -> Self {
        let content_checksum = compute_sha256(content);

        let mut header = Self {
            magic: MAGIC,
            version: VERSION,
            schema_version,
            flags,
            content_length: content.len() as u64,
            content_checksum,
            header_checksum: [0u8; 32], // Placeholder
        };

        // Compute header checksum
        header.header_checksum = header.compute_header_checksum();
        header
    }

    fn compute_header_checksum(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.magic);
        hasher.update(&self.version.to_le_bytes());
        hasher.update(&self.schema_version.to_le_bytes());
        hasher.update(&self.flags.to_le_bytes());
        hasher.update(&self.content_length.to_le_bytes());
        hasher.update(&self.content_checksum);
        hasher.finalize().into()
    }

    pub fn verify(&self) -> Result<(), FormatError> {
        // Check magic
        if self.magic != MAGIC {
            return Err(FormatError::InvalidMagic);
        }

        // Check version compatibility
        if !is_version_compatible(self.version, VERSION) {
            return Err(FormatError::IncompatibleVersion {
                file_version: self.version,
                reader_version: VERSION,
            });
        }

        // Verify header checksum
        let expected = self.compute_header_checksum();
        if self.header_checksum != expected {
            return Err(FormatError::HeaderChecksumMismatch);
        }

        Ok(())
    }

    pub fn verify_content(&self, content: &[u8]) -> Result<(), FormatError> {
        if content.len() as u64 != self.content_length {
            return Err(FormatError::ContentLengthMismatch {
                expected: self.content_length,
                actual: content.len() as u64,
            });
        }

        let actual_checksum = compute_sha256(content);
        if actual_checksum != self.content_checksum {
            return Err(FormatError::ContentChecksumMismatch);
        }

        Ok(())
    }
}

fn compute_sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn is_version_compatible(file_version: u32, reader_version: u32) -> bool {
    let file_major = (file_version >> 24) & 0xFF;
    let reader_major = (reader_version >> 24) & 0xFF;

    // Major version must match
    if file_major != reader_major {
        return false;
    }

    let file_minor = (file_version >> 16) & 0xFF;
    let reader_minor = (reader_version >> 16) & 0xFF;

    // Reader must support at least the file's minor version
    reader_minor >= file_minor
}

// ============================================================================
// Feature Flags
// ============================================================================

pub mod flags {
    pub const MODAL_LOGIC: u64 = 1 << 0;
    pub const PROBABILISTIC: u64 = 1 << 1;
    pub const TEMPORAL: u64 = 1 << 2;
    pub const EPISTEMIC: u64 = 1 << 3;
    pub const DEONTIC: u64 = 1 << 4;
    pub const HOTT: u64 = 1 << 5;
    pub const COMPRESSED: u64 = 1 << 6;
    pub const ENCRYPTED: u64 = 1 << 7;
}

// ============================================================================
// CBOR Serialization
// ============================================================================

/// Serialize to verified format
pub fn serialize_verified<T: Serialize>(
    data: &T,
    schema_version: u32,
    flags: u64,
) -> Result<Vec<u8>, FormatError> {
    // Serialize content to CBOR
    let mut content = Vec::new();
    ciborium::into_writer(data, &mut content)
        .map_err(|e| FormatError::SerializationError(e.to_string()))?;

    // Create header
    let header = VerifiedHeader::new(&content, schema_version, flags);

    // Write header + content
    let mut output = Vec::new();
    ciborium::into_writer(&header, &mut output)
        .map_err(|e| FormatError::SerializationError(e.to_string()))?;
    output.extend_from_slice(&content);

    Ok(output)
}

/// Deserialize from verified format
pub fn deserialize_verified<T: for<'de> Deserialize<'de>>(
    data: &[u8],
) -> Result<(T, VerifiedHeader), FormatError> {
    let mut cursor = std::io::Cursor::new(data);

    // Read header
    let header: VerifiedHeader = ciborium::from_reader(&mut cursor)
        .map_err(|e| FormatError::DeserializationError(e.to_string()))?;

    // Verify header
    header.verify()?;

    // Read content
    let pos = cursor.position() as usize;
    let content = &data[pos..];

    // Verify content
    header.verify_content(content)?;

    // Deserialize content
    let value: T = ciborium::from_reader(content)
        .map_err(|e| FormatError::DeserializationError(e.to_string()))?;

    Ok((value, header))
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("Invalid magic bytes")]
    InvalidMagic,

    #[error("Incompatible version: file {file_version:#x}, reader {reader_version:#x}")]
    IncompatibleVersion {
        file_version: u32,
        reader_version: u32,
    },

    #[error("Header checksum mismatch")]
    HeaderChecksumMismatch,

    #[error("Content length mismatch: expected {expected}, got {actual}")]
    ContentLengthMismatch { expected: u64, actual: u64 },

    #[error("Content checksum mismatch")]
    ContentChecksumMismatch,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Schema validation error: {0}")]
    SchemaError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// ============================================================================
// Verified Types (match the Lean spec)
// ============================================================================

/// Fixed-point probability matching Lean `VProb`
/// Value is in range [0, 1_000_000]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FixedProb(u32);

impl FixedProb {
    pub const PRECISION: u32 = 1_000_000;

    pub fn new(value: u32) -> Result<Self, FormatError> {
        if value > Self::PRECISION {
            return Err(FormatError::SchemaError(format!(
                "Probability {} exceeds precision {}",
                value,
                Self::PRECISION
            )));
        }
        Ok(Self(value))
    }

    pub fn from_f64(value: f64) -> Result<Self, FormatError> {
        if value < 0.0 || value > 1.0 {
            return Err(FormatError::SchemaError(format!(
                "Probability {} out of range [0, 1]",
                value
            )));
        }
        let fixed = (value * Self::PRECISION as f64).round() as u32;
        Self::new(fixed)
    }

    pub fn to_f64(&self) -> f64 {
        self.0 as f64 / Self::PRECISION as f64
    }

    pub fn raw(&self) -> u32 {
        self.0
    }

    /// Multiply two probabilities (with proper scaling)
    pub fn multiply(&self, other: &Self) -> Self {
        let product = (self.0 as u64 * other.0 as u64) / Self::PRECISION as u64;
        Self(product.min(Self::PRECISION as u64) as u32)
    }
}

// ============================================================================
// Reconciliation State (CBOR-Serializable)
// ============================================================================

/// Reconciliation state with verified serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedReconciliationState {
    pub version: u32,
    pub sources: Vec<VerifiedSource>,
    pub facts: Vec<VerifiedFact>,
    pub conflicts: Vec<VerifiedConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedSource {
    pub id: String,
    pub credibility: FixedProb,
    pub track_record: (u32, u32), // (correct, incorrect)
    pub domain_expertise: Vec<(String, FixedProb)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedFact {
    pub id: [u8; 16], // UUID bytes
    pub weight: FixedProb,
    pub upvotes: u32,
    pub downvotes: u32,
    pub sources: Vec<String>,
    pub content_type: u8,
    pub content: Vec<u8>, // CBOR-encoded content
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedConflict {
    pub new_fact_id: [u8; 16],
    pub existing_fact_id: [u8; 16],
    pub conflict_type: u8,
    pub resolution: u8,
    pub weights: Option<(FixedProb, FixedProb)>,
    pub timestamp_ms: i64,
}

// ============================================================================
// Conversion from Runtime Types
// ============================================================================

impl VerifiedReconciliationState {
    pub fn from_runtime(
        state: &crate::reconciliation_format::ReconciliationState,
    ) -> Result<Self, FormatError> {
        let sources = state
            .sources
            .iter()
            .map(|s| {
                Ok(VerifiedSource {
                    id: s.source_id.clone(),
                    credibility: FixedProb::from_f64(s.base_credibility.value() as f64)?,
                    track_record: (s.track_record.correct, s.track_record.incorrect),
                    domain_expertise: s
                        .domain_expertise
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), FixedProb::from_f64(v.value() as f64)?)))
                        .collect::<Result<Vec<_>, FormatError>>()?,
                })
            })
            .collect::<Result<Vec<_>, FormatError>>()?;

        let facts = state
            .facts
            .iter()
            .map(|f| {
                Ok(VerifiedFact {
                    id: *f.fact_id.as_bytes(),
                    weight: FixedProb::from_f64(f.weight.value() as f64)?,
                    upvotes: f.upvotes,
                    downvotes: f.downvotes,
                    sources: f.sources.clone(),
                    content_type: 0, // TODO: proper type encoding
                    content: {
                        let mut out = Vec::new();
                        ciborium::into_writer(&f.content, &mut out)
                            .map_err(|e| FormatError::SerializationError(e.to_string()))?;
                        out
                    },
                })
            })
            .collect::<Result<Vec<_>, FormatError>>()?;

        let conflicts = state
            .conflicts
            .iter()
            .map(|c| {
                Ok(VerifiedConflict {
                    new_fact_id: *c.new_fact_id.as_bytes(),
                    existing_fact_id: *c.existing_fact_id.as_bytes(),
                    conflict_type: crate::reconciliation_format::conflict_type_to_byte(
                        &c.conflict_type,
                    ),
                    resolution: crate::reconciliation_format::resolution_to_byte(&c.resolution),
                    weights: match &c.resolution {
                        crate::Resolution::Merge { weights } => Some((
                            FixedProb::from_f64(weights.0 as f64)?,
                            FixedProb::from_f64(weights.1 as f64)?,
                        )),
                        _ => None,
                    },
                    timestamp_ms: c.timestamp.timestamp_millis(),
                })
            })
            .collect::<Result<Vec<_>, FormatError>>()?;

        Ok(Self {
            version: 1,
            sources,
            facts,
            conflicts,
        })
    }

    /// Save to file with verification
    pub fn save(&self, path: &std::path::Path) -> Result<(), FormatError> {
        let data = serialize_verified(
            self,
            1, // schema version
            flags::PROBABILISTIC,
        )?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load from file with verification
    pub fn load(path: &std::path::Path) -> Result<Self, FormatError> {
        let data = std::fs::read(path)?;
        let (state, _header) = deserialize_verified(&data)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_prob_precision() {
        let p = FixedProb::from_f64(0.123456).unwrap();
        assert!((p.to_f64() - 0.123456).abs() < 0.000001);
    }

    #[test]
    fn test_fixed_prob_bounds() {
        assert!(FixedProb::from_f64(-0.1).is_err());
        assert!(FixedProb::from_f64(1.1).is_err());
        assert!(FixedProb::from_f64(0.0).is_ok());
        assert!(FixedProb::from_f64(1.0).is_ok());
    }

    #[test]
    fn test_fixed_prob_multiply() {
        let a = FixedProb::from_f64(0.5).unwrap();
        let b = FixedProb::from_f64(0.5).unwrap();
        let c = a.multiply(&b);
        assert!((c.to_f64() - 0.25).abs() < 0.000001);
    }

    #[test]
    fn test_header_verification() {
        let content = b"test content";
        let header = VerifiedHeader::new(content, 1, 0);

        assert!(header.verify().is_ok());
        assert!(header.verify_content(content).is_ok());
        assert!(header.verify_content(b"wrong").is_err());
    }

    #[test]
    fn test_roundtrip() {
        let state = VerifiedReconciliationState {
            version: 1,
            sources: vec![VerifiedSource {
                id: "test".to_string(),
                credibility: FixedProb::from_f64(0.9).unwrap(),
                track_record: (10, 1),
                domain_expertise: vec![],
            }],
            facts: vec![],
            conflicts: vec![],
        };

        let data = serialize_verified(&state, 1, 0).unwrap();
        let (restored, _): (VerifiedReconciliationState, _) = deserialize_verified(&data).unwrap();

        assert_eq!(restored.sources.len(), 1);
        assert_eq!(restored.sources[0].id, "test");
    }
}
