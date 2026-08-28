//! End-to-end tests for reconciliation state persistence.
//!
//! These tests verify that:
//! 1. ReconciliationState is serialized through the verified CBOR envelope
//! 2. The verified envelope checks headers, schema versions, lengths, and content
//! 3. Weights, evidence, conflicts, and Unicode strings survive state roundtrips
//! 4. Invalid/corrupted inputs are rejected

use axiograph_llm_sync::format::{
    deserialize_verified, MAGIC as VERIFIED_MAGIC, VERSION as VERIFIED_FORMAT_VERSION,
};
use axiograph_llm_sync::reconciliation::*;
use axiograph_llm_sync::reconciliation_format::{ReconciliationState, VERSION};
use axiograph_llm_sync::{ConflictType, Resolution, StructuredFact};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Test Helpers
// ============================================================================

fn entity_fact(name: &str) -> StructuredFact {
    StructuredFact::Entity {
        entity_type: "Test".to_string(),
        name: name.to_string(),
        attributes: HashMap::new(),
    }
}

fn roundtrip(state: ReconciliationState) -> ReconciliationState {
    let bytes = state.to_bytes().unwrap();
    ReconciliationState::from_bytes(&bytes).unwrap()
}

fn evidence_kind(evidence_type: &EvidenceType) -> &'static str {
    match evidence_type {
        EvidenceType::Supports => "supports",
        EvidenceType::Refutes => "refutes",
        EvidenceType::Neutral => "neutral",
        EvidenceType::Clarifies => "clarifies",
    }
}

fn conflict_kind(conflict_type: &ConflictType) -> &'static str {
    match conflict_type {
        ConflictType::Contradiction => "contradiction",
        ConflictType::AttributeMismatch => "attribute_mismatch",
        ConflictType::ConfidenceConflict => "confidence_conflict",
        ConflictType::SchemaViolation => "schema_violation",
    }
}

fn resolution_kind(resolution: &Resolution) -> &'static str {
    match resolution {
        Resolution::ReplaceOld => "replace_old",
        Resolution::KeepOld => "keep_old",
        Resolution::Merge { .. } => "merge",
        Resolution::HumanReview => "human_review",
    }
}

// ============================================================================
// Format Validation Tests
// ============================================================================

#[test]
fn test_verified_envelope_magic_and_versions() {
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();

    let (_state, header): (ReconciliationState, _) = deserialize_verified(&bytes).unwrap();
    assert_eq!(header.magic, VERIFIED_MAGIC);
    assert_eq!(header.version, VERIFIED_FORMAT_VERSION);
    assert_eq!(header.schema_version, VERSION);
}

#[test]
fn test_verified_envelope_checks_content() {
    let mut state = ReconciliationState::new();
    state.sources.push(SourceCredibility::new("test", 0.5));
    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        entity_fact("TestEntity"),
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
fn test_source_weight_precision_preservation() {
    let test_weights = [0.0, 0.123456, 0.5, 0.999999, 1.0];

    for &w in &test_weights {
        let mut state = ReconciliationState::new();
        state.sources.push(SourceCredibility::new("test", w));

        let restored = roundtrip(state);
        let restored_weight = restored.sources[0].base_credibility.value();
        let diff = (restored_weight - w).abs();
        assert!(
            diff < 0.0001,
            "Weight {w} not preserved (got {restored_weight})"
        );
    }
}

#[test]
fn test_weighted_fact_weight_preservation() {
    let weights = [0.1, 0.5, 0.9, 0.95, 0.99];

    for &w in &weights {
        let mut state = ReconciliationState::new();
        state.facts.push(WeightedFact::new(
            Uuid::new_v4(),
            entity_fact("WeightedEntity"),
            w,
        ));

        let restored = roundtrip(state);
        let restored_weight = restored.facts[0].weight.value();
        let diff = (restored_weight - w).abs();
        assert!(diff < 0.0001, "Fact weight {w} not preserved");
    }
}

// ============================================================================
// Evidence Preservation Tests
// ============================================================================

#[test]
fn test_evidence_roundtrip_preserves_all_variants() {
    let expected_types = [
        EvidenceType::Supports,
        EvidenceType::Refutes,
        EvidenceType::Neutral,
        EvidenceType::Clarifies,
    ];

    let mut fact = WeightedFact::new(Uuid::new_v4(), entity_fact("EvidenceEntity"), 0.85);
    for (idx, evidence_type) in expected_types.iter().cloned().enumerate() {
        fact.evidence.push(Evidence {
            id: Uuid::new_v4(),
            source_id: format!("source_{idx}"),
            evidence_type,
            strength: Weight::new(0.5 + idx as f32 * 0.1),
            timestamp: Utc::now(),
            description: format!("Evidence {idx}"),
        });
    }

    let mut state = ReconciliationState::new();
    state.facts.push(fact);

    let restored = roundtrip(state);
    let evidence = &restored.facts[0].evidence;
    assert_eq!(evidence.len(), expected_types.len());

    for (idx, restored_evidence) in evidence.iter().enumerate() {
        assert_eq!(
            evidence_kind(&restored_evidence.evidence_type),
            evidence_kind(&expected_types[idx])
        );
        assert_eq!(restored_evidence.source_id, format!("source_{idx}"));
        assert!((restored_evidence.strength.value() - (0.5 + idx as f32 * 0.1)).abs() < 0.0001);
    }
}

// ============================================================================
// Conflict Resolution Preservation Tests
// ============================================================================

#[test]
fn test_conflict_resolution_roundtrip_preserves_all_variants() {
    let inputs = [
        (ConflictType::Contradiction, Resolution::ReplaceOld),
        (ConflictType::AttributeMismatch, Resolution::KeepOld),
        (
            ConflictType::ConfidenceConflict,
            Resolution::Merge {
                weights: (0.7, 0.3),
            },
        ),
        (ConflictType::SchemaViolation, Resolution::HumanReview),
    ];

    let mut state = ReconciliationState::new();
    for (conflict_type, resolution) in &inputs {
        state.conflicts.push(ResolvedConflict {
            new_fact_id: Uuid::new_v4(),
            existing_fact_id: Uuid::new_v4(),
            conflict_type: conflict_type.clone(),
            resolution: resolution.clone(),
            timestamp: Utc::now(),
        });
    }

    let restored = roundtrip(state);
    assert_eq!(restored.conflicts.len(), inputs.len());

    for (idx, restored_conflict) in restored.conflicts.iter().enumerate() {
        let (expected_conflict_type, expected_resolution) = &inputs[idx];
        assert_eq!(
            conflict_kind(&restored_conflict.conflict_type),
            conflict_kind(expected_conflict_type)
        );
        assert_eq!(
            resolution_kind(&restored_conflict.resolution),
            resolution_kind(expected_resolution)
        );
    }

    match &restored.conflicts[2].resolution {
        Resolution::Merge { weights } => {
            assert!((weights.0 - 0.7).abs() < 0.001);
            assert!((weights.1 - 0.3).abs() < 0.001);
        }
        _ => panic!("Expected Merge resolution"),
    }
}

// ============================================================================
// Full State Tests
// ============================================================================

#[test]
fn test_empty_state_roundtrip() {
    let state = ReconciliationState::new();
    let restored = roundtrip(state);

    assert!(restored.sources.is_empty());
    assert!(restored.facts.is_empty());
    assert!(restored.conflicts.is_empty());
}

#[test]
fn test_complex_state_roundtrip() {
    let mut state = ReconciliationState::new();

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

    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::TacitKnowledge {
            rule: "titanium -> use coolant".to_string(),
            confidence: 0.92,
            domain: "machining".to_string(),
        },
        0.92,
    ));

    state.conflicts.push(ResolvedConflict {
        new_fact_id: Uuid::new_v4(),
        existing_fact_id: Uuid::new_v4(),
        conflict_type: ConflictType::ConfidenceConflict,
        resolution: Resolution::Merge {
            weights: (0.6, 0.4),
        },
        timestamp: Utc::now(),
    });

    let restored = roundtrip(state);

    assert_eq!(restored.sources.len(), 3);
    assert_eq!(restored.facts.len(), 2);
    assert_eq!(restored.conflicts.len(), 1);

    let expert = &restored.sources[0];
    assert_eq!(expert.source_id, "expert");
    assert!((expert.base_credibility.value() - 0.95).abs() < 0.001);
    assert_eq!(expert.track_record.correct, 100);
    assert!(expert.domain_expertise.contains_key("machining"));

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
    let path = dir.path().join("test_reconciliation.cbor");

    let mut state = ReconciliationState::new();
    state
        .sources
        .push(SourceCredibility::new("test_source", 0.75));
    state.facts.push(WeightedFact::new(
        Uuid::new_v4(),
        entity_fact("TestEntity"),
        0.8,
    ));

    state.save(&path).unwrap();
    assert!(path.exists());

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
fn test_verified_cbor_format_specification() {
    let state = ReconciliationState::new();
    let bytes = state.to_bytes().unwrap();

    let (_state, header): (ReconciliationState, _) = deserialize_verified(&bytes).unwrap();
    assert_eq!(header.magic, VERIFIED_MAGIC);
    assert_eq!(header.schema_version, VERSION);
    assert_eq!(
        header.content_length as usize,
        bytes.len() - ciborium_header_len(&bytes)
    );
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
    let prior = Weight::new(0.5);
    let posterior = prior.bayesian_update(0.9, 0.5);

    assert!((posterior.value() - 0.9).abs() < 0.01);
}

#[test]
fn test_weight_combine_consistency() {
    let w1 = Weight::new(0.8);
    let w2 = Weight::new(0.5);
    let combined = w1.combine(w2);

    assert!((combined.value() - 0.4).abs() < 0.001);
}

// ============================================================================
// Unicode and Edge Cases
// ============================================================================

#[test]
fn test_unicode_strings() {
    let mut source = SourceCredibility::new("专家", 0.9);
    source
        .domain_expertise
        .insert("加工".to_string(), Weight::new(0.95));

    let mut state = ReconciliationState::new();
    state.sources.push(source);

    let restored = roundtrip(state);
    assert_eq!(restored.sources[0].source_id, "专家");
    assert!(restored.sources[0].domain_expertise.contains_key("加工"));
}

#[test]
fn test_empty_strings() {
    let mut state = ReconciliationState::new();
    state.sources.push(SourceCredibility::new("", 0.5));

    let restored = roundtrip(state);
    assert_eq!(restored.sources[0].source_id, "");
}

#[test]
fn test_large_evidence_list() {
    let mut fact = WeightedFact::new(Uuid::new_v4(), entity_fact("LargeEvidenceEntity"), 0.5);

    for i in 0..100 {
        fact.evidence.push(Evidence {
            id: Uuid::new_v4(),
            source_id: format!("source_{i}"),
            evidence_type: EvidenceType::Supports,
            strength: Weight::new(0.5),
            timestamp: Utc::now(),
            description: format!("Evidence {i}"),
        });
    }

    let mut state = ReconciliationState::new();
    state.facts.push(fact);

    let restored = roundtrip(state);
    assert_eq!(restored.facts[0].evidence.len(), 100);
}
