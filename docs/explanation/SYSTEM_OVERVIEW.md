# Axiograph System Overview

**Diataxis:** Explanation  
**Audience:** users (and contributors)

For the end-to-end mathematical documentation (semantics → certificates → production readiness → use cases), see `docs/explanation/BOOK.md`.

Axiograph is a typed ontology workbench. For the current V1 user workflow,
start with `docs/howto/CANONICAL_SEMANTIC_SPINE.md`. The short rule is:
accepted canonical `.axi` plus compiled semantic IR is the meaning plane;
PathDB, backends, embeddings, LLM outputs, and exports are derived
execution/evidence/projection surfaces.

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
│     ┌───────────────────────────────────────────────────────────────┐         │
│     │ Canonical compiled snapshot                                    │         │
│     │  - CompiledKernelSnapshot                                      │         │
│     │  - KernelSnapshotIr + SchemaPresentationIr + InstanceModelIr   │         │
│     │  - derived RuntimeSemanticIndex citations                      │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │                                                          │
│                    ├────────► typed runtime reports                           │
│                    │          - theory/query/CQ/coverage/merge/backend        │
│                    │                                                          │
│                    ├────────► PathDB (.axpd) derived execution snapshot       │
│                    │                                                          │
│                    └────────► optional Lean certificates for supported claims  │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

## Repository Components

### Rust (runtime / engine)

Workspace: `rust/`

- `axiograph-cli`: CLI/server/MCP/LSP orchestration for validation, ingestion, typed reports, promotion, and projections
- `axiograph-dsl`: canonical `.axi` parsing (`axi_v1` entrypoint + dialects)
- `axiograph-ingest-docs`: docs/conversations → `proposals.json` (+chunks/facts)
- `axiograph-ingest-sql`: SQL DDL → `proposals.json`
- `axiograph-ingest-json`: JSON schema → `proposals.json`
- `axiograph-pathdb`: runtime graph engine, compiled IR, theory checks, query, and certificate emission types
- `axiograph-storage`: runtime evidence storage and PathDB cache materialization
- `axiograph-llm-sync`: evidence-plane extraction, grounding, and reconciliation inputs

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

### 4) Promote reviewed `.axi` through AxiStore

Promotion is the mutation boundary for accepted knowledge. Build a typed
`PromotionPlan` from the exact reviewed `.axi` bytes and complete gate closure,
then call `AxiStore::promote` with the current generation. There is no legacy
`db accept` CLI or filesystem `HEAD`; see `docs/howto/SNAPSHOT_STORE.md`.

### 5) Publish derived PathDB execution state

PathDB is the indexed execution/query substrate. Compile exact accepted `.axi`
into `KernelSnapshotIr`, construct an explicit `AxpdBuildSpec`, and publish it
with `AxiStore::publish_axpd`. AxiStore records an immutable SQLite
image and receipt named by `MaterializationIdV2`.

Machine/report query flows should compile `query_ir_v1` into the sole executable
family, `CompiledFiniteQuery`, and return typed metadata/trust reports. Certified query
results use envelope V3 / `query_result_v4` witnesses bound to canonical `.axi`
bytes, prepared queries, and returned answers.

### 6) Verify derived storage

Open durable query state only through
`AxiStore::open_axpd`/`load_verified_pathdb`. These paths
recompute schema, limits, anchors, logical digest, exact-image digest, and
materialization identity before hydration. There is no PathDB-to-`.axi`
round-trip or alternate binary reader.

### 7) Rust → Lean certificate verification (e2e)

```bash
make verify-semantics
```

## Data Formats

| Format | Meaning |
| -------- | --------- |
| `.axi` | Canonical accepted knowledge (schema + content) |
| `.axpd` | Authenticated SQLite PathDB materialization (derived, indexed, rebuildable) |
| `proposals.json` | Generic Evidence/Proposals output (untrusted) |
| `chunks.json` | `EvidenceChunkBundleV1` RAG/evidence overlay, not accepted truth |
| `facts.json` | Optional raw extractor output |
| `EmbeddingEvidenceOverlayV1` / tooling overlays | Evidence/review attachments, not accepted truth |
| `EvolutionPreviewV1` and typed reports | Review, trust, coverage, and refinement-handle surfaces |
| `CompiledFiniteQuery` metadata | Typed compiled-query/report envelope |
| `query_result_v4` | Canonical `.axi`, prepared-query, ordered-answer-bound witnesses plus exact finite denotation check |
| `certificate.json` | Rust→Lean proof payloads (versioned) |

## Key Invariants

1. **`.axi` is canonical**: accepted knowledge is reviewable and diffable.
2. **Evidence is explicit**: ingestion outputs are proposals with provenance, not truth.
3. **Reports are typed**: previews, query metadata, trust contracts, and refinement handles are machine-readable.
4. **PathDB is derived**: `.axpd` indexes are rebuildable from accepted `.axi`/semantic refs.
5. **Certified query rows use only `query_result_v4`**: Lean checks the module,
   prepared-query, ordered answer, every witness row, and exact equality with
   the declared bounded finite denotation.
6. **Overlays stay reviewable**: evidence, embeddings, LLM/proposal-adapter output, and tooling overlays need typed validation before promotion.
