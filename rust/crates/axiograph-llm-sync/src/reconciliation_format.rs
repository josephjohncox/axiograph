//! Reconciliation binary format (Rust runtime)
//!
//! This module defines the wire format for reconciliation data,
//! enabling persistence, debugging, and (eventually) certificate anchoring
//! for reconciliation decisions.
//!
//! ## Binary Format
//!
//! ```text
//! +---------------+
//! | Header (56B)  |
//! +---------------+
//! | Sources       |
//! +---------------+
//! | Weighted Facts|
//! +---------------+
//! | Conflicts     |
//! +---------------+
//! ```

#![allow(unused_imports)]

use crate::reconciliation::*;
use crate::{ConflictType, Resolution, StructuredFact};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use uuid::Uuid;

/// Magic bytes: "AXRC" (Axiograph Reconciliation)
pub const MAGIC: [u8; 4] = [0x41, 0x58, 0x52, 0x43];

/// Current format version
pub const VERSION: u32 = 1;

// ============================================================================
// Header
// ============================================================================

/// Header for reconciliation data (currently 56 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ReconciliationHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub fact_count: u32,
    pub source_count: u32,
    pub conflict_count: u32,
    pub _reserved: u32,
    pub weighted_fact_offset: u64,
    pub source_credibility_offset: u64,
    pub resolved_conflict_offset: u64,
    pub total_size: u64,
}

impl ReconciliationHeader {
    pub const SIZE: usize = std::mem::size_of::<ReconciliationHeader>();

    pub fn new() -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            fact_count: 0,
            source_count: 0,
            conflict_count: 0,
            _reserved: 0,
            weighted_fact_offset: Self::SIZE as u64,
            source_credibility_offset: 0,
            resolved_conflict_offset: 0,
            total_size: Self::SIZE as u64,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC && self.version <= VERSION
    }

    pub fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&self.magic)?;
        w.write_u32::<LittleEndian>(self.version)?;
        w.write_u32::<LittleEndian>(self.fact_count)?;
        w.write_u32::<LittleEndian>(self.source_count)?;
        w.write_u32::<LittleEndian>(self.conflict_count)?;
        w.write_u32::<LittleEndian>(self._reserved)?;
        w.write_u64::<LittleEndian>(self.weighted_fact_offset)?;
        w.write_u64::<LittleEndian>(self.source_credibility_offset)?;
        w.write_u64::<LittleEndian>(self.resolved_conflict_offset)?;
        w.write_u64::<LittleEndian>(self.total_size)?;
        Ok(())
    }

    pub fn read<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        Ok(Self {
            magic,
            version: r.read_u32::<LittleEndian>()?,
            fact_count: r.read_u32::<LittleEndian>()?,
            source_count: r.read_u32::<LittleEndian>()?,
            conflict_count: r.read_u32::<LittleEndian>()?,
            _reserved: r.read_u32::<LittleEndian>()?,
            weighted_fact_offset: r.read_u64::<LittleEndian>()?,
            source_credibility_offset: r.read_u64::<LittleEndian>()?,
            resolved_conflict_offset: r.read_u64::<LittleEndian>()?,
            total_size: r.read_u64::<LittleEndian>()?,
        })
    }
}

// ============================================================================
// Type Tags
// ============================================================================

/// Evidence type as byte
pub fn evidence_type_to_byte(et: &EvidenceType) -> u8 {
    match et {
        EvidenceType::Supports => 0,
        EvidenceType::Refutes => 1,
        EvidenceType::Neutral => 2,
        EvidenceType::Clarifies => 3,
    }
}

pub fn byte_to_evidence_type(b: u8) -> Option<EvidenceType> {
    match b {
        0 => Some(EvidenceType::Supports),
        1 => Some(EvidenceType::Refutes),
        2 => Some(EvidenceType::Neutral),
        3 => Some(EvidenceType::Clarifies),
        _ => None,
    }
}

/// Conflict type as byte
pub fn conflict_type_to_byte(ct: &ConflictType) -> u8 {
    match ct {
        ConflictType::Contradiction => 0,
        ConflictType::AttributeMismatch => 1,
        ConflictType::ConfidenceConflict => 2,
        ConflictType::SchemaViolation => 3,
    }
}

pub fn byte_to_conflict_type(b: u8) -> Option<ConflictType> {
    match b {
        0 => Some(ConflictType::Contradiction),
        1 => Some(ConflictType::AttributeMismatch),
        2 => Some(ConflictType::ConfidenceConflict),
        3 => Some(ConflictType::SchemaViolation),
        _ => None,
    }
}

/// Resolution type as byte
pub fn resolution_to_byte(r: &Resolution) -> u8 {
    match r {
        Resolution::ReplaceOld => 0,
        Resolution::KeepOld => 1,
        Resolution::Merge { .. } => 2,
        Resolution::HumanReview => 3,
    }
}

pub fn byte_to_resolution(b: u8, w1: Option<f32>, w2: Option<f32>) -> Option<Resolution> {
    match b {
        0 => Some(Resolution::ReplaceOld),
        1 => Some(Resolution::KeepOld),
        2 => Some(Resolution::Merge {
            weights: (w1?, w2?),
        }),
        3 => Some(Resolution::HumanReview),
        _ => None,
    }
}

// ============================================================================
// Serialization Helpers
// ============================================================================

fn write_string<W: Write>(w: &mut W, s: &str) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    w.write_u16::<LittleEndian>(bytes.len() as u16)?;
    w.write_all(bytes)?;
    Ok(())
}

fn read_string<R: Read>(r: &mut R) -> std::io::Result<String> {
    let len = r.read_u16::<LittleEndian>()? as usize;
    let mut bytes = vec![0u8; len];
    r.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn write_uuid<W: Write>(w: &mut W, uuid: &Uuid) -> std::io::Result<()> {
    w.write_all(uuid.as_bytes())
}

fn read_uuid<R: Read>(r: &mut R) -> std::io::Result<Uuid> {
    let mut bytes = [0u8; 16];
    r.read_exact(&mut bytes)?;
    Ok(Uuid::from_bytes(bytes))
}

// ============================================================================
// Source Credibility Serialization
// ============================================================================

impl SourceCredibility {
    pub fn write_binary<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        write_string(w, &self.source_id)?;
        w.write_f32::<LittleEndian>(self.base_credibility.value())?;
        w.write_u32::<LittleEndian>(self.track_record.correct)?;
        w.write_u32::<LittleEndian>(self.track_record.incorrect)?;

        // Domain expertise
        w.write_u16::<LittleEndian>(self.domain_expertise.len() as u16)?;
        for (domain, weight) in &self.domain_expertise {
            write_string(w, domain)?;
            w.write_f32::<LittleEndian>(weight.value())?;
        }

        Ok(())
    }

    pub fn read_binary<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let source_id = read_string(r)?;
        let base_credibility = Weight::new(r.read_f32::<LittleEndian>()?);
        let correct = r.read_u32::<LittleEndian>()?;
        let incorrect = r.read_u32::<LittleEndian>()?;

        let domain_count = r.read_u16::<LittleEndian>()? as usize;
        let mut domain_expertise = std::collections::HashMap::new();
        for _ in 0..domain_count {
            let domain = read_string(r)?;
            let weight = Weight::new(r.read_f32::<LittleEndian>()?);
            domain_expertise.insert(domain, weight);
        }

        Ok(Self {
            source_id,
            base_credibility,
            domain_expertise,
            track_record: TrackRecord { correct, incorrect },
        })
    }
}

// ============================================================================
// Evidence Serialization
// ============================================================================

impl Evidence {
    pub fn write_binary<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_u8(evidence_type_to_byte(&self.evidence_type))?;
        w.write_f32::<LittleEndian>(self.strength.value())?;
        write_string(w, &self.source_id)?;
        write_string(w, &self.description)?;
        // Timestamp as unix millis
        w.write_i64::<LittleEndian>(self.timestamp.timestamp_millis())?;
        write_uuid(w, &self.id)?;
        Ok(())
    }

    pub fn read_binary<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let evidence_type = byte_to_evidence_type(r.read_u8()?).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid evidence type")
        })?;
        let strength = Weight::new(r.read_f32::<LittleEndian>()?);
        let source_id = read_string(r)?;
        let description = read_string(r)?;
        let timestamp_millis = r.read_i64::<LittleEndian>()?;
        let timestamp = chrono::DateTime::from_timestamp_millis(timestamp_millis)
            .unwrap_or_else(chrono::Utc::now);
        let id = read_uuid(r)?;

        Ok(Self {
            id,
            source_id,
            evidence_type,
            strength,
            timestamp,
            description,
        })
    }
}

// ============================================================================
// Weighted Fact Serialization
// ============================================================================

impl WeightedFact {
    pub fn write_binary<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        write_uuid(w, &self.fact_id)?;
        w.write_f32::<LittleEndian>(self.weight.value())?;
        w.write_u32::<LittleEndian>(self.upvotes)?;
        w.write_u32::<LittleEndian>(self.downvotes)?;

        // Timestamps
        w.write_i64::<LittleEndian>(self.created_at.timestamp_millis())?;
        w.write_i64::<LittleEndian>(self.updated_at.timestamp_millis())?;

        // Sources
        w.write_u16::<LittleEndian>(self.sources.len() as u16)?;
        for source in &self.sources {
            write_string(w, source)?;
        }

        // Evidence
        w.write_u16::<LittleEndian>(self.evidence.len() as u16)?;
        for ev in &self.evidence {
            ev.write_binary(w)?;
        }

        // Content (as JSON for flexibility)
        let content_json = serde_json::to_string(&self.content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_string(w, &content_json)?;

        Ok(())
    }

    pub fn read_binary<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let fact_id = read_uuid(r)?;
        let weight = Weight::new(r.read_f32::<LittleEndian>()?);
        let upvotes = r.read_u32::<LittleEndian>()?;
        let downvotes = r.read_u32::<LittleEndian>()?;

        let created_millis = r.read_i64::<LittleEndian>()?;
        let updated_millis = r.read_i64::<LittleEndian>()?;
        let created_at = chrono::DateTime::from_timestamp_millis(created_millis)
            .unwrap_or_else(chrono::Utc::now);
        let updated_at = chrono::DateTime::from_timestamp_millis(updated_millis)
            .unwrap_or_else(chrono::Utc::now);

        let source_count = r.read_u16::<LittleEndian>()? as usize;
        let mut sources = Vec::with_capacity(source_count);
        for _ in 0..source_count {
            sources.push(read_string(r)?);
        }

        let evidence_count = r.read_u16::<LittleEndian>()? as usize;
        let mut evidence = Vec::with_capacity(evidence_count);
        for _ in 0..evidence_count {
            evidence.push(Evidence::read_binary(r)?);
        }

        let content_json = read_string(r)?;
        let content: StructuredFact = serde_json::from_str(&content_json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Self {
            fact_id,
            content,
            weight,
            evidence,
            sources,
            created_at,
            updated_at,
            upvotes,
            downvotes,
        })
    }
}

// ============================================================================
// Resolved Conflict Serialization
// ============================================================================

impl ResolvedConflict {
    pub fn write_binary<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        write_uuid(w, &self.new_fact_id)?;
        write_uuid(w, &self.existing_fact_id)?;
        w.write_u8(conflict_type_to_byte(&self.conflict_type))?;
        w.write_u8(resolution_to_byte(&self.resolution))?;

        // Write merge weights if applicable
        if let Resolution::Merge { weights } = &self.resolution {
            w.write_f32::<LittleEndian>(weights.0)?;
            w.write_f32::<LittleEndian>(weights.1)?;
        } else {
            w.write_f32::<LittleEndian>(0.0)?;
            w.write_f32::<LittleEndian>(0.0)?;
        }

        w.write_i64::<LittleEndian>(self.timestamp.timestamp_millis())?;

        Ok(())
    }

    pub fn read_binary<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let new_fact_id = read_uuid(r)?;
        let existing_fact_id = read_uuid(r)?;
        let conflict_type = byte_to_conflict_type(r.read_u8()?).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid conflict type")
        })?;
        let resolution_byte = r.read_u8()?;
        let w1 = r.read_f32::<LittleEndian>()?;
        let w2 = r.read_f32::<LittleEndian>()?;
        let resolution =
            byte_to_resolution(resolution_byte, Some(w1), Some(w2)).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid resolution")
            })?;
        let timestamp_millis = r.read_i64::<LittleEndian>()?;
        let timestamp = chrono::DateTime::from_timestamp_millis(timestamp_millis)
            .unwrap_or_else(chrono::Utc::now);

        Ok(Self {
            new_fact_id,
            existing_fact_id,
            conflict_type,
            resolution,
            timestamp,
        })
    }
}

// ============================================================================
// Full State Serialization
// ============================================================================

/// Complete reconciliation state
#[derive(Debug, Clone)]
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
        let mut buf = Vec::new();

        // Write placeholder header
        let mut header = ReconciliationHeader::new();
        header.fact_count = self.facts.len() as u32;
        header.source_count = self.sources.len() as u32;
        header.conflict_count = self.conflicts.len() as u32;
        header.write(&mut buf)?;

        // Sources
        header.source_credibility_offset = buf.len() as u64;
        for source in &self.sources {
            source.write_binary(&mut buf)?;
        }

        // Facts
        header.weighted_fact_offset = buf.len() as u64;
        for fact in &self.facts {
            fact.write_binary(&mut buf)?;
        }

        // Conflicts
        header.resolved_conflict_offset = buf.len() as u64;
        for conflict in &self.conflicts {
            conflict.write_binary(&mut buf)?;
        }

        header.total_size = buf.len() as u64;

        // Rewrite header with correct offsets
        let mut cursor = Cursor::new(&mut buf[..ReconciliationHeader::SIZE]);
        header.write(&mut cursor)?;

        Ok(buf)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let mut cursor = Cursor::new(bytes);

        let header = ReconciliationHeader::read(&mut cursor)?;
        if !header.is_valid() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid reconciliation file header",
            ));
        }

        // Read sources
        cursor.set_position(header.source_credibility_offset);
        let mut sources = Vec::with_capacity(header.source_count as usize);
        for _ in 0..header.source_count {
            sources.push(SourceCredibility::read_binary(&mut cursor)?);
        }

        // Read facts
        cursor.set_position(header.weighted_fact_offset);
        let mut facts = Vec::with_capacity(header.fact_count as usize);
        for _ in 0..header.fact_count {
            facts.push(WeightedFact::read_binary(&mut cursor)?);
        }

        // Read conflicts
        cursor.set_position(header.resolved_conflict_offset);
        let mut conflicts = Vec::with_capacity(header.conflict_count as usize);
        for _ in 0..header.conflict_count {
            conflicts.push(ResolvedConflict::read_binary(&mut cursor)?);
        }

        Ok(Self {
            sources,
            facts,
            conflicts,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    #[test]
    fn test_header_roundtrip() {
        let mut header = ReconciliationHeader::new();
        header.fact_count = 42;
        header.source_count = 5;

        let mut buf = Vec::new();
        header.write(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let restored = ReconciliationHeader::read(&mut cursor).unwrap();

        assert!(restored.is_valid());
        assert_eq!(restored.fact_count, 42);
        assert_eq!(restored.source_count, 5);
    }

    #[test]
    fn test_source_credibility_roundtrip() {
        let mut source = SourceCredibility::new("expert", 0.95);
        source
            .domain_expertise
            .insert("machining".to_string(), Weight::new(0.99));
        source.track_record = TrackRecord {
            correct: 100,
            incorrect: 5,
        };

        let mut buf = Vec::new();
        source.write_binary(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let restored = SourceCredibility::read_binary(&mut cursor).unwrap();

        assert_eq!(restored.source_id, "expert");
        assert!((restored.base_credibility.value() - 0.95).abs() < 0.001);
        assert_eq!(restored.track_record.correct, 100);
    }

    #[test]
    fn test_weighted_fact_roundtrip() {
        let fact = WeightedFact::new(
            Uuid::new_v4(),
            StructuredFact::Entity {
                entity_type: "Material".to_string(),
                name: "Steel".to_string(),
                attributes: HashMap::new(),
            },
            0.85,
        );

        let mut buf = Vec::new();
        fact.write_binary(&mut buf).unwrap();

        let mut cursor = Cursor::new(&buf);
        let restored = WeightedFact::read_binary(&mut cursor).unwrap();

        assert_eq!(restored.fact_id, fact.fact_id);
        assert!((restored.weight.value() - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_full_state_roundtrip() {
        let mut state = ReconciliationState::new();

        state.sources.push(SourceCredibility::new("expert", 0.9));
        state.facts.push(WeightedFact::new(
            Uuid::new_v4(),
            StructuredFact::Entity {
                entity_type: "Material".to_string(),
                name: "Titanium".to_string(),
                attributes: HashMap::new(),
            },
            0.8,
        ));

        let bytes = state.to_bytes().unwrap();
        let restored = ReconciliationState::from_bytes(&bytes).unwrap();

        assert_eq!(restored.sources.len(), 1);
        assert_eq!(restored.facts.len(), 1);
    }

    #[test]
    fn test_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reconciliation.axrc");

        let mut state = ReconciliationState::new();
        state.sources.push(SourceCredibility::new("test", 0.5));

        state.save(&path).unwrap();
        let restored = ReconciliationState::load(&path).unwrap();

        assert_eq!(restored.sources.len(), 1);
        assert_eq!(restored.sources[0].source_id, "test");
    }
}
