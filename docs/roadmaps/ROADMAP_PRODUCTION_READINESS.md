# Production Readiness Roadmap (typed ontology workbench as a product)

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap turns the “book” (`docs/explanation/BOOK.md`) into concrete engineering work:

- make trust tiers explicit and enforced,
- make certificates ubiquitous and anchorable,
- make outputs safe to use for grounding and decision-making,
- and make PathDB evolvable into a distributed system without breaking semantics.

This is intentionally **certificate-first**:

> Rust computes operational reports and witnesses; Lean checks the supported
> certificate and gate fragments.

For ontology-process roadmapping (CQs, linting, patterns, reuse), see `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`.
For math/migration roadmapping (Δ/Σ/Π, rewrite theory), see `docs/roadmaps/ROADMAP_MATHEMATICAL.md`.

Last updated: 2026-04-28.

---

## 0) Current status snapshot (what’s true in code today)

### 0.1 Trust tiers (partial)

- Implemented: ingestion artifacts are explicitly “proposal-shaped” (`proposals.json`), and promotion emits reviewable candidate `.axi` modules.
  - Code: `rust/crates/axiograph-ingest-docs/src/proposals.rs`, `rust/crates/axiograph-ingest-docs/src/promotion.rs`, CLI: `rust/crates/axiograph-cli/src/main.rs`.
- Implemented: a deterministic “augmentation” pass for `proposals.json` (`discover augment-proposals`), producing an auditable trace and feeding the promotion stage.
  - Code: `rust/crates/axiograph-ingest-docs/src/augment.rs`, CLI: `rust/crates/axiograph-cli/src/main.rs`.
- Implemented: LLM sync keeps “pending review” facts and conflicts.
  - Code: `rust/crates/axiograph-llm-sync/src/sync.rs`.
- Implemented: process-local ingestion and LLM-sync state has no accepted-plane
  mutation authority. Every current `GroundingContext` carries versioned
  `evidence` provenance, and the public grounding plane enum intentionally has
  no accepted/certified variant.
- Missing: an accepted-derived grounding constructor that can be created only
  from a verified materialization plus an authenticated accepted snapshot.

### 0.2 Certificates (implemented, but not yet everywhere)

- Implemented: envelope V3 / `query_result_v4` binds exact finite query
  completeness to the accepted module bytes, compiled query, ordered answer,
  and approved Lean receipt through typed SHA-256 identities.
- The legacy query family was deleted; `query_result_v4` is the only query
  certificate family. Separate non-query families include `reachability_v3`,
  `rewrite_derivation_v3`, `resolution_v2`, `normalize_path_v2`,
  `path_equiv_v2`, and `delta_f_v1`; none can satisfy the V4 query gate.
  - Specs: `docs/reference/CERTIFICATES.md`
  - Rust: `rust/crates/axiograph-pathdb/src/certificate.rs`
  - Lean: `lean/Axiograph/Certificate/Format.lean`,
    `lean/Axiograph/Certificate/Check.lean`
- Missing: verified-by-default behavior across every query/grounding endpoint;
  unsupported query fragments must continue to fail closed or remain explicit
  execution-only results.

### 0.3 Anchoring (partial)

- Implemented: `ModuleDigestV2` uses full SHA-256 over one domain-separated,
  length-framed preimage of the exact accepted UTF-8 bytes. Rust and Lean
  recompute it independently.
- `axi_digest_v1` is not an accepted-state or materialization identity;
  security-sensitive anchors use the typed V2 SHA-256 families.
- Removed: PathDB-to-`.axi` export, bincode/sectioned readers, custom WAL,
  sidecars, and bare-file query-service loading.
- Typed query witnesses and exact canonical `.axi` anchors are the
  user/server/agent certificate path.
- Operational hardening: `CanonicalFactLogV1`, stable typed fact ids, and the
  authenticated SQLite materializer provide deterministic logical rows, exact
  image identity, semantic anchors, and bounded read-only verification.
- Guarded: semantic inspection, proposal, certificate, and server paths reject
  derived images where exact accepted bytes or AxiStore receipts are required.
  - Tests: `axiograph-store --test materialization`,
    `axiograph-pathdb --test materialization_tests`, and
    `axiograph-cli --test db_server_e2e`.
- Partial: imported canonical `.axi` tuple facts already carry `axi_fact_id`;
  typed runtime construction can now fail closed through `commit_certified_only`.
  Remaining work is to thread those ids through every public certificate,
  accepted-plane manifest, and compiled-IR object/projection id.
- Missing: a distributed replica protocol that transfers and validates the
  complete AxiStore object/receipt closure before advancing local refs.

### 0.4 Release/build audit (repository gate implemented, release open)

- Implemented: `make release-gate` pins rustc 1.88.0 and Node.js 24.19.0 and
  combines catalog, formatting, no-unsafe, full locked workspace, CLI
  feature-matrix, locked frontend advisory/build checks, deep verification,
  identity/certificate/lineage/merge/storage, semantics, and diff checks.
- Implemented: tag bundles use Rust host triples, deterministic archives,
  inner/outer SHA-256 checks, fresh extraction, Unix mode checks, CLI version,
  and a real envelope V3 / stdio V2 accepted verification.
- Implemented: the container is non-root with a dedicated group, root-owned
  non-writable binaries, an installed-checker checksum check, canonical-input
  validation, authenticated DB command-surface checks, bare-`.axpd` rejection,
  BuildKit `--check`, and
  digest-first multi-architecture publication with safe arrays.
- Open CI-only blocker: no tag workflow has yet built and smoked every hosted
  runner lane and published both release assets and the GHCR manifest. Until an
  immutable run records that evidence, Linux x86_64, macOS arm64, and Windows
  x86_64 are configured candidates, not supported release platforms.
- Unsupported: native Linux arm64 bundles, macOS Intel bundles, Windows arm64
  bundles, and any other unexecuted hosted-runner path. Linux arm64 remains a
  container candidate only.
- W05 is a greenfield cutover: obsolete PathDB readers/writers are deleted;
  old bytes fail closed and must be rebuilt from exact accepted inputs.

### 0.5 Grounding / “safe to use” (evidence labeling enforced)

- Implemented: grounding uses process-local PathDB content, guardrails, and
  schema hints and always labels the resulting context as evidence-plane.
  - Code: `rust/crates/axiograph-llm-sync/src/{lib,grounding,sync}.rs`
- Missing: accepted/certified grounding. Adding it requires a separate typed
  constructor bound to a verified AxiStore materialization and accepted
  snapshot; callers cannot relabel current contexts as accepted.

---

## 1) Top priority (do these first)

### 1.1 Enforce knowledge planes (proposal vs accepted)

- [x] Establish one AxiStore directory family for accepted objects, refs,
  commits, audit, and authenticated materialization receipts.
- [x] Remove persistence authority from `axiograph-storage`; it is process-local
  evidence staging only.
- [x] Keep accepted-state promotion behind typed `AxiStore::promote` plans,
  complete gate closures, and generation CAS. Do not restore the removed
  filesystem `accepted/` layout or `db accept` CLI.

### 1.2 Make review policies real (not warnings)

- [x] Keep review-required `UnifiedStorage` changes pending instead of applying
  them automatically.
  - Constraints, low-confidence facts, and schema extensions require explicit
    approval.
  - Review configuration and evidence probabilities fail closed on invalid
    bounds; the pending queue has a hard configured limit.
  - PathDB and schema accessors expose read guards rather than mutable lock
    handles, so callers cannot bypass the review transition.
- [~] Add explicit approval and rejection flows.
  - Implemented: serialized in-process `approve_change(change_id)` and
    `reject_change(change_id, reason)` APIs with atomic rollback on failed
    application and an audit-log rejection record.
  - Missing: a stateful review service/CLI session. A standalone CLI command
    cannot address the intentionally process-local queue by id.

### 1.3 Fix entity/relation identity (stop using placeholder IDs)

- [x] Remove placeholder relation endpoints in storage writes (`source_id = 0`, `target_id = 1`).
  - Implemented: relation materialization is fail-closed when endpoints cannot
    be resolved; storage no longer invents `0 -> 1` placeholders.
- [ ] Add a stable name→entity_id index (or content-addressed entity ids) for
  in-memory PathDB construction.
- [x] Assert relation endpoints are correct after authenticated SQLite
  materialization hydration; direct PathDB persistence reload no longer exists.

### 1.4 Put “certified” on the API boundary

- [x] Keep uncertified grounding explicitly non-authoritative.
  - Every current grounding route emits versioned `evidence` provenance; there
    is no accepted/certified enum variant. Query routes separately expose
    execution-only versus verified trust contracts.
- [ ] Add an accepted-derived grounding mode only from authenticated accepted
  snapshot and verified materialization handles.
- [ ] Add a small “verifier service” boundary option:
  - engine returns `(answer, certificate)`,
  - verifier checks and returns `(answer, verified=true/false, explanation)`.

---

## 2) Next (high leverage once planes are enforced)

### 2.1 Anchor certificates to real inputs everywhere

- [~] Gate: deterministic module digest anchors.
  - Pass when fixed canonical `.axi` inputs produce the same exact-byte
    `revision_digest_v2` in Rust and Lean, and certificate writers surface that
    typed digest in their anchor fields.
  - Current checks: `make verify-semantics`, `make verify-axi-digest-e2e`,
    `axiograph-cli --test examples_e2e querycert_canonical_axi_v3_regression`.
- [~] Gate: stable canonical fact ids.
  - Pass when canonical domain `.axi` emits stable ids for facts, relation
    objects, projection arrows, theory obligations, and snapshots from module
    digest plus local/content identity.
  - Current implemented slice: imported tuple facts carry `axi_fact_id`, typed builders
    can preview/commit certified-only facts with stable ids, and
    `CanonicalFactLogV1` rejects mismatched fact ids.
  - Required regression: parse, compile, promote, and materialize the same
    canonical module twice and assert identical ids, manifest entries,
    certificates, logical digests, and exact image digests. Numeric PathDB row
    positions must not appear in public certificates.
- [ ] Gate: production certificates require anchors.
  - Pass when every endpoint/command that claims "certified" fails closed if the
    certificate lacks a canonical anchor or if verification cannot bind the
    anchor to the requested accepted snapshot.

### 2.2 Determinism and reproducibility

- [~] Gate: no floats across trusted boundaries.
  - Pass when certificate JSON, anchors, canonical snapshots, accepted-plane
    manifests, and trust contracts reject floating-point fields or encode them
    through fixed-point/domain-specific types.
  - Current enforced slice: certificate probabilities use checked fixed-point
    integers; typed anchor/store identities contain no float fields; and the
    query trust-contract family rejects unknown fields and floating-point values
    where integer counts are required. A repository-wide serialized-schema
    inventory remains before this gate can be closed.
- [x] Gate: deterministic certificate JSON golden bytes.
  - Fixed typecheck, constraints, and `query_result_v4` inputs emit
    byte-identical JSON across repeated runs. Tests compare exact bytes and the
    query certificate digest, not only parsed JSON.
- [ ] Gate: same inputs imply same outputs.
  - Pass when CLI and server certificate emitters, semantic previews, accepted
    snapshot manifests, and SQLite `.axpd` materialization run twice from the
    same accepted closure and produce identical bytes except for explicitly
    scoped run metadata.

### 2.3 Certificate ubiquity for real queries

- [ ] Decide the “certified query core” (start narrow):
  - reachability / bounded paths,
  - simple reconciliation decisions,
  - normalization, equivalence.
- [ ] Make the PathDB query executor emit Lean-checkable certificates for that core.
- [ ] Build certificate composition:
  - query certificate references subcertificates (reachability + resolution + rewrite derivations).

---

## 3) Hardening track (parallel, practical)

- [x] Gate: initial named parser and storage fuzz lane.
  - Property checks include `axi_export_property_tests`,
    `axi_constraints_ok_property_tests`, `typed_builder_property_tests`,
    `fact_index_property_tests`, `path_expr_property_tests`,
    `follow_path_property_tests`, `reachability_property_tests`, and
    `fixed_prob_property_tests`.
  - `make verify-fuzz` runs checked-corpus targets for Rust `.axi` parsing,
    Certificate V2/V3 JSON, authenticated `.axpd` bytes before PathDB hydration,
    the production CLI/REPL tokenizer, and the shared predictive-proposal
    command/HTTP envelope plus validated payload under explicit resource bounds.
- [x] Gate: optional deep-verification lanes are executable.
  - `make verify-fuzz`, `make verify-miri`, `make verify-loom`, and
    `make verify-kani` run named suites. Release verification requires the exact
    applicable tools rather than accepting skips. The Kani harness covers every
    `u32` constructor input; silent no-ops do not count.
- [x] Gate: concurrency tests only cover real concurrency.
  - `make verify-loom` models the production `ChildLimiter` reservation loop
    with the same compare/exchange algorithm under two-thread contention. It
    asserts that the configured maximum is never exceeded and all acquired
    permits are released.
- [x] Gate: release-visible frontend dependencies are audited and reproducible.
  - `make verify-viz` requires Node.js 24.19.0, installs only the npm lock with
    lifecycle scripts disabled, rejects moderate-or-higher advisories, and
    builds Vite 8.2.1. The vulnerable Rollup dependency is no longer present.

---

## 4) Distributed system readiness (when single-node is clean)

This section should be pursued only after “planes + anchors + certificates” are solid.

- [x] Gate: canonical fact log plus snapshots.
  - AxiStore owns immutable objects, snapshot/tree identities, semantic commits,
    repository-bound refs, audit records, and materialization receipts. Derived
    query images cannot mutate or reconstruct accepted state.
- [x] Gate: live `.axpd` and verified `.axpd` convergence.
  - SQLite is the only runtime format. `MaterializationIdV2` binds accepted
    snapshot/tree, ordered module closure, kernel/fact-log/configuration/overlay
    inputs, logical rows, and exact image bytes. Read-only open verifies all of
    them before hydration.
- [x] Gate: indexes are derived rebuildable state.
  - Fact, text, path, interning, and LRU indexes are process-local. Tests delete
    and deterministically rebuild images without changing logical digests or
    finite query answers.
- [ ] Gate: optional snapshot commitments.
  - Pass only if offline/third-party verification is required; then add a Merkle
    root or transparency-log commitment test that verifies a committed snapshot
    from bytes alone.

See `docs/explanation/DISTRIBUTED_PATHDB.md`.
