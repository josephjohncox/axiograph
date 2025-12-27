# Mathematical Extensions Roadmap

**Diataxis:** Roadmap  
**Audience:** contributors

> NOTE (Rust+Lean release): Lean is the trusted semantics/checker layer. Any Idris snippets below are historical notes from an earlier prototype and should be ported/updated to Lean.

For process/quality/reuse roadmapping (competency questions, linting, patterns, modularisation, alignment, OBDA), see `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`.

## 2025–2026 migration-aligned deliverables (Lean + Rust, certificate-first)

This repo’s committed architecture is **untrusted engine / trusted checker**:

- Rust implements execution/optimization/migration and emits certificates.
- Lean (mathlib-backed) defines semantics and verifies certificates.

The literature in Appendix C of `docs/explanation/BOOK.md` points to a very concrete path:

1. Treat schemas as **finitely presented categories** and instances as **Set-valued functors**.
2. Implement **functorial data migration** operators (`Δ_F`, `Σ_F`, `Π_F`) as runtime code + certificates + checkers.
3. Tighten rewrite/groupoid semantics by reusing mathlib’s free constructions and proving rewrite-step soundness.
4. Harden Rust via fuzzing + Miri/Kani/Verus layering (optional, additive).

### Near-term (1–4 weeks): close the first migration loop

- [x] Add a `delta_f_v1` certificate kind and Lean checker (recompute-and-compare first).
- [x] Add a canonical “schema migration” example + e2e test (Rust emits cert, Lean checks).
- [x] Extend `verify-semantics` to include migration certificates.
- [x] Add a `path_equiv_v2` certificate kind (groupoid equivalence via shared normalization + optional derivations).
- [x] Add a `rewrite_derivation_v2` certificate kind (replayable rewrite traces: rule + position).
- [x] Add anchored reachability v2 (optional `axi_digest_v1` + `relation_id` checked against `PathDBExportV1` snapshots).
- [x] Add reversible PathDB snapshot export/import as `.axi` (`PathDBExportV1`) with Rust↔Lean parse parity checks.
- [ ] Add a `sigma_f_v1` scaffold certificate kind (explicit TODOs: ID generation, quotienting/colimits).
- [ ] Anchor certificates to canonical `.axi` inputs (module digest + extracted fact IDs).
- [ ] Add a provenance/explanation algebra for query certificates (query provenance / semiring-style composition), aligning certificate composition with query operators.

### Next (1–2 months): functoriality + equivalences

- [ ] Define schema morphisms + **natural transformations** (2-cells) as first-class.
- [ ] Implement `Π_F` (right Kan extension) scaffold + certificates.
- [ ] Represent relations as **edge objects** (with projection arrows) in the core semantics so all migration operators treat relations uniformly.
- [ ] Add proof-carrying rewrite derivations beyond normalization/equivalence (reconciliation explanations, domain/unit rewrites).

### Hardening track (parallel, optional)

- [ ] Add fuzz targets for PathDB bytes, certificate JSON, `.axi` parsing, and FFI.
- [ ] Add `make verify-miri` / `make verify-kani` (optional; no-op if tools not installed).
- [ ] Add Verus proofs for the smallest, highest-risk invariants (bounds, offsets, witness endpoints, determinism).

## Priority Matrix

```
                    IMPACT
              Low    Medium    High
         ┌─────────┬─────────┬─────────┐
    Low  │         │ Sized   │ Enriched│
         │         │ Types   │ Cat     │
EFFORT   ├─────────┼─────────┼─────────┤
  Medium │ Subtype │ Graded  │ Natural │
         │         │ Monads  │ Trans   │
         ├─────────┼─────────┼─────────┤
    High │ OTT     │ Topos   │ Cubical │
         │         │ Sheaves │ TT      │
         └─────────┴─────────┴─────────┘
```

---

## Tier 1: Quick Wins (1-2 weeks each)

### 1.1 Natural Transformations

**What:** Add 2-cells between schema functors.

**Why:** Schema morphisms need to compose up to isomorphism, not just equality.

**Implementation:**
```idris
-- Natural transformation between functors
record NatTrans (F G : Functor s1 s2) where
  component : (a : s1.Obj) -> Hom (F.mapObj a) (G.mapObj a)
  naturality : {a, b : s1.Obj} -> (f : Hom a b) ->
               component b . F.mapMor f = G.mapMor f . component a
```

**Effort:** Low | **Impact:** High

### 1.2 Enriched Confidence

**What:** Formally treat confidence as enrichment over $[0,1]$.

**Why:** Ad-hoc confidence handling lacks compositionality.

**Implementation:**
```idris
-- [0,1]-enriched category
record EnrichedCat where
  Obj : Type
  Hom : Obj -> Obj -> Interval  -- [0,1] instead of Type
  id  : (a : Obj) -> Hom a a = 1.0
  comp : Hom a b -> Hom b c -> Hom a c  -- min or product

-- Confidence-weighted relation
data WeightedRel : Type where
  MkWRel : (src : Entity) -> (tgt : Entity) -> 
           (conf : Interval) -> WeightedRel
```

**Effort:** Low | **Impact:** High

### 1.3 Prop/Set Truncation

**What:** Systematic distinction between propositions and sets.

**Why:** Proof irrelevance for efficiency; set-level for data.

**Implementation:**
```idris
-- Propositional truncation
data Squash : Type -> Type where
  MkSquash : a -> Squash a
  squash : (x y : Squash a) -> x = y

-- Set truncation (0-truncation)
data SetTrunc : Type -> Type where
  MkSet : a -> SetTrunc a
  setTrunc : (x y : SetTrunc a) -> (p q : x = y) -> p = q
```

**Effort:** Medium | **Impact:** Medium

---

## Tier 2: Significant Extensions (1-2 months each)

### 2.1 Coalgebra and Codata

**What:** Full support for coinductive types and final coalgebras.

**Why:** Streams, traces, and infinite structures are essential for temporal reasoning.

**Implementation:**
```idris
-- Coalgebraic stream with copatterns
codata Stream : Type -> Type where
  head : Stream a -> a
  tail : Stream a -> Stream a

-- Final coalgebra (behavior)
data Behavior : (F : Type -> Type) -> Type where
  MkBehavior : F (Behavior F) -> Behavior F

-- Bisimulation
record Bisimilar (s1 s2 : Stream a) where
  headEq : head s1 = head s2
  tailBisim : Bisimilar (tail s1) (tail s2)
```

**Effort:** High | **Impact:** High

### 2.2 Graded Monads for Effects

**What:** Index monads by a semiring for effect tracking.

**Why:** Track confidence, resource usage, and side effects compositionally.

**Implementation:**
```idris
-- Graded monad over semiring R
interface GradedMonad (R : Semiring) (M : R -> Type -> Type) where
  pure : a -> M R.one a
  (>>=) : M r a -> (a -> M s b) -> M (r * s) b

-- Confidence-graded computation
data Conf : Prob -> Type -> Type where
  Pure : a -> Conf certain a
  Bind : Conf p a -> (a -> Conf q b) -> Conf (andIndep p q) b

-- Usage example
query : Conf 0.9 (List Entity)  -- Result has 0.9 confidence
```

**Effort:** Medium | **Impact:** High

### 2.3 Algebraic Effects

**What:** Structured effect handling with handlers.

**Why:** Cleaner separation of "what" (effect signature) from "how" (handler).

**Implementation:**
```idris
-- Effect signature
effect KGOps where
  GetEntity : EntityId -> EntityOps (Maybe Entity)
  Query : SemanticQuery -> EntityOps (List Entity)
  Propose : ExtractedFact -> EntityOps FactStatus

-- Handler
runPure : Eff a [KGOps] -> PathDB -> (a, PathDB)
runPure (Pure x) db = (x, db)
runPure (GetEntity id >>= k) db = 
  runPure (k (lookup id db)) db
```

**Effort:** High | **Impact:** High

---

## Tier 3: Advanced Features (3-6 months each)

### 3.1 Cubical Type Theory

**What:** Computational interpretation of HoTT using interval type.

**Why:** Univalence becomes computable; higher inductive types work.

**Approach:** 
- Port core modules to Cubical Agda
- Or wait for Cubical Idris
- Extract proofs to SMT via Verus

**Key Types:**
```agda
-- Cubical path
PathP : (A : I → Type) → A i0 → A i1 → Type

-- Glue types for equivalence
Glue : (A : Type) → (φ : I) → (Te : Partial φ (Σ Type (λ B → B ≃ A))) → Type

-- Univalence computation
ua-β : (e : A ≃ B) → transport (ua e) ≡ e .fst
```

**Effort:** Very High | **Impact:** Very High

### 3.2 Sheaves and Topos

**What:** Context-dependent truth via presheaves/sheaves.

**Why:** Multi-perspective ontologies, contextual validity.

**Implementation:**
```idris
-- Presheaf over category C
record Presheaf (C : Category) where
  Ob : C.Obj -> Type
  restrict : (f : C.Hom a b) -> Ob b -> Ob a
  functorial : restrict (g . f) = restrict f . restrict g

-- Sheaf condition (gluing)
record Sheaf (C : Category) (J : Coverage C) extends Presheaf C where
  glue : (cover : J.Cover U) -> 
         (sections : (V : cover.Patches) -> Ob V) ->
         (compatible : ...) ->
         Ob U
```

**Effort:** Very High | **Impact:** High

### 3.3 Double Categories for Spans

**What:** Formalize relational data using spans and cospans.

**Why:** Relations are naturally spans; processes are cospans.

**Implementation:**
```idris
-- Span in category C
record Span (C : Category) (A B : C.Obj) where
  apex : C.Obj
  left : C.Hom apex A
  right : C.Hom apex B

-- Double category
record DoubleCat where
  HObj : Type                    -- Horizontal objects
  VObj : Type                    -- Vertical objects  
  HMor : HObj -> HObj -> Type    -- Horizontal morphisms
  VMor : VObj -> VObj -> Type    -- Vertical morphisms
  Cell : (f : HMor a b) -> (g : HMor c d) -> 
         (α : VMor a c) -> (β : VMor b d) -> Type
```

**Effort:** High | **Impact:** Medium

---

## Tier 4: Research Extensions

### 4.1 Differential Categories

**What:** Categorical differentiation for optimization.

**Why:** Gradient-based learning over knowledge graphs.

**Approach:** Implement cartesian differential categories for smooth functors.

### 4.2 Opetopic Type Theory

**What:** Higher dimensional type theory via opetopes.

**Why:** True ∞-categories without truncation.

**Status:** Research-level, not practical yet.

### 4.3 Polynomial Functors

**What:** Database schemas as polynomial functors.

**Why:** Elegant treatment of dependent relations.

**Reference:** Spivak's polynomial functors for databases.

---

## Implementation Plan

### Q1: Foundations
- [ ] Natural transformations
- [ ] Enriched categories for confidence
- [ ] Prop/Set truncation

### Q2: Effects
- [ ] Graded monads
- [ ] Basic algebraic effects
- [ ] Effect handlers for KG operations

### Q3: Coinduction
- [ ] Codata with copatterns
- [ ] Streams and traces
- [ ] Bisimulation proofs

### Q4: Advanced
- [ ] Evaluate cubical options
- [ ] Prototype sheaf semantics
- [ ] Double category for spans

---

## Measurement

### Type Safety Metrics
- Percentage of operations with full type coverage
- Runtime vs compile-time constraint enforcement
- Proof burden (LOC proofs / LOC code)

### Expressivity Metrics
- Schemas expressible (% of target domains)
- Queries expressible (% of user needs)
- Inference rules encodable

### Performance Metrics
- Compile time (with proofs)
- Query latency (indexed vs proof-carrying)
- Memory overhead (proof terms vs data)

---

## Dependencies

```
Natural Transformations
        │
        ▼
Enriched Categories ──────► Graded Monads
        │                        │
        ▼                        ▼
Double Categories          Algebraic Effects
        │                        │
        └──────────┬─────────────┘
                   ▼
            Sheaf Semantics
                   │
                   ▼
           Cubical Type Theory
```

---

## Resources Required

| Extension | Expertise | Time | Dependencies |
|-----------|-----------|------|--------------|
| Nat Trans | Category theory | 1 week | None |
| Enriched | Cat theory + Prob | 2 weeks | None |
| Graded Monad | Effect systems | 1 month | None |
| Coalgebra | Domain theory | 1 month | None |
| Algebraic Eff | PL theory | 2 months | None |
| Cubical | HoTT expertise | 6 months | Agda/new lang |
| Sheaves | Topos theory | 3 months | Nat Trans |
| Double Cat | 2-categories | 2 months | Nat Trans |

---

## Conclusion

The mathematical foundations are solid for:
- Basic categorical semantics
- Dependent types for constraints
- Modal and probabilistic logics

Key gaps to address:
1. **Compositionality** via natural transformations
2. **Effects** via graded monads
3. **Coinduction** for infinite structures
4. **Contextuality** via sheaves (long-term)

The roadmap prioritizes high-impact, achievable extensions while noting research directions for the future.
