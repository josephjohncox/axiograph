# Axiograph System Overview

**Diataxis:** Explanation  
**Audience:** users (and contributors)

For the end-to-end mathematical documentation (semantics → certificates → production readiness → use cases), see `docs/explanation/BOOK.md`.

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
│     │  - .axi modules (Git/audit trail)                               │         │
│     └──────────────┬────────────────────────────────────────────────┘         │
│                    │                                                          │
│                    ▼                                                          │
│     ┌───────────────────────────────────────────┐    ┌─────────────────────┐ │
│     │ PathDB (.axpd) derived from snapshots      │    │ Certificates (JSON) │ │
│     │  - fast indexed query/optimization engine  │───►│ Rust emits, Lean    │ │
│     └───────────────────────────────────────────┘    │ verifies            │ │
│                                                      └─────────────────────┘ │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

## Repository Components

### Rust (runtime / engine)

Workspace: `rust/`

- `axiograph-cli`: CLI for validation, ingestion, promotion, and PathDB snapshots
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

### 4) PathDB snapshots (`.axpd` ↔ `.axi`)

For auditability and diffability, PathDB can round-trip through a reversible `.axi` export format (`PathDBExportV1`):

```bash
axiograph db pathdb export-axi knowledge.axpd --out build/snapshot.axi
axiograph db pathdb import-axi build/snapshot.axi --out build/knowledge.axpd
```

`pathdb import-axi` also accepts canonical `.axi` modules (schema/theory/instance) and imports them into a fresh PathDB:

```bash
axiograph db pathdb import-axi examples/machining/PhysicsKnowledge.axi --out build/physics.axpd
```

### 5) Rust → Lean certificate verification (e2e)

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
| `certificate.json` | Rust→Lean proof payloads (versioned) |

## Key Invariants

1. **`.axi` is canonical**: accepted knowledge is reviewable and diffable.
2. **Evidence is explicit**: ingestion outputs are proposals with provenance, not truth.
3. **PathDB is derived**: `.axpd` indexes are rebuildable from snapshots.
4. **Certified results carry certificates**: Lean is the trusted checker.
5. **Determinism in checking**: fixed-point probabilities in certificates/checker.
