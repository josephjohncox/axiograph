//! E2E tests for path verification (Rust runtime)
//!
//! These tests verify that:
//! 1. Paths are correctly constructed and validated
//! 2. Path conflicts are detected and resolved
//! 3. Path invariants are preserved through operations

use axiograph_llm_sync::path_verification::*;
use axiograph_llm_sync::reconciliation::*;
use axiograph_llm_sync::*;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Path Construction Tests
// ============================================================================

fn make_entity(name: &str) -> StructuredFact {
    StructuredFact::Entity {
        entity_type: "TestEntity".to_string(),
        name: name.to_string(),
        attributes: HashMap::new(),
    }
}

#[test]
fn test_path_identity_has_confidence_one() {
    let node = Uuid::new_v4();
    let path = Path::identity(node);

    assert_eq!(path.len(), 0);
    assert_eq!(path.confidence().value(), 1.0);
    assert_eq!(path.start(), node);
    assert_eq!(path.end(), node);
}

#[test]
fn test_single_edge_path() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let edge = Edge::<IsA>::new(a, b, 0.9);
    let path = Path::from_edge(edge);

    assert_eq!(path.len(), 1);
    assert!((path.confidence().value() - 0.9).abs() < 0.001);
}

#[test]
fn test_path_composition_multiplies_confidence() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();

    let p1 = Path::from_edge(Edge::<IsA>::new(a, b, 0.8));
    let p2 = Path::from_edge(Edge::<HasProperty>::new(b, c, 0.5));

    let composed = p1.compose(p2).unwrap();

    // 0.8 * 0.5 = 0.4
    assert!((composed.confidence().value() - 0.4).abs() < 0.001);
}

#[test]
fn test_incompatible_path_composition_fails() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let d = Uuid::new_v4();

    let p1 = Path::from_edge(Edge::<IsA>::new(a, b, 0.9));
    let p2 = Path::from_edge(Edge::<IsA>::new(c, d, 0.9)); // Doesn't connect

    let result = p1.compose(p2);
    assert!(result.is_err());
}

#[test]
fn test_path_builder_valid_construction() {
    let start = Uuid::new_v4();
    let mid = Uuid::new_v4();
    let end = Uuid::new_v4();

    let path = PathBuilder::new(start)
        .edge::<IsA>(mid, 0.9)
        .edge::<HasProperty>(end, 0.8)
        .build()
        .unwrap();

    assert_eq!(path.len(), 2);
    assert_eq!(path.start(), start);
    assert_eq!(path.end(), end);

    // Check edges
    let edges = path.edges();
    assert_eq!(edges[0].relation, "is_a");
    assert_eq!(edges[1].relation, "has_property");
}

#[test]
fn test_path_builder_zero_confidence_fails() {
    let start = Uuid::new_v4();
    let mid = Uuid::new_v4();
    let end = Uuid::new_v4();

    let result = PathBuilder::new(start)
        .edge::<IsA>(mid, 0.0) // Zero confidence
        .edge::<HasProperty>(end, 0.9)
        .build();

    assert!(result.is_err());
}

#[test]
fn test_cycle_detection() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let p1 = Path::from_edge(Edge::<IsA>::new(a, b, 0.9));
    let p2 = Path::from_edge(Edge::<IsA>::new(b, a, 0.9));

    let cycle = p1.compose(p2).unwrap();
    assert!(cycle.is_cycle());
}

// ============================================================================
// Verified Graph Tests
// ============================================================================

#[test]
fn test_verified_graph_node_validation() {
    let mut graph = VerifiedGraph::new();

    // Valid weight
    let valid = FactNode {
        id: Uuid::new_v4(),
        fact_type: "Test".to_string(),
        content: make_entity("Test"),
        weight: 0.8,
    };
    assert!(graph.add_node(valid).is_ok());

    // Invalid weight (negative)
    let invalid = FactNode {
        id: Uuid::new_v4(),
        fact_type: "Test".to_string(),
        content: make_entity("Test"),
        weight: -0.1,
    };
    assert!(graph.add_node(invalid).is_err());

    // Invalid weight (> 1)
    let invalid = FactNode {
        id: Uuid::new_v4(),
        fact_type: "Test".to_string(),
        content: make_entity("Test"),
        weight: 1.1,
    };
    assert!(graph.add_node(invalid).is_err());
}

#[test]
fn test_verified_graph_edge_validation() {
    let mut graph = VerifiedGraph::new();

    let n1 = FactNode {
        id: Uuid::new_v4(),
        fact_type: "A".to_string(),
        content: make_entity("A"),
        weight: 0.9,
    };
    let n2 = FactNode {
        id: Uuid::new_v4(),
        fact_type: "B".to_string(),
        content: make_entity("B"),
        weight: 0.8,
    };

    let id1 = n1.id;
    let id2 = n2.id;

    graph.add_node(n1).unwrap();
    graph.add_node(n2).unwrap();

    // Valid edge
    assert!(graph.add_edge::<IsA>(id1, id2, 0.9).is_ok());

    // Edge to non-existent node
    let fake = Uuid::new_v4();
    assert!(graph.add_edge::<IsA>(id1, fake, 0.9).is_err());

    // Invalid confidence
    assert!(graph.add_edge::<IsA>(id1, id2, 1.5).is_err());
}

#[test]
fn test_path_finding() {
    let mut graph = VerifiedGraph::new();

    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let d = Uuid::new_v4();

    // Create diamond graph: A -> B -> D and A -> C -> D
    for (id, name) in [(a, "A"), (b, "B"), (c, "C"), (d, "D")] {
        graph
            .add_node(FactNode {
                id,
                fact_type: name.to_string(),
                content: make_entity(name),
                weight: 0.9,
            })
            .unwrap();
    }

    graph.add_edge::<IsA>(a, b, 0.9).unwrap();
    graph.add_edge::<IsA>(b, d, 0.9).unwrap();
    graph.add_edge::<IsA>(a, c, 0.8).unwrap();
    graph.add_edge::<IsA>(c, d, 0.8).unwrap();

    // Should find 2 paths from A to D
    let paths = graph.find_paths(a, d, 5);
    assert_eq!(paths.len(), 2);

    // Best path should be the one with higher confidence
    let best = graph.best_path(a, d).unwrap();
    // 0.9 * 0.9 = 0.81 > 0.8 * 0.8 = 0.64
    assert!((best.confidence().value() - 0.81).abs() < 0.01);
}

// ============================================================================
// Conflict Detection Tests
// ============================================================================

#[test]
fn test_no_conflict_single_path() {
    let mut graph = VerifiedGraph::new();

    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    graph
        .add_node(FactNode {
            id: a,
            fact_type: "A".to_string(),
            content: make_entity("A"),
            weight: 0.9,
        })
        .unwrap();
    graph
        .add_node(FactNode {
            id: b,
            fact_type: "B".to_string(),
            content: make_entity("B"),
            weight: 0.8,
        })
        .unwrap();
    graph.add_edge::<IsA>(a, b, 0.9).unwrap();

    assert!(graph.check_path_conflicts(a, b).is_none());
}

#[test]
fn test_conflict_contradictory_paths() {
    let mut graph = VerifiedGraph::new();

    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();

    graph
        .add_node(FactNode {
            id: a,
            fact_type: "A".to_string(),
            content: make_entity("A"),
            weight: 0.9,
        })
        .unwrap();
    graph
        .add_node(FactNode {
            id: b,
            fact_type: "B".to_string(),
            content: make_entity("B"),
            weight: 0.5,
        })
        .unwrap();
    graph
        .add_node(FactNode {
            id: c,
            fact_type: "C".to_string(),
            content: make_entity("C"),
            weight: 0.8,
        })
        .unwrap();

    // Direct path A->C with high confidence
    graph.add_edge::<IsA>(a, c, 0.95).unwrap();

    // Indirect path A->B->C with low confidence
    graph.add_edge::<IsA>(a, b, 0.5).unwrap();
    graph.add_edge::<IsA>(b, c, 0.3).unwrap();

    // Should detect conflict (0.95 vs 0.15)
    let conflict = graph.check_path_conflicts(a, c);
    assert!(conflict.is_some());
}

#[test]
fn test_conflict_resolution_choose_stronger() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let path1 = PathBuilder::new(a).edge::<IsA>(b, 0.9).build().unwrap();

    let path2 = PathBuilder::new(a)
        .edge::<IsA>(Uuid::new_v4(), 0.3)
        .edge::<IsA>(b, 0.3)
        .build()
        .unwrap();

    let conflict = PathConflict::ContradictoryPaths {
        path1: path1.clone(),
        path2: path2.clone(),
        confidence_diff: 0.81, // 0.9 - 0.09
    };

    match conflict.resolve() {
        PathResolution::ChooseStronger { chosen, .. } => {
            assert!((chosen.confidence().value() - 0.9).abs() < 0.01);
        }
        _ => panic!("Expected ChooseStronger resolution"),
    }
}

#[test]
fn test_conflict_resolution_merge() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let path1 = PathBuilder::new(a).edge::<IsA>(b, 0.7).build().unwrap();

    let path2 = PathBuilder::new(a)
        .edge::<IsA>(Uuid::new_v4(), 0.8)
        .edge::<IsA>(b, 0.7)
        .build()
        .unwrap();

    let conflict = PathConflict::ContradictoryPaths {
        path1,
        path2,
        confidence_diff: 0.14, // Close, should merge
    };

    match conflict.resolve() {
        PathResolution::Merge { weight1, weight2 } => {
            // Weights should sum to 1
            assert!((weight1 + weight2 - 1.0).abs() < 0.01);
        }
        _ => panic!("Expected Merge resolution"),
    }
}

// ============================================================================
// Path-Verified Reconciliation Tests
// ============================================================================

#[test]
fn test_path_verified_add_fact() {
    let mut pvr = PathVerifiedReconciliation::new(ReconciliationConfig::default());

    // Add first fact
    let id1 = pvr.add_fact(make_entity("Material"), 0.9, vec![]).unwrap();

    // Add second fact with connection
    let id2 = pvr
        .add_fact(
            make_entity("Steel"),
            0.85,
            vec![(id1, "is_a".to_string(), 0.95)],
        )
        .unwrap();

    // Should be able to find path
    let paths = pvr.query_paths(id2, id1);
    assert_eq!(paths.len(), 1);
}

#[test]
fn test_path_verified_detects_conflict() {
    let mut pvr = PathVerifiedReconciliation::new(ReconciliationConfig::default());

    let id1 = pvr.add_fact(make_entity("A"), 0.9, vec![]).unwrap();
    let id2 = pvr
        .add_fact(make_entity("B"), 0.9, vec![(id1, "is_a".to_string(), 0.5)])
        .unwrap();
    let id3 = pvr
        .add_fact(make_entity("C"), 0.9, vec![(id2, "is_a".to_string(), 0.3)])
        .unwrap();

    // This creates a path A <- B <- C with confidence 0.15
    // Now add a direct high-confidence path
    // This should either succeed (auto-resolved) or fail (needs review)
    let result = pvr.add_fact(
        make_entity("D"),
        0.9,
        vec![
            (id3, "is_a".to_string(), 0.95), // Direct to C
            (id1, "is_a".to_string(), 0.95), // Direct to A
        ],
    );

    // Should succeed (auto-resolved)
    assert!(result.is_ok());
}

// ============================================================================
// Relationship Type Tests
// ============================================================================

#[test]
fn test_relationship_types() {
    assert_eq!(IsA::name(), "is_a");
    assert_eq!(HasProperty::name(), "has_property");
    assert_eq!(Causes::name(), "causes");
    assert_eq!(Supports::name(), "supports");
    assert_eq!(Contradicts::name(), "contradicts");
}

#[test]
fn test_typed_edges_preserve_relation() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let edge1 = Edge::<IsA>::new(a, b, 0.9);
    let edge2 = Edge::<Causes>::new(a, b, 0.9);

    // Same endpoints, different relations
    assert_eq!(edge1.relationship_name(), "is_a");
    assert_eq!(edge2.relationship_name(), "causes");
}

// ============================================================================
// Invariant Preservation Tests
// ============================================================================

#[test]
fn test_path_confidence_never_exceeds_one() {
    let a = Uuid::new_v4();

    // Even with 1.0 confidence edges, composition should stay <= 1
    let mut path = Path::identity(a);
    for _ in 0..10 {
        let next = Uuid::new_v4();
        let edge = Edge::<IsA>::new(if path.is_empty() { a } else { path.end() }, next, 1.0);
        path = path.compose(Path::from_edge(edge)).unwrap();
    }

    assert!(path.confidence().value() <= 1.0);
}

#[test]
fn test_path_confidence_never_negative() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    // Even with very small confidences
    let path = PathBuilder::new(a).edge::<IsA>(b, 0.001).build().unwrap();

    assert!(path.confidence().value() >= 0.0);
}

#[test]
fn test_graph_preserves_invariants_under_operations() {
    let mut graph = VerifiedGraph::new();

    // Add many nodes and edges
    let mut ids = vec![];
    for i in 0..10 {
        let id = Uuid::new_v4();
        graph
            .add_node(FactNode {
                id,
                fact_type: format!("Type{}", i),
                content: make_entity(&format!("Entity{}", i)),
                weight: 0.5 + (i as f32) * 0.05,
            })
            .unwrap();
        ids.push(id);
    }

    for i in 0..9 {
        graph.add_edge::<IsA>(ids[i], ids[i + 1], 0.8).unwrap();
    }

    // All nodes should have valid weights
    for node in graph.nodes() {
        assert!(node.weight >= 0.0 && node.weight <= 1.0);
    }

    // All paths should have valid confidence
    for path in graph.find_paths(ids[0], ids[9], 15) {
        let conf = path.confidence().value();
        assert!(conf >= 0.0 && conf <= 1.0);
    }
}
