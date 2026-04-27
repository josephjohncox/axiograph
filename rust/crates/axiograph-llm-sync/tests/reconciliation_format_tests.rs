//! End-to-end tests for reconciliation persistence and format stability
//!
//! These tests verify that:
//! 1. Rust can serialize reconciliation state
//! 2. The binary format is stable
//! 3. Weights and evidence are preserved through roundtrip
//! 4. Invalid/corrupted inputs are rejected

use axiograph_llm_sync::reconciliation::*;
use axiograph_llm_sync::reconciliation_format::*;
use axiograph_llm_sync::format::{
    deserialize_verified, MAGIC as VERIFIED_MAGIC, VERSION as VERIFIED_FORMAT_VERSION,
};
use axiograph_llm_sync::*;
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Format Validation Tests
// ============================================================================

#[test]
fn test_verified_envelope_magic() {
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();

    let (_state, header): (ReconciliationState, _) = deserialize_verified(&bytes).unwrap();
    assert_eq!(header.magic, VERIFIED_MAGIC);
    assert_ne!(&bytes[0..4], b"AXRC");
}

#[test]
fn test_verified_envelope_versions() {
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();

    let (_state, header): (ReconciliationState, _) = deserialize_verified(&bytes).unwrap();
    assert_eq!(header.version, VERIFIED_FORMAT_VERSION);
    assert_eq!(header.schema_version, VERSION);
}

#[test]
fn test_verified_envelope_checks_content() {
    let mut state = ReconciliationState::new();
    state.sources.push(SourceCredibility::new("test", 0.5));
    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: "Test".to_string(),
            attributes: HashMap::new(),
        },
        0.5,
    ));

    let bytes = state.to_bytes().unwrap();
    let mut corrupted = bytes.clone();
    let last = corrupted.last_mut().unwrap();
    *last ^= 0xFF;

    assert!(ReconciliationState::from_bytes(&bytes).is_ok());
    assert!(ReconciliationState::from_bytes(&corrupted).is_err());
}

// ============================================================================
// Weight Precision Tests
// ============================================================================

#[test]
fn test_weight_precision_preservation() {
    let test_weights = [0.0, 0.123456, 0.5, 0.999999, 1.0];

    for &w in &test_weights {
        let source = SourceCredibility::new("test", w);

        let mut buf = Vec::new();
        source.write_binary(&mut buf).unwrap();

        let mut cursor = std::io::Cursor::new(&buf);
        let restored = SourceCredibility::read_binary(&mut cursor).unwrap();

        let diff = (restored.base_credibility.value() - w).abs();
        assert!(
            diff < 0.0001,
            "Weight {} not preserved (got {})",
            w,
            restored.base_credibility.value()
        );
    }
}

#[test]
fn test_weighted_fact_weight_preservation() {
    let weights = [0.1, 0.5, 0.9, 0.95, 0.99];

    for &w in &weights {
        let fact = WeightedFact::new(
            Uuid::new_v4(),
            StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "Test".to_string(),
                attributes: HashMap::new(),
            },
            w,
        );

        let mut buf = Vec::new();
        fact.write_binary(&mut buf).unwrap();

        let mut cursor = std::io::Cursor::new(&buf);
        let restored = WeightedFact::read_binary(&mut cursor).unwrap();

        let diff = (restored.weight.value() - w).abs();
        assert!(diff < 0.0001, "Fact weight {} not preserved", w);
    }
}

// ============================================================================
// Evidence Preservation Tests
// ============================================================================

#[test]
fn test_evidence_type_roundtrip() {
    let types = [
        EvidenceType::Supports,
        EvidenceType::Refutes,
        EvidenceType::Neutral,
        EvidenceType::Clarifies,
    ];

    for et in types {
        let byte = evidence_type_to_byte(&et);
        let restored = byte_to_evidence_type(byte).unwrap();

        // Compare by converting back to byte
        assert_eq!(evidence_type_to_byte(&restored), byte);
    }
}

#[test]
fn test_evidence_serialization() {
    let evidence = Evidence {
        id: Uuid::new_v4(),
        source_id: "expert_machinist".to_string(),
        evidence_type: EvidenceType::Supports,
        strength: Weight::new(0.95),
        timestamp: Utc::now(),
        description: "Based on 20 years of experience".to_string(),
    };

    let mut buf = Vec::new();
    evidence.write_binary(&mut buf).unwrap();

    let mut cursor = std::io::Cursor::new(&buf);
    let restored = Evidence::read_binary(&mut cursor).unwrap();

    assert_eq!(restored.id, evidence.id);
    assert_eq!(restored.source_id, "expert_machinist");
    assert!((restored.strength.value() - 0.95).abs() < 0.0001);
}

// ============================================================================
// Conflict Resolution Preservation Tests
// ============================================================================

#[test]
fn test_conflict_type_roundtrip() {
    let types = [
        ConflictType::Contradiction,
        ConflictType::AttributeMismatch,
        ConflictType::ConfidenceConflict,
        ConflictType::SchemaViolation,
    ];

    for ct in types {
        let byte = conflict_type_to_byte(&ct);
        let restored = byte_to_conflict_type(byte).unwrap();
        assert_eq!(conflict_type_to_byte(&restored), byte);
    }
}

#[test]
fn test_resolution_roundtrip() {
    let resolutions = [
        Resolution::ReplaceOld,
        Resolution::KeepOld,
        Resolution::Merge {
            weights: (0.6, 0.4),
        },
        Resolution::HumanReview,
    ];

    for res in resolutions {
        let byte = resolution_to_byte(&res);
        let (w1, w2) = if let Resolution::Merge { weights } = &res {
            (Some(weights.0), Some(weights.1))
        } else {
            (Some(0.0), Some(0.0))
        };

        let restored = byte_to_resolution(byte, w1, w2).unwrap();
        assert_eq!(resolution_to_byte(&restored), byte);
    }
}

#[test]
fn test_resolved_conflict_serialization() {
    let conflict = ResolvedConflict {
        new_fact_id: Uuid::new_v4(),
        existing_fact_id: Uuid::new_v4(),
        conflict_type: ConflictType::AttributeMismatch,
        resolution: Resolution::Merge {
            weights: (0.7, 0.3),
        },
        timestamp: Utc::now(),
    };

    let mut buf = Vec::new();
    conflict.write_binary(&mut buf).unwrap();

    let mut cursor = std::io::Cursor::new(&buf);
    let restored = ResolvedConflict::read_binary(&mut cursor).unwrap();

    assert_eq!(restored.new_fact_id, conflict.new_fact_id);
    assert_eq!(restored.existing_fact_id, conflict.existing_fact_id);

    if let Resolution::Merge { weights } = restored.resolution {
        assert!((weights.0 - 0.7).abs() < 0.001);
        assert!((weights.1 - 0.3).abs() < 0.001);
    } else {
        panic!("Expected Merge resolution");
    }
}

// ============================================================================
// Full State Tests
// ============================================================================

#[test]
fn test_empty_state_roundtrip() {
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();
    let restored = ReconciliationState::from_bytes(&bytes).unwrap();

    assert!(restored.sources.is_empty());
    assert!(restored.facts.is_empty());
    assert!(restored.conflicts.is_empty());
}

#[test]
fn test_complex_state_roundtrip() {
    let mut state = ReconciliationState::new();

    // Add multiple sources
    let mut expert = SourceCredibility::new("expert", 0.95);
    expert
        .domain_expertise
        .insert("machining".to_string(), Weight::new(0.99));
    expert.track_record = TrackRecord {
        correct: 100,
        incorrect: 5,
    };
    state.sources.push(expert);

    state.sources.push(SourceCredibility::new("llm", 0.7));
    state.sources.push(SourceCredibility::new("user", 0.5));

    // Add facts with evidence
    let mut fact1 = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Material".to_string(),
            name: "Titanium".to_string(),
            attributes: [("hardness".to_string(), "36".to_string())]
                .into_iter()
                .collect(),
        },
        0.85,
    );
    fact1.upvotes = 5;
    fact1.downvotes = 1;
    fact1.evidence.push(Evidence {
        id: Uuid::new_v4(),
        source_id: "expert".to_string(),
        evidence_type: EvidenceType::Supports,
        strength: Weight::new(0.9),
        timestamp: Utc::now(),
        description: "Verified from handbook".to_string(),
    });
    state.facts.push(fact1);

    // Add tacit knowledge
    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::TacitKnowledge {
            rule: "titanium -> use coolant".to_string(),
            confidence: 0.92,
            domain: "machining".to_string(),
        },
        0.92,
    ));

    // Add conflict
    state.conflicts.push(ResolvedConflict {
        new_fact_id: Uuid::new_v4(),
        existing_fact_id: Uuid::new_v4(),
        conflict_type: ConflictType::ConfidenceConflict,
        resolution: Resolution::Merge {
            weights: (0.6, 0.4),
        },
        timestamp: Utc::now(),
    });

    // Roundtrip
    let bytes = state.to_bytes().unwrap();
    let restored = ReconciliationState::from_bytes(&bytes).unwrap();

    // Verify
    assert_eq!(restored.sources.len(), 3);
    assert_eq!(restored.facts.len(), 2);
    assert_eq!(restored.conflicts.len(), 1);

    // Check expert source
    let expert = &restored.sources[0];
    assert_eq!(expert.source_id, "expert");
    assert!((expert.base_credibility.value() - 0.95).abs() < 0.001);
    assert_eq!(expert.track_record.correct, 100);
    assert!(expert.domain_expertise.contains_key("machining"));

    // Check fact
    let fact = &restored.facts[0];
    assert_eq!(fact.upvotes, 5);
    assert_eq!(fact.downvotes, 1);
    assert_eq!(fact.evidence.len(), 1);
}

// ============================================================================
// File I/O Tests
// ============================================================================

#[test]
fn test_file_save_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_reconciliation.axrc");

    let mut state = ReconciliationState::new();
    state
        .sources
        .push(SourceCredibility::new("test_source", 0.75));
    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: "TestEntity".to_string(),
            attributes: HashMap::new(),
        },
        0.8,
    ));

    // Save
    state.save(&path).unwrap();
    assert!(path.exists());

    // Load
    let restored = ReconciliationState::load(&path).unwrap();
    assert_eq!(restored.sources.len(), 1);
    assert_eq!(restored.facts.len(), 1);
}

#[test]
fn test_corrupted_verified_header_rejected() {
    let mut bytes = ReconciliationState::new().to_bytes().unwrap();
    bytes[0] ^= 0xFF;

    let result = ReconciliationState::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_truncated_verified_envelope_rejected() {
    let mut bytes = ReconciliationState::new().to_bytes().unwrap();
    bytes.truncate(bytes.len() / 2);

    let result = ReconciliationState::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_unsupported_schema_version_rejected() {
    let state = ReconciliationState::new();
    let bytes = axiograph_llm_sync::format::serialize_verified(&state, VERSION + 1, 0)
        .expect("future schema envelope should serialize");

    let result = ReconciliationState::from_bytes(&bytes);
    assert!(result
        .expect_err("future reconciliation schema must fail closed")
        .to_string()
        .contains("unsupported reconciliation schema version"));
}

#[test]
fn test_binary_format_specification() {
    // This test documents the state persistence contract: reconciliation state
    // uses the shared verified CBOR envelope, not the historical AXRC fixed
    // offset layout.
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();

    let (_state, header): (ReconciliationState, _) = deserialize_verified(&bytes).unwrap();
    assert_eq!(header.magic, VERIFIED_MAGIC);
    assert_eq!(header.schema_version, VERSION);
    assert_eq!(header.content_length as usize, bytes.len() - ciborium_header_len(&bytes));
}

fn ciborium_header_len(bytes: &[u8]) -> usize {
    let mut cursor = std::io::Cursor::new(bytes);
    let _header: axiograph_llm_sync::format::VerifiedHeader =
        ciborium::from_reader(&mut cursor).unwrap();
    cursor.position() as usize
}

// ============================================================================
// Bayesian Update Consistency Tests
// ============================================================================

#[test]
fn test_bayesian_update_consistency() {
    // Verify Bayesian updates are consistent with the documented formula.
    let prior = Weight::new(0.5);

    // Test: strong evidence in favor
    let posterior = prior.bayesian_update(0.9, 0.5);

    // Expected: P(H|E) = 0.9 * 0.5 / 0.5 = 0.9
    assert!((posterior.value() - 0.9).abs() < 0.01);
}

#[test]
fn test_weight_combine_consistency() {
    // Verify weight combination matches the documented algebra.
    let w1 = Weight::new(0.8);
    let w2 = Weight::new(0.5);

    let combined = w1.combine(w2);

    // Expected: 0.8 * 0.5 = 0.4
    assert!((combined.value() - 0.4).abs() < 0.001);
}

// ============================================================================
// Unicode and Edge Cases
// ============================================================================

#[test]
fn test_unicode_strings() {
    let mut source = SourceCredibility::new("专家", 0.9); // Chinese for "expert"
    source
        .domain_expertise
        .insert("加工".to_string(), Weight::new(0.95)); // "machining"

    let mut buf = Vec::new();
    source.write_binary(&mut buf).unwrap();

    let mut cursor = std::io::Cursor::new(&buf);
    let restored = SourceCredibility::read_binary(&mut cursor).unwrap();

    assert_eq!(restored.source_id, "专家");
    assert!(restored.domain_expertise.contains_key("加工"));
}

#[test]
fn test_empty_strings() {
    let source = SourceCredibility::new("", 0.5);

    let mut buf = Vec::new();
    source.write_binary(&mut buf).unwrap();

    let mut cursor = std::io::Cursor::new(&buf);
    let restored = SourceCredibility::read_binary(&mut cursor).unwrap();

    assert_eq!(restored.source_id, "");
}

#[test]
fn test_large_evidence_list() {
    let mut fact = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: "Test".to_string(),
            attributes: HashMap::new(),
        },
        0.5,
    );

    // Add 100 evidence items
    for i in 0..100 {
        fact.evidence.push(Evidence {
            id: Uuid::new_v4(),
            source_id: format!("source_{}", i),
            evidence_type: EvidenceType::Supports,
            strength: Weight::new(0.5),
            timestamp: Utc::now(),
            description: format!("Evidence {}", i),
        });
    }

    let mut buf = Vec::new();
    fact.write_binary(&mut buf).unwrap();

    let mut cursor = std::io::Cursor::new(&buf);
    let restored = WeightedFact::read_binary(&mut cursor).unwrap();

    assert_eq!(restored.evidence.len(), 100);
}
