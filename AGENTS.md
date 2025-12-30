## Axiograph — Agent Notes (where we are so far)

This file is a shared context note for humans/agents working in this repo. It captures the current technical reality and the direction we’ve committed to.

### Current state (as-is in `axiograph_v6/`)

- **Build system**: `axiograph_v6/Makefile` builds **Rust + Lean** (Idris/FFI compatibility removed for the initial Rust+Lean release).
- **Runtime / system backbone (Rust)**: `axiograph_v6/rust/` is a workspace with production crates:
  - `axiograph-dsl` (canonical `.axi` parsing)
  - `axiograph-pathdb` (binary storage + “verified” layer)
  - ingestion crates (`axiograph-ingest-*`), `axiograph-llm-sync`, `axiograph-storage`
- **Snapshot store (Accepted plane + PathDB WAL)**:
  - `.axi` accepted snapshots are canonical (meaning plane).
  - `.axpd` is derived and can be rebuilt from accepted snapshots.
  - `axiograph db accept pathdb-commit` stores **evidence-plane** overlays (currently: `proposals.json` + `chunks.json`) as append-only WAL ops with checkpoints.
- **Proof/verification layer (Lean)**: `axiograph_v6/lean/Axiograph/` contains:
  - **Rewrite/groupoid semantics** (mathlib-backed): `Axiograph.HoTT.*`
  - **Certificate format + checking**: `Axiograph.Certificate.*`
  - `.axi` parsing parity: `Axiograph.Axi.*`
- **Historical Idris prototype**: Idris was used early as a proof-layer prototype. The initial Rust+Lean release removes Idris/FFI compatibility; refer to git history if you need to consult the old Idris modules while porting remaining theory into Lean.
- **Rust already has “certificate-like” witnesses** we can reuse:
  - `axiograph_v6/rust/crates/axiograph-pathdb/src/verified.rs` defines `ReachabilityProof` and `ProvenQueryResult` (explicit witness chains + confidence composition), plus `VerifiedProb` invariants.
  - `axiograph_v6/rust/crates/axiograph-llm-sync/src/path_verification.rs` implements a typed `VerifiedGraph`, `Path`, conflict detection.
- **Rust formal verification tooling is already present (partial)**:
  - `axiograph_v6/rust/verus/` exists as a verification crate (Verus-oriented).

### Decision (committed direction)

We will move to:

- **Lean4 + mathlib** for the **proof/spec/certificate-checking layer**.
- **Rust** remains the **runtime/ABI/performance layer** (compiler, ingestion, PathDB, sync, etc.).
- Keep existing **examples and all features** working during migration (no “flag day”).

Core idea: **untrusted engine, trusted checker**

- Rust computes (rewriting, path search, reconciliation, optimization).
- Rust emits a **certificate** (witness of rewrite/groupoid derivation).
- Lean checks the certificate against the formal rewrite/groupoid semantics.

### End goal (north star)

Deliver a **proof-carrying knowledge backend** where every high-value inference is *auditable* and *machine-checkable*:

- **Rewrite/groupoid semantics are the canonical source of truth**:
  - The meaning of “paths”, “equivalence”, normalization, and confidence propagation is defined in Lean.
  - Any future engine optimization must preserve these semantics (proved once, then reused).
- **All critical operations become certificate-producing** (untrusted) and certificate-checked (trusted):
  - **Path derivations** (reachability, composition, inverse) emit explicit witness chains.
  - **Reconciliation** emits a derivation explaining why a merge/rewrite is valid.
  - **Normalization** emits a proof that the normalized form is equivalent to the original.
  - **Confidence propagation** emits evidence that bounds/invariants were preserved.
- **Rust remains the production runtime**:
  - Fast ingestion, search, optimization, PathDB storage, and LLM sync live in Rust.
  - Rust is allowed to be “clever”; correctness is ensured by emitting checkable certificates.
- **Lean is the trusted checker and spec**:
  - A small Lean checker verifies certificates (and thus the outputs) against the formal semantics.
  - The system can reject/flag results whose certificates do not verify.
- **No regression in functionality**:
  - All existing `.axi` examples and Rust demos continue to work.
  - Lean is the only trusted checker for the initial release.

### Migration constraints (non-negotiable)

- **No feature loss**: all existing functionality remains available during the transition.
- **Examples keep working**: `axiograph_v6/examples/` and the Rust+Lean test suites must continue to pass.

### Docs (Diataxis)

- `docs/README.md` is the canonical navigation entrypoint (Tutorials / How-to / Reference / Explanation).

### Near-term work plan (next concrete steps)

- **Add Lean project scaffold in `axiograph_v6/`**:
  - Create `axiograph_v6/lean/` with `lakefile.lean`, `lean-toolchain`, and a minimal module tree.
  - Add an optional `make lean` (and eventually `make verify-lean`) target.
- **Define a minimal certificate format** shared between Rust and Lean:
  - Goal: represent “rewrite/groupoid semantics” proofs, not just reachability.
  - Start from existing Rust `ReachabilityProof` structure and Lean `Axiograph.HoTT` constructors.
  - Prefer a stable serialization (JSON/CBOR) already used in the Rust workspace (`serde`, `ciborium`).
- **Implement end-to-end slice**:
  - Rust emits certificate for a small representative example.
  - Lean verifies certificate.
  - Wire into an e2e test (Rust emits, Lean verifies).
- **Rust verification tools (Verus/Aeneas/Prusti/Kani)**:
  - Use **Verus** first (already scaffolded) for core invariants in PathDB/probability/path witnesses.
  - Add other tools selectively where they add distinct value (memory safety proofs, model checking, etc.).

### Pointers for new agents

- **Rewrite/groupoid semantics are in Lean**:
  - `axiograph_v6/lean/Axiograph/HoTT/KnowledgeGraph.lean`
  - `axiograph_v6/lean/Axiograph/HoTT/PathAlgebraProofs.lean`
- **Rust “witness” starting points**:
  - `axiograph_v6/rust/crates/axiograph-pathdb/src/verified.rs` (`ReachabilityProof`, `VerifiedProb`)
  - `axiograph_v6/rust/crates/axiograph-llm-sync/src/path_verification.rs`
- **Build entrypoint**: `axiograph_v6/Makefile`

### Migration checklist (living)

**Lean (spec + checker)**
- [x] Lake project scaffold (`lean/`) + mathlib dependency pinned.
- [x] Targets: `make lean`, `make verify-lean*`, `make verify-lean-e2e*`.
- [x] Port: historical Idris `Axiograph.HoTT.Core` → `lean/Axiograph/HoTT/Core.lean`.
- [x] Port (partial): historical Idris `Axiograph.HoTT.KnowledgeGraph` → `lean/Axiograph/HoTT/KnowledgeGraph.lean` (`KGPath`, `KGPathEquiv`, facts).
- [ ] Port (remaining): `KnowledgeGraph.idr` transport, quotient, `KGEquiv` / migration scaffolding (Lean `Quot`-based).
- [x] Port (partial): historical Idris `Axiograph.HoTT.PathAlgebraProofs` → `lean/Axiograph/HoTT/PathAlgebraProofs.lean` (length + confidence).
- [ ] Port (remaining): inverse paths, equivalence congruence, normalization, functoriality proofs (prefer mathlib groupoid/free groupoid where applicable).
- [x] Port: historical Idris `Axiograph.Prob.Verified` → `lean/Axiograph/Prob/Verified.lean` (fixed-point `VProb`, combination laws, Bayes update, etc).
- [ ] Port: historical Idris `Axiograph.Prob.PathVerificationVerified` → `lean/Axiograph/Prob/PathVerificationVerified.lean`.
- [ ] Port: historical Idris `Axiograph.Prob.ReconciliationVerified` → `lean/Axiograph/Prob/ReconciliationVerified.lean`.
- [ ] Port: remaining theory modules (modal/temporal/tacit, reconciliation) into Lean as needed for the trusted checker.
- [x] Parse canonical `.axi` in Lean: unified `axi_v1` parser (`lean/Axiograph/Axi/AxiV1.lean`).
- [x] Unified `.axi` entrypoint in Lean: `axi_v1` dialect detection (`lean/Axiograph/Axi/AxiV1.lean`).
- [ ] Converge parsers to a shared `Axiograph.ModuleAST` (schema/theory/instance), anchored to `examples/canonical/corpus.json`.
  - Canonical corpus manifest: `examples/canonical/corpus.json`
  - Current corpus uses the unified `axi_v1` entrypoint (dialect-detecting); the versioned parsers remain for clarity.

**Certificates**
- [x] Cert v1 (JSON): reachability witness + Lean checker.
- [x] Cert v2 (JSON): fixed-point reachability witness (`rel_confidence_fp`) + Lean checker.
- [x] Cert v2: reconciliation decision (`resolution_v2`) + Lean checker.
- [x] Cert v2: normalization (`normalize_path_v2`) + Lean checker.
- [x] Cert v2: path equivalence (`path_equiv_v2`) + Lean checker (shared normal form + optional derivations).
- [x] Extend `normalize_path_v2` to groupoid rewrite derivations (assoc/inv/cancel + explicit step list).
- [ ] Extend cert v2 rewrite derivations beyond normalization (reconciliation proofs, domain rewrites).
- [ ] Anchor certificates to canonical `.axi` inputs (stable module hash + extracted facts).
- [x] Keep backward compatibility: accept v1 certificates during transition.
  - Spec notes: `docs/reference/CERTIFICATES.md`

**Rust runtime / emitters**
- [x] Rust e2e: emit reachability cert (`make verify-lean-e2e`).
- [x] Rust e2e: emit fixed-point reachability cert v2 (`make verify-lean-e2e-v2`).
- [x] Rust e2e: emit resolution cert v2 (`make verify-lean-e2e-resolution-v2`).
- [x] Rust e2e: emit normalize_path cert v2 (`make verify-lean-e2e-normalize-path-v2`).
- [x] Rust proof-mode scaffolding: `ProofMode` generic + proof-producing optimizer (normalize/resolution/Δ_F).
- [x] PathDB snapshot export/import: `.axpd` ↔ `.axi` (`PathDBExportV1`) + CLI (`axiograph db pathdb export-axi|import-axi`).
- [x] DB server wrapper: `axiograph db serve` (read-only replica + optional write master) serving `/query` (AxQL) + `/status` + admin endpoints.
- [x] DB server viz endpoints: `GET /viz` (HTML), `GET /viz.json` (graph JSON), `GET /viz.dot` + `refresh_secs=N` for live-ish auto-refresh.
- [x] DB server certificate endpoints:
  - `GET /anchor.axi` (export PathDBExportV1 anchor),
  - `POST /cert/reachability` (reachability cert from relation-id chains),
  - `/query` supports `certify/verify/include_anchor` and optional default `contexts`.
- [x] DB server can optionally verify certificates server-side:
  - verifier discovery: `--verify-bin`, `AXIOGRAPH_VERIFY_BIN`, `bin/axiograph_verify`, repo dev fallback,
  - exposed in `/status.certificates.*`.
- [x] DB server can enforce a “fail closed” certificate gate for high-value answers:
  - `/llm/agent` supports `require_query_certs` / `require_verified_queries`,
  - `/viz` LLM tab exposes “require verified query certificates (fail closed)”.
- [x] DB server demos (scripts): `scripts/db_server_api_demo.sh`, `scripts/db_server_distributed_demo.sh`, `scripts/db_server_live_viz_demo.sh`.
- [ ] Rust emits v2 certificates for reachability + confidence (aligned with Lean `VProb`).
- [ ] Rust emits certificates for normalization and reconciliation from the real runtime (untrusted engine, trusted Lean checker).
- [x] Canonical `.axi` parser in Rust: unified `axi_v1` entrypoint (`rust/crates/axiograph-dsl/src/axi_v1.rs`).
- [x] Unified `.axi` entrypoint in Rust: `axi_v1` dialect detection (`rust/crates/axiograph-dsl/src/axi_v1.rs`).
- [x] Rust↔Lean parsing parity (canonical corpus): `make verify-axi-parse-e2e`.
- [x] Rust↔Lean parsing parity (PathDB snapshot export): `make verify-pathdb-export-axi-v1`.
- [ ] Keep Rust/Lean parsers in lockstep (surface grammar + AST) for the canonical corpus.

**Ingestion / promotion (evidence → candidates)**
- [x] `proposals.json` → candidate domain `.axi` modules (entity resolution + schema mapping + promotion trace) via `axiograph discover promote-proposals`.
- [x] Proto ingestion emits semantic annotation edges (auth scopes/idempotency/stability/tags + field required/PII/units/examples).
- [x] Optional chunk overlay: `axiograph db pathdb import-chunks` + `fts(...)` for doc search and LLM grounding (extension layer; not certifiable).
- [x] Promote reviewed candidates into the accepted `.axi` plane (append-only log + snapshot ids) and rebuild PathDB from snapshots.

**Rust formal verification (Verus, additive)**
- [x] Optional Verus target: `make verify-verus` (runs `rust/verus/src/lib.rs` if Verus is installed).
- [ ] Align Verus probability model with Lean fixed-point `VProb` (avoid floats in verified core).
- [ ] Prove certificate invariants for runtime witnesses (reachability, normalization, reconciliation).

**Rust “dependent type” encodings (design)**
- [x] Documented typestate/branding/witness patterns: `docs/explanation/RUST_DEPENDENT_TYPES.md`.
- [x] Documented Rust↔Lean “Topos view” correspondence for type-directed execution (`docs/explanation/TOPOS_THEORY.md`, `docs/explanation/RUST_DEPENDENT_TYPES.md`).

**Mathlib targets to reuse (preferred over re-inventing)**
- `Mathlib.CategoryTheory.Groupoid.FreeGroupoid` (free groupoid on a quiver; good fit for “paths up to groupoid laws”).
- `Mathlib.CategoryTheory.Quotient` + `Quiver.Paths` (quotiented path semantics and rewriting relations).
- `Mathlib.CategoryTheory.Sites.Grothendieck` + `Mathlib.CategoryTheory.Sites.Sheaf` (sites/sheaves/subtopoi; good fit for “contexts/worlds + modalities”).
- `Mathlib.CategoryTheory.FintypeCat` (finite-set semantics for `.axi` instances).
- `Mathlib.MeasureTheory.Measure.GiryMonad` (a reference point for probability semantics; not yet in the trusted checker).

**Topos / sheaf semantics (knowledge, contexts, modalities)**
- [ ] Define a schema presentation category in Lean (relations as objects + projection arrows), then treat instances as functors into `FintypeCat`.
- [ ] Define context/world scoping semantics in Lean as a presheaf/sheaf story (avoid implicit closed-world assumptions; keep “unknown ≠ false” explicit).
- [ ] Express modal operators (`□`, `◇`) via Lawvere–Tierney / Grothendieck topologies (proof-only; not in `axiograph_verify` imports).
- [x] Add a concrete, readable explanation doc: `docs/explanation/TOPOS_THEORY.md` (ties `.axi` + PathDB + certificates to the topos view).

**Probabilistic / approximate semantics (extension layer; certifiable subsets later)**
- [x] Add untrusted analysis tools to measure “semantic drift” between contexts/snapshots (KL/JS divergence over distributions of relations/types).
- [ ] Define a discrete distribution interface in Lean (finite, fixed-point) and document how to keep probability analytics outside the trusted checker.
- [ ] Explore a future “probability in a topos” track (Giry monad / Markov categories) for semantics-level documentation and later certificate-bounded checks.

### Literature-driven roadmap deltas (Appendix C of `docs/explanation/BOOK.md`)

These items are “best practices” backed by the related work list in Appendix C
(CQL/functorial migration, proof-carrying code, Semantic Web interop, Rust verification tooling).

**Category / migration semantics (CQL / functorial data migration)**
- [x] `Δ_F` (pullback) runtime scaffold (Rust) + functoriality test.
- [ ] `Δ_F` certificate + Lean checker (recompute-and-compare first; tighten later).
- [ ] `Σ_F` (left Kan extension) runtime scaffold (Rust) + certificate/checker.
- [ ] `Π_F` (right Kan extension) runtime scaffold (Rust) + certificate/checker.
- [ ] Natural transformations between schema functors (2-cells) as first-class (for “composition up to iso”).
- [ ] Relations as edge-objects + projection arrows in the core schema semantics (so migration treats relations uniformly).

**Rewrite/groupoid tightening (HoTT/groupoids/rewrite literature + mathlib)**
- [x] Normalization certificate with optional explicit rewrite derivation replay.
- [ ] Prove rewrite-step soundness against the mathlib free-groupoid denotation (move from “recompute” to “sound-by-theory”).
- [x] First-class rewrite rules in canonical `.axi` theories (structured `rewrite ...` blocks), imported/exported via the PathDB meta-plane.
- [x] `rewrite_derivation_v3`: derivations can reference either builtin rules or `.axi` rules by `(axi_digest_v1, theory, rule)` (anchor-scoped).

**Interop layering (Semantic Web / SHACL / provenance)**
- [x] RDF import boundary using Sophia (TriG/N-Quads/Turtle + RDF/XML), mapping named graphs → `Context`/world scopes (adapter emits `Context` entities and `context`-scoped relation proposals).
- [x] Public dataset fixtures + demos for RDF/OWL/SHACL ingestion:
  - committed: `examples/rdfowl/w3c_shacl_minimal/`
  - optional fetch: `scripts/fetch_public_rdfowl_datasets.sh` (W3C data-shapes + LUBM)
  - demo: `scripts/rdfowl_public_datasets_demo.sh`
- [ ] RDF-star / statement-level metadata support in the adapter layer (quoted triples → fact/evidence/provenance objects) so we can attach attribution to individual claims.
- [ ] OWL/RDFS mapping layer (`TBox` → `.axi` schema+theory, `ABox` → `.axi` instance) with an explicit “unknown ≠ false” story (no silent closed-world assumptions).
- [ ] SPARQL SELECT subset as an interop dialect (compile into AxQL IR; keep SPARQL as an untrusted boundary language).
- [ ] SHACL validation as a promotion/ingestion gate (prefer best-in-class Rust libs like `oxirs-shacl` or `rudof`), emitting:
  - a stored validation report (evidence plane), and
  - optional `.axi` constraint proposals derived from shapes (reviewable before acceptance).
- [ ] SHACL subset → certificate-checked ingestion step (“raw → validated”), keeping “unknown” explicit (start with core constraints that map to our `.axi` constraint vocabulary).
- [ ] Dataset/named-graph validation support (SHACL-DS-style semantics) for multi-context snapshots.
- [ ] Shape learning / constraint discovery stage (heuristics + optional LLM assist) to propose shapes/constraints from data, then reconcile/promote into the accepted plane.
- [ ] PROV-inspired provenance conventions for contexts (source/authority/time/policy/conversation), plus docs/examples that emphasize:
  - “certificate-checked” ≠ “true”, and
  - no implicit unique-name assumption (identity must be asserted/reconciled).
- [ ] Optional tooling integration for ontology-engineering UX: `rudof` conversions (SHACL/ShEx/DCTAP) + useful visualization outputs.
- [ ] Certificate kind(s) for “validated import”: anchor the canonical `.axi` digest + (optional) adapter inputs, then prove the adapter produced the claimed `.axi` snapshot/module.

**Rust hardening (RustBelt/Oxide + verification ecosystem)**
- [ ] Fuzz untrusted surfaces (PathDB bytes, certificate JSON, `.axi` parsing, FFI entrypoints).
- [ ] Run Miri on core crates in CI/dev loops (UB detection for tests).
- [ ] Add Kani harnesses for small critical kernels (fixed-point arithmetic, bounds-checked parsing, “no panic” invariants).
- [ ] Add Loom/Shuttle tests once we introduce concurrency kernels (async ingestion, background indexing).
- [ ] Consider Aeneas later for *tiny* byte→AST→certificate kernels (not the whole runtime).

**Semantics-relevant test suite**
- [x] Focused target: `make verify-semantics` (runs Rust+Lean semantics checks without requiring full workspace compilation).
- [x] CI workflow runs `make rust-test` + `make verify-semantics` (Lean installed via elan).
- [x] CI validates Helm render + raw k8s manifests with `kubectl --dry-run=client`.
- [ ] Keep expanding `verify-semantics` as the “must pass” suite while other crates are mid-migration.

**CI/CD + deployment**
- [x] Release workflow builds multi-OS binaries and publishes GitHub release assets.
- [x] GHCR container build/push with multi-arch image.
- [x] Dockerfile defaults to `axiograph db serve` for `/viz` + `/query`.
- [x] K8s manifests + Helm chart for a StatefulSet + PVC-backed storage.
- [x] CI builds the Dockerfile (no push) to catch container regressions.
- [ ] Add smoke tests for container startup + `/status` in CI.
- [ ] Provide a chart values profile for replicas + read-only replicas (fan-out) and ingress examples.

**AxQL performance (planner/runtime)**
- [x] Keep simple path chains as RPQ and route to PathIndex when beneficial; add fast single-path execution.
- [x] Add selectivity heuristics (relation-type counts + candidate sizes) and atom ordering; reduce RPQ clone overhead.
- [x] Add shared prepared-query cache keyed by snapshot + query IR (REPL uses).
- [ ] Add a perf harness for cache wins (repeat queries) and report hit/miss + timing deltas.
- [ ] Calibrate `PATH_INDEX_MIN_LEN` by workload; consider adaptive switching based on relation fan-out.
- [ ] Extend cache usage to `db serve` and non-REPL query paths where safe; keep LRU bound configurable.
- [ ] Add per-relation degree stats (avg/median/out-degree histograms) to improve selectivity estimates.

**REPL ergonomics (discovery-first UX)**
- [x] Add REPL discovery primitives:
  - `describe <entity>` (attrs + contexts + equivalences + grouped in/out edges),
  - `open chunk|doc|evidence|entity ...` (evidence navigation),
  - `diff ctx ...` (context/world diffs),
  - `neigh ...` (REPL-driven viz export).
- [x] Add `q --explain` plan output (join order + candidate domains + FactIndex hints).
- [x] Add a typed JSON query IR for tooling/LLMs (`query_ir_v1`) that compiles into the same AxQL core (REPL `llm query` prints this; the LLM tool-loop emits it; raw AxQL is fallback only).
- [x] Add schema-qualified AxQL for multi-schema “one universe” snapshots:
  - `?x is Fam.Person` / `?x -Fam.Parent-> ?y` / `?f = Fam.Parent(child=..., parent=...)`,
  - ambiguous unqualified edge labels elaborate to either a chosen schema (when inferred) or a union alternation.
- [x] Add first-class disjunction (`or`) in AxQL with a certifiable subset:
  - execution: UCQ semantics (union of conjunctive branches),
  - certificates: `query_result_v2` (Lean checks each row against the chosen branch).
- [x] Add an LLM “tool loop” (lookup + elaborate + run + propose) so `llm ask`/`llm answer` are multi-step and models don’t emit raw AxQL by default.

**Ontology exploration / visualization (tooling)**
- [ ] Extend the self-contained HTML explorer (`axiograph tools viz --format html`) with ontology-explorer-style ergonomics:
  - node/edge export (CSV/JSON) for external tools (Gephi),
  - basic graph metrics table (degree, in/out degree; optional betweenness),
  - quick “compare” views for snapshots/contexts (diff/union/intersection overlays).
- [x] Add plane-aware filters in the HTML explorer (accepted / evidence / data) to support ontology-engineering loops over multiple planes.
- [x] Make confidence + evolution exploration easier in the viz UI:
  - confidence slider (hide/attenuate low-confidence edges),
  - time-travel snapshot selector when served via `axiograph db serve` (`GET /snapshots`, `GET /viz?...&snapshot=<id>`).
- [x] Add certified exploration hooks in `/viz` (server mode):
  - AxQL query panel (`run`, `certify`, `certify+verify`) calling `POST /query` and highlighting result ids,
  - path certification (`Certify path`, `Verify path`) calling `POST /cert/reachability` (directed relation-only paths).
- [x] Add UI-driven evidence-plane mutation in `/viz` (server mode):
  - generate a reviewable `proposals.json` overlay (and optional `DocChunk` evidence),
  - commit it to the PathDB WAL via the master-only admin endpoint, producing a new PathDB snapshot id.
- [x] Make “add data” proposals schema-aware + validated:
  - infer `(axi_schema, axi_source_field, axi_target_field)` from the meta-plane (when available),
  - default common required fields (`ctx`, `time`) deterministically,
  - import relation proposals as typed tuple facts (field edges + `axi_fact_of`) instead of opaque edges,
  - support n-ary facts via `extra_fields` and the LLM tool `propose_fact_proposals` (field map),
  - treat `axi_fact_in_context` as uniform scoping metadata (even when a relation signature lacks a `ctx` field),
  - return a preview validation report (axi typecheck + delta quality findings) from `/proposals/relation(s)` and LLM proposal tools.
- [x] Speed up PathDB WAL replay by caching derived CBOR sidecars for `chunks.json` and `proposals.json` blobs (JSON remains the human-readable source of record).
- [x] Add a network analysis CLI surface (untrusted tooling; evidence-plane friendly) that can emit JSON/text summaries:
  - connected components (weak/strong), “giant component” ratio,
  - degree distribution + top hubs/authorities (PageRank),
  - bridge detection (approx betweenness), and
  - community detection (Louvain; Leiden later) for “topic islands” in discovered graphs.
- [ ] Wire network-analysis results into viz as optional overlays (color by community/centrality; facet/filter by component/context).
- [ ] Add a snapshot diff + “set ops” CLI surface (graph union/intersection/difference) over `.axpd` and/or accepted-plane snapshots.
- [ ] Add “schema-first” exploration modes: tree views for object/subtype/relation signatures, plus theory panels (constraints + rewrite rules).
- [ ] Make contexts/worlds first-class in viz/query UX (facets/filters, per-context overlays, provenance display).
- [ ] Add a “competency questions” runner: executable query suites attached to examples + CI checks (ontology engineering best practice).

**Web/GitHub ingestion (tooling; evidence plane)**
- [x] GitHub importer: `axiograph ingest github import` (repo index + optional proto ingest) emits merged `chunks.json` + `proposals.json`.
- [x] Web importer: `axiograph ingest web ingest` (URL list or crawl) emits `manifest.jsonl` + `chunks.json` + `facts.json` + `proposals.json` (rate limits + robots.txt + size caps).
- [x] Large-ish scrape demos (networked scripts):
  - `scripts/web_mixed_sources_demo.sh`
  - `scripts/web_wikipedia_crawl_demo.sh`
- [x] Offline “all source types” ontology-engineering demo:
  - `scripts/ontology_engineering_all_sources_offline_demo.sh`

**Quality checks (ontology + data)**
- [x] Add a first-class `axiograph check quality` command family that produces a structured report (JSON + human-readable).
  Initial checks:
  - meta-plane lint: detect “missing meta-plane” and subtyping cycles (warning),
  - data-plane lint: dangling references (error),
  - schema constraints (best-effort): key/functional violations (error),
  - context scoping coverage (info, strict profile only).
- [ ] Expand quality checks to cover more ontology/data hygiene:
  - unused/near-duplicate symbols, ambiguous n-ary field naming,
  - symmetric/transitive closure checks (as info/warn, not “truth”),
  - rewrite-rule lint (unreachable rules, non-terminating orientations heuristics).
- [x] Make `axiograph db accept promote` optionally run a quality profile (fast/strict) and attach the report to the accepted-plane commit metadata.
- [x] Add a certifiable subset of quality gates:
  - “well-typed module” (already exists),
  - “constraint-satisfied for core constraints” (`axi_constraints_ok_v1`: key/functional), and
  - “rewrite derivation validates” (already exists; extend to rule-set soundness later).

**Docs system (Diataxis)**
- [ ] Re-organize docs into Diataxis quadrants (Tutorials / How-to / Reference / Explanation), keeping existing content but improving navigation.
- [ ] Add “Start here” tutorial flows for: `.axi` authoring, accepted-plane promotion, PathDB snapshots, AxQL querying, certificates + Lean verification, and ontology-engineering loops.
