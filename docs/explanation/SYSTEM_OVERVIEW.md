# Axiograph System Overview

**Diataxis:** Explanation  
**Audience:** users (and contributors)

For the end-to-end mathematical documentation (semantics → certificates → production readiness → use cases), see `docs/explanation/BOOK.md`.

For the current V1 user workflow, start with
`docs/howto/CANONICAL_SEMANTIC_SPINE.md`. The short rule is: accepted
canonical `.axi` plus compiled semantic IR is the meaning plane; PathDB,
backends, embeddings, and exports are derived execution/evidence/projection
surfaces.

## End-to-End Data Flow

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                             AXIOGRAPH v6 (today)                              │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  Knowledge sources                                                            │
│   - code/docs/conversations                                                   │
│   - SQL schemas / JSON schemas                                                │
│                                                                               │
│     ┌───────────────────────────────────────────────────────────────┐         │
│     │ Rust ingestion (untrusted)                                    │         │
│     │  - chunking + extraction                                      │         │
│     │  - emits evidence-plane artifacts                             │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │                                                          │
│                    ▼                                                          │
│     ┌───────────────────────────────────────────────────────────────┐         │
│     │ Evidence plane (approximate, auditable)                        │         │
│     │  - chunks.json                                                 │         │
│     │  - facts.json (optional)                                       │         │
│     │  - proposals.json (generic Evidence/Proposals schema)          │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │                                                          │
│                    ▼                                                          │
│     ┌───────────────────────────────────────────────────────────────┐         │
│     │ Promotion (explicit, reviewable)                               │         │
│     │  proposals.json → candidate domain .axi modules                │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │ (manual/policy acceptance)                                │
│                    ▼                                                          │
│     ┌───────────────────────────────────────────────────────────────┐         │
│     │ Accepted knowledge (canonical)                                 │         │
│     │  - .axi modules + semantic VCS refs                             │         │
│     │  - typed previews/reports for reviewed changes                  │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │                                                          │
│                    ▼                                                          │
│     ┌───────────────────────────────────────────┐    ┌─────────────────────┐ │
│     │ PathDB (.axpd) derived execution snapshot  │    │ Typed witnesses     │ │
│     │  - PreparedQueryV1/query_result_v3 surface │───►│ Rust emits, Lean    │ │
│     └───────────────────────────────────────────┘    │ verifies            │ │
│                                                      └─────────────────────┘ │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

## Repository Components

### Rust (runtime / engine)

Workspace: `rust/`

- `axiograph-cli`: CLI for validation, ingestion, typed reports, promotion, and PathDB snapshots
- `axiograph-dsl`: canonical `.axi` parsing (`axi_v1` entrypoint + dialects)
- `axiograph-ingest-docs`: docs/conversations → `proposals.json` (+chunks/facts)
- `axiograph-ingest-sql`: SQL DDL → `proposals.json`
- `axiograph-ingest-json`: JSON schema → `proposals.json`
- `axiograph-pathdb`: binary indexed store (`.axpd`) + certificate emission types
- `axiograph-storage`: helpers for `.axi` + `.axpd` workflows
- `axiograph-llm-sync`: untrusted extraction/sync scaffolding

### Lean (trusted checker / semantics)

Project: `lean/`

- Certificate parsing + checking: `lean/Axiograph/Certificate/*`
- Canonical `.axi` parsers: `lean/Axiograph/Axi/*`
- HoTT/groupoid vocabulary and proofs: `lean/Axiograph/HoTT/*`
- Verified fixed-point probabilities: `lean/Axiograph/Prob/Verified.lean`

## Common Workflows

### 1) Ingest sources → proposals (evidence plane)

```bash
axiograph ingest doc manual.txt --out build/manual_proposals.json --chunks build/manual_chunks.json
axiograph ingest sql schema.sql --out build/sql_proposals.json
```

### 2) Promote proposals → candidate domain `.axi` (explicit)

```bash
axiograph discover promote-proposals build/manual_proposals.json --out-dir build/candidates
```

### 3) Validate canonical `.axi`

```bash
axiograph check validate examples/learning/MachinistLearning.axi
```

### 4) Promote reviewed `.axi` through the accepted plane

Promotion is the mutation boundary for accepted knowledge. It parses the
candidate as canonical `.axi`, writes typed preview/review artifacts such as
`EvolutionPreviewV1`, and advances semantic VCS history.

```bash
axiograph db accept promote build/candidates/MachinistLearning.proposals.axi \
  --dir build/accepted_plane \
  --message "reviewed: initial machinist learning module"
```

### 5) Build a derived PathDB execution snapshot

PathDB is the indexed execution/query substrate. Build it from canonical `.axi`
or accepted-plane snapshots; do not make it the ontology authority.

```bash
axiograph db pathdb materialize-axi examples/machining/PhysicsKnowledge.axi --out build/physics.axpd
```

Machine/report query flows should compile to `query_ir_v1`, prepare a
`PreparedQueryV1`, and return typed metadata/trust reports. Certified query
results use canonical `.axi`-anchored `query_result_v3` witnesses.

### 6) Storage round-trip checks

For low-level storage checks, PathDB can round-trip through explicit DB debug
commands:

```bash
axiograph db pathdb export-axi knowledge.axpd --out build/snapshot_pathdb_export_v1.axi
axiograph db pathdb import-axi build/snapshot_pathdb_export_v1.axi --out build/knowledge.axpd
```

Do not use derived snapshots as semantic, query, tutorial, or certificate
authority. Public semantic flows should load canonical `.axi` modules directly.

### 7) Rust → Lean certificate verification (e2e)

```bash
make verify-semantics
```

## Data Formats

| Format | Meaning |
|--------|---------|
| `.axi` | Canonical accepted knowledge (schema + content) |
| `.axpd` | Binary PathDB (derived, indexed, rebuildable) |
| `proposals.json` | Generic Evidence/Proposals output (untrusted) |
| `chunks.json` | RAG-friendly chunk store |
| `facts.json` | Optional raw extractor output |
| `EmbeddingEvidenceOverlayV1` / tooling overlays | Evidence/review attachments, not accepted truth |
| `EvolutionPreviewV1` and typed reports | Review, trust, coverage, and refinement-handle surfaces |
| `PreparedQueryV1` metadata | Typed prepared-query/report envelope |
| `query_result_v3` | Canonical `.axi`-anchored query witness rows |
| `certificate.json` | Rust→Lean proof payloads (versioned) |

## Key Invariants

1. **`.axi` is canonical**: accepted knowledge is reviewable and diffable.
2. **Evidence is explicit**: ingestion outputs are proposals with provenance, not truth.
3. **Reports are typed**: previews, query metadata, trust contracts, and refinement handles are machine-readable.
4. **PathDB is derived**: `.axpd` indexes are rebuildable from accepted `.axi`/semantic refs.
5. **Certified query rows use `query_result_v3`**: Lean is the trusted checker for supported witnesses.
6. **Overlays stay reviewable**: evidence, embeddings, LLM/world-model output, and tooling overlays need typed validation before promotion.
