# Production Readiness Roadmap (verifiable KG as a product)

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap turns the “book” (`docs/explanation/BOOK.md`) into concrete engineering work:

- make trust tiers explicit and enforced,
- make certificates ubiquitous and anchorable,
- make outputs safe to use for grounding and decision-making,
- and make PathDB evolvable into a distributed system without breaking semantics.

This is intentionally **certificate-first**:

> Rust (untrusted) computes; Lean (trusted) checks.

For ontology-process roadmapping (CQs, linting, patterns, reuse), see `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`.
For math/migration roadmapping (Δ/Σ/Π, rewrite theory), see `docs/roadmaps/ROADMAP_MATHEMATICAL.md`.

Last updated: 2025-12-20.

---

## 0) Current status snapshot (what’s true in code today)

### 0.1 Trust tiers (partial)

- Implemented: ingestion artifacts are explicitly “proposal-shaped” (`proposals.json`), and promotion emits reviewable candidate `.axi` modules.
  - Code: `rust/crates/axiograph-ingest-docs/src/proposals.rs`, `rust/crates/axiograph-ingest-docs/src/promotion.rs`, CLI: `rust/crates/axiograph-cli/src/main.rs`.
- Implemented (prototype): a deterministic “augmentation” pass for `proposals.json` (`discover augment-proposals`), producing an auditable trace and feeding the promotion stage.
  - Code: `rust/crates/axiograph-ingest-docs/src/augment.rs`, CLI: `rust/crates/axiograph-cli/src/main.rs`.
- Implemented (prototype): LLM sync keeps “pending review” facts and conflicts.
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
- Implemented: anchored reachability uses `relation_id` fact IDs against `PathDBExportV1` `.axi` snapshots.
- Missing: stable fact ids for **canonical domain `.axi`** (not just PathDB export snapshots), and a “snapshot id” notion for distributed settings.

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
  - `knowledge/snapshots/` (PathDB export `.axi` + `.axpd`),
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

- [ ] Remove placeholder relation endpoints in storage writes (`source_id = 0`, `target_id = 1`).
  - Code: `rust/crates/axiograph-storage/src/lib.rs` (`StorableFact::Relation` branch).
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

- [ ] Standardize how snapshot `.axi` text is generated (canonical formatting) so `axi_digest_v1` is stable.
- [ ] Require `anchor` in production certificates (not optional) for endpoints that claim “certified”.
- [ ] Extend anchoring beyond `relation_id`:
  - canonical domain `.axi` should emit stable fact ids (module digest + local id, or content addressing),
  - certificates refer to those ids (not fragile numeric positions).

### 2.2 Determinism and reproducibility

- [ ] Ban floats in anything that crosses the trusted boundary (certificates, anchors, canonical snapshots).
- [ ] Add deterministic JSON emission checks (golden bytes) for certificate writers.
- [ ] Add a “same inputs ⇒ same outputs” regression harness for cert emitters.

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

- [ ] Fuzz the untrusted surfaces:
  - `.axi` parsing (Rust),
  - certificate JSON parsing (Rust),
  - PathDB bytes parsing (Rust),
  - CLI/repl script surfaces (Rust).
- [ ] Add optional tool targets (no-op if tool missing):
  - `make verify-miri`, `make verify-kani`, `make verify-verus`.
- [ ] Add concurrency testing only when concurrency exists (Loom/Shuttle).

---

## 4) Distributed system readiness (when single-node is clean)

This section should be pursued only after “planes + anchors + certificates” are solid.

- [ ] Implement canonical fact log + snapshots (append-only, strongly consistent first).
- [ ] Treat indexes as derived rebuildable state per replica/shard.
- [ ] Add snapshot commitments (Merkle root / transparency log) if you need offline/third-party verification.

See `docs/explanation/DISTRIBUTED_PATHDB.md`.
