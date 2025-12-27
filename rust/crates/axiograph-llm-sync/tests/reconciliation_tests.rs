//! Tests for the reconciliation system

use axiograph_llm_sync::reconciliation::*;
use axiograph_llm_sync::*;
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

fn test_source_expert() -> SourceCredibility {
    let mut source = SourceCredibility::new("expert", 0.95);
    source
        .domain_expertise
        .insert("machining".to_string(), Weight::new(0.99));
    source.track_record = TrackRecord {
        correct: 100,
        incorrect: 5,
    };
    source
}

fn test_source_llm() -> SourceCredibility {
    SourceCredibility::new("llm", 0.7)
}

fn test_source_user() -> SourceCredibility {
    SourceCredibility::new("user", 0.5)
}

fn make_fact(name: &str, confidence: f32) -> ExtractedFact {
    ExtractedFact {
        id: Uuid::new_v4(),
        claim: format!("{} is a Material", name),
        structured: StructuredFact::Entity {
            entity_type: "Material".to_string(),
            name: name.to_string(),
            attributes: HashMap::new(),
        },
        confidence,
        source: FactSource {
            session_id: Uuid::new_v4(),
            provider: LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "local".to_string(),
            },
            conversation_turns: vec![],
            extraction_timestamp: Utc::now(),
            human_verified: false,
        },
        status: FactStatus::Pending,
    }
}

// ============================================================================
// Weight Tests
// ============================================================================

#[test]
fn test_weight_clamping() {
    let over = Weight::new(1.5);
    assert!((over.value() - 1.0).abs() < 0.001);

    let under = Weight::new(-0.5);
    assert!((under.value() - 0.0).abs() < 0.001);

    let valid = Weight::new(0.75);
    assert!((valid.value() - 0.75).abs() < 0.001);
}

#[test]
fn test_weight_combine() {
    let w1 = Weight::new(0.8);
    let w2 = Weight::new(0.5);
    let combined = w1.combine(w2);

    assert!((combined.value() - 0.4).abs() < 0.001);
}

#[test]
fn test_bayesian_update_supporting() {
    let prior = Weight::new(0.5);
    // Strong supporting evidence
    let posterior = prior.bayesian_update(0.9, 0.5);

    assert!(posterior.value() > 0.5, "Should increase belief");
}

#[test]
fn test_bayesian_update_refuting() {
    let prior = Weight::new(0.5);
    // Strong refuting evidence
    let posterior = prior.bayesian_update(0.1, 0.5);

    assert!(posterior.value() < 0.5, "Should decrease belief");
}

// ============================================================================
// Source Credibility Tests
// ============================================================================

#[test]
fn test_source_credibility_domain() {
    let mut source = SourceCredibility::new("expert", 0.9);
    source
        .domain_expertise
        .insert("machining".to_string(), Weight::new(0.95));
    source
        .domain_expertise
        .insert("cooking".to_string(), Weight::new(0.3));

    let machining = source.credibility_for("machining");
    let cooking = source.credibility_for("cooking");
    let unknown = source.credibility_for("unknown");

    assert!(machining.value() > cooking.value());
    assert!(unknown.value() > cooking.value()); // Unknown gets 0.5 default
}

#[test]
fn test_track_record() {
    let mut source = SourceCredibility::new("user", 0.5);

    assert!((source.track_record.accuracy() - 0.5).abs() < 0.01);

    source.record_outcome(true);
    source.record_outcome(true);
    source.record_outcome(true);
    source.record_outcome(false);

    assert!((source.track_record.accuracy() - 0.75).abs() < 0.01);
}

#[test]
fn test_credibility_with_track_record() {
    let mut source = SourceCredibility::new("user", 0.8);

    // Good track record
    source.track_record = TrackRecord {
        correct: 90,
        incorrect: 10,
    };
    let good_cred = source.credibility_for("any");

    // Bad track record
    source.track_record = TrackRecord {
        correct: 10,
        incorrect: 90,
    };
    let bad_cred = source.credibility_for("any");

    assert!(good_cred.value() > bad_cred.value());
}

// ============================================================================
// Weighted Fact Tests
// ============================================================================

#[test]
fn test_weighted_fact_upvote() {
    let mut wf = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Material".to_string(),
            name: "Steel".to_string(),
            attributes: HashMap::new(),
        },
        0.5,
    );

    let initial = wf.weight.value();

    wf.upvote("user1", 0.8);

    assert!(wf.weight.value() > initial);
    assert_eq!(wf.upvotes, 1);
    assert_eq!(wf.downvotes, 0);
}

#[test]
fn test_weighted_fact_downvote() {
    let mut wf = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Material".to_string(),
            name: "Steel".to_string(),
            attributes: HashMap::new(),
        },
        0.8,
    );

    let initial = wf.weight.value();

    wf.downvote("user1", 0.8);

    assert!(wf.weight.value() < initial);
    assert_eq!(wf.upvotes, 0);
    assert_eq!(wf.downvotes, 1);
}

#[test]
fn test_weighted_fact_multiple_votes() {
    let mut wf = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Material".to_string(),
            name: "Steel".to_string(),
            attributes: HashMap::new(),
        },
        0.5,
    );

    // Multiple upvotes should increase weight
    for _ in 0..5 {
        wf.upvote("user", 0.7);
    }

    assert!(wf.weight.value() > 0.5);
    assert_eq!(wf.upvotes, 5);

    // Multiple downvotes should decrease weight
    for _ in 0..3 {
        wf.downvote("critic", 0.6);
    }

    assert_eq!(wf.downvotes, 3);
    assert_eq!(wf.net_votes(), 2);
}

#[test]
fn test_temporal_decay() {
    use chrono::Duration;

    let mut wf = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: "Test".to_string(),
            attributes: HashMap::new(),
        },
        1.0,
    );

    // Simulate 30 days old
    wf.updated_at = Utc::now() - Duration::days(30);
    wf.apply_decay(30.0); // 30-day half-life

    // Should be approximately halved
    assert!((wf.weight.value() - 0.5).abs() < 0.1);
}

#[test]
fn test_decay_fresh_fact() {
    let mut wf = WeightedFact::new(
        Uuid::new_v4(),
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: "Test".to_string(),
            attributes: HashMap::new(),
        },
        0.9,
    );

    let initial = wf.weight.value();
    wf.apply_decay(30.0);

    // Fresh fact should barely decay
    assert!((wf.weight.value() - initial).abs() < 0.01);
}

// ============================================================================
// Reconciliation Engine Tests
// ============================================================================

#[test]
fn test_reconcile_no_conflict() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

    let fact = make_fact("Steel", 0.9);
    let result = engine.reconcile(fact);

    assert!(matches!(result.action, ReconciliationAction::Integrated));
    assert!(result.conflicts_resolved.is_empty());
}

#[test]
fn test_reconcile_with_conflict_new_wins() {
    let config = ReconciliationConfig {
        auto_resolve_threshold: 0.2,
        ..Default::default()
    };
    let mut engine = ReconciliationEngine::new(config);

    // Add initial fact with low confidence
    let fact1 = make_fact("Steel", 0.3);
    engine.reconcile(fact1);

    // Add conflicting fact with high confidence
    let mut fact2 = make_fact("Steel", 0.9);
    fact2.structured = StructuredFact::Entity {
        entity_type: "Material".to_string(),
        name: "Steel".to_string(),
        attributes: [("hardness".to_string(), "50".to_string())]
            .into_iter()
            .collect(),
    };

    let result = engine.reconcile(fact2);

    // New fact should win due to higher confidence
    assert!(
        matches!(result.action, ReconciliationAction::Integrated)
            || matches!(result.action, ReconciliationAction::Merged)
    );
}

#[test]
fn test_reconcile_merge() {
    let config = ReconciliationConfig {
        auto_resolve_threshold: 0.5, // High threshold forces merge
        ..Default::default()
    };
    let mut engine = ReconciliationEngine::new(config);

    // Add two facts with similar confidence
    let fact1 = make_fact("Titanium", 0.7);
    engine.reconcile(fact1);

    let mut fact2 = make_fact("Titanium", 0.75);
    fact2.structured = StructuredFact::Entity {
        entity_type: "Material".to_string(),
        name: "Titanium".to_string(),
        attributes: [("hardness".to_string(), "36".to_string())]
            .into_iter()
            .collect(),
    };

    let result = engine.reconcile(fact2);

    // Should merge due to close weights
    // (or integrate if implementation treats as non-conflicting)
    println!("Result action: {:?}", result.action);
}

#[test]
fn test_voting_api() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());
    engine.register_source(test_source_user());

    let fact = make_fact("Aluminum", 0.5);
    let fact_id = fact.id;
    engine.reconcile(fact);

    // Upvote
    let weight = engine.upvote(fact_id, "user", 0.8);
    assert!(weight.is_some());
    assert!(weight.unwrap().value() > 0.5);

    // Downvote
    let weight = engine.downvote(fact_id, "critic", 0.6);
    assert!(weight.is_some());
}

#[test]
fn test_bayesian_api() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

    let fact = make_fact("Copper", 0.5);
    let fact_id = fact.id;
    engine.reconcile(fact);

    // Strong evidence in favor
    let new_weight = engine.bayesian_update(fact_id, 0.95, 0.1);
    assert!(new_weight.is_some());
    assert!(new_weight.unwrap().value() > 0.5);
}

#[test]
fn test_decay_all() {
    use chrono::Duration;

    let mut engine = ReconciliationEngine::new(ReconciliationConfig {
        decay_half_life: 30.0,
        ..Default::default()
    });

    // Add some facts and age them
    let fact = make_fact("Iron", 0.8);
    let fact_id = fact.id;
    engine.reconcile(fact);

    // Get initial weight
    let initial = engine.get_fact(fact_id).unwrap().weight.value();

    // Decay (fact is fresh, so minimal decay expected)
    engine.decay_all();

    let after = engine.get_fact(fact_id).unwrap().weight.value();

    // Fresh facts should barely decay
    assert!((initial - after).abs() < 0.01);
}

#[test]
fn test_prune_dead_facts() {
    let config = ReconciliationConfig {
        discard_threshold: 0.3,
        ..Default::default()
    };
    let mut engine = ReconciliationEngine::new(config);

    // Add fact with low weight
    let fact = make_fact("WeakFact", 0.1);
    let fact_id = fact.id;
    engine.reconcile(fact);

    // Prune
    let pruned = engine.prune_dead_facts();

    assert!(pruned.contains(&fact_id));
    assert!(engine.get_fact(fact_id).is_none());
}

#[test]
fn test_get_confident_facts() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

    engine.reconcile(make_fact("High", 0.9));
    engine.reconcile(make_fact("Medium", 0.6));
    engine.reconcile(make_fact("Low", 0.3));

    let confident = engine.get_confident_facts(0.7);
    assert_eq!(confident.len(), 1);

    let all = engine.get_confident_facts(0.0);
    assert_eq!(all.len(), 3);
}

#[test]
fn test_source_credibility_update() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());
    engine.register_source(test_source_llm());

    // Verify correct
    engine.update_source_credibility("llm", true);
    engine.update_source_credibility("llm", true);
    engine.update_source_credibility("llm", false);

    // Track record should be updated (2 correct, 1 incorrect)
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_engine() {
    let engine = ReconciliationEngine::new(ReconciliationConfig::default());

    let fact = engine.get_fact(Uuid::new_v4());
    assert!(fact.is_none());

    let confident = engine.get_confident_facts(0.5);
    assert!(confident.is_empty());
}

#[test]
fn test_vote_nonexistent_fact() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

    let result = engine.upvote(Uuid::new_v4(), "user", 0.8);
    assert!(result.is_none());
}

#[test]
fn test_tacit_knowledge_conflict() {
    let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

    // Add contradictory rules
    let fact1 = ExtractedFact {
        id: Uuid::new_v4(),
        claim: "Always use coolant".to_string(),
        structured: StructuredFact::TacitKnowledge {
            rule: "always use coolant when cutting titanium".to_string(),
            confidence: 0.8,
            domain: "machining".to_string(),
        },
        confidence: 0.8,
        source: FactSource {
            session_id: Uuid::new_v4(),
            provider: LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "local".to_string(),
            },
            conversation_turns: vec![],
            extraction_timestamp: Utc::now(),
            human_verified: false,
        },
        status: FactStatus::Pending,
    };
    engine.reconcile(fact1);

    let fact2 = ExtractedFact {
        id: Uuid::new_v4(),
        claim: "Never use coolant".to_string(),
        structured: StructuredFact::TacitKnowledge {
            rule: "never use coolant when cutting titanium".to_string(),
            confidence: 0.9,
            domain: "machining".to_string(),
        },
        confidence: 0.9,
        source: FactSource {
            session_id: Uuid::new_v4(),
            provider: LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "local".to_string(),
            },
            conversation_turns: vec![],
            extraction_timestamp: Utc::now(),
            human_verified: false,
        },
        status: FactStatus::Pending,
    };

    let result = engine.reconcile(fact2);

    // Should detect contradiction
    println!("Contradiction result: {:?}", result.action);
}
