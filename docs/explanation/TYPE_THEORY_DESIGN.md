# Type Theory Design for Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

> Current status: Axiograph's trusted type-theoretic foundation is Lean 4 +
> mathlib. Rust is the operational runtime and must expose typed refs, checked
> builders, residual obligations, and actionable diagnostics.

## Core Type System

Axiograph uses **Lean 4 + mathlib** for trusted semantics and certificate
checking. Rust remains the operational engine, but Rust-generated claims are
trusted only after Lean accepts a certificate.

Lean provides the type-theoretic substrate for:

- Dependent types
- Explicit universe levels
- Inductive families
- Proof irrelevance through `Prop`
- Proof-relevant certificate witnesses in `Type`
- Mathlib category theory, algebra, order, and finite-data libraries

This document describes the current design target.

---

## 1. Universe Hierarchy

Lean has explicit universe levels, which matter when schemas, categories, and
semantic interpretations quantify over other structured objects.

```lean
universe u v

structure Schema where
  Obj : Type u
  Hom : Obj -> Obj -> Type v

structure MetaSchema where
  SchemaId : Type u
  schema : SchemaId -> Schema
```

Design rule: keep kernel objects universe-polymorphic when the abstraction
really ranges over schemas or categories. Keep executable certificate checkers
concrete when possible so the checked surface stays small.

**Status:** Current Lean foundation; expand only where kernel modules need it.

---

## 2. Inductive Families

Path witnesses and reachability proofs are naturally indexed by endpoints.

```lean
inductive Path (Obj : Type u) : Obj -> Obj -> Type u where
  | id : Path Obj a a
  | edge : Edge a b r -> Path Obj a b
  | trans : Path Obj a b -> Path Obj b c -> Path Obj a c
```

This gives the trusted checker:

- Endpoint alignment by construction
- Explicit identity and composition constructors
- A compact replay target for reachability certificates

Runtime Rust builders should mirror these invariants with checked constructors,
but the trusted claim is the Lean replay result.

**Status:** Implemented for the finite presentation fragment in
`Axiograph.Theory.Finite.Path`; certificate dispatch remains narrower.

---

## 3. Proof Relevance vs Erasure

Axiograph needs both proof-relevant and proof-irrelevant data.

Proof-relevant objects are part of the audit trail:

- Reachability witnesses
- Rewrite derivations
- Normalization certificates
- Reconciliation decisions

Proof-irrelevant propositions justify local invariants:

```lean
structure VProb where
  numerator : Nat
  bound : numerator <= precision
```

The `bound` proof matters to Lean, but it is not runtime evidence that users
need to inspect. Certificates and derivation traces, by contrast, are data.

**Status:** Current Lean/Rust boundary. Rust may cache or emit witnesses; Lean
decides whether the witness establishes the semantic claim.

---

## 4. Paths, Equations, and Rewriting

Path equivalence is a semantic relation, not a string or byte-level equality.

```lean
inductive PathEquiv : Path Obj a b -> Path Obj a b -> Prop where
  | refl : PathEquiv p p
  | idLeft : PathEquiv (Path.trans Path.id p) p
  | idRight : PathEquiv (Path.trans p Path.id) p
  | assoc : PathEquiv (Path.trans (Path.trans p q) r)
                      (Path.trans p (Path.trans q r))
```

Certificates should name the equations or rewrite rules they use. The checker
replays those steps against the accepted `.axi` module closure and compiled
semantic IR.

**Status:** `Axiograph.Theory.Finite.PathEquiv` now includes category laws and
accepted parallel-path equations. `GroupoidPath` is endpoint-indexed and its
identity, associativity, and inverse laws are proved by denotation into
mathlib's free groupoid. General rewrite termination/confluence remains a
non-claim.

---

## 5. Probability and Approximation

Probability-like confidence values need deterministic, checkable arithmetic.
The trusted fragment should use fixed-point values with explicit bounds, not
ambient floating-point assumptions.

```lean
structure FixedProb where
  numerator : Nat
  bounded : numerator <= precision

def composeConfidence (a b : FixedProb) : FixedProb :=
  -- fixed-point multiplication plus a proof that the result is bounded
  sorry
```

Rust can use convenient runtime types, but certificate payloads should lower to
the fixed-point representation that Lean checks.

**Status:** Current direction for `VProb` and certificate checking.

---

## 6. Runtime Type Discipline in Rust

Rust is not the trusted dependently typed kernel. It still carries useful type
discipline at operational boundaries:

- Phantom brands for snapshot-scoped witnesses
- Typestate wrappers for checked query IR and normalized paths
- Checked builders for facts, relations, paths, and certificates
- Strong enums for certificate kinds and reconciliation outcomes

These patterns reduce malformed certificate emission and make runtime behavior
reviewable. They do not replace Lean.

**Status:** Rust should stay strict and typed, but semantic authority remains in
Lean.

---

## 7. Effects and IO

The trusted checker should remain small and mostly pure. IO-heavy operations
belong in Rust:

- Ingestion
- Query planning
- Indexing
- Backend projection
- LLM-assisted extraction
- Visualization

Lean-facing certificate checkers should consume explicit inputs and produce
explicit accept/reject results. If a semantic rule depends on a snapshot,
module, world, or context, that dependency must be carried by the certificate
or anchor.

**Status:** Keep side effects out of trusted semantic kernels.

---

## 8. Modal, Temporal, and Contextual Types

Axiograph needs modalities for real-world claims:

- Time-indexed facts
- Belief or knowledge by agent
- Obligations and permissions
- Context-dependent tacit knowledge
- Possible worlds and scenario branches

The design target is Lean semantics for these modalities plus Rust certificate
emitters for concrete operations. Runtime proposal-adapter or LLM output stays in
the evidence plane until it passes typed validation, review, and promotion.

**Status:** Finite context-indexed values and proof-carrying context transports
are implemented in `Axiograph.Theory.Finite`. Modal logic, arbitrary context
categories, sheaf descent, and proposal-adapter trust remain unimplemented.

---

## 9. Reflection and Code Generation

Lean metaprogramming is useful for reducing boilerplate in the trusted checker,
but generated proof code should still compile to ordinary reviewable Lean
definitions and theorems.

Rust code generation should target typed runtime surfaces and certificate
schemas, not a separate proof authority. Generated Rust remains untrusted until
its emitted certificate checks in Lean.

**Status:** Use generation to reduce repetition, not to widen the trusted base.

---

## 10. Verified Rust and Local Invariants

Rust verification tools can harden the untrusted engine:

- Verus for local invariants
- Kani for bounded model checking
- Miri for undefined-behavior detection
- Fuzzing for byte parsers and certificate decoders

These tools complement Lean. They can show that Rust code is less likely to
emit malformed data or violate memory/format invariants, but they do not define
Axiograph's semantic truth.

**Status:** Recommended for high-risk runtime surfaces; not a replacement for
certificate checking.

---

## 11. Current Gaps

| Feature | Importance | Current owner | Status |
| --------- | ------------ | --------------- | -------- |
| Finite indexed path/groupoid laws | High | Lean | Implemented as theorem support; not a `VerifyMain` certificate family |
| `.axi` parser parity | High | Lean + Rust | In progress |
| Finite category presentation and relation projections | High | Rust canonical IR + Lean finite theory | Implemented finite model and anchored `category_kernel_v3` serialization/dispatch in `VerifyMain`: formation, equation congruence, formal inverse cancellation, and bounded generator reachability replay. Acceptance-to-denotation theorem remains open. |
| Reconciliation certificates | High | Lean + Rust emitters | Planned |
| Modal/temporal semantics | Medium | Lean | Planned |
| Semantic coverage reports | Medium | Rust, checked anchors | Planned |
| Rust local invariant proofs | Medium | Rust tooling | Selective |

---

The category wire checker reconstructs the presentation from exact anchored
`.axi` bytes; it does not trust serialized Rust IR as source meaning. Its finite
decision procedures and replay do not yet retype wire paths into dependent
`GroupoidPath` values or connect acceptance to `GroupoidPath.denote`/`PathEquiv`.
The separate indexed path families' denotation theorems do not close this gap.
See [Lean theory evaluation](../reference/LEAN_THEORY_EVALUATION.md) for the
current claim boundary and [EQ-14](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md#eq-14-category-certificate-acceptance-to-denotation-theorem)
for the remaining theorem work.

## 12. Recommended Path Forward

### Phase 1: Tighten the trusted checker

- Keep `lean/Axiograph/VerifyMain.lean` as the trusted import closure.
- Expand certificate replay for path normalization, reachability, and rewrites.
- Keep probability and parser claims pinned to actual checked Lean modules.

### Phase 2: Align runtime emission

- Make Rust emit certificates for the operations users rely on.
- Use typed Rust builders to prevent malformed certificate payloads.
- Add golden vectors that Rust emits and Lean verifies.

### Phase 3: Promote richer semantics

- Add modal, temporal, and contextual kernels in Lean.
- Require explicit anchors for worlds, snapshots, contexts, and accepted modules.
- Keep LLM/proposal-adapter output in evidence branches until validated and promoted.

---

## References

1. **Theorem Proving in Lean 4** - Lean community
2. **Mathematics in Lean** - Lean community
3. **Homotopy Type Theory: Univalent Foundations of Mathematics**
4. **Category Theory in Context** - Emily Riehl
5. **Quantitative Type Theory** - Atkey, 2018
