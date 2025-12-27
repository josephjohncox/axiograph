# Path Verification: Dependent Types for Knowledge Graph Connections

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): Path equivalence/normalization/reachability are checked via Lean certificates.
> Idris snippets below are historical notes from an earlier prototype and should be ported/updated to Lean.

## Core Insight

**Paths in the knowledge graph are proofs of relationships.**

When we say "Steel is-a Material", we're asserting a connection. When we have multiple ways to derive this (direct assertion vs. inference chain), these are different *proofs* of the same relationship.

This maps perfectly to dependent type theory:
- **Path = Proof of connection**
- **Path equivalence = Multiple proofs of same fact**
- **Path composition = Transitive reasoning**
- **Path conflict = Contradictory proofs**

## Type-Theoretic Foundation

### In Idris

```idris
-- A typed path between facts
data TypedPath : (start : Type) -> (end : Type) -> Type where
  PathId    : TypedPath a a                              -- Identity
  PathEdge  : Edge a b r -> TypedPath a b                -- Single step
  PathTrans : TypedPath a b -> TypedPath b c -> TypedPath a c  -- Composition

-- Path equivalence (HoTT-style)
data PathEquiv : TypedPath a b -> TypedPath a b -> Type where
  PathRefl    : PathEquiv p p
  PathIdRight : (p : TypedPath a b) -> PathEquiv (PathTrans p PathId) p
  PathAssoc   : PathEquiv (PathTrans (PathTrans p q) r) (PathTrans p (PathTrans q r))
```

### Key Properties (Proved in Idris)

1. **Weight Preservation**: Path operations preserve probability bounds
   ```idris
   equivPreservesConfidence : PathEquiv p q -> 
                              So (abs (pathConfidence p).value - (pathConfidence q).value < ε)
   ```

2. **Composition Preserves Validity**: Valid paths compose to valid paths
   ```idris
   composePreservesValidity : (p : TypedPath a b) -> (q : TypedPath b c) ->
                              So ((pathConfidence p).value > 0.0) ->
                              So ((pathConfidence q).value > 0.0) ->
                              So ((pathConfidence (PathTrans p q)).value > 0.0)
   ```

3. **Path Equivalence → Same Derivation**: Equivalent paths derive same relationship
   ```idris
   equivSameDerivation : PathEquiv p q -> (derives : TypedPath a b -> c) -> derives p = derives q
   ```

## Rust Implementation

### Typed Edges

```rust
/// Marker trait for relationship types
pub trait Relationship: Clone + Send + Sync + 'static {
    fn name() -> &'static str;
}

/// "Is-A" relationship (subtype)
#[derive(Debug, Clone)]
pub struct IsA;
impl Relationship for IsA {
    fn name() -> &'static str { "is_a" }
}

/// A typed edge between facts
pub struct Edge<R: Relationship> {
    pub source: Uuid,
    pub target: Uuid,
    pub confidence: Weight,
    pub _marker: PhantomData<R>,
}
```

### Path Builder (Ensures Valid Construction)

```rust
let path = PathBuilder::new(start)
    .edge::<IsA>(mid, 0.9)           // Steel is-a Metal
    .edge::<HasProperty>(end, 0.8)   // Metal has-property Conductivity
    .build()
    .unwrap();

// Confidence is automatically computed: 0.9 * 0.8 = 0.72
assert!((path.confidence().value() - 0.72).abs() < 0.01);
```

### Verified Graph

```rust
pub struct VerifiedGraph {
    nodes: HashMap<Uuid, FactNode>,
    edges: Vec<EdgeData>,
    // Invariant: all edges connect existing nodes
    // Invariant: all weights in [0, 1]
}

impl VerifiedGraph {
    /// Add node with validation
    pub fn add_node(&mut self, node: FactNode) -> Result<(), GraphError> {
        if node.weight < 0.0 || node.weight > 1.0 {
            return Err(GraphError::InvalidWeight(node.weight));
        }
        self.nodes.insert(node.id, node);
        Ok(())
    }
}
```

## Path-Based Conflict Detection

### The Problem

Consider two paths from A to C:
- Direct: `A --[0.95]--> C`
- Indirect: `A --[0.3]--> B --[0.3]--> C` (confidence: 0.09)

The confidence difference (0.86) suggests contradictory evidence.

### Detection

```rust
pub fn check_path_conflicts(&self, from: Uuid, to: Uuid) -> Option<PathConflict> {
    let paths = self.find_paths(from, to, 5);
    
    if paths.len() < 2 { return None; }
    
    for i in 0..paths.len() {
        for j in (i+1)..paths.len() {
            let conf_diff = (paths[i].confidence().value() - 
                            paths[j].confidence().value()).abs();
            if conf_diff > 0.3 {
                return Some(PathConflict::ContradictoryPaths { ... });
            }
        }
    }
    None
}
```

### Resolution Strategies

```rust
pub enum PathResolution {
    /// Choose the stronger path
    ChooseStronger { chosen: Path, rejected: Path },
    /// Merge path confidences (when close)
    Merge { weight1: f32, weight2: f32 },
    /// Need more evidence
    NeedMoreEvidence,
    /// Human review required
    HumanReview,
}
```

## Reconciliation with Path Verification

```rust
let mut pvr = PathVerifiedReconciliation::new(config);

// Add base knowledge
let material_id = pvr.add_fact(make_entity("Material"), 0.9, vec![])?;

// Add fact with connection - automatically checks for conflicts
let steel_id = pvr.add_fact(
    make_entity("Steel"),
    0.85,
    vec![(material_id, "is_a".to_string(), 0.95)],
)?;

// Query verified paths
let paths = pvr.query_paths(steel_id, material_id);
```

## Idris Verification

### Export for Verification

```rust
let export = pvr.export_for_idris();
// {
//   "nodes": [("uuid1", 0.9), ("uuid2", 0.85)],
//   "edges": [("uuid2", "uuid1", 0.95)]
// }
```

### Verify in Idris

```idris
-- Load and verify
verifyExport : IdrisVerificationData -> Either String ()
verifyExport data = do
  -- All weights valid
  for_ data.nodes $ \(id, w) =>
    unless (w >= 0.0 && w <= 1.0) $
      Left "Invalid weight"
  
  -- All paths have valid confidence
  for_ (allPaths data) $ \path =>
    unless (pathConfidence path).value >= 0.0 $
      Left "Invalid path confidence"
  
  Right ()
```

## Mathematical Properties

### 1. Category Structure

Paths form a category:
- Objects: Facts (nodes)
- Morphisms: Paths
- Identity: `PathId`
- Composition: `PathTrans`

Category laws hold:
- Left identity: `PathId ∘ p = p`
- Right identity: `p ∘ PathId = p`
- Associativity: `(p ∘ q) ∘ r = p ∘ (q ∘ r)`

### 2. Groupoid Structure (with HoTT)

When paths are reversible (bidirectional relations), we get a groupoid:
- Every path has an inverse
- Path equivalence is an equivalence relation

### 3. Confidence as Functor

Confidence maps paths to probabilities:
- `conf(PathId) = 1.0`
- `conf(p ∘ q) = conf(p) × conf(q)`

This is a functor from the path category to `([0,1], ×, 1)`.

## Integration Points

### With PathDB

```rust
// PathDB uses same path model
let pathdb = PathDB::new();
pathdb.add_entity("Steel", "Material");
pathdb.add_relation("Steel", "is_a", "Material", 0.95);

// Query paths with confidence
let paths = pathdb.find_paths_with_confidence("Steel", "Metal");
```

### With Reconciliation Engine

```rust
// Path verification integrated with reconciliation
let mut engine = ReconciliationEngine::new(config);

// When reconciling, check paths
engine.on_reconcile(|new_fact, existing| {
    let paths = graph.find_paths(new_fact.id, existing.id);
    if paths.len() > 1 {
        // Check for conflicts
    }
});
```

### With LLM Sync

```rust
// When LLM suggests new fact, verify paths
sync_manager.on_llm_suggestion(|fact| {
    // Build connections
    let connections = extract_connections(&fact);
    
    // Verify no path conflicts
    pvr.add_fact(fact, confidence, connections)?;
});
```

## Guarantees

| Property | Verified By | Mechanism |
|----------|-------------|-----------|
| Weight bounds [0,1] | Idris + Rust | `So` proofs + runtime checks |
| Path composition valid | Idris | `PathTrans` type |
| No orphan edges | Rust | `VerifiedGraph` invariants |
| Conflict detection | Rust | `check_path_conflicts` |
| Resolution valid | Idris | `ValidResolution` type |

## Files

| File | Purpose |
|------|---------|
| `idris/Axiograph/Prob/ReconciliationProofs.idr` | Core proofs |
| `idris/Axiograph/Prob/PathVerification.idr` | Path-specific verification |
| `rust/.../path_verification.rs` | Rust implementation |
| `tests/path_verification_tests.rs` | E2E tests |
