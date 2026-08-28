# Path Verification: Typed Path Witnesses And Certificates

**Diataxis:** Explanation  
**Audience:** contributors

> Current status: path equivalence and normalization are checked by Lean
> certificates for supported finite fragments. Runtime path checks remain
> operational guardrails unless paired with accepted anchors and a verified
> certificate.

## Core Insight

Anchored path witnesses can become proof-carrying evidence for supported
finite fragments.

When we say "Steel is-a Material", we're asserting a connection. When we have multiple ways to derive this (direct assertion vs. inference chain), these are different *proofs* of the same relationship.

This maps perfectly to dependent type theory:

- **Path = Proof of connection**
- **Path equivalence = Multiple proofs of same fact**
- **Path composition = Transitive reasoning**
- **Path conflict = Contradictory proofs**

## Type-Theoretic Foundation

### In Lean

The current trusted checker lives in Lean. Rust emits certificates; Lean checks
those certificates against a small semantic kernel.

```lean
inductive TypedPath (obj : Type) : obj → obj → Type where
  | id : TypedPath obj a a
  | edge : Edge a b r → TypedPath obj a b
  | trans : TypedPath obj a b → TypedPath obj b c → TypedPath obj a c

inductive PathEquiv : TypedPath obj a b → TypedPath obj a b → Prop where
  | refl : PathEquiv p p
  | idRight : PathEquiv (TypedPath.trans p TypedPath.id) p
  | assoc : PathEquiv (TypedPath.trans (TypedPath.trans p q) r)
                      (TypedPath.trans p (TypedPath.trans q r))
```

### Key Properties Checked by Lean

1. **Probability bounds are separate**: `VProb` operations stay in range, but their rounded values are not path-equality invariants.
2. **Composition preserves validity**: valid endpoint-aligned paths compose to valid paths.
3. **Path equivalence preserves denotation**: accepted mandatory traces connect endpoint-indexed paths with the same free-groupoid morphism.

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

## Lean Certificate Verification

Path verification is no longer exposed as a raw relation-id HTTP/MCP/CLI
certificate API. User- and agent-facing certification should go through typed
query witnesses over canonical `.axi` anchors. Low-level `reachability_v3`
remains available as a canonical certificate family for fixtures and narrow
path-witness work.

### Verify in Lean

```lean
def verifyReachabilityCertificate
    (snapshot : Snapshot)
    (cert : ReachabilityCertificate) : Except CheckError VerifiedReachability := do
  checkNodes snapshot cert.nodes
  checkEdges snapshot cert.edges
  replayReachabilityProof snapshot cert.proof
```

## Mathematical Properties

### 1. Endpoint-Indexed Category Paths

`Axiograph.Theory.Finite.Path` and the certificate retyping boundary index every
path by its source and target. Identity is an endomorphism and composition is
constructible only when the middle endpoints agree. Left unit, right unit, and
associativity hold under the declared path equivalence or free-groupoid
denotation; differently parenthesized syntax trees are not definitionally equal.

### 2. Formal Free-Groupoid Paths

`GroupoidPath` adds formal inverse syntax and denotes it into mathlib's
`Quiver.FreeGroupoid`. Lean proves unit, inverse, associativity,
composition-congruence, and inverse-congruence laws. The anchored
`category_kernel_v3` certificate also replays an explicit cancellation trace
for both inverse laws of every presented generator.

This is a statement about formal proof syntax. It does not assert that every
ontology relation has a converse fact or that a backend may execute an inverse
step. Runtime inverse traversal requires an explicitly reversible generator.

### 3. Confidence Is Not Path Equality

Fixed-point `vMult` rounds after each multiplication and is not associative.
Consequently confidence is a syntax-directed evidence fold, not a functor from
the path category, not invariant under reassociation, and not preserved by
free-groupoid cancellation. Path equality and normalization certificates carry
no confidence field. Confidence may rank unreduced derivations, but it cannot
justify or distinguish their semantic equality.

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

// When reconciling, inspect multiple derivations.
engine.on_reconcile(|new_fact, existing| {
    let paths = graph.find_paths(new_fact.id, existing.id);
    if paths.len() > 1 {
        // Compare evidence; do not infer a logical contradiction from weights.
    }
});
```

### With LLM Sync

```rust
// When LLM suggests new fact, verify paths
sync_manager.on_llm_suggestion(|fact| {
    // Build connections
    let connections = extract_connections(&fact);
    
    // Apply runtime path/evidence checks; ontology contradictions require
    // explicit polarity, disjointness, cardinality, or another typed constraint.
    pvr.add_fact(fact, confidence, connections)?;
});
```

## Guarantees

| Property | Verified By | Mechanism |
| ---------- | ------------- | ----------- |
| Certificate probability bounds | Lean | `VProb` construction and parsing |
| Path composition valid | Lean + Rust parity | Endpoint-indexed constructors and replay |
| Rewrite preserves path denotation | Lean | Mandatory traces plus free-groupoid soundness theorem |
| Formal inverse cancellation | Lean + Rust parity | `category_kernel_v3` signed normalization traces |
| No orphan edges in one runtime graph | Rust | `VerifiedGraph` local invariants |
| Confidence-difference scan | Rust heuristic | `check_path_conflicts`; not contradiction detection |
| Resolution procedure parity | Lean | Fixed-point reconciliation certificate recomputation |

## Files

| File | Purpose |
| ------ | --------- |
| `lean/Axiograph/HoTT/*` | Path/groupoid semantics |
| `lean/Axiograph/Certificate/*` | Certificate formats and checkers |
| `lean/Axiograph/Prob/*` | Fixed-point probability proofs |
| `rust/.../path_verification.rs` | Rust implementation |
| `tests/path_verification_tests.rs` | E2E tests |
