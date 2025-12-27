//! Path Verification: Type-safe path validation for reconciliation
//!
//! This module provides runtime verification of paths that mirrors
//! the Lean-checked semantics and certificate discipline. While we can't have full dependent
//! types in Rust, we use:
//!
//! 1. **Phantom types** for relationship types
//! 2. **Builder patterns** for constructing valid paths
//! 3. **Verus-compatible annotations** for verification
//! 4. **Runtime checks** that enforce invariants
//!
//! The goal is to ensure that when facts are reconciled, their
//! connections form valid paths that can be audited and (where appropriate)
//! certificate-checked in Lean.

#![allow(unused_imports, unused_variables, dead_code)]

use crate::reconciliation::*;
use crate::{ConflictType, StructuredFact};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use uuid::Uuid;

// ============================================================================
// Typed Edges
// ============================================================================

/// Marker trait for relationship types
pub trait Relationship: Clone + Send + Sync + 'static {
    fn name() -> &'static str;
}

/// Generic relationship (for dynamic typing)
#[derive(Debug, Clone)]
pub struct GenericRel;
impl Relationship for GenericRel {
    fn name() -> &'static str {
        "generic"
    }
}

/// "Is-A" relationship (subtype)
#[derive(Debug, Clone)]
pub struct IsA;
impl Relationship for IsA {
    fn name() -> &'static str {
        "is_a"
    }
}

/// "Has-Property" relationship
#[derive(Debug, Clone)]
pub struct HasProperty;
impl Relationship for HasProperty {
    fn name() -> &'static str {
        "has_property"
    }
}

/// "Causes" relationship
#[derive(Debug, Clone)]
pub struct Causes;
impl Relationship for Causes {
    fn name() -> &'static str {
        "causes"
    }
}

/// "Supports" relationship (evidence)
#[derive(Debug, Clone)]
pub struct Supports;
impl Relationship for Supports {
    fn name() -> &'static str {
        "supports"
    }
}

/// "Contradicts" relationship
#[derive(Debug, Clone)]
pub struct Contradicts;
impl Relationship for Contradicts {
    fn name() -> &'static str {
        "contradicts"
    }
}

/// A typed edge between facts
#[derive(Debug, Clone)]
pub struct Edge<R: Relationship> {
    pub source: Uuid,
    pub target: Uuid,
    pub confidence: Weight,
    pub _marker: PhantomData<R>,
}

impl<R: Relationship> Edge<R> {
    pub fn new(source: Uuid, target: Uuid, confidence: f32) -> Self {
        Self {
            source,
            target,
            confidence: Weight::new(confidence),
            _marker: PhantomData,
        }
    }

    pub fn relationship_name(&self) -> &'static str {
        R::name()
    }
}

// ============================================================================
// Typed Paths
// ============================================================================

/// A path is a sequence of edges
#[derive(Debug, Clone)]
pub struct Path {
    edges: Vec<EdgeData>,
    start: Uuid,
    end: Uuid,
}

#[derive(Debug, Clone)]
pub struct EdgeData {
    pub source: Uuid,
    pub target: Uuid,
    pub relation: String,
    pub confidence: Weight,
}

impl Path {
    /// Create an identity path
    pub fn identity(node: Uuid) -> Self {
        Self {
            edges: vec![],
            start: node,
            end: node,
        }
    }

    /// Create a path from a single edge
    pub fn from_edge<R: Relationship>(edge: Edge<R>) -> Self {
        Self {
            edges: vec![EdgeData {
                source: edge.source,
                target: edge.target,
                relation: R::name().to_string(),
                confidence: edge.confidence,
            }],
            start: edge.source,
            end: edge.target,
        }
    }

    /// Compose two paths (if compatible)
    pub fn compose(self, other: Path) -> Result<Path, PathError> {
        if self.end != other.start {
            return Err(PathError::IncompatibleEndpoints {
                first_end: self.end,
                second_start: other.start,
            });
        }

        let mut edges = self.edges;
        edges.extend(other.edges);

        Ok(Path {
            edges,
            start: self.start,
            end: other.end,
        })
    }

    /// Get path length
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Compute path confidence (product of edge confidences)
    pub fn confidence(&self) -> Weight {
        if self.edges.is_empty() {
            return Weight::new(1.0);
        }

        let product: f32 = self.edges.iter().map(|e| e.confidence.value()).product();

        Weight::new(product)
    }

    /// Get start node
    pub fn start(&self) -> Uuid {
        self.start
    }

    /// Get end node
    pub fn end(&self) -> Uuid {
        self.end
    }

    /// Check if this is a cycle
    pub fn is_cycle(&self) -> bool {
        self.start == self.end && !self.edges.is_empty()
    }

    /// Get edges
    pub fn edges(&self) -> &[EdgeData] {
        &self.edges
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("Cannot compose paths: first ends at {first_end}, second starts at {second_start}")]
    IncompatibleEndpoints { first_end: Uuid, second_start: Uuid },
    #[error("Path has zero confidence")]
    ZeroConfidence,
    #[error("Invalid path: {reason}")]
    Invalid { reason: String },
}

// ============================================================================
// Path Builder (Ensures Valid Construction)
// ============================================================================

/// Builder for constructing valid paths
pub struct PathBuilder {
    current: Option<Uuid>,
    edges: Vec<EdgeData>,
}

impl PathBuilder {
    pub fn new(start: Uuid) -> Self {
        Self {
            current: Some(start),
            edges: vec![],
        }
    }

    /// Add an edge to the path
    pub fn edge<R: Relationship>(mut self, target: Uuid, confidence: f32) -> Self {
        let source = self.current.expect("Path already terminated");
        self.edges.push(EdgeData {
            source,
            target,
            relation: R::name().to_string(),
            confidence: Weight::new(confidence),
        });
        self.current = Some(target);
        self
    }

    /// Build the path
    pub fn build(self) -> Result<Path, PathError> {
        let start = self
            .edges
            .first()
            .map(|e| e.source)
            .ok_or(PathError::Invalid {
                reason: "Empty path".to_string(),
            })?;

        let end = self.current.ok_or(PathError::Invalid {
            reason: "Path not properly constructed".to_string(),
        })?;

        let path = Path {
            edges: self.edges,
            start,
            end,
        };

        // Verify confidence is positive
        if path.confidence().value() == 0.0 {
            return Err(PathError::ZeroConfidence);
        }

        Ok(path)
    }
}

// ============================================================================
// Verified Graph
// ============================================================================

/// A knowledge graph with verified structure
#[derive(Debug, Clone)]
pub struct VerifiedGraph {
    nodes: HashMap<Uuid, FactNode>,
    edges: Vec<EdgeData>,
    // Index: source -> edges from source
    outgoing: HashMap<Uuid, Vec<usize>>,
    // Index: target -> edges to target
    incoming: HashMap<Uuid, Vec<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactNode {
    pub id: Uuid,
    pub fact_type: String,
    pub content: StructuredFact,
    pub weight: f32,
}

impl VerifiedGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: vec![],
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    /// Add a node with validation
    pub fn add_node(&mut self, node: FactNode) -> Result<(), GraphError> {
        // Validate weight
        if node.weight < 0.0 || node.weight > 1.0 {
            return Err(GraphError::InvalidWeight(node.weight));
        }

        self.nodes.insert(node.id, node);
        Ok(())
    }

    /// Add an edge with validation
    pub fn add_edge<R: Relationship>(
        &mut self,
        source: Uuid,
        target: Uuid,
        confidence: f32,
    ) -> Result<(), GraphError> {
        // Validate nodes exist
        if !self.nodes.contains_key(&source) {
            return Err(GraphError::NodeNotFound(source));
        }
        if !self.nodes.contains_key(&target) {
            return Err(GraphError::NodeNotFound(target));
        }

        // Validate confidence
        if confidence < 0.0 || confidence > 1.0 {
            return Err(GraphError::InvalidConfidence(confidence));
        }

        let edge_idx = self.edges.len();
        self.edges.push(EdgeData {
            source,
            target,
            relation: R::name().to_string(),
            confidence: Weight::new(confidence),
        });

        self.outgoing.entry(source).or_default().push(edge_idx);
        self.incoming.entry(target).or_default().push(edge_idx);

        Ok(())
    }

    /// Find all paths between two nodes up to a maximum length
    pub fn find_paths(&self, from: Uuid, to: Uuid, max_len: usize) -> Vec<Path> {
        if from == to {
            return vec![Path::identity(from)];
        }

        let mut results = vec![];
        let mut visited = HashSet::new();
        let mut stack = vec![(from, vec![], 0)];

        while let Some((current, path_edges, depth)) = stack.pop() {
            if depth > max_len {
                continue;
            }

            if current == to && !path_edges.is_empty() {
                results.push(Path {
                    edges: path_edges.clone(),
                    start: from,
                    end: to,
                });
                continue;
            }

            visited.insert(current);

            if let Some(edge_indices) = self.outgoing.get(&current) {
                for &idx in edge_indices {
                    let edge = &self.edges[idx];
                    if !visited.contains(&edge.target) {
                        let mut new_path = path_edges.clone();
                        new_path.push(edge.clone());
                        stack.push((edge.target, new_path, depth + 1));
                    }
                }
            }

            visited.remove(&current);
        }

        results
    }

    /// Find the best (highest confidence) path
    pub fn best_path(&self, from: Uuid, to: Uuid) -> Option<Path> {
        let paths = self.find_paths(from, to, 5);
        paths.into_iter().max_by(|a, b| {
            a.confidence()
                .value()
                .partial_cmp(&b.confidence().value())
                .unwrap()
        })
    }

    /// Check for conflicts along paths
    pub fn check_path_conflicts(&self, from: Uuid, to: Uuid) -> Option<PathConflict> {
        let paths = self.find_paths(from, to, 5);

        if paths.len() < 2 {
            return None;
        }

        // Check if any two paths have significantly different confidences
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                let conf_diff =
                    (paths[i].confidence().value() - paths[j].confidence().value()).abs();
                if conf_diff > 0.3 {
                    return Some(PathConflict::ContradictoryPaths {
                        path1: paths[i].clone(),
                        path2: paths[j].clone(),
                        confidence_diff: conf_diff,
                    });
                }
            }
        }

        None
    }

    /// Get node by ID
    pub fn get_node(&self, id: &Uuid) -> Option<&FactNode> {
        self.nodes.get(id)
    }

    /// Get all nodes
    pub fn nodes(&self) -> impl Iterator<Item = &FactNode> {
        self.nodes.values()
    }

    /// Get all edges (crate-visible; `EdgeData` is an internal representation).
    pub(crate) fn edges(&self) -> &[EdgeData] {
        &self.edges
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get edge count
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(Uuid),
    #[error("Invalid weight: {0} (must be 0-1)")]
    InvalidWeight(f32),
    #[error("Invalid confidence: {0} (must be 0-1)")]
    InvalidConfidence(f32),
}

// ============================================================================
// Path Conflicts
// ============================================================================

#[derive(Debug, Clone)]
pub enum PathConflict {
    /// Two paths to same target with different confidences
    ContradictoryPaths {
        path1: Path,
        path2: Path,
        confidence_diff: f32,
    },
    /// Cycle that implies impossible constraint
    ImpossibleCycle { cycle: Path },
}

#[derive(Debug, Clone)]
pub enum PathResolution {
    /// Choose the stronger path
    ChooseStronger { chosen: Path, rejected: Path },
    /// Merge path confidences
    Merge { weight1: f32, weight2: f32 },
    /// Need more evidence
    NeedMoreEvidence,
    /// Human review required
    HumanReview,
}

impl PathConflict {
    /// Resolve the conflict
    pub fn resolve(&self) -> PathResolution {
        match self {
            PathConflict::ContradictoryPaths {
                path1,
                path2,
                confidence_diff,
            } => {
                let c1 = path1.confidence().value();
                let c2 = path2.confidence().value();

                if *confidence_diff > 0.5 {
                    // Clear winner
                    if c1 > c2 {
                        PathResolution::ChooseStronger {
                            chosen: path1.clone(),
                            rejected: path2.clone(),
                        }
                    } else {
                        PathResolution::ChooseStronger {
                            chosen: path2.clone(),
                            rejected: path1.clone(),
                        }
                    }
                } else {
                    // Close, merge
                    let total = c1 + c2;
                    PathResolution::Merge {
                        weight1: c1 / total,
                        weight2: c2 / total,
                    }
                }
            }
            PathConflict::ImpossibleCycle { .. } => PathResolution::HumanReview,
        }
    }
}

// ============================================================================
// Path-Verified Reconciliation
// ============================================================================

/// Reconciliation with path verification
pub struct PathVerifiedReconciliation {
    graph: VerifiedGraph,
    engine: ReconciliationEngine,
}

impl PathVerifiedReconciliation {
    pub fn new(config: ReconciliationConfig) -> Self {
        Self {
            graph: VerifiedGraph::new(),
            engine: ReconciliationEngine::new(config),
        }
    }

    /// Add a fact with path verification
    pub fn add_fact(
        &mut self,
        fact: StructuredFact,
        weight: f32,
        connections: Vec<(Uuid, String, f32)>, // (target, relation, confidence)
    ) -> Result<Uuid, PathVerificationError> {
        let fact_id = Uuid::new_v4();

        // Create node
        let node = FactNode {
            id: fact_id,
            fact_type: fact.type_name(),
            content: fact.clone(),
            weight,
        };

        // Add to graph
        self.graph.add_node(node)?;

        // Add connections and check for conflicts
        for (target, relation, confidence) in connections {
            self.graph
                .add_edge::<GenericRel>(fact_id, target, confidence)?;

            // Check if this creates any path conflicts
            for existing in self.graph.nodes() {
                if existing.id != fact_id && existing.id != target {
                    if let Some(conflict) = self.graph.check_path_conflicts(fact_id, existing.id) {
                        // Resolve or report
                        match conflict.resolve() {
                            PathResolution::HumanReview => {
                                return Err(PathVerificationError::ConflictNeedsReview(conflict));
                            }
                            PathResolution::NeedMoreEvidence => {
                                return Err(PathVerificationError::InsufficientEvidence);
                            }
                            _ => {
                                // Auto-resolved, continue
                            }
                        }
                    }
                }
            }
        }

        Ok(fact_id)
    }

    /// Query paths between facts
    pub fn query_paths(&self, from: Uuid, to: Uuid) -> Vec<Path> {
        self.graph.find_paths(from, to, 5)
    }

    /// Get best path
    pub fn best_path(&self, from: Uuid, to: Uuid) -> Option<Path> {
        self.graph.best_path(from, to)
    }

    /// Get underlying graph
    pub fn graph(&self) -> &VerifiedGraph {
        &self.graph
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PathVerificationError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("Conflict needs human review: {0:?}")]
    ConflictNeedsReview(PathConflict),
    #[error("Insufficient evidence to establish connection")]
    InsufficientEvidence,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_entity(name: &str) -> StructuredFact {
        StructuredFact::Entity {
            entity_type: "Test".to_string(),
            name: name.to_string(),
            attributes: HashMap::new(),
        }
    }

    #[test]
    fn test_path_builder() {
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
        assert!((path.confidence().value() - 0.72).abs() < 0.01);
    }

    #[test]
    fn test_path_composition() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        let p1 = Path::from_edge(Edge::<IsA>::new(a, b, 0.9));
        let p2 = Path::from_edge(Edge::<HasProperty>::new(b, c, 0.8));

        let composed = p1.compose(p2).unwrap();
        assert_eq!(composed.len(), 2);
        assert_eq!(composed.start(), a);
        assert_eq!(composed.end(), c);
    }

    #[test]
    fn test_verified_graph() {
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
        graph.add_edge::<IsA>(id1, id2, 0.85).unwrap();

        let paths = graph.find_paths(id1, id2, 3);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_conflict_detection() {
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
                weight: 0.8,
            })
            .unwrap();
        graph
            .add_node(FactNode {
                id: c,
                fact_type: "C".to_string(),
                content: make_entity("C"),
                weight: 0.7,
            })
            .unwrap();

        // Two paths A->C with very different confidences
        graph.add_edge::<IsA>(a, c, 0.95).unwrap();
        graph.add_edge::<IsA>(a, b, 0.9).unwrap();
        graph.add_edge::<IsA>(b, c, 0.3).unwrap();

        let conflict = graph.check_path_conflicts(a, c);
        assert!(conflict.is_some());
    }
}
