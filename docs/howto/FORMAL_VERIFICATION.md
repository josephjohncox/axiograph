# Formal Verification in Axiograph v6

**Diataxis:** How-to  
**Audience:** contributors

This repo is migrating to a **proof-carrying** architecture:

- **Rust** is the high-performance *untrusted engine* (ingestion, search, optimization, PathDB).
- **Lean** is the *trusted checker* and specification (mathlib-backed).

The end goal is: **every high-value inference is accompanied by a certificate** that a small
Lean checker can validate against the formal semantics.

## Trusted vs untrusted boundary

**Untrusted (Rust runtime)**
- Can be “clever” and fast.
- Produces *results + certificates*.
- May be wrong; correctness comes only from passing the Lean checker.

**Trusted (Lean checker)**
- Defines the semantics we care about (paths/groupoid laws, normalization, confidence composition).
- Validates certificates produced by Rust.
- Rejects results whose certificates do not verify.

This repo previously used Idris2 as a prototype proof layer. The initial
Rust+Lean release removes Idris/FFI compatibility; Lean is the only trusted
checker.

## Ways this can be broken (and how we avoid it)

These are common failure modes in “proof-carrying” systems. We explicitly
design against them.

### 1) Conflating “certificate-checked” with “true”

A certificate check proves **derivability from the inputs under the semantics**,
not correctness of the inputs.

Mitigations:
- Keep provenance explicit: what snapshot/modules did this derive from?
- Treat ingestion as *evidence*: keep “raw” vs “accepted/canonical” planes separate.
- Prefer “soundness-only” claims in certificates (“this result is valid”), not
  completeness (“these are all results”) unless we can prove it.

### 2) Building a checker that just re-runs the same algorithm

If the Lean checker re-implements the whole engine, you’re “proving the code”,
not pinning down the meaning. It also couples bugs between emitter and checker.

Mitigations:
- Keep the trusted checker small and semantics-driven.
- Prefer certificates that are:
  - *replayable* (a derivation trace), or
  - *simple to recompute* (a small decision procedure), and
  - proved sound against the denotational semantics (e.g. free-groupoid denotation).

### 3) Not making “unknown vs false” explicit

Graphs assembled from heterogeneous sources are not closed-world databases.
Silently treating missing facts as false causes inconsistency blowups and
invalid inferences.

Mitigations:
- Keep the “unknown” state explicit in query/validation layers.
- Treat absence-of-evidence separately from evidence-of-absence.
- Make inconsistency handling a first-class, explicit policy (reject, quarantine,
  or surface contradictions with provenance), not an implicit default.

### 4) Treating inverses/groupoids as factual invertibility

Groupoid inverses are **semantic equalities / rewrites**, not “facts are always
invertible in the world”.

Mitigations:
- Keep a clear boundary between:
  - observational edges (direct claims/evidence), and
  - equivalences/rewrites (how we are allowed to transport along them).
- Make rewrite rules explicit and certificate-checked.

### 5) Treating confidence math as calibrated truth-probability

Confidence propagation here is an *evidence-weight / trust* calculus, not a
guarantee of calibrated probabilistic truth.

Mitigations:
- Keep confidence semantics explicit (fixed-point representation in v2).
- Prefer interpretable operations (monotone bounds, conservative composition).
- Avoid over-claiming: confidence supports ranking/thresholding, not “truth”.

## What is being verified (today)

**Lean (mathlib-backed)**
- HoTT-style path/groupoid vocabulary: `lean/Axiograph/HoTT/Core.lean`
- Knowledge graph paths and 2-cells: `lean/Axiograph/HoTT/KnowledgeGraph.lean`
- Path length + confidence composition (fixed-point): `lean/Axiograph/HoTT/PathAlgebraProofs.lean`
- Free groupoid semantics bridge (mathlib): `lean/Axiograph/HoTT/FreeGroupoid.lean`
- Denotational congruence lemmas (whiskering + inverse): `lean/Axiograph/HoTT/PathCongruence.lean`
- Fixed-point verified probabilities and decision procedures: `lean/Axiograph/Prob/Verified.lean`
- Mathlib building blocks we reuse (non-exhaustive):
  - Free groupoid on a quiver: `Mathlib.CategoryTheory.Groupoid.FreeGroupoid`
  - Path category / quiver paths: `Mathlib.CategoryTheory.PathCategory.Basic`
  - Kan extensions (roadmap for Σ_F/Π_F): `Mathlib.CategoryTheory.Functor.KanExtension.Adjunction`
  - Sites/sheaves/subtopoi (roadmap for contexts + modalities): `Mathlib.CategoryTheory.Sites.Grothendieck`, `Mathlib.CategoryTheory.Sites.Sheaf`
  - Probability reference point (semantics-level; not in the trusted checker): `Mathlib.MeasureTheory.Measure.GiryMonad`

Topos-theoretic semantics notes (explanation-level) live in:
- `docs/explanation/TOPOS_THEORY.md`

**Rust**
- PathDB “verified layer” scaffolding (Verus-oriented): `rust/crates/axiograph-pathdb/src/verified.rs`
- Certificate emission types: `rust/crates/axiograph-pathdb/src/certificate.rs`
- Proof-mode + proof-producing optimizer scaffold: `rust/crates/axiograph-pathdb/src/proof_mode.rs`, `rust/crates/axiograph-pathdb/src/optimizer.rs`

## Certificates (Rust → Lean)

Certificates are versioned, inspectable JSON objects. Lean parses a certificate and runs a
small verifier against it.

Detailed schema + running instructions live in:
- `docs/reference/CERTIFICATES.md`

Today we support:
- **v1 reachability**: float confidences (transition-only).
- **v2 reachability**: fixed-point confidences (trusted representation).
- **v2 reachability (anchored)**: optional `.axi` anchor + `relation_id` fact IDs checked against `PathDBExportV1` (endpoints, rel-type, confidence).
- **v2 axi_well_typed_v1 (anchored)**: canonical `.axi` module well-typedness gate (small decision procedure; Lean re-checks the parsed AST).
- **v2 axi_constraints_ok_v1 (anchored)**: core theory-constraint gate (keys/functionals; Lean re-checks against the parsed AST).
- **v2 query_result_v1 (anchored)**: conjunctive query results (AxQL / SQL-ish), with per-atom witnesses checked against `PathDBExportV1`.
- **v2 resolution**: fixed-point reconciliation decision (Lean recomputes `decideResolution`).
- **v2 normalize_path**: free-groupoid path normalization (Lean recomputes normalization) and
  optional explicit rewrite derivations (Lean replays rule+position steps).
- **v2 rewrite_derivation**: replayable rewrite traces (rule + position), reusable for domain rewrites and reconciliation explanations.
- **v2 rewrite_derivation_v3**: replayable rewrite traces with *first-class rule references* (builtin rules + `.axi`-declared rules, anchored by module digest).
- **v2 path_equiv**: groupoid path equivalence via shared normalization, with optional
  explicit derivations for both sides.

The migration direction is to keep expanding **v2** to cover:
- reconciliation and domain rewrite derivations (normalization is now derivation-capable),
- reconciliation derivations and decisions,
- confidence invariants and bounds,
- and eventually `.axi`-anchored semantics (certificates refer to facts derived from canonical input).

## Canonical `.axi` inputs (parsing parity)

`.axi` is the canonical source format. During migration, we keep a “canonical corpus”
of `.axi` modules that both Rust and Lean parsers must accept.

- Corpus manifest: `examples/canonical/corpus.json`

The corpus is parsed through a single **unified entrypoint** (`axi_v1`).

For the initial Rust+Lean release we intentionally keep exactly one canonical
surface syntax: `axi_v1` is the schema/theory/instance language implemented by
`schema_v1` on both sides (no dialect splitting).

In addition to the domain corpus, we also maintain a *reversible* PathDB snapshot export
format (`PathDBExportV1`) rendered as `.axi`. This is not user-facing, but it is part of the
auditable pipeline and is checked for Rust↔Lean parsing parity:

- `make verify-pathdb-export-axi-v1`

## How to run the checkers

Focused semantics suite (recommended during migration):
- `make verify-semantics`

Lean build:
- `make lean`
- macOS note: building Lean **executables** (e.g. `axiograph_verify`) requires a
  valid macOS SDK (`SDKROOT`). Prefer:
  - `make lean-exe` (repo root; sets `SDKROOT` via `xcrun`), or
  - `cd lean && lake script run buildExe`
  - If you run `lake build axiograph_verify` directly, set:
    `SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"`.

Lean `.axi` parsing (schema dialect):
- `make verify-lean-axi-schema-v1`

Lean `.axi` parsing (canonical corpus via unified `axi_v1`):
- `make verify-lean-axi-v1`

Lean certificate checks:
- v1 sample: `make verify-lean`
- v2 sample: `make verify-lean-v2`
- v2 resolution sample: `make verify-lean-resolution-v2`
- v2 normalize_path sample: `make verify-lean-normalize-path-v2`

Rust → Lean end-to-end checks:
- v1: `make verify-lean-e2e`
- v2: `make verify-lean-e2e-v2`
- axi well-typed gate: `make verify-lean-e2e-axi-well-typed-v1`
- axi constraints gate: `make verify-lean-e2e-axi-constraints-ok-v1`
- query results (anchored): `make verify-lean-e2e-query-result-v1`
- query results (anchored, disjunction): `make verify-lean-e2e-query-result-v2`
- query results (from canonical module): `make verify-lean-e2e-query-result-module-v3`
- v2 resolution: `make verify-lean-e2e-resolution-v2`
- v2 normalize_path: `make verify-lean-e2e-normalize-path-v2`
- v3 rewrite_derivation (axi rules): `make verify-lean-e2e-rewrite-derivation-v3`
- v2 path_equiv congruence: `make verify-lean-e2e-path-equiv-congr-v2`

## Rust-side verification (Verus, optional)

In addition to “untrusted engine, trusted checker” via Lean certificates, we use
**Verus** to formally verify selected Rust invariants (e.g. probability bounds,
bitmap bounds, and length-indexed path shapes).

The Verus-oriented crate lives at:
- `rust/verus/`

Run (if Verus is installed and on your `PATH`):
- `make verify-verus`

Or directly:
- `cd rust/verus && verus src/lib.rs`

## How this evolves (next milestones)

1. **Lean semantics completeness**
   - Port remaining semantics modules to Lean (path verification, reconciliation, modalities).
2. **`.axi` parsers in Lean and Rust**
   - Keep `axi_v1` parsing parity and expand the canonical grammar as needed.
3. **Certificate completeness**
   - Extend v2 certificates beyond reachability into groupoid/rewrite derivations and reconciliation.
4. **Operational engine alignment**
   - Rust emits certificates for the actual runtime operations (normalization, reconciliation, queries).
   - Lean checks those certificates against the formal semantics.

## Literature-driven production hardening (Appendix C.12)

Beyond semantic certificate checking, the Rust literature suggests layering complementary tools:

1. **Minimize and isolate `unsafe`**
   - Keep invariants documented at module boundaries (FFI, binary parsing, custom indexes).
2. **Fuzz untrusted surfaces**
   - PathDB bytes/decoding, certificate JSON decoding, `.axi` parsing, and FFI entrypoints.
3. **Run Miri**
   - Detect UB in tests (especially around `unsafe` and tricky aliasing/lifetimes).
4. **Use model checking selectively (Kani)**
   - Fixed-point arithmetic, bounds checks, and “no panic/overflow” kernels.
5. **Use Verus for local invariants that matter**
   - Index consistency, bounds, witness endpoint alignment, determinism where feasible.
6. **Concurrency schedule testing when we add concurrency**
   - Loom for small exhaustive kernels; Shuttle for larger randomized schedules.

These tools do not replace Lean’s semantic checking; they reduce the engine bug surface area.
