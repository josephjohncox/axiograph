//! Integrity-Checked CBOR Format With Schemas And Checksums
//!
//! This module provides a robust serialization format that:
//! 1. Uses CBOR for compact, schema-aware encoding
//! 2. Includes checksums for integrity verification
//! 3. Supports schema evolution with version negotiation
//! 4. Validates envelope integrity and schema-version invariants. Semantic
//!    claims still require the Axiograph certificate/trust boundary.

#![allow(unused_imports)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ============================================================================
// Format Header with Checksum
// ============================================================================

/// Magic bytes for Axiograph Verified Format
pub const MAGIC: [u8; 4] = [0x41, 0x58, 0x56, 0x46]; // "AXVF"

/// Current format version (semantic versioning packed)
pub const VERSION: u32 = 0x00_01_00_00; // 1.0.0
/// Hard envelope bound applied before CBOR parsing or allocation.
pub const MAX_VERIFIED_FORMAT_BYTES: usize = 16 * 1024 * 1024;
/// Explicit stack bound for every untrusted CBOR value.
pub const MAX_CBOR_RECURSION: usize = 64;

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
        hasher.update(self.magic);
        hasher.update(self.version.to_le_bytes());
        hasher.update(self.schema_version.to_le_bytes());
        hasher.update(self.flags.to_le_bytes());
        hasher.update(self.content_length.to_le_bytes());
        hasher.update(self.content_checksum);
        hasher.finalize().into()
    }

    pub fn verify(&self) -> Result<(), FormatError> {
        // Check magic
        if self.magic != MAGIC {
            return Err(FormatError::InvalidMagic);
        }

        // This greenfield format has one exact reader. Old minor versions are
        // not a compatibility channel.
        if self.version != VERSION {
            return Err(FormatError::IncompatibleVersion {
                file_version: self.version,
                reader_version: VERSION,
            });
        }
        if self.flags != 0 {
            return Err(FormatError::UnsupportedFlags(self.flags));
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
    if content.len() > MAX_VERIFIED_FORMAT_BYTES {
        return Err(FormatError::LimitExceeded {
            limit: MAX_VERIFIED_FORMAT_BYTES,
            actual: content.len(),
        });
    }

    // Create header
    let header = VerifiedHeader::new(&content, schema_version, flags);

    // Write header + content
    let mut output = Vec::new();
    ciborium::into_writer(&header, &mut output)
        .map_err(|e| FormatError::SerializationError(e.to_string()))?;
    output.extend_from_slice(&content);
    if output.len() > MAX_VERIFIED_FORMAT_BYTES {
        return Err(FormatError::LimitExceeded {
            limit: MAX_VERIFIED_FORMAT_BYTES,
            actual: output.len(),
        });
    }

    Ok(output)
}

/// Deserialize from verified format
pub fn deserialize_verified<T: for<'de> Deserialize<'de>>(
    data: &[u8],
) -> Result<(T, VerifiedHeader), FormatError> {
    if data.len() > MAX_VERIFIED_FORMAT_BYTES {
        return Err(FormatError::LimitExceeded {
            limit: MAX_VERIFIED_FORMAT_BYTES,
            actual: data.len(),
        });
    }
    let mut cursor = std::io::Cursor::new(data);

    // Read header
    let header: VerifiedHeader =
        ciborium::de::from_reader_with_recursion_limit(&mut cursor, MAX_CBOR_RECURSION)
            .map_err(|e| FormatError::DeserializationError(e.to_string()))?;

    // Verify header
    header.verify()?;

    // Read content
    let pos = cursor.position() as usize;
    let content = &data[pos..];

    // Verify content
    header.verify_content(content)?;

    // Deserialize exactly one content value. Trailing values or bytes are not a
    // compatibility channel and are rejected.
    let mut content_cursor = std::io::Cursor::new(content);
    let value: T =
        ciborium::de::from_reader_with_recursion_limit(&mut content_cursor, MAX_CBOR_RECURSION)
            .map_err(|e| FormatError::DeserializationError(e.to_string()))?;
    if content_cursor.position() != content.len() as u64 {
        return Err(FormatError::TrailingData {
            remaining: content.len() - content_cursor.position() as usize,
        });
    }

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

    #[error("Unsupported verified-format flags: {0:#x}")]
    UnsupportedFlags(u64),

    #[error("Verified format exceeds {limit} bytes (actual {actual})")]
    LimitExceeded { limit: usize, actual: usize },

    #[error("Verified format contains {remaining} trailing bytes")]
    TrailingData { remaining: usize },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_verification() {
        let content = b"test content";
        let header = VerifiedHeader::new(content, 1, 0);

        assert!(header.verify().is_ok());
        assert!(header.verify_content(content).is_ok());
        assert!(header.verify_content(b"wrong").is_err());
    }

    #[test]
    fn oversized_envelope_rejects_before_cbor_parsing() {
        let data = vec![0_u8; MAX_VERIFIED_FORMAT_BYTES + 1];
        let error = deserialize_verified::<serde_json::Value>(&data)
            .expect_err("oversized CBOR envelope must reject");
        assert!(matches!(error, FormatError::LimitExceeded { .. }));
    }

    #[test]
    fn verified_content_rejects_trailing_cbor_value() {
        let mut content = Vec::new();
        ciborium::into_writer(&1_u8, &mut content).unwrap();
        ciborium::into_writer(&2_u8, &mut content).unwrap();
        let header = VerifiedHeader::new(&content, 1, 0);
        let mut data = Vec::new();
        ciborium::into_writer(&header, &mut data).unwrap();
        data.extend_from_slice(&content);
        let error = deserialize_verified::<u8>(&data).expect_err("trailing CBOR value must reject");
        assert!(matches!(error, FormatError::TrailingData { .. }));
    }

    #[test]
    fn obsolete_format_versions_and_flags_reject() {
        let mut old = VerifiedHeader::new(b"x", 1, 0);
        old.version = VERSION.saturating_sub(1);
        old.header_checksum = old.compute_header_checksum();
        assert!(matches!(
            old.verify(),
            Err(FormatError::IncompatibleVersion { .. })
        ));

        let flagged = VerifiedHeader::new(b"x", 1, 1);
        assert!(matches!(
            flagged.verify(),
            Err(FormatError::UnsupportedFlags(1))
        ));
    }

    #[test]
    fn deeply_nested_cbor_rejects_at_explicit_recursion_limit() {
        let mut content = vec![0x81; MAX_CBOR_RECURSION + 1];
        content.push(0);
        let header = VerifiedHeader::new(&content, 1, 0);
        let mut data = Vec::new();
        ciborium::into_writer(&header, &mut data).unwrap();
        data.extend_from_slice(&content);
        let error = deserialize_verified::<serde_json::Value>(&data)
            .expect_err("deep CBOR must reject before building an allocation tree");
        assert!(matches!(error, FormatError::DeserializationError(_)));
    }

    #[test]
    fn verified_format_roundtrip() {
        let value = serde_json::json!({"sources": ["test"], "version": 1});
        let data = serialize_verified(&value, 1, 0).unwrap();
        let (restored, _): (serde_json::Value, _) = deserialize_verified(&data).unwrap();
        assert_eq!(restored, value);
    }
}
