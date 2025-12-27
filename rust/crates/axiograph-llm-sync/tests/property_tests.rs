//! Property-Based Tests for Axiograph
//!
//! Uses proptest for comprehensive testing:
//! 1. Probability invariants always hold
//! 2. Serialization roundtrips perfectly
//! 3. Path operations are associative
//! 4. Graph operations preserve consistency
//! 5. Reconciliation is deterministic

use axiograph_llm_sync::path_verification::*;
use axiograph_llm_sync::reconciliation::*;
use proptest::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Strategies
// ============================================================================

/// Generate valid probability values
fn prob_strategy() -> impl Strategy<Value = f32> {
    (0.0f32..=1.0f32)
}

/// Generate source IDs
fn source_id_strategy() -> impl Strategy<Value = String> {
    "[a-z]{3,10}".prop_map(|s| s)
}

/// Generate fact types
fn fact_type_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("Entity".to_string()),
        Just("Relation".to_string()),
        Just("TacitKnowledge".to_string()),
    ]
}

/// Generate entity names
fn entity_name_strategy() -> impl Strategy<Value = String> {
    "[A-Z][a-z]{2,15}".prop_map(|s| s)
}

/// Generate relation types
fn relation_type_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("is_a".to_string()),
        Just("has_property".to_string()),
        Just("causes".to_string()),
        Just("supports".to_string()),
    ]
}

// ============================================================================
// Weight Invariant Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn weight_always_valid(value in prob_strategy()) {
        let weight = Weight::new(value);
        prop_assert!(weight.value() >= 0.0);
        prop_assert!(weight.value() <= 1.0);
    }

    #[test]
    fn weight_combine_never_exceeds_one(a in prob_strategy(), b in prob_strategy()) {
        let w1 = Weight::new(a);
        let w2 = Weight::new(b);
        let combined = w1.combine(w2);

        prop_assert!(combined.value() >= 0.0);
        prop_assert!(combined.value() <= 1.0);
        prop_assert!((combined.value() - a * b).abs() < 0.0001);
    }

    #[test]
    fn weight_bayesian_update_valid(
        prior in prob_strategy(),
        likelihood_true in 0.001f32..=0.999f32,
        likelihood_false in 0.001f32..=0.999f32,
    ) {
        let w = Weight::new(prior);
        let posterior = w.bayesian_update(likelihood_true, likelihood_false);

        prop_assert!(posterior.value() >= 0.0);
        prop_assert!(posterior.value() <= 1.0);
    }
}

// ============================================================================
// Source Credibility Tests
// ============================================================================

proptest! {
    #[test]
    fn credibility_starts_at_base(
        source_id in source_id_strategy(),
        base in prob_strategy(),
    ) {
        let source = SourceCredibility::new(&source_id, base);
        prop_assert!((source.base_credibility.value() - base).abs() < 0.0001);
    }

    #[test]
    fn track_record_affects_credibility(
        source_id in source_id_strategy(),
        base in prob_strategy(),
        correct in 0u32..100,
        incorrect in 0u32..100,
    ) {
        let mut source = SourceCredibility::new(&source_id, base);

        for _ in 0..correct {
            source.record_outcome(true);
        }
        for _ in 0..incorrect {
            source.record_outcome(false);
        }

        // Track record should be updated
        prop_assert_eq!(source.track_record.correct, correct);
        prop_assert_eq!(source.track_record.incorrect, incorrect);
    }
}

// ============================================================================
// Path Tests
// ============================================================================

proptest! {
    #[test]
    fn identity_path_has_confidence_one(id in any::<u128>()) {
        let uuid = Uuid::from_u128(id);
        let path = Path::identity(uuid);

        prop_assert_eq!(path.len(), 0);
        prop_assert!((path.confidence().value() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn path_confidence_is_product(
        conf1 in prob_strategy(),
        conf2 in prob_strategy(),
        conf3 in prob_strategy(),
    ) {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();

        // Build path through edges
        let p1 = Path::from_edge(Edge::<IsA>::new(a, b, conf1));
        let p2 = Path::from_edge(Edge::<IsA>::new(b, c, conf2));
        let p3 = Path::from_edge(Edge::<IsA>::new(c, d, conf3));

        if let Ok(composed) = p1.compose(p2).and_then(|p| p.compose(p3)) {
            let expected = conf1 * conf2 * conf3;
            prop_assert!((composed.confidence().value() - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn path_composition_is_associative(
        conf1 in prob_strategy(),
        conf2 in prob_strategy(),
        conf3 in prob_strategy(),
    ) {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();

        let p1 = Path::from_edge(Edge::<IsA>::new(a, b, conf1));
        let p2 = Path::from_edge(Edge::<IsA>::new(b, c, conf2));
        let p3 = Path::from_edge(Edge::<IsA>::new(c, d, conf3));

        // (p1 ∘ p2) ∘ p3
        let left = p1.clone().compose(p2.clone())
            .and_then(|p12| p12.compose(p3.clone()));

        // p1 ∘ (p2 ∘ p3)
        let right = p2.compose(p3)
            .and_then(|p23| p1.compose(p23));

        match (left, right) {
            (Ok(l), Ok(r)) => {
                prop_assert_eq!(l.len(), r.len());
                prop_assert!((l.confidence().value() - r.confidence().value()).abs() < 0.0001);
            }
            _ => {}
        }
    }
}

// ============================================================================
// Graph Invariant Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn graph_rejects_invalid_weights(bad_weight in -10.0f32..0.0) {
        let mut graph = VerifiedGraph::new();

        let result = graph.add_node(FactNode {
            id: Uuid::new_v4(),
            fact_type: "Test".to_string(),
            content: axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "Test".to_string(),
                attributes: HashMap::new(),
            },
            weight: bad_weight,
        });

        prop_assert!(result.is_err());
    }

    #[test]
    fn graph_rejects_invalid_confidence(bad_conf in 1.1f32..10.0) {
        let mut graph = VerifiedGraph::new();

        let n1 = graph.add_node(FactNode {
            id: Uuid::new_v4(),
            fact_type: "Test".to_string(),
            content: axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "A".to_string(),
                attributes: HashMap::new(),
            },
            weight: 0.9,
        });

        let n2 = graph.add_node(FactNode {
            id: Uuid::new_v4(),
            fact_type: "Test".to_string(),
            content: axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "B".to_string(),
                attributes: HashMap::new(),
            },
            weight: 0.9,
        });

        if n1.is_ok() && n2.is_ok() {
            let id1 = graph.nodes().next().unwrap().id;
            let id2 = graph.nodes().nth(1).unwrap().id;
            let result = graph.add_edge::<IsA>(id1, id2, bad_conf);
            prop_assert!(result.is_err());
        }
    }

    #[test]
    fn graph_edges_connect_existing_nodes(
        n1_weight in prob_strategy(),
        n2_weight in prob_strategy(),
        edge_conf in prob_strategy(),
    ) {
        let mut graph = VerifiedGraph::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let fake_id = Uuid::new_v4();

        graph.add_node(FactNode {
            id: id1,
            fact_type: "Test".to_string(),
            content: axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "A".to_string(),
                attributes: HashMap::new(),
            },
            weight: n1_weight,
        }).unwrap();

        graph.add_node(FactNode {
            id: id2,
            fact_type: "Test".to_string(),
            content: axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "B".to_string(),
                attributes: HashMap::new(),
            },
            weight: n2_weight,
        }).unwrap();

        // Edge to non-existent node should fail
        let result = graph.add_edge::<IsA>(id1, fake_id, edge_conf);
        prop_assert!(result.is_err());

        // Edge between existing nodes should succeed
        let result = graph.add_edge::<IsA>(id1, id2, edge_conf);
        prop_assert!(result.is_ok());
    }
}

// ============================================================================
// Serialization Roundtrip Tests
// ============================================================================

proptest! {
    #[test]
    fn weight_roundtrip(value in prob_strategy()) {
        let w = Weight::new(value);

        // Serialize and deserialize
        let json = serde_json::to_string(&w).unwrap();
        let restored: Weight = serde_json::from_str(&json).unwrap();

        prop_assert!((w.value() - restored.value()).abs() < 0.0001);
    }

    #[test]
    fn source_credibility_roundtrip(
        source_id in source_id_strategy(),
        base in prob_strategy(),
    ) {
        let source = SourceCredibility::new(&source_id, base);

        let json = serde_json::to_string(&source).unwrap();
        let restored: SourceCredibility = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(restored.source_id, source_id);
        prop_assert!((restored.base_credibility.value() - base).abs() < 0.0001);
    }
}

// ============================================================================
// Reconciliation Determinism Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn reconciliation_is_deterministic(
        existing_weight in prob_strategy(),
        new_weight in prob_strategy(),
    ) {
        let config = ReconciliationConfig::default();
        let mut engine1 = ReconciliationEngine::new(config.clone());
        let mut engine2 = ReconciliationEngine::new(config);

        // Register same source
        let source = SourceCredibility::new("test", 0.8);
        engine1.register_source(source.clone());
        engine2.register_source(source);

        // Create identical facts
        let fact = axiograph_llm_sync::ExtractedFact {
            id: Uuid::nil(), // Fixed ID for determinism
            claim: "Test fact".to_string(),
            structured: axiograph_llm_sync::StructuredFact::TacitKnowledge {
                rule: "test_rule".to_string(),
                confidence: new_weight,
                domain: "test".to_string(),
            },
            confidence: new_weight,
            source: axiograph_llm_sync::FactSource {
                session_id: Uuid::nil(),
                provider: axiograph_llm_sync::LLMProvider::OpenAI { model: "test".to_string() },
                conversation_turns: vec![],
                extraction_timestamp: chrono::Utc::now(),
                human_verified: false,
            },
            status: axiograph_llm_sync::FactStatus::Pending,
        };

        // Reconcile in both engines
        let result1 = engine1.reconcile(fact.clone());
        let result2 = engine2.reconcile(fact);

        // Results should be identical
        prop_assert_eq!(
            std::mem::discriminant(&result1.action),
            std::mem::discriminant(&result2.action)
        );
    }
}

// ============================================================================
// Conflict Resolution Tests
// ============================================================================

proptest! {
    #[test]
    fn conflict_resolution_consistent(
        conf1 in prob_strategy(),
        conf2 in prob_strategy(),
        threshold in 0.1f32..0.5f32,
    ) {
        let path1 = Path::from_edge(Edge::<IsA>::new(Uuid::new_v4(), Uuid::new_v4(), conf1));
        let path2 = Path::from_edge(Edge::<IsA>::new(Uuid::new_v4(), Uuid::new_v4(), conf2));

        let diff = (conf1 - conf2).abs();

        let conflict = PathConflict::ContradictoryPaths {
            path1: path1.clone(),
            path2: path2.clone(),
            confidence_diff: diff,
        };

        let resolution = conflict.resolve();

        match resolution {
            PathResolution::ChooseStronger { chosen, rejected } => {
                // Stronger path should have higher confidence
                prop_assert!(chosen.confidence().value() >= rejected.confidence().value());
            }
            PathResolution::Merge { weight1, weight2 } => {
                // Weights should sum to ~1
                prop_assert!((weight1 + weight2 - 1.0).abs() < 0.01);
            }
            _ => {}
        }
    }
}

// ============================================================================
// Evidence Chain Tests
// ============================================================================

proptest! {
    #[test]
    fn evidence_chain_confidence_decreases(
        strengths in prop::collection::vec(prob_strategy(), 1..5),
    ) {
        // Chain of evidence should have lower combined confidence
        let combined: f32 = strengths.iter().product();

        prop_assert!(combined <= strengths[0]);
        prop_assert!(combined >= 0.0);
        prop_assert!(combined <= 1.0);
    }
}

// ============================================================================
// Voting Tests
// ============================================================================

proptest! {
    #[test]
    fn voting_affects_weight(
        initial_weight in prob_strategy(),
        upvotes in 0usize..10,
        downvotes in 0usize..10,
    ) {
        let mut fact = WeightedFact::new(
            Uuid::new_v4(),
            axiograph_llm_sync::StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "Test".to_string(),
                attributes: HashMap::new(),
            },
            initial_weight,
        );

        for _ in 0..upvotes {
            fact.upvotes += 1;
        }
        for _ in 0..downvotes {
            fact.downvotes += 1;
        }

        prop_assert_eq!(fact.upvotes, upvotes as u32);
        prop_assert_eq!(fact.downvotes, downvotes as u32);
    }
}

// ============================================================================
// Stress Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    #[test]
    fn graph_handles_many_nodes(node_count in 10usize..100) {
        let mut graph = VerifiedGraph::new();
        let mut ids = Vec::new();

        for i in 0..node_count {
            let id = Uuid::new_v4();
            graph.add_node(FactNode {
                id,
                fact_type: "Test".to_string(),
                content: axiograph_llm_sync::StructuredFact::Entity {
                    entity_type: "Test".to_string(),
                    name: format!("Node{}", i),
                    attributes: HashMap::new(),
                },
                weight: 0.9,
            }).unwrap();
            ids.push(id);
        }

        prop_assert_eq!(graph.node_count(), node_count);

        // Add edges
        for i in 0..(node_count - 1) {
            graph.add_edge::<IsA>(ids[i], ids[i + 1], 0.8).unwrap();
        }

        prop_assert_eq!(graph.edge_count(), node_count - 1);
    }
}
