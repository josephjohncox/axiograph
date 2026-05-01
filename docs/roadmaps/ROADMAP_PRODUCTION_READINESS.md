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
- Missing (critical): a hard enforcement boundary so untrusted/proposal facts do not silently become “accepted knowledge” or become default grounding inputs.

### 0.2 Certificates (implemented, but not yet everywhere)

- Implemented Lean-checked certificates: reachability v1/v2 (incl. anchored), resolution v2, normalize_path v2 (optional derivations), rewrite_derivation v2, path_equiv v2, delta_f v1.
  - Specs: `docs/reference/CERTIFICATES.md`
  - Rust: `rust/crates/axiograph-pathdb/src/certificate.rs`
  - Lean: `lean/Axiograph/Certificate/Format.lean`, `lean/Axiograph/Certificate/Check.lean`
- Missing (critical): “query answers are certified by default” for the real query endpoints.

### 0.3 Anchoring (partial)

- Implemented: `axi_digest_v1` (FNV-1a 64-bit) as a snapshot-scoping anchor for `.axi` text (stability identity, not a security primitive).
  - Rust: `rust/crates/axiograph-dsl/src/digest.rs`
  - Lean: `lean/Axiograph/Util/Fnv1a.lean`
- Implemented historically: anchored reachability used `relation_id` fact IDs
  against `PathDBExportV1` `.axi` snapshots; that public raw path-cert surface
  is retired.
- Current direction: typed query witnesses and canonical `.axi` anchors are the
  user/server/agent certificate path.
- Implemented scaffolding: `CanonicalFactLogV1`,
  `PathDB::stable_live_snapshot_digest_v1`, and
  `TypedFactBuilder::commit_certified_only` provide deterministic fact-log,
  live-byte digest, and stable `axi_fact_id` hardening without changing the live
  `.axpd` reader.
- Guarded: generic semantic/query/certificate/viz loading and accepted-plane
  promotion reject `PathDBExportV1` snapshots; examples foreground canonical
  `.axi`, typed reports, certificates, and semantic previews.
  - Tests: `axiograph-cli --test examples_e2e`
    `canonical_only_cert_commands_reject_pathdb_export_snapshots`,
    `accept_promote_rejects_pathdb_export_snapshot_without_mutating_store`,
    `querycert_rejects_pathdb_export_snapshot_smoke`,
    `repl_scripts_canonical_smoke`, and
    `repl_rejects_stale_export_axi_command`.
- Partial: imported canonical `.axi` tuple facts already carry `axi_fact_id`;
  typed runtime construction can now fail closed through `commit_certified_only`.
  Remaining work is to thread those ids through every public certificate,
  accepted-plane manifest, and compiled-IR object/projection id.
- Missing: a distributed accepted snapshot id contract that binds canonical
  module digest, fact log digest, live `.axpd` checkpoint digest, and manifest.

### 0.4 Grounding / “safe to use” (not enforced)

- Implemented: grounding uses PathDB content, guardrails, and schema hints.
  - Code: `rust/crates/axiograph-llm-sync/src/grounding.rs`
- Missing: default grounding must come only from **accepted/certified** knowledge, with explicit labeling for any proposal/approximate context.

---

## 1) Top priority (do these first)

### 1.1 Enforce knowledge planes (proposal vs accepted)

- [ ] Establish a standard on-disk layout:
  - `knowledge/proposals/` (untrusted; LLM extractions; promotion candidates),
  - `knowledge/accepted/` (reviewed; canonical `.axi`),
  - `knowledge/snapshots/` (`.axpd` checkpoints and explicit PathDBExport debug/parity snapshots),
  - `knowledge/certificates/` (JSON cert fixtures or emitted proofs).
- [ ] Update `axiograph-storage` to load schema/indexes from **accepted** modules only.
  - Code: `rust/crates/axiograph-storage/src/lib.rs` (`load_axi_files`, `append_to_axi`).
- [ ] Add a “promotion gate” command that moves reviewed candidate `.axi` into `accepted/` and records provenance.
  - CLI: add `axiograph db accept ...` (or `axiograph promote accept ...`) in `rust/crates/axiograph-cli/src/main.rs`.

### 1.2 Make review policies real (not warnings)

- [ ] Change `UnifiedStorage::apply_change` so review-required changes do **not** apply automatically.
  - Constraints: hold as pending until explicit approval.
  - Low-confidence: hold as pending until approval (or until corroborated).
  - Schema changes: hold as pending until approval.
- [ ] Add `approve_change(change_id)` / `reject_change(change_id)` APIs and CLI commands.

### 1.3 Fix entity/relation identity (stop using placeholder IDs)

- [x] Remove placeholder relation endpoints in storage writes (`source_id = 0`, `target_id = 1`).
  - Implemented: relation materialization is fail-closed when endpoints cannot
    be resolved; storage no longer invents `0 -> 1` placeholders.
- [ ] Add a stable name→entity_id index (or content-addressed entity ids) for PathDB writes.
- [ ] Add tests that assert relation endpoints are correct after persistence reload.

### 1.4 Put “certified” on the API boundary

- [ ] Add a “certified-only” mode for any grounding/query endpoint:
  - if certificate verification fails, do not use the result for grounding (fail-closed or label explicitly).
- [ ] Add a small “verifier service” boundary option:
  - engine returns `(answer, certificate)`,
  - verifier checks and returns `(answer, verified=true/false, explanation)`.

---

## 2) Next (high leverage once planes are enforced)

### 2.1 Anchor certificates to real inputs everywhere

- [~] Gate: deterministic module digest anchors.
  - Pass when fixed canonical `.axi` fixtures produce the same `axi_digest_v1`
    in Rust and Lean, and certificate writers surface that digest in their
    anchor fields.
  - Current checks: `make verify-semantics`, `make verify-axi-digest-e2e`,
    `axiograph-cli --test examples_e2e querycert_canonical_axi_v3_smoke`.
- [~] Gate: stable canonical fact ids.
  - Pass when canonical domain `.axi` emits stable ids for facts, relation
    objects, projection arrows, theory obligations, and snapshots from module
    digest plus local/content identity.
  - Current scaffolding: imported tuple facts carry `axi_fact_id`, typed builders
    can preview/commit certified-only facts with stable ids, and
    `CanonicalFactLogV1` rejects mismatched fact ids.
  - Required regression: parse/import/export-module/promote the same canonical
    module twice and assert identical ids, manifest entries, and certificate
    references. Numeric PathDB row positions must not appear in public
    certificates.
- [ ] Gate: production certificates require anchors.
  - Pass when every endpoint/command that claims "certified" fails closed if the
    certificate lacks a canonical anchor or if verification cannot bind the
    anchor to the requested accepted snapshot.

### 2.2 Determinism and reproducibility

- [ ] Gate: no floats across trusted boundaries.
  - Pass when certificate JSON, anchors, canonical snapshots, accepted-plane
    manifests, and trust contracts reject floating-point fields or encode them
    through fixed-point/domain-specific types.
- [ ] Gate: deterministic certificate JSON golden bytes.
  - Pass when fixed typecheck, constraints, and query-certificate fixtures emit
    byte-identical JSON across repeated runs. Tests must compare exact bytes,
    not only parsed JSON.
- [ ] Gate: same inputs imply same outputs.
  - Pass when CLI and server certificate emitters, semantic previews, accepted
    snapshot manifests, and PathDB export-module output run twice from the same
    accepted snapshot and produce identical bytes except for explicitly scoped
    run metadata.

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

- [~] Gate: property tests for existing untrusted surfaces.
  - Current checks include `axi_export_property_tests`,
    `axi_constraints_ok_property_tests`, `typed_builder_property_tests`,
    `fact_index_property_tests`, `path_expr_property_tests`,
    `follow_path_property_tests`, `reachability_property_tests`, and
    `fixed_prob_property_tests`.
  - Remaining pass condition: add fuzz targets for Rust `.axi` parsing,
    certificate JSON parsing, PathDB bytes parsing, and CLI/REPL command parsing.
- [ ] Gate: optional deep-verification lanes are executable.
  - Pass when `make verify-fuzz`, `make verify-miri`, `make verify-kani`, and
    `make verify-loom` or `make verify-shuttle` exist. Each target must either
    run a named minimal suite or skip with an explicit "tool unavailable"
    message; silent no-ops do not count.
- [ ] Gate: concurrency tests only cover real concurrency.
  - Pass when Loom/Shuttle models are added only for code that actually shares
    mutable state across threads; otherwise the roadmap entry stays intentionally
    unimplemented.

---

## 4) Distributed system readiness (when single-node is clean)

This section should be pursued only after “planes + anchors + certificates” are solid.

- [~] Gate: canonical fact log plus snapshots.
  - Pass when accepted-plane promotion appends an immutable JSONL event, writes a
    snapshot manifest, updates `HEAD` only after durable writes, and can rebuild
    the accepted state from log plus canonical module bytes. Rejected
    `PathDBExportV1` snapshots must not mutate `HEAD`, logs, or manifests.
  - Current scaffolding: `CanonicalFactLogV1::certified_from_db` extracts a
    deterministic fact log from canonical fact nodes after Rust-side checks, and
    `PathDB::stable_live_snapshot_digest_v1` provides a deterministic digest for
    live PathDB facts excluding rebuildable indexes.
- [ ] Gate: live `.axpd` and verified `.axpd` convergence.
  - Pass when an actual production `.axpd` checkpoint created from accepted
    canonical state can be parsed, served, snapshotted, reloaded, and compared
    against the accepted module digest and snapshot id. A `PathDBExportV1`
    `.axi` roundtrip is only a debug/parity subcheck.
  - Current status: production live reads still use the v1 `PathDB::to_bytes`
    envelope. The sectioned v2 `BinaryHeader` in `verified.rs` is verified
    scaffolding and is not the runtime format; `axpd_convergence_status_v1()`
    intentionally reports this as not converged.
- [ ] Gate: indexes are derived rebuildable state.
  - Pass when a replica/shard can delete and rebuild indexes from canonical
    facts plus PathDB/WAL bytes without changing accepted ids or query answers.
- [ ] Gate: optional snapshot commitments.
  - Pass only if offline/third-party verification is required; then add a Merkle
    root or transparency-log commitment test that verifies a committed snapshot
    from bytes alone.

See `docs/explanation/DISTRIBUTED_PATHDB.md`.
