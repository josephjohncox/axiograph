# Axiograph Book: Verifiable Knowledge Graphs with Proofs, Modalities, and Approximation

**Diataxis:** Explanation (longform)  
**Audience:** contributors

This document is an end-to-end, math-first description of what `axiograph_v6/` is building: a **generalized verifiable knowledge graph** that supports:

- Proof-carrying answers: queries return answers plus a machine-checkable explanation.
- Approximation: heuristic/incomplete inference is allowed, but must be bounded and auditable.
- Tacit + encoded knowledge: experience-derived rules live alongside formally stated facts.
- Modal and temporal reasoning: “knows”, “should”, “possibly”, “as of time t”.
- LLM interoperability: LLMs can propose and consume knowledge without being trusted.

The core system idea is:

> Untrusted engines compute; a small trusted checker verifies.

In this repo:

- Rust is the untrusted runtime engine (ingestion, indexing, search, reconciliation, certificate emission).
- Lean is the trusted checker/spec (mathlib-backed).
- Idris2 was used historically as a prototype proof layer; the initial Rust+Lean release removes Idris/FFI compatibility.

This book focuses on the mathematics and semantics, then maps those semantics onto the codebase and production concerns.

## How to read this

- If you’re implementing runtime features: read Part IV (Certificates) and Part V (Production) first.
- If you’re tightening theory: read Parts I–III, then §15 (Tightening the theory).
- If you want domain impact: start at Part VI (Use cases).

## Status conventions used here

- Implemented: exists today in code and/or `make verify-lean*` targets.
- Prototype: partially implemented or only in one layer (Lean or Rust).
- Planned: design direction; not in code yet.

---

## Table of Contents

**Part I — Semantics**
1. [Goals and non-goals](#1-goals-and-non-goals)
2. [Core objects: entities, relations, facts](#2-core-objects-entities-relations-facts)
3. [Paths, groupoids, and rewriting](#3-paths-groupoids-and-rewriting)
4. [Evidence and probability as a verified algebra](#4-evidence-and-probability-as-a-verified-algebra)

**Part II — Types and logic features**
5. [Dependent types: what we prove “by construction”](#5-dependent-types-what-we-prove-by-construction)
6. [Modalities: time, belief, obligation, tacit knowledge](#6-modalities-time-belief-obligation-tacit-knowledge)
7. [Subtyping, refinement, and “is-a” in a graph](#7-subtyping-refinement-and-is-a-in-a-graph)
8. [Linear/quantitative types (future)](#8-linearquantitative-types-future)

**Part III — Querying**
9. [Queries as programs with semantics](#9-queries-as-programs-with-semantics)
10. [Approximate querying and bounds](#10-approximate-querying-and-bounds)

**Part IV — Proof-carrying results**
11. [Certificates: the Rust-to-Lean interchange](#11-certificates-the-rust-to-lean-interchange)
12. [What is checked today (and what’s next)](#12-what-is-checked-today-and-whats-next)

**Part V — Production readiness**
13. [Trusted computing base and threat model](#13-trusted-computing-base-and-threat-model)
14. [Engineering checklist](#14-engineering-checklist)
15. [Tightening the theory](#15-tightening-the-theory)

**Part VI — Application areas (detailed use cases)**
16. [Manufacturing / engineering](#16-manufacturing--engineering)
17. [Biomedical / clinical](#17-biomedical--clinical)
18. [Cybersecurity](#18-cybersecurity)
19. [Legal / compliance](#19-legal--compliance)
20. [LLM memory + grounding](#20-llm-memory--grounding)

**Appendices**
- [Appendix A: Code map](#appendix-a-code-map)
- [Appendix B: Glossary and notation](#appendix-b-glossary-and-notation)
- [Appendix C: Related work and literature](#appendix-c-related-work-and-literature)

---

# Part I — Semantics

## 1. Goals and non-goals

### 1.1 Goals

1. Auditable inference
   - Every high-value inference can be accompanied by a certificate that a trusted checker validates.
2. Stable semantics
   - “What does this mean?” is defined in Lean (eventually fully), not by “whatever the runtime currently does”.
3. Approximation with explicit bounds
   - We permit heuristic search, approximate ranking, and uncertainty, but we must not conflate them with truth.
4. Explicit provenance
   - Claims are attached to sources and derivations. The graph is a knowledge ledger, not just a lookup table.
5. LLM interop without trust
   - LLM output is treated as a noisy proposal; the checker and reconciliation layer decide what becomes accepted.

### 1.2 Non-goals (currently)

- Global completeness: the system may fail to find a derivation even if one exists.
- One logic to rule them all: we combine compatible fragments (paths/rewrite, probability, modal/temporal, constraints) via explicit interfaces.
- Perfect probabilistic semantics: our “confidence” is an evidence calculus with engineering-friendly invariants, not a claim of Bayesian omniscience.

---

## 2. Core objects: entities, relations, facts

### 2.1 A typed multigraph (quiver) as the base layer

At the bottom is a graph-like object:

- Entities: vertices `a, b, c, …`
- Relations: directed edges labeled by relation type and (often) confidence/provenance.

In Lean, we use a dependent notion of relations:

```lean
structure KnowledgeGraph (numEntities : Nat) where
  Rel : EntityId numEntities → EntityId numEntities → Type
  relComp : Rel a b → Rel b c → Rel a c
  relId : (a : EntityId numEntities) → Rel a a
```

Implemented: `lean/Axiograph/HoTT/KnowledgeGraph.lean`.

This gives us a category-shaped interface. We then make equality/equivalence explicit (next section).

### 2.2 Paths as first-class witnesses

Axiograph does not merely store reachability; it stores reasons as compositional structures:

- `KGRefl`: identity path
- `KGRel r`: a single edge
- `KGTrans p q`: concatenation/composition

In Lean:

```lean
inductive KGPath (kg : KnowledgeGraph n) : EntityId n → EntityId n → Type where
  | KGRefl : KGPath kg a a
  | KGRel : kg.Rel a b → KGPath kg a b
  | KGTrans : KGPath kg a b → KGPath kg b c → KGPath kg a c
```

Implemented: `lean/Axiograph/HoTT/KnowledgeGraph.lean`.

The value of this representation is that a path is already a certificate skeleton: it is an explicit witness that endpoints match.

### 2.3 Facts: content + confidence + derivation

A fact is not just a boolean statement; it is a record with:

- Content: the proposition/data (possibly structured)
- Confidence: a verified value in `[0, 1]`
- Derivation: optionally, a path witnessing how we got it
- Evidence count and provenance metadata

In Lean we already model this pattern:

```lean
structure Fact (kg : KnowledgeGraph n) (subject : EntityId n) (contentType : Type) where
  content : contentType
  confidence : VProb
  derivationSource : EntityId n
  derivationPath : KGPath kg derivationSource subject
  evidenceCount : Nat
```

Implemented: `lean/Axiograph/HoTT/KnowledgeGraph.lean`.

### 2.4 Schema vs instance (and why `.axi` matters)

We distinguish:

- Schema-level knowledge: what kinds of entities/relations are allowed; constraints; typing.
- Instance-level knowledge: concrete entities and their relations/facts.

`.axi` is the human-facing canonical source format. Keeping `.axi` canonical matters because certificates and audits need stable, reproducible inputs.

Implemented (parsing parity direction):

- Canonical corpus: `examples/canonical/corpus.json`
- Rust unified parser entrypoint: `rust/crates/axiograph-dsl/src/axi_v1.rs`
- Lean unified parser entrypoint: `lean/Axiograph/Axi/AxiV1.lean`

### 2.5 Relations as edge objects (RDF/OWL-friendly modeling)

RDF/OWL and property graphs treat relations as first-class data: you can attach provenance, time, confidence, and “why we believe this” to an edge.

A categorical way to model this (used in categorical databases and in the earlier exploratory note `ChatGPT-Dependently_typed_ontology (3).md`) is:

> Represent a binary relation `R ⊆ A × B` as an *edge object* `E_R` with projections  
> `src : E_R → A` and `dst : E_R → B`.

Benefits:

- You can have many edges between the same pair of nodes (multigraph).
- You can attach attributes to the edge object (provenance, confidence, effective dates, citations).
- Higher-arity relations become ordinary entities (“hyperedges”) with multiple projections.
- This matches how real “facts” behave in production systems.

This approach is compatible with our proof-carrying goal because:

- A certificate can cite specific edge objects as evidence.
- A derivation path can be a path *through edge objects*, not just raw labels.

Interop implications:

- RDF triples become edge objects with `src`, `predicate`, `dst` (reification), plus metadata.
- OWL class axioms can be compiled into constraints/shapes or rewrite rules where feasible; when not feasible, they remain assumptions that do not get “certified truth” status.

---

## 3. Paths, groupoids, and rewriting

The heart of Axiograph is reasoning about equivalence of derivations.

Two different derivations can mean the same thing:

- Differing only by parenthesization (associativity),
- Inserting/removing identity steps,
- Cancellation with inverses (when supported),
- Applying domain rewrite rules (normalization, unit conversions, schema migration).

### 3.1 Path equivalence as explicit “2-cells”

We represent “path `p` is equivalent to path `q`” as a proof object:

```lean
inductive KGPathEquiv : KGPath kg a b → KGPath kg a b → Type where
  | KGPERefl : KGPathEquiv p p
  | KGPESym : KGPathEquiv p q → KGPathEquiv q p
  | KGPETrans : KGPathEquiv p q → KGPathEquiv q r → KGPathEquiv p r
  | KGPEIdL : KGPathEquiv (KGTrans KGRefl p) p
  | KGPEIdR : KGPathEquiv (KGTrans p KGRefl) p
  | KGPEAssoc : KGPathEquiv (KGTrans (KGTrans p q) r) (KGTrans p (KGTrans q r))
```

Implemented: `lean/Axiograph/HoTT/KnowledgeGraph.lean`.

This is a deliberately small generating set. The long-term direction is to:

- Add inverse/cancellation laws (groupoid completion),
- Add congruence (equivalence preserved under composition),
- And quotient paths by this equivalence where appropriate.

Implemented (certificates): the certificate-side path expression language supports formal inverses and cancellation, and the checker supports:

- `normalize_path_v2`: recompute-and-compare normalization **plus optional replayable derivations** (rule + position),
- `rewrite_derivation_v2`: a generic “replay this derivation” certificate (rule + position),
- `path_equiv_v2`: equivalence via common normalization, with optional derivations from both sides.

Planned: extend beyond these **local groupoid rules** into domain-specific rewrite systems (unit conversions, schema migration rewrites), and reuse the same derivation mechanism for reconciliation explanations.

### 3.2 The “free groupoid on a graph” interpretation

Conceptually, you can think of:

1. Start with a directed multigraph (“quiver”).
2. Form formal paths by concatenation.
3. Add inverses formally (free groupoid) if you want reversible reasoning.
4. Quotient by the rewrite/equivalence relation generated by your semantics.

Mathlib already contains structures that are close to this direction:

- `Mathlib.CategoryTheory.Groupoid.FreeGroupoid`
- `Quiver.Paths`

Planned: rebase more of `lean/Axiograph/HoTT/*` onto mathlib’s free constructions to avoid re-proving standard results, and relate certificate syntax to those canonical semantics.

### 3.3 Rewriting as a congruence

To make rewrite rules behave correctly, equivalence must be a congruence:

If `p ≈ q`, then:

- `r ∘ p ≈ r ∘ q` when composition is defined,
- `p ∘ r ≈ q ∘ r` when composition is defined.

This is what lets you rewrite a subpath inside a larger path.

Implemented: the certificate checker supports congruence-aware rewriting via explicit **positions** (`pos`) in rewrite steps:

- `normalize_path_v2`: recompute-and-compare normalization, plus optional replayable derivations (rule + position)
- `rewrite_derivation_v2`: generic replayable derivations (rule + position)

Planned: reuse this derivation style for domain rewrites and reconciliation proofs (policy explanations), not just local groupoid normalization.

### 3.4 Normalization

Normalization is “choose a canonical representative of an equivalence class of derivations”.

Why normalize?

- To compare derivations efficiently,
- To avoid exponential blowups from associativity rebracketing,
- To make caching and hashing stable,
- To reduce certificate size by canonicalization.

Implementation note (engine-side): equality saturation with **e-graphs** is a practical way for Rust to do rewrite search and normalization while keeping the meaning stable in Lean. Axiograph can treat e-graphs as an *untrusted optimizer* that must emit either replayable rewrite-step certificates or a normal form that the checker validates (recompute-and-compare as a scaffold). See Appendix C.17.

Implemented: `normalize_path_v2` certificates:

- Rust emits an input path expression and a purported normalized form.
- Lean re-computes normalization and checks the result matches the claimed normal form.
- If a derivation is included, Lean also replays every rewrite step (rule + position) and checks it reaches the claimed normalized form.

See:

- Rust certificate types: `rust/crates/axiograph-pathdb/src/certificate.rs` (`NormalizePathProofV2`)
- Lean parsing: `lean/Axiograph/Certificate/Format.lean` (`PathExprV2`, `NormalizePathProofV2`)
- Running docs: `docs/reference/CERTIFICATES.md`

Next: extend the same derivation mechanism beyond **local groupoid rules** to cover **domain rewrites** (unit conversions, schema migration rewrites) and reconciliation derivations (policy explanations).

---

## 4. Evidence and probability as a verified algebra

We want confidence/uncertainty to be:

- Bounded (always in `[0,1]`),
- Deterministic in the trusted checker (no floats),
- Compositional along derivations.

### 4.1 Fixed-point verified probabilities (`VProb`)

The trusted checker uses a fixed-point probability:

- Denominator `Precision = 1_000_000`
- A probability is a numerator in `[0, Precision]`

In Lean:

```lean
def Precision : Nat := 1_000_000
structure VProb where
  numerator : Fin (Precision + 1)
```

Implemented: `lean/Axiograph/Prob/Verified.lean`.

This choice is pragmatic:

- Certificates are stable across platforms,
- Arithmetic is decidable and proof-friendly,
- Bounds are enforced by construction.

Rust mirrors this for v2 certificates:

- `FixedPointProbability` in `rust/crates/axiograph-pathdb/src/certificate.rs`
- `FIXED_POINT_DENOMINATOR = 1_000_000`

### 4.2 Confidence combination as an algebra

At minimum, we need:

- A notion of combining confidence along a derivation chain,
- A notion of combining multiple evidence sources.

Today, reachability/path certificates use a simple independent-conjunction model:

- Path confidence is the product of edge confidences.

Lean side:

- `vMult` multiplies fixed-point probabilities with rounding down and proves bounds.

Rust side:

- v1: `VerifiedProb` uses `f32` but checks bounds on parse and construction.
- v2: fixed-point `FixedPointProbability.mul` and Lean `Prob.vMult`.

This is an evidence algebra: multiplying confidences is interpretable and conservative even if strict statistical independence is not literally true.

### 4.3 Evidence strength, thresholds, and reconciliation

Many production actions are decision-y:

- Choose between conflicting facts,
- Merge two facts,
- Or require human review.

In Lean we model decision procedures on verified probabilities. In Rust we mirror the same decision and emit a certificate asserting the decision; Lean re-computes and checks.

Implemented:

- Rust: `ResolutionProofV2` in `rust/crates/axiograph-pathdb/src/certificate.rs`
- Lean: parsing and decision representation in `lean/Axiograph/Certificate/Format.lean` plus `lean/Axiograph/Prob/*`

This is the pattern: the engine computes; the checker re-computes the meaning.

### 4.4 Approximation as “bounded uncertainty”

We treat approximation in two different places:

1. Inference/search approximation (engine may not find the best derivation)
2. Knowledge uncertainty (confidence bounds / evidence aggregation)

The verifier’s job is not to guarantee the engine did an optimal search. The verifier’s job is to guarantee:

- The returned derivation is well-formed,
- The returned confidence is computed according to the agreed semantics,
- Any approximation claims are explicitly encoded and validated (planned).

Planned: extend certificates with “search envelope” metadata, e.g.:

- Upper bounds on missed alternatives,
- Proof that a reported lower bound is valid,
- Or a statement like “this is a sound under-approximation”.

---

# Part II — Types and logic features

## 5. Dependent types: what we prove “by construction”

Dependent types are not a vibe; they are a method:

> Encode invariants in types so ill-formed objects cannot be constructed.

### 5.1 Canonical invariants we care about

Endpoints match:

- A path from `a` to `b` is typed as `KGPath a b`. You cannot compose incompatible paths without a proof that endpoints align.

Confidence is bounded:

- `VProb` carries the proof of `0 ≤ n ≤ Precision` in its constructor.

Derivations are structured:

- Facts can carry typed derivation paths (`Fact.derivationPath`), not just free-form text.

Decision procedures have explicit contracts:

- Resolution decisions depend only on fixed-point values, so they are deterministic and checkable.

### 5.2 Proof relevance vs erasure

Axiograph needs both:

- Proof-relevant data (certificates, derivation witnesses),
- Proof-irrelevant propositions (bounds proofs that should erase at runtime).

In Lean, many proofs live in `Prop` and erase automatically; certificates and witnesses live in `Type` and are data.

Historically, an Idris prototype used similar patterns; we aim to keep:

- Proofs for invariants erased where possible,
- And proof objects only where they are part of the auditable story.

### 5.3 Rust cannot be dependently typed—so we simulate where it matters

Rust provides:

- Ownership/borrowing (a practical linear subset),
- Phantom types and type-state builders,
- Strong enums and pattern matching,
- Runtime checks at boundary points.

Example: typed path construction in `rust/crates/axiograph-llm-sync/src/path_verification.rs` uses:

- Marker traits for relation types (`Relationship`),
- Builders to prevent malformed paths,
- Explicit endpoint checks during composition.

This is not a substitute for dependent typing, but it is enough to keep the engine honest and produce correct certificates most of the time.

### 5.4 A recommended “kernel first” style

To keep the trusted checker small and stable:

- Put the minimal semantic core in Lean modules with no IO and a small dependency surface.
- Make everything else (parsers, adapters, optimizers) produce certificates validated against that core.

That’s the shape of a production proof-carrying system.

---

## 6. Modalities: time, belief, obligation, tacit knowledge

Modalities are how we represent knowledge that depends on worlds:

- Time (“was true at t”, “eventually”, “until”),
- Belief/knowledge (“agent i knows φ”, “it is possible that φ”),
- Obligation (“must”, “may”, “forbidden”),
- Tacit knowledge (“in practice, do φ”).

Modal reasoning is not optional if you want a knowledge system that matches real domains.

### 6.1 Kripke semantics (the common base)

Modal logics are often modeled with:

- A set of worlds `W`,
- An accessibility relation `R : W → W → Prop`,
- And an interpretation of propositions at each world.

Operators:

- `□φ` (“box”): φ holds in all accessible worlds.
- `◇φ` (“diamond”): φ holds in some accessible world.

Prototype (historical): an Idris proof-layer explored modal modules alongside the math notes (`docs/explanation/MATHEMATICAL_FOUNDATIONS.md` section “Modal Logics”).

Planned: port the modal/tacit/temporal semantics into Lean as part of the trusted checker.

### 6.2 Temporal modalities

Temporal reasoning is foundational for “as of X” claims and evolving corpora:

- Facts can expire,
- Guidelines supersede older guidance,
- Policies have effective dates.

Prototype (historical): an Idris temporal logic module explored interval reasoning and temporal operators.

Planned: a Lean temporal kernel plus certificates for time-indexed inferences.

### 6.3 Tacit knowledge: modeling “useful but revisable” claims

Tacit knowledge is typically:

- Sourced from humans (“20 years of experience…”),
- Not universally valid,
- Context-dependent,
- Often a heuristic rather than a law.

Representation strategy:

1. Treat tacit claims as first-class facts, but with:
   - Explicit provenance (speaker, context),
   - Confidence below “handbook” facts by default,
   - Revision hooks (they can be overridden by stronger evidence).
2. Use a modality to mark tacitness, e.g. a type former like `Tacit φ` or a modal operator “in practice”.
3. Let reconciliation compute how tacit evidence interacts with encoded rules (see §4.3).

Prototype (historical): an Idris tacit-knowledge module explored typed provenance + heuristics.

Planned: Lean port and certificate-backed reconciliation for tacit-vs-encoded conflicts.

### 6.4 Modalities + probability (graded modalities)

Many useful combinations are graded:

- “likely □φ” vs “possibly φ”
- “obligatory with exceptions” under uncertain evidence

One tightening direction is to treat modalities as computational effects (monads/comonads) and probability as a grading on those effects.

Example design sketches (planned):

- `Know_i : Prop → Prop` with evidence strength
- `Must : Prop → Prop` with exception paths and override priorities
- `Possibly : VProb → Prop → Prop` (“φ is possible with confidence p”)

The principle is compositionality: combining modal/probabilistic structures should be an algebra, not ad-hoc if/else.

### 6.5 Contexts as first-class worlds (provenance, time, perspective)

The earlier exploration note (`ChatGPT-Dependently_typed_ontology (3).md`) correctly emphasizes that “context” must be first-class for a practical knowledge system.

In practice, “truth” is almost never absolute; it is indexed by some notion of world/context:

- time (as-of, valid-until),
- provenance (source, method, instrument),
- perspective (team A vs team B),
- conditions (operating regime, environment),
- policy (jurisdiction, contractual scope).

Kripke semantics already gives the right mathematical shape: a modality is truth indexed by worlds plus an accessibility relation. In a knowledge graph setting, a useful tightening is:

- Treat `Context` as an entity sort (or world index).
- Make “fact holds” explicitly indexed: `HoldsIn : FactEdge → Context → Prop` (or an edge from facts to contexts).
- Make certificates include the context they are valid in (or a transport proof between contexts).

This unlocks:

- multi-perspective reasoning without “overwriting” facts,
- temporal supersession as a first-class construction (not ad-hoc timestamps),
- clean semantics for LLM “memory” (conversation context is a world), and
- policy/governance proofs (facts usable only in certain contexts).

---

## 7. Subtyping, refinement, and “is-a” in a graph

Subtyping appears in two distinct ways:

1. Programming-language subtyping: `Material` can be used where `Entity` is expected.
2. Ontology/knowledge subtyping: “Ti-6Al-4V is-a TitaniumAlloy is-a Material”.

We want both, but we must keep them conceptually separate.

### 7.1 Graph-level “is-a” as a partial order (plus exceptions)

In the graph, `is-a` edges generate a preorder:

- Reflexive: everything is-a itself,
- Transitive: if `A is-a B` and `B is-a C`, then `A is-a C`.

This is a reachability/closure operation, so it naturally fits the path witness model:

- An `is-a` chain is literally a path certificate.

Implemented (engine-side pattern): Rust has an `IsA` relationship marker in `rust/crates/axiograph-llm-sync/src/path_verification.rs`.

Planned (checker-side): express `is-a` closure and subtype reasoning in Lean and check certificates for type coercions in query results.

### 7.2 Refinement types: subsets and constraints

Many domain rules are refinements:

- “CuttingSpeed is a number between 0 and 300”
- “This entity has a hardness attribute”
- “This probability is bounded”

Lean is well suited for this:

- `{x : α // P x}` (Subtype) packages a value with a proof of a predicate.

We already use this pattern for probabilities (`VProb`).

Planned: use refinements for schema constraints and query result validation, so violations are impossible to certify.

### 7.3 Coercions and “subtype polymorphism”

In a production system you want ergonomic usage:

- If `Material` is-a `Entity`, code should be able to upcast without boilerplate.

Lean supports coercions via `Coe` typeclasses; Rust can simulate with explicit conversion traits.

The key is: coercions must be semantics-preserving and, when they reflect a knowledge-graph inference (“A is-a B”), they should be certificate-backed.

---

## 8. Linear/quantitative types (future)

Linear and quantitative typing are not required to build a verifiable KG, but they become extremely valuable in production, especially around:

- Resource tracking,
- Privacy and data-use governance,
- Protocol correctness (session types),
- One-time operations like issuing access tokens or signing actions.

### 8.1 Why linear/affine typing fits Axiograph

Many facts are not “free to use”:

- Protected health data (PHI),
- Export-controlled engineering data,
- Licensed documents,
- Secrets and credentials in security graphs.

We can treat permission to use as a resource.

A linear type discipline lets you express:

- This evidence/token can be consumed at most once,
- This data may only flow to approved sinks,
- This query requires a capability and produces a usage receipt.

Rust already provides an ownership discipline that resembles a linear core; Lean can provide the semantics and certificates.

### 8.2 A concrete integration sketch (planned)

Add to the certificate and checker:

- A capability object (or label) with explicit usage policy,
- A proof that the query execution respected the policy.

Add to the runtime:

- Policy enforcement in the query planner,
- Auditing hooks that record “data used” receipts.

This is the path from verifiable correctness to verifiable compliance.

---

# Part III — Querying

## 9. Queries as programs with semantics

In Axiograph, a query is not just a filter; it is (eventually) a program in a small, typed language with a defined semantics.

We want queries that can:

- Traverse relations (path queries),
- Apply constraints (dependent refinements),
- Aggregate evidence (confidence),
- Return results with justification.

### 9.1 What a “verified query result” means

A query result should include:

1. The answer (entity/value),
2. The confidence of the answer,
3. The derivation witness (path or proof object),
4. Provenance (which sources contributed).

There is a large, mature database-theory literature on **query provenance** (why/where/how provenance and semiring provenance). Conceptually, an Axiograph certificate is a structured *how-provenance object* for a result, but with a small trusted checker that validates the witness against a formal semantics (rather than treating provenance as purely observational metadata). See Appendix C.14.

Rust already has the conceptual shape:

- `ProvenQueryResult` in `rust/crates/axiograph-pathdb/src/verified.rs`
- It packages `entity_id`, `confidence`, and a `ReachabilityProof`.

Planned: expand this pattern beyond reachability into:

- Normalization and rewrite-derived results,
- Reconciliation justifications,
- Compositional query certificates.

### 9.2 Query semantics: soundness before completeness

Soundness condition:

> If the system returns `(answer, certificate)`, then the checker accepts the certificate and therefore the answer is valid under the semantics.

Completeness is optional and can be layered:

- A slow, complete engine could exist,
- But the system remains useful if a fast engine returns partial results with correct certificates.

This is how you make “approximate but safe” querying possible.

### 9.3 Query language directions (planned)

There are multiple viable choices for the query language core:

1. Datalog-like (good for monotone inference, indexing, optimization).
2. Typed functional queries (good for compositional semantics, dependent refinements).
3. Modal/temporal query fragments (good for “as of”, “must”, “possibly”).
4. Probabilistic query fragments (ranked answers with verified combination).

The codebase already has query components:

- Rust: PathDB query engine + AxQL (`rust/crates/axiograph-pathdb/` and `rust/crates/axiograph-cli/`)

Recommendation: keep the trusted kernel small (Lean), and let Rust implement the planner/executor that emits certificates.

---

## 10. Approximate querying and bounds

Approximation is inevitable:

- The graph is large,
- Inference can be expensive,
- LLM-derived knowledge is uncertain.

The goal is not to eliminate approximation; it is to make approximation explicit and checkable.

### 10.1 Two kinds of approximation (keep them separate)

1. Search approximation (algorithmic):
   - The engine may return some path, not necessarily the best one.
2. Epistemic approximation (knowledge uncertainty):
   - The best available evidence still yields a confidence < 1.

Certificates can guarantee correctness about (2) and local correctness about (1), but cannot prove optimality unless we include additional proof data.

### 10.2 “Anytime” answers with certified soundness

An engineering-friendly design:

- Return the first K answers found, each with a certificate,
- Optionally return a bound like “no answer above confidence p was found in explored region” (planned),
- Allow the user to request more time/compute to improve results.

This aligns with LLM grounding:

- You want safe, explainable context quickly,
- And you can refine if needed.

### 10.3 Probability intervals and conservative bounds (planned)

A tight theoretical extension is to represent confidence as an interval `[l, u]`:

- `l` is a sound lower bound,
- `u` is an upper bound,
- Combination rules are verified to preserve bounds.

This gives you:

- Principled approximations,
- Explicit “how wrong could this be?” semantics,
- A route to safe decision-making under uncertainty.

---

# Part IV — Proof-carrying results

## 11. Certificates: the Rust-to-Lean interchange

Certificates are the bridge between:

- Fast, untrusted computation,
- And slow but trusted checking.

They are the externalized proof objects that let Lean validate results without re-running the whole engine.

### 11.1 Design requirements for certificates

1. Versioned: the shape is stable and evolvable.
2. Deterministic: no floating-point dependence in the trusted checker.
3. Small trusted parser: parsing should be strict and minimal.
4. Recomputable semantics: the checker should re-run the meaning, not trust the engine’s claim.

### 11.2 Certificate formats in this repo (implemented)

Rust defines certificate types here:

- `rust/crates/axiograph-pathdb/src/certificate.rs`

Lean parses them here:

- `lean/Axiograph/Certificate/Format.lean`

Two generations exist:

- v1: reachability with float confidences (still bounded).
- v2: fixed-point confidences (`Precision = 1_000_000`) and additional proof kinds:
  - anchored reachability (optional `.axi` anchor + snapshot `relation_id` fact IDs),
  - resolution decisions,
  - path normalization (with optional replayable derivations),
  - generic rewrite derivations (rule + position),
  - path equivalence via normalization,
  - and a Δ_F migration scaffold (`delta_f_v1`).

For running and schema details, see:

- `docs/reference/CERTIFICATES.md`
- `docs/howto/FORMAL_VERIFICATION.md`

### 11.3 A representative v2 reachability certificate (shape)

```json
{
  "version": 2,
  "kind": "reachability_v2",
  "proof": {
    "type": "step",
    "from": 1,
    "rel_type": 7,
    "to": 9,
    "rel_confidence_fp": 850000,
    "rest": {
      "type": "reflexive",
      "entity": 9
    }
  }
}
```

Meaning:

- This claims a path from entity 1 to entity 9 by a single step of relation type 7.
- The step has confidence 0.85 (fixed-point numerator 850000).
- The checker computes path confidence by multiplying step confidences along the chain.

### 11.4 The “untrusted engine, trusted checker” loop

For any certificate kind, the pattern is:

1. Rust constructs a result and a certificate.
2. Lean parses the certificate into a typed object.
3. Lean recomputes the semantic function and checks it matches the claimed result.

Example (implemented):

- Resolution certificates (`resolution_v2`) claim a conflict-resolution decision.
- Lean recomputes the decision function from the fixed-point inputs and checks equality.

This is powerful because Rust can be optimized or replaced without changing the meaning.

### 11.5 Extending certificates responsibly (recommended rules)

When adding a new certificate kind:

1. Define the meaning function in Lean first (pure, total if possible).
2. Keep the certificate a witness that can be checked by recomputation:
   - Do not ship opaque proofs that require trusting large proof terms.
3. Add:
   - Positive test vectors (should verify),
   - Negative test vectors (should fail).
4. Make the Rust emitter deterministic (stable ordering, stable rounding).
5. Version the kind if you change semantics.

---

## 12. What is checked today (and what’s next)

### 12.1 Implemented today

- Lean ports of core HoTT/path vocabulary:
  - `lean/Axiograph/HoTT/Core.lean`
  - `lean/Axiograph/HoTT/KnowledgeGraph.lean` (`KGPath`, `KGPathEquiv`, `Fact`)
  - `lean/Axiograph/HoTT/PathAlgebraProofs.lean` (path confidence/length proofs)
- Verified probability core:
  - `lean/Axiograph/Prob/Verified.lean`
- Canonical `.axi` parsing:
  - `lean/Axiograph/Axi/*` and Rust equivalents in `rust/crates/axiograph-dsl/src/*`
- Certificates:
  - Reachability v1 + v2 (including optional `.axi` anchoring via `axi_digest_v1` and snapshot `relation_id` fact IDs)
  - Resolution v2 (decision re-check)
  - Normalize-path v2 (recompute normalization, plus optional replayable derivation replay)
  - Rewrite-derivation v2 (replayable rewrite traces: rule + position)
  - Path-equivalence v2 (equivalence via shared normalization, plus optional derivations)
  - Δ_F migration cert (`delta_f_v1`, recompute-and-compare scaffold)

To run:

- `make lean`
- `make verify-lean*` and `make verify-lean-e2e*` (see `docs/howto/FORMAL_VERIFICATION.md`)

### 12.2 What’s next (the high-value tightening path)

1. Domain rewrite derivations (beyond local groupoid rules)
   - Extend the rewrite-step machinery from local groupoid normalization to domain rewrite systems (unit conversions, schema migration rewrites, reconciliation explanations).
2. Reconciliation proofs
   - Not only “decision was X”, but “decision is justified by a derivation under the policy”.
3. Query certificates
   - Every “certified” query answer from the PathDB executor carries a certificate, not just reachability demos.
4. Anchoring certificates to canonical inputs
   - Expand beyond the current `axi_digest_v1` + `relation_id` anchoring: introduce stable fact ids for canonical domain `.axi` (module digest + local id, or content addressing).
5. Trusted kernels for modalities/temporal logic
   - A small Lean core for modal/temporal semantics with certificates for inferences involving time and obligation.

---

# Part V — Production readiness

## 13. Trusted computing base and threat model

Production readiness begins with a clear correctness envelope.

### 13.1 Trusted computing base (TCB)

The TCB should be as small as possible. Ideal TCB:

1. Lean kernel + mathlib foundations used
2. Certificate parsers in Lean (strict, minimal)
3. The semantic meaning functions in Lean

Everything else is untrusted:

- Rust runtime and all optimizations
- Ingestion pipelines
- LLM behavior and prompts
- Storage, indexing, caching layers

### 13.2 Threat model (what can go wrong)

- Malicious or corrupted certificates: crafted JSON meant to exploit parsing or overflow.
- Nondeterminism: different machines produce different certificates; verification becomes flaky.
- Semantic drift: engine behavior changes silently; certificates no longer mean what auditors expect.
- LLM hallucinations: injected false facts pollute the graph.
- Supply-chain risk: dependencies affect correctness/security.

### 13.3 Security and robustness principles

Recommended principles for production:

1. Fail closed: if certificate verification fails, reject the result (or mark it unverified explicitly).
2. Strict parsing: no lossy coercions; bounds checks everywhere.
3. Deterministic serialization: stable order; canonical JSON; fixed-point arithmetic in checker.
4. Version everything: `.axi` dialect, certificate kinds, PathDB format.
5. Separate untrusted inputs: LLM-derived facts are quarantined until reconciled and (when possible) certified.

### 13.4 Conceptual failure modes (“how this can become dumb”)

This project only works if we are honest about what is being guaranteed. Common ways systems like this fail:

1. **Treating “certificate-checked” as “true”**
   - Certificates do not make base facts correct. They only prove that a result follows from some set of inputs under the agreed semantics.
   - Recommendation: track acceptance tiers explicitly (e.g., proposed vs accepted vs certified derivation), and require provenance for all promoted facts.
2. **Verifying the engine instead of the meaning**
   - If the checker merely recomputes the same algorithm as the engine, you can accidentally “prove equivalence to the algorithm” rather than correctness relative to a stable spec.
   - Recommendation: keep Lean meaning functions small and stable; treat “recompute-and-compare” as a scaffold, and move toward replayable derivation steps where it matters.
3. **Inconsistency blowups and “silent unknown”**
   - Real KGs contain contradictions and incompleteness. Classical logic can explode under inconsistency; closed-world assumptions can silently turn “unknown” into “false”.
   - Recommendation: make open-world vs closed-world assumptions explicit (layering + shapes); decide early whether conflict handling is fail-closed, paraconsistent, or context-indexed.
4. **Misusing inverses / groupoid completion**
   - Adding formal inverses can be correct for *equivalence/rewrites*, but wrong if interpreted as factual invertibility of arbitrary relations.
   - Recommendation: only treat inverses as part of a declared rewrite/equivalence theory (or only for relations explicitly declared invertible).
5. **“Probability math-washing”**
   - Multiplying confidences is an evidence heuristic; it is not automatically calibrated Bayesian probability of truth.
   - Recommendation: document the intended interpretation of confidence; consider interval bounds if you need conservative guarantees; avoid presenting confidence as “truth probability” unless justified.
6. **Intractability in the trusted layer**
   - Combining dependent types, rewriting, modal/temporal logic, and probability can easily become undecidable or computationally infeasible if the kernel tries to “solve everything”.
   - Recommendation: the kernel checks certificates; the cluster does search/heuristics. Avoid global proof search in Lean for production paths.
7. **Proof size / verification cost explosions**
   - Naive certificates can grow with query complexity and graph diameter.
   - Recommendation: normalize proofs, DAG/Merkleize derivations, cache verified certificates, and verify only what you intend to treat as trusted/grounding.
8. **Identity drift across snapshots**
   - If entity ids / fact ids aren’t stable across versions, certificates become non-reproducible and audits fail.
   - Recommendation: anchor certificates to snapshot ids and stable fact identifiers (module hash + local id, or content addressing).
9. **LLM contamination**
   - If LLM output is allowed to bypass gating, the system becomes a high-latency hallucination amplifier.
   - Recommendation: quarantine LLM/tacit proposals; require reconciliation + quality gates + (where possible) certificates before using them for grounding.

---

## 14. Engineering checklist

Actionable production TODOs (trust tiers, anchoring, certificate ubiquity, hardening) are tracked in `docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md`.

### 14.1 Determinism

- Use stable identifiers for entities/relations derived from canonical inputs (planned: module hash + local id).
- Ensure certificate emission:
  - Uses fixed-point numerators (v2),
  - Has stable ordering of steps,
  - Has stable rounding rules (documented).
- Ensure query planners don’t depend on hash-map iteration order (use ordered maps where needed).

### 14.2 Versioning and migrations

- `.axi` dialects: keep parsers in Rust and Lean in lockstep; maintain `examples/canonical/corpus.json` as the compatibility contract.
- Certificates: never change meaning without bumping `version` or `kind`.
- PathDB: maintain a stable on-disk format version (`FORMAT_VERSION` etc in `rust/crates/axiograph-pathdb/src/verified.rs`).

### 14.3 Testing strategy (recommended layering)

1. Pure semantics tests (Lean)
   - Golden certificate vectors that must verify/fail
   - Theorems about invariants (`VProb` bounds, normalization idempotence, etc.)
2. Emitter tests (Rust)
   - Round-trip encode/decode of certificates
   - Deterministic emission tests (“same inputs, same JSON bytes”)
3. End-to-end tests
   - `make verify-lean-e2e*` style: Rust emits certs; Lean verifies.
4. Fuzzing (production hardening)
   - Fuzz certificate parsers on the Rust side (untrusted input)
   - Fuzz `.axi` parser on both sides using the same corpus

### 14.4 Performance and scaling

- Certificate checking should be cheap enough to run inline for high-value answers:
  - Keep the semantic checkers small (recompute, don’t re-derive).
- Caching:
  - Cache verified certificates by hash,
  - Cache normalization outputs and path confidences,
  - Separate “unverified candidate cache” from “verified cache”.
- Use PathDB indexes for reachability and retrieval; keep certificate sizes bounded by using normalization and factoring.
- Distributed evolution: keep facts as canonical replicated state (log + snapshots) and treat PathDB indexes as derived per shard/replica; see `docs/explanation/DISTRIBUTED_PATHDB.md`.

### 14.5 Observability and auditability

Every production answer should carry:

- An answer id and certificate hash,
- The `.axi` module hashes it depended on,
- A human-readable derivation explanation derived from the certificate witness.

### 14.6 Ontology engineering workflow and quality controls (Keet-inspired)

Beyond “the checker verifies certificates”, production ontologies need a disciplined development loop: requirements, reuse, tests, and quality gates.

A practical workflow (summarizing standard ontology engineering practice, e.g. Keet’s *An Introduction to Ontology Engineering*, 2nd ed.) looks like:

1. **Requirements + scope**
   - Maintain a scope statement and a list of intended use cases.
2. **Competency questions (CQs) as tests**
   - Treat CQs as query tests: store them, run them, and keep regression suites.
   - In Axiograph, a CQ should compile to a query whose answers are certificate-backed.
3. **Reuse strategy**
   - Reuse foundational/core ontologies or patterns where it improves interoperability.
   - Keep reuse modular: import only what you need; record alignment assumptions explicitly.
4. **Quality checks**
   - Taxonomy quality (e.g. OntoClean-style rigidity/identity/unity/dependence checks).
   - Common pitfall linting (OOPS!-style checks: cycles, wrong inverses, missing disjointness, naming/pathology patterns).
   - “No silent unknown”: make open-world vs closed-world assumptions explicit (layering + shapes).
5. **Release engineering**
   - Versioning, changelogs, deprecations/supersession, publishing metadata (who, why, when).

Actionable TODOs for this repo are tracked in `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`.

---

## 15. Tightening the theory

### 15.1 Identify the minimal semantic kernel

The kernel should define:

1. The base typed graph interface.
2. Path formation and equivalence (groupoid/rewrite semantics).
3. Confidence algebra (`VProb`) and its combination laws.
4. The meaning of reconciliation decisions (as a function).
5. The meaning of normalization (as a function).

Everything else is an implementation detail that must emit certificates against this kernel.

### 15.2 Use mathlib structures where possible

Avoid bespoke proofs of standard category/groupoid facts. Candidates:

- `Mathlib.CategoryTheory.Groupoid.FreeGroupoid` for “paths up to groupoid laws”.
- `Quiver.Paths` for raw path syntax.
- Quotient/congruence infrastructure for rewriting relations.

The goal is to spend proof effort on Axiograph-specific meaning (probability + policies + tacit knowledge), not on re-proving associativity lemmas.

### 15.3 Make normalization a theorem, not just a function

For a normalization procedure `norm : Path → Path`, we want at least:

1. Soundness: `p ≈ norm p`
2. Idempotence: `norm (norm p) = norm p`
3. Congruence: normalization respects composition appropriately
4. Optionally: completeness for the chosen rewrite theory (hard; domain-dependent)

The current `normalize_path_v2` certificate is a scaffold: Lean recomputes normalization and checks it matches the
claimed normal form. The next tightening step is to add explicit rewrite-step derivations (rule + position),
prove those steps sound against the chosen semantics (ideally reusing mathlib’s free constructions), and extend the
pattern to domain rewrites.

### 15.4 Make probability laws explicit and scoped

Be explicit about what confidence means operationally:

- Is confidence “probability of truth”? Often not.
- Is it “reliability of source”? Sometimes.
- Is it “strength of evidence”? Often.

Engineering target:

- A small set of algebraic laws that hold by construction,
- Clear separation between interpretation (domain meaning) and combination (mechanical rule).

One productive direction is to structure confidence as a semiring/ordered monoid, and interpret different evidence channels (handbook vs tacit vs sensor) via homomorphisms into this algebra.

### 15.5 Tighten reconciliation as a policy-checked derivation

Reconciliation is where real-world messiness lives:

- Conflicting values,
- Different provenance,
- Tacit vs encoded rules,
- Temporal supersession.

Production tightening path:

1. Define a reconciliation policy language with a semantics in Lean.
2. Make Rust produce a reconciliation certificate that includes:
   - The conflicting claims,
   - Their evidence summaries,
   - The policy branch taken,
   - Any resulting merged value.
3. Lean re-checks the policy evaluation and validates any merge computations.

### 15.6 Tighten querying as certificate composition

Ultimately, a query answer certificate should be compositional:

- Path certificates compose via `KGTrans`,
- Normalization certificates justify canonicalization,
- Reconciliation certificates justify conflicts resolved along the way,
- Probability certificates justify confidence numbers.

### 15.7 Other possibilities (extensions that fit the architecture)

Because Axiograph separates meaning (Lean) from computation (Rust), many extensions are additive: define semantics + define a certificate kind + emit certificates from the engine.

Examples that fit naturally:

- Algebraic effects / graded monads: model “confidence-carrying computation”, provenance accumulation, or resource usage as typed effects with checkable laws.
- Context-first semantics: make context/world indexing explicit (see §6.5) and add certificates for context transport, supersession, and perspective-alignment.
- Mapping strength contracts: treat schema alignments as functors plus a contract (exact/broader/narrower/overlap) and certify which inferences are preserved (or emit counterexample certificates).
- OWL + SHACL bridging: treat OWL-style open-world entailment and SHACL-style closed-world validation as two layers, with certificates for “shape validation” and explicit “unknown” rather than silent failure.
- Information-flow control: attach security labels to facts and prove (by certificate) that a query answer’s derivation does not leak restricted sources.
- Imprecise/interval uncertainty: move from point confidences to intervals or credal sets; certify conservative bounds (lower/upper) rather than a single number.
- Counterexample certificates: return “why not” proofs (e.g., minimal missing edge set, violated constraint witness) when a query fails or a rule does not apply.
- Learned heuristics with verified outputs: let ML/LLM components propose candidate derivations, but only accept those that pass certificate checking.
- Proof compression / Merkleization: hash-cons large derivations, store proofs as DAGs, and verify by hash references (useful for huge audit trails).
- Storage and index verification: use Verus/Kani/Prusti-style tooling to prove PathDB invariants, then rely on Lean for semantic correctness of returned certificates.

---

# Part VI — Application areas (detailed use cases)

Each use case below follows the same end-to-end pattern:

1. Ingest sources (documents, conversations, telemetry).
2. Normalize into `.axi` and/or PathDB.
3. Reconcile conflicts and attach confidence/provenance.
4. Query for an answer.
5. Produce a certificate (witness) and verify it in Lean.

## 16. Manufacturing / engineering

### 16.1 Problem

You want a knowledge backend that can answer:

- “What cutting speed should I use for Ti-6Al-4V with a carbide end mill?”
- “Why is that recommendation safe?”
- “Which sources disagree, and what did we do about it?”

The domain contains:

- Authoritative but sometimes outdated handbooks,
- Local shop-floor tacit knowledge,
- Changing tool vendors and materials,
- Safety-critical constraints.

### 16.2 Inputs

- PDFs: machining handbooks, vendor datasheets.
- Conversations: experienced machinist notes (“never do X”, “watch for Y”).
- Structured: tool catalogs, material properties.
- Telemetry: CNC logs, tool wear outcomes (future).

### 16.3 Representation strategy

Entities:

- `Material`, `Tool`, `Operation`, `Parameter`, `Constraint`, `Source`.

Relations (examples):

- `has_property(Material, Property)`
- `recommended_speed(Operation, SpeedRange)`
- `uses_tool(Operation, Tool)`
- `supported_by(Fact, Source)`
- `contradicts(Fact, Fact)`

Confidence:

- Handbook claims start high,
- Tacit claims start moderate and gain strength with repeated corroboration,
- Sensor-derived claims can be high but context-scoped.

### 16.4 Conflict + reconciliation example

Two sources disagree:

- Handbook: “Ti-6Al-4V speed ≤ 60 m/min” (confidence 0.95)
- Veteran: “Keep it under 50 m/min to be safe” (confidence 0.85)

Reconciliation policy might:

- Choose the stricter constraint for safety,
- Preserve both as contextual recommendations,
- Record the decision.

In v2 certificates, Rust can emit a `resolution_v2` certificate asserting the decision; Lean checks the decision function deterministically.

### 16.5 Query example (and what “proof” means here)

Query: “Recommend a speed bound for Ti-6Al-4V milling with carbide.”

Answer should return:

- A recommended speed bound (e.g. 50 m/min),
- A confidence score,
- A derivation that cites the handbook rule, the veteran’s tacit rule, and the reconciliation policy decision.

In early phases, we can compose:

- Reachability/path certificates for traversal,
- Resolution certificates for conflict decisions,
- Human-readable explanations derived from those certificates.

---

## 17. Biomedical / clinical

### 17.1 Problem

You want to answer:

- “Is drug X contraindicated for a patient with condition Y?”
- “Which guidelines support that?”
- “How confident are we, and what evidence conflicts?”

Clinical knowledge is high-stakes, time-sensitive, full of exceptions, and mixes statistical evidence with normative obligations.

### 17.2 Inputs

- Clinical guidelines (versioned, time-bounded),
- Papers and meta-analyses (evidence strength varies),
- Local hospital policies (deontic obligations),
- Patient-specific context (strict privacy constraints).

### 17.3 Why modalities matter here

- Deontic: “must not prescribe”, “should prefer”, “may consider”.
- Temporal: “as of guideline version 2025-03”.
- Epistemic: “we know X given lab results”, “it’s possible Y”.

The system needs to keep these distinctions explicit, or it will conflate “is” with “should”.

### 17.4 A concrete flow

1. Ingest guideline statements as encoded rules (high confidence, time-scoped).
2. Ingest paper claims as probabilistic evidence with provenance and confidence.
3. Ingest clinician tacit rules (“in practice we avoid…”) as tacit facts.
4. Reconcile guideline vs paper vs tacit suggestions with a policy that prioritizes authoritative sources but records dissent.
5. Query: return a recommended action + justification, not just raw facts.

### 17.5 Production notes (privacy and governance)

This is where linear/quantitative typing (future) becomes valuable:

- Represent “permission to use patient data” as a resource,
- Ensure queries consume permissions appropriately,
- Produce auditable “data use receipts”.

Even before linear typing, production requires strong access control, audit logs, and careful separation of untrusted inputs.

---

## 18. Cybersecurity

### 18.1 Problem

You want to answer:

- “Is host DB reachable from the internet via an exploit chain?”
- “Show the exploit path.”
- “How confident are we, given incomplete telemetry?”

This is naturally a reachability/path problem with uncertainty.

### 18.2 Inputs

- Asset inventory, network topology,
- Vulnerability scanners (imperfect),
- Logs and alerts (noisy),
- Analyst notes (tacit).

### 18.3 Path certificates are immediately useful here

A reachability certificate can literally be an exploit chain:

- `Internet → WebServer → RCE(vuln) → LateralMove → DB`

The certificate includes:

- Node ids and relation types,
- Confidence per step (scanner confidence, exploit reliability, telemetry trust),
- A computed confidence for the chain.

Implemented pieces:

- Rust: `ReachabilityProofV2` in `rust/crates/axiograph-pathdb/src/certificate.rs`
- Lean: parsing and confidence recomputation in `lean/Axiograph/Certificate/Format.lean`

### 18.4 Approximate search, verified answers

Security graphs are large and dynamic. You often want:

- Fast heuristics to propose candidate attack paths,
- Then verified acceptance of those candidate paths.

This is the ideal proof-carrying scenario:

- The verifier does not need to reproduce the full search,
- It only checks that the returned chain is a valid chain and that confidence is computed correctly.

---

## 19. Legal / compliance

### 19.1 Problem

You want to answer:

- “Are we allowed to do X under policy P in jurisdiction J as of date D?”
- “Which obligations and exceptions apply?”
- “What is the provenance?”

This domain demands:

- Deontic modalities (must/may/forbidden),
- Temporal scopes (effective dates, supersession),
- Exceptions and precedence rules,
- Deep audit requirements.

### 19.2 Representation strategy

Model:

- Obligations as rules with explicit jurisdiction and temporal scope,
- Exceptions as higher-priority rules,
- Conflicts resolved by policy precedence (certificate-checked).

The proof returned to the user is an audit narrative derived from the certificate:

- Which rule applied,
- Why exceptions did/did not trigger,
- Which sources define those rules.

---

## 20. LLM memory + grounding

### 20.1 Problem

LLMs are useful at:

- Summarizing conversations and documents into candidate structured facts,
- Proposing query expansions,
- Drafting explanations.

LLMs are unreliable at:

- Strict truthfulness,
- Consistent reasoning,
- Stable citations.

So we use LLMs as untrusted assistants and make the KG the source of verified memory.

### 20.2 KG → LLM: grounded context

The system builds a grounding context containing:

- Facts and their confidences,
- Provenance/citations,
- Guardrails and constraints,
- Suggested follow-up queries.

See: `docs/explanation/LLM_KG_SYNC.md`.

Production requirement:

- Grounded facts must come from verified/accepted graph state, not raw LLM output.

### 20.3 LLM → KG: proposal and reconciliation

LLM-extracted facts flow through:

1. Validation (schema fit, basic constraints),
2. Conflict detection (does it contradict?),
3. Reconciliation (merge/choose/review),
4. Acceptance into `.axi` + PathDB with provenance.

The proof-carrying extension is:

- Reconciliation produces certificates,
- Query results cite those certificates,
- The system can display “why we believe this” with a verifiable chain.

---

# Appendix A: Code map

This appendix points to where the math described above lives in the repo.

## A.1 Lean (trusted checker / spec)

- HoTT/path core:
  - `lean/Axiograph/HoTT/Core.lean`
  - `lean/Axiograph/HoTT/KnowledgeGraph.lean` (`KGPath`, `KGPathEquiv`, `Fact`)
  - `lean/Axiograph/HoTT/PathAlgebraProofs.lean` (path confidence/length proofs)
  - `lean/Axiograph/HoTT/FreeGroupoid.lean` (mathlib-backed free groupoid semantics bridge)
- Verified probabilities:
  - `lean/Axiograph/Prob/Verified.lean` (`VProb`, `Precision`, `vMult`, Bayes update)
- Certificate parsing (the bridge):
  - `lean/Axiograph/Certificate/Format.lean` (v1/v2 parsing, normalize_path scaffold)
- `.axi` parsing:
  - `lean/Axiograph/Axi/*`

## A.2 Rust (untrusted runtime engine)

- PathDB verified layer scaffolding + proof-shaped data:
  - `rust/crates/axiograph-pathdb/src/verified.rs` (`VerifiedProb`, `ReachabilityProof`, `ProvenQueryResult`)
- Certificates emitted to Lean:
  - `rust/crates/axiograph-pathdb/src/certificate.rs` (`CertificateV2`, `ReachabilityProofV2`, `ResolutionProofV2`, `NormalizePathProofV2`)
- LLM sync and typed path validation patterns:
  - `rust/crates/axiograph-llm-sync/src/path_verification.rs`
- DSL parsing and canonical `.axi` entrypoint:
  - `rust/crates/axiograph-dsl/src/axi_v1.rs`

## A.3 Historical Idris2 prototype (removed)

An early Idris2 proof-layer prototype informed several Lean ports (HoTT/path algebra, probability, etc.).
The initial Rust+Lean release removes Idris/FFI compatibility; refer to git history if you need the original Idris sources.

---

# Appendix B: Glossary and notation

- Entity: a node in the knowledge graph.
- Relation: a typed edge between entities.
- Path: a compositional witness (sequence/tree) of relations connecting two entities.
- Path equivalence: a proof that two paths mean the same thing under groupoid/rewrite laws.
- Rewrite rule: a generator of path equivalence, usually domain-specific.
- Normalization: mapping a path to a canonical representative of its equivalence class.
- Certificate: serialized witness object emitted by Rust and checked by Lean.
- Trusted checker: Lean code that validates certificates against formal semantics.
- Untrusted engine: Rust and all ingestion/LLM components; may be wrong unless certified.
- Confidence / `VProb`: a bounded evidence value in `[0,1]` represented as fixed-point in the trusted checker.
- Tacit knowledge: experience- or context-derived claims that are useful but revisable; modeled explicitly with provenance and uncertainty.
- Modalities: operators/types that express world-indexed truth (time, knowledge, obligation, possibility).

---

# Appendix C: Related work and literature

This appendix is a curated reading list plus “what it implies for Axiograph”. It is not meant to be exhaustive; it is meant to be a practical map.

## C.1 Prior internal exploration

- `ChatGPT-Dependently_typed_ontology (3).md`
  - Useful takeaways:
    - Model schemas as “finitely presented categories” (objects + generating edges + path equations).
    - Model instances as functors into `Type`.
    - Treat relations as edge objects (reification) to support provenance/metadata.
    - Treat queries as dependent types whose inhabitants are matches/witnesses.
  - Feasibility note: this is a good design *kernel*; the production trick is to separate untrusted execution from trusted checking via certificates (the direction this repo has taken).

## C.2 Dependent type theory and proof assistants

- Nordström, Petersson, Smith — *Programming in Martin-Löf’s Type Theory*.
- Harper — *Practical Foundations for Programming Languages*.
- Brady — *Type-Driven Development with Idris* (and Idris2 / QTT materials).
- Avigad, de Moura, Kong, et al. — *Theorem Proving in Lean* (Lean4 book).
- Pientka and collaborators — *Beluga* and contextual type theory (useful background for “contexts/worlds as first-class”):
  - Boespflug & Pientka — “Multi-Level Contextual Type Theory”.
  - Cave & Pientka — “A Case Study on Logical Relations using Contextual Types”.
  - Errington, Jang, Pientka — “Harpoon: Mechanizing Metatheory Interactively” (Beluga’s interactive proof engine).
  - Schwartzentruber & Pientka — “Semi-Automation of Meta-Theoretic Proofs in Beluga”.

How this relates:

- Dependent types let us encode invariants (endpoints match; bounds; well-typed derivations) so ill-formed objects are unconstructable in the trusted kernel.
- Con: full dependent-type reasoning is not generally decidable; production systems should avoid “proof search in the kernel” and instead check certificates.

## C.3 HoTT, groupoids, and rewriting

- *Homotopy Type Theory: Univalent Foundations of Mathematics* (“The HoTT Book”).
- For category/groupoid background: Fong & Spivak — *Seven Sketches in Compositionality*; Spivak — *Category Theory for the Sciences*.

How this relates:

- HoTT’s path/groupoid vocabulary is a natural fit for “derivations up to equivalence”.
- Con: full HoTT foundations (univalence, HITs) are heavy; the pragmatic approach is to use the groupoid/rewrite fragment as a semantics layer and keep the trusted checker small.

## C.4 Categorical databases and CQL

- Spivak and collaborators — work on *Functorial Data Migration* and the *Categorical Query Language (CQL)* tooling ecosystem.
- A small starter set (good entry points):
  - Spivak — “Functorial Data Migration” (arXiv:1009.1166).
  - Wisnesky, Spivak, Schultz, Subrahmanian — “Functorial Data Migration: From Theory to Practice” (also appears as a NISTIR).
  - Spivak & Wisnesky — “Relational Foundations for Functorial Data Migration” (graph schemas as finitely presented categories; adjoint data migrations; FQL).
  - Brown, Spivak, Wisnesky — “Categorical Data Integration for Computational Science” (CQL in scientific/data integration settings).

How this relates:

- Schema mappings as functors and data migration via functoriality match Axiograph’s “paths + equivalences + transport” goals.
- Pro: a clean semantics for schema evolution and alignment.
- Con: most categorical database work is equational; Axiograph adds probability, modality, and certificate checking, which complicates the story but keeps it auditable.

## C.5 Semantic Web: RDF, OWL, SHACL, description logics

- W3C Recommendations: RDF 1.1, OWL 2, SHACL.
- Core W3C specs (good anchors for interop):
  - RDF 1.1 Concepts and Abstract Syntax; RDF 1.1 Semantics.
  - SPARQL 1.1 Query Language.
  - OWL 2 Web Ontology Language (Document Overview / Second Edition).
  - SHACL: Shapes Constraint Language.
- Relevant W3C building blocks for “contexts/provenance” (often used alongside RDF/OWL):
  - Named graphs: Carroll, Bizer, Hayes, Stickler — “Named Graphs, Provenance and Trust” (HPL-2004-57).
  - PROV (provenance): PROV-DM (data model) and PROV-O (OWL2 ontology).
- Work-in-progress to watch (as of late 2025):
  - RDF 1.2 drafts (e.g., updated N-Triples/N-Quads).
  - RDF-star / SPARQL-star working group charter (statement-level metadata without full reification).
- Allemang & Hendler — *Semantic Web for the Working Ontologist*.
- Baader, Calvanese, McGuinness, Nardi, Patel-Schneider (eds.) — *The Description Logic Handbook*.

How this relates:

- OWL gives a well-studied, decidable entailment fragment under open-world semantics (good for interoperability and for some automated reasoning).
- SHACL gives closed-world validation (“shapes”) (good for ingestion and data quality).
- Axiograph can incorporate both as layers:
  - treat OWL/DL entailments as one admissible reasoning fragment (engine may propose; checker validates within that fragment),
  - treat SHACL-like validation as a certificate-checked ingestion step (“raw” → “validated” graph),
  - preserve “unknown” explicitly rather than silently assuming closure.
- Con: OWL’s restrictions are real; the moment you want richer dependent constraints, you leave decidable DL territory and must rely on certificates and/or bounded checking.

## C.6 Modal, temporal, and deontic logic

- Blackburn, de Rijke, Venema — *Modal Logic*.
- Pnueli — “The Temporal Logic of Programs”.
- Clarke, Grumberg, Peled — *Model Checking*.

How this relates:

- Modal/temporal formalisms give clean semantics for “as of”, “must/may”, and multi-context truth.
- Pro: matches production needs (policies, versioning, supersession).
- Con: unrestricted modal/temporal reasoning can be expensive; the certificate approach lets the engine do the heavy lifting while the checker validates locally.

## C.7 Linear logic, quantitative types, and session types (future)

- Girard — “Linear Logic”.
- Wadler — “Linear types can change the world!” (linearity as a programming discipline).
- Honda, Vasconcelos, Kubo — session types (protocol-as-type line).

How this relates:

- Linear/quantitative typing is a natural next step for privacy/compliance, tokenized capabilities, and protocol correctness around query execution.
- Pro: a principled way to prevent data misuse by construction (or by certificate).
- Con: adds substantial complexity; best introduced as an optional layer with a small semantics, not as a rewrite of the whole system.

## C.8 Probabilistic reasoning in graphs and logic

- Pearl — *Probabilistic Reasoning in Intelligent Systems*.
- Koller & Friedman — *Probabilistic Graphical Models*.
- Richardson & Domingos — “Markov Logic Networks” (probability + logic).
- Probabilistic logic programming: ProbLog (De Raedt, Kimmig, Toivonen, et al.; IJCAI 2007 is a common entry point).
- Shafer — *A Mathematical Theory of Evidence* (Dempster–Shafer style evidence).

How this relates:

- There are many “probability on knowledge graphs” formalisms; Axiograph’s current choice is conservative: a verified evidence algebra (`VProb`) with deterministic arithmetic in the checker.
- Pro: certificate checking stays simple and deterministic.
- Con: the semantics is not automatically “true Bayesian inference”; if you need that, treat Bayesian models as an additional layer with its own certified meaning function.

## C.9 Proof-carrying code, small kernels, and certificate checking

- Necula — “Proof-Carrying Code”.
- Appel & Felten — “Proof-Carrying Authentication”.
- Schneider, Felten, Bauer — “A Proof-Carrying Authorization System”.
- LCF-style theorem proving (small trusted kernel; untrusted tactics).
- Certified compilation projects (e.g., CompCert) as an existence proof that “untrusted generation + trusted checking” scales.

How this relates:

- Axiograph is “proof-carrying data”: results are untrusted until a small checker validates the certificate.
- Pro: you can optimize/replace the engine without changing semantics.
- Con: designing certificates is a real product/design problem; it requires discipline (versioning, determinism, bounded size).

## C.10 LLM + KG integration and grounded generation

- Lewis et al. — “Retrieval-Augmented Generation (RAG) for Knowledge-Intensive NLP Tasks”.
- Petroni et al. — “Language Models as Knowledge Bases?” (and follow-ups).
- Yao et al. — “ReAct: Synergizing Reasoning and Acting in Language Models”.

How this relates:

- LLMs are best treated as proposal/heuristic engines.
- Axiograph’s certificates give you a crisp separation: LLM output can be stored, reconciled, and only promoted to “trusted/grounding” status when it is connected to verified facts and certified derivations.

## C.11 Feasibility and trade-offs (pros/cons summary)

Feasible:

- A dependently typed ontology/mapping/query kernel is feasible (this repo already has the core ingredients: typed paths, verified probability representation, and a certificate-checking pipeline).
- Scaling comes from certificate checking, not from global proof search in the trusted kernel.

Key trade-offs:

- Expressiveness vs decidability: OWL/DL gives decidable automation; dependent types give expressiveness. The certificate architecture lets you have both by being explicit about which fragment you are in.
- Proof burden vs product velocity: keep the Lean kernel small; put search/heuristics in Rust; add certificates incrementally for high-value operations.
- Open-world vs closed-world: support both by layering (raw/unknown vs validated/checked), and make “unknown” explicit.

Practical recommendation:

- Use Semantic Web standards for interop at the edges (import/export), but keep the internal trust story anchored in Lean semantics + certificates.

## C.12 Rust foundations and verification ecosystem (highly relevant)

Rust is the untrusted engine in Axiograph, so “Rust literature” matters in two ways:

1. **What Rust itself guarantees (and where it doesn’t)**: ownership/borrowing in safe Rust; soundness boundaries around `unsafe`.
2. **What we can verify beyond Rust’s type system**: functional correctness, storage invariants, concurrency behavior, and robustness against adversarial inputs.

### C.12.1 Foundations: what Rust safety means (and how it fails)

- RustBelt (POPL 2018): formal (machine-checked) safety proof for a realistic Rust subset, and a method for stating verification conditions for unsafe libraries.
  - Project page: https://plv.mpi-sws.org/rustbelt/popl18/
  - Publication record: https://doi.org/10.1145/3158154
- Oxide (2019): a core formalization of Rust’s ownership/borrowing model (“the essence” of borrow checking).
  - https://arxiv.org/abs/1903.00982
- Polonius: a Datalog-style model of borrow checking with a “book” explaining the analysis.
  - Repo: https://github.com/rust-lang/polonius
  - Book: https://rust-lang.github.io/polonius/
- Unsafe guidance (practical + operational semantics discussion):
  - Rustonomicon: https://doc.rust-lang.org/nomicon/
  - Rust Unsafe Code Guidelines repo + glossary: https://github.com/rust-lang/unsafe-code-guidelines and https://rust-lang.github.io/unsafe-code-guidelines/glossary.html
  - Rust Reference (`unsafe` keyword): https://doc.rust-lang.org/reference/unsafe-keyword.html

Why this matters for Axiograph:

- Any use of `unsafe` (FFI, packed I/O formats, custom indexing) must establish and re-establish invariants at module boundaries. This maps directly to our “untrusted engine” discipline: unsafe blocks must be locally auditable and covered by tests/analysis, while *semantic correctness* is ensured by certificates checked in Lean.

### C.12.2 Verification tools for Rust (beyond the compiler)

Model checking / symbolic tools:

- Kani (CBMC-based): model checking for Rust code via proof harnesses (good for panics, overflows, many UB patterns).
  - https://model-checking.github.io/kani/
- Verify Rust Std effort (Rust stdlib verification contest and tool ecosystem): Kani, ESBMC, Flux, VeriFast, etc.
  - https://model-checking.github.io/verify-rust-std/
  - https://github.com/model-checking/verify-rust-std

Deductive verifiers:

- Prusti (Viper-based): contracts/specs to prove functional properties, absence of panics/overflows, etc.
  - https://github.com/viperproject/prusti-dev
- Creusot (Why3-based): deductive verification; used for nontrivial verified Rust projects.
  - https://github.com/creusot-rs/creusot

“Verified Rust” / SMT-assisted subsets:

- Verus (SMT-based): verify Rust-like code with specs/proofs, including low-level invariants; supports reasoning about pointers/concurrency via ghost state.
  - Tool: https://github.com/verus-lang/verus
  - Paper (extended): https://arxiv.org/abs/2303.05491

Translation to theorem provers:

- Aeneas (ICFP 2022): translates safe Rust into a functional form for proof assistants; includes a Lean backend.
  - Paper: https://arxiv.org/abs/2206.07185
  - Tool: https://github.com/AeneasVerif/aeneas
  - Background work on LLBC soundness (2024): https://arxiv.org/abs/2404.02680

Static analysis / UB detection:

- Miri: interpreter for Rust MIR that detects many UB classes in executions and isolates nondeterminism by default.
  - https://github.com/rust-lang/miri
- MIRAI: abstract interpreter for MIR; can find panics and check user-encoded contracts; also supports taint analysis.
  - https://github.com/endorlabs/MIRAI

How this relates:

- Rust verification tools can harden PathDB storage invariants, numeric bounds, and unsafe boundaries. They do **not** replace Lean certificate checking, because they do not define the same semantics we care about (path/groupoid/rewrite meaning, reconciliation policy meaning). Instead, they reduce “engine bug surface” and complement the certificate approach.

### C.12.3 Concurrency testing and fuzzing for production hardening

Concurrency testing:

- Loom: exhaustive concurrency permutation testing for small tests (C11 memory model).
  - https://github.com/tokio-rs/loom
- Shuttle: randomized scheduler testing inspired by Loom (scales to larger tests; not exhaustive).
  - https://github.com/awslabs/shuttle

Fuzzing:

- cargo-fuzz (libFuzzer): practical fuzzing harness support for Rust crates.
  - https://github.com/rust-fuzz/cargo-fuzz
- AFL.rs / honggfuzz-rs: alternative fuzzing backends in the Rust fuzz ecosystem.
  - https://github.com/rust-fuzz/afl.rs
  - https://github.com/rust-fuzz/honggfuzz-rs

Why this matters:

- PathDB parsers, certificate serialization/deserialization, and FFI boundaries are classic “fuzz me” surfaces.
- Concurrency tests matter if we introduce background indexing, async ingestion, or concurrent PathDB reads/writes.

### C.12.4 Rust semantics research (optional, but helpful for deep assurance)

Executable semantics work (useful to understand edge cases and for tool building):

- KRust (K Framework): https://arxiv.org/abs/1804.10806
- RustSEM (K Framework): https://arxiv.org/abs/1804.07608

These are not necessary for day-to-day Axiograph development, but they’re relevant if we ever want “semantics-aware” analysis of Rust engine code or if we need to reason about tricky lifetime/aliasing corner cases at the semantic level.

### C.12.5 Practical recommendations for Axiograph (Rust-side hardening)

These are prioritized steps that fit the “untrusted engine, trusted checker” architecture:

1. **Minimize and isolate `unsafe`**
   - Keep `unsafe` code localized (FFI + binary parsing); document invariants per module boundary; prefer safe parsing patterns over transmutes/packed reads.
2. **Fuzz the untrusted surfaces**
   - Add fuzz targets for: PathDB parsing/reading, certificate JSON parsing/serialization, `.axi` parsing, and any FFI entrypoints that accept bytes/strings.
3. **Run Miri on core crates**
   - Use Miri to detect UB in tests (especially around `unsafe`, pointer aliasing assumptions, and tricky lifetime patterns).
4. **Use model checking for small but critical functions**
   - Apply Kani to: fixed-point arithmetic (`FixedPointProbability`), bounds-checked parsing, path/certificate constructors, and “no panic/overflow” guarantees.
5. **Use Verus (already scaffolded) for invariants that matter**
   - Prove local invariants like: binary offsets within bounds; index consistency; probability bounds; “endpoints match” conditions for witness chains; determinism where feasible.
6. **Add concurrency schedule testing only where concurrency exists**
   - If/when PathDB or sync pipelines become concurrent, add Loom tests for the smallest concurrency kernels; use Shuttle for larger randomized schedules.
7. **Treat Lean verification as a production gate**
   - For high-value query endpoints, require `make verify-lean-e2e*`-style checks in CI; on failure, reject or label results “unverified” explicitly (fail-closed).
8. **Consider Aeneas selectively (later)**
   - Once certificate + parsing interfaces stabilize, consider translating the smallest, most security-critical “byte→AST→certificate” kernels into Lean/Coq for deep assurance. Do not attempt to translate the whole runtime.

## C.13 Ontology engineering methodology, quality, and reuse (very relevant)

Keet’s textbook is especially relevant to Axiograph because it covers the *engineering lifecycle* around ontologies (requirements, reuse, testing, quality, publishing), not only the logic.

- Keet — *An Introduction to Ontology Engineering* (Second Edition, 2025).

Selected “practice primitives” worth reflecting into Axiograph features and CI gates:

- **Competency questions (CQs)** as requirements/tests:
  - Grüninger & Fox — “Methodology for the Design and Evaluation of Ontologies”.
  - In Axiograph terms: CQs should compile to queries whose results are certificate-backed and regression-tested.
- **Taxonomy quality / OntoClean-style meta-properties** (rigidity, identity, unity, dependence):
  - Guarino & Welty — OntoClean papers (FOIS-era work; “cleaning up” taxonomies with philosophical meta-properties).
  - In Axiograph terms: meta-properties become optional annotations + a lint/check layer (and eventually certificate-checked guarantees for key taxonomic claims).
- **Pitfall scanning / linting** (OOPS! and similar):
  - Poveda-Villalón et al. — OOPS! (Ontology Pitfall Scanner!) and follow-up work.
  - In Axiograph terms: add an `axi lint` stage that checks for structural and semantic anti-patterns (cycles, wrong inverses, “misc” buckets, naming pathologies, misuse of negation, etc.).
- **Ontology Design Patterns (ODPs)**:
  - Gangemi and the WOP (Workshop on Ontology Patterns) community.
  - In Axiograph terms: treat patterns as reusable `.axi` templates/modules with explicit instantiation steps, and ensure pattern instantiation preserves invariants (certificate-friendly).
- **Modularisation and reuse**:
  - Module extraction for reuse, scalability, and privacy (locality modules, abstraction modules, expressiveness modules).
  - In Axiograph terms: support “module packaging” and “locality extraction” over `.axi` corpora and PathDB snapshots; integrate with distributed deployment (`docs/explanation/DISTRIBUTED_PATHDB.md`).
- **Ontology matching and alignment**:
  - Euzenat & Shvaiko — *Ontology Matching*; OAEI (Ontology Alignment Evaluation Initiative).
  - In Axiograph terms: represent correspondences with confidence and distinguish “alignment candidate” from “mapping that preserves satisfiability/invariants”; emit certificates for alignment decisions where feasible.
- **OBDA (Ontology-Based Data Access)**:
  - The DL-Lite / OBDA line of work (Calvanese et al. and the OBDA community).
  - In Axiograph terms: treat data-source mappings as first-class, versioned artifacts (like schema maps), and use certificates to validate query rewriting results against the formal semantics.
- **Publishing and metadata**:
  - FAIR principles; MIRO guidelines (and related ontology metadata practices).
  - In Axiograph terms: make module metadata explicit (`.axi` module headers), hash/version snapshots, and require provenance in production answers.

Concrete TODOs derived from this section are tracked in `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`.

## C.14 Database provenance (why/where/how) and explanation algebras (high leverage)

There is a deep database literature on “why did I get this answer?” and “which inputs contributed?” that maps directly onto Axiograph’s goal of **proof-carrying answers**.

Selected entry points:

- Buneman, Khanna, Tan — “Why and Where: A Characterization of Data Provenance” (ICDT 2001). https://www.cs.cornell.edu/~bkhanna/papers/whywhere.pdf
- Green, Karvounarakis, Tannen — “Provenance Semirings” (PODS 2007). https://doi.org/10.1145/1265530.1265535
- Amsterdamer et al. — “Putting Lipstick on Pig: Enabling Database Provenance for Datalog” (VLDB 2018). https://doi.org/10.14778/3229863.3229866

How this relates:

- Axiograph certificates are “how-provenance”, but with a **trusted checker** that gives them semantic force: a derivation is not just an explanation; it is a checkable witness that an answer follows under the agreed meaning.
- Provenance semirings suggest an *algebra of explanations*. This can inform a clean interface between:
  - “confidence/evidence combination” (currently `VProb`),
  - “provenance accumulation” (sources, citations, contexts),
  - and “derivation structure” (paths + rewrite decisions).

Production insight:

- If we align certificate composition with a provenance algebra, we get principled composition of explanations across query operators and across distributed shards.

## C.15 Typed Datalog and monotone computation (useful query kernel options)

If we want automation and optimization for a decidable query/inference fragment, Datalog and its typed variants are a strong candidate.

- Arntzenius, Krishnaswami, Greenberg, et al. — “Datafun: A Functional Datalog” (ICFP 2016). https://doi.org/10.1145/2951913.2951948

How this relates:

- Datafun’s “monotone/semilattice” discipline is a good fit for incremental inference, caching, and safe approximation layers.
- Axiograph can treat a Datalog-like core as an *untrusted execution engine* that must emit certificates (or derivation traces) checked by Lean, while still benefiting from decades of Datalog optimization.

## C.16 Certified query compilation and verifiable execution (optional, but clarifying)

Proof-carrying answers can be approached at multiple layers:

1. **Derivation certificates (Axiograph approach)**: the engine emits a witness; the checker validates it against the semantics.
2. **Certified compilation**: prove that a compiler preserves query meaning, then trust the compiled code.
3. **Cryptographic verification**: prove that an execution result matches a computation on committed data (SNARK/STARK/ZK).

Representative pointers:

- Benzaken, Contejean, Dumbrava, et al. — Q\*cert: a Coq query compiler (paper + artifact). https://dl.acm.org/doi/10.1145/3563323
- ZKP-based SQL (research direction): PoneglyphDB “Verifiable Query Execution for Blockchain-based Databases” (preprint). https://kira.cs.umd.edu/papers/poneglyphdb_preprint.pdf

How this relates:

- Certified compilation is relevant if we want *high assurance* in a stable query core, but it increases proof burden and reduces flexibility.
- Cryptographic verification becomes relevant primarily for cross-trust-boundary deployments (multi-tenant, inter-org audits). Axiograph’s immediate “trustless-ish” path is simpler: snapshot commitments + membership proofs (see `docs/explanation/DISTRIBUTED_PATHDB.md`), with ZK as an optional later layer.

## C.17 Equality saturation and e-graphs for rewrite engines (very relevant for normalization)

E-graphs provide a high-performance way to explore and apply equational rewrite systems (“rewrite/groupoid semantics”) in an untrusted engine.

Key references:

- Tate, Stepp, Tatlock, Lerner — “Equality Saturation: A New Approach to Optimization” (PLDI 2009). https://dl.acm.org/doi/10.1145/1542476.1542528
- Willsey et al. — “egg: Fast and Extensible Equality Saturation” (POPL 2021). https://doi.org/10.1145/3434304

How this relates:

- Axiograph’s normalization and reconciliation steps naturally look like equational reasoning tasks.
- E-graphs can make the engine much faster and can help produce smaller certificates by normalizing results.
- The trusted story remains: e-graphs compute candidates; Lean checks meaning (either by replaying rewrite steps or by validating a normal form against a meaning function).
