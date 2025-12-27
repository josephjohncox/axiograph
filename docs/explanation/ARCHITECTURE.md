# Axiograph v6 Architecture

**Diataxis:** Explanation  
**Audience:** contributors

## Design Principles

1. **Lean is the trusted source of semantics** (mathlib-backed checker and spec).
2. **Rust is the high-performance untrusted engine** — ingestion, indexing, search, reconciliation, PathDB — and must emit checkable certificates.
3. **All critical meaning is certificate-checked** — the engine may be “clever”, but results are only trusted if Lean verifies them.
4. **Determinism in the checker** — avoid floating point in trusted verification; prefer fixed-point probabilities (`VProb`).

## System Overview

```
                    Knowledge Sources
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
    ┌────────┐        ┌─────────┐        ┌────────┐
    │  Docs  │        │  SQL    │        │  JSON  │
    │ Convo  │        │ DDL     │        │ Schema │
    └───┬────┘        └────┬────┘        └───┬────┘
        │                  │                  │
        └──────────────┬───┴───────┬──────────┘
                       │           │
                       ▼           ▼
          ┌──────────────────────────────────┐
          │ Rust ingestion (untrusted)       │
          │  - chunking / extraction         │
          │  - emits proposals + evidence    │
          └──────────────┬───────────────────┘
                         │
                         ▼
            ┌──────────────────────────────┐
            │ Evidence plane (approximate) │
            │  - chunks.json               │
            │  - facts.json (optional)     │
            │  - proposals.json (generic)  │
            └──────────────┬───────────────┘
                           │
                           ▼
            ┌──────────────────────────────┐
            │ Promotion (explicit)         │
            │  proposals.json → candidates │
            │  (.axi modules for review)   │
            └──────────────┬───────────────┘
                           │
                           ▼
            ┌──────────────────────────────┐
            │ Accepted knowledge (canonical)│
            │  - .axi modules (Git/audit)   │
            └──────────────┬───────────────┘
                           │
                           ▼
      ┌──────────────────────────────┬──────────────────────────────┐
      │                              │                              │
      ▼                              ▼                              ▼
┌──────────────┐            ┌────────────────┐            ┌────────────────┐
│ PathDB (.axpd)│            │ Rust runtime   │            │ Lean checker    │
│ derived index │◄──────────►│ query/opt/mig  │──────────►│ verifies certs  │
└──────────────┘   certs     └────────────────┘            └────────────────┘
```

## Knowledge Ingestion Flow

```
Knowledge Sources → Rust Ingestion → proposals.json (+chunks/facts) → Promote → candidate .axi → accept → PathDB + certificates
```

See [KNOWLEDGE_INGESTION.md](./KNOWLEDGE_INGESTION.md) for details on:
- Conversation parsing (Slack, meeting transcripts)
- Confluence wiki ingestion
- Probabilistic fact extraction
- Physics knowledge engine
- RAG integration

## Module Breakdown

### Lean checker (`lean/`)

Lean is the trusted semantics and certificate checker.

Key modules:

- Certificate format + checking: `lean/Axiograph/Certificate/Format.lean`, `lean/Axiograph/Certificate/Check.lean`
- `.axi` parsers (canonical corpus): `lean/Axiograph/Axi/*`
- HoTT/groupoid rewrite vocabulary: `lean/Axiograph/HoTT/*`
 
### Historical Idris prototype (removed)

This repo previously used Idris2 as a prototype proof layer. The initial
Rust+Lean release removes Idris/FFI compatibility; see git history for the old
Idris sources if needed.

### Rust Crates (`rust/crates/`)

| Crate | Purpose |
|-------|---------|
| `axiograph-cli` | CLI for validation, ingestion, promotion, and PathDB snapshots |
| `axiograph-dsl` | Canonical `.axi` parsing (`axi_v1` entrypoint) |
| `axiograph-ingest-docs` | Docs/conversation ingestion → `proposals.json` (+chunks/facts) |
| `axiograph-ingest-sql` | SQL DDL ingestion → `proposals.json` |
| `axiograph-ingest-json` | JSON schema ingestion → `proposals.json` |
| `axiograph-storage` | Unified storage for `.axi` + PathDB snapshots |
| `axiograph-pathdb` | Binary indexed KG store (`.axpd`) + certificate emission |
| `axiograph-llm-sync` | LLM-assisted extraction/sync scaffolding (untrusted) |

## Data Flow

### Canonical source: `.axi`

`.axi` is the canonical, human-facing input format. Both Rust and Lean parse the canonical corpus
through the unified `axi_v1` entrypoint (schema/theory/instance).

### Ingestion: sources → evidence artifacts

Ingestion is *untrusted* and produces an evidence plane:

- `chunks.json` (RAG-friendly)
- `facts.json` (optional; raw extractor output)
- `proposals.json` (generic Evidence/Proposals schema)

Promotion into canonical `.axi` is explicit and reviewable:

```bash
axiograph discover promote-proposals proposals.json --out-dir build/candidates
```

### Storage: `.axpd` + reversible `.axi` snapshots

For runtime, we use PathDB `.axpd` (fast, indexed). For auditability and diffability we also support a reversible
snapshot export format rendered as `.axi` (`PathDBExportV1`):

```bash
axiograph db pathdb export-axi knowledge.axpd --out snapshot.axi
axiograph db pathdb import-axi snapshot.axi --out knowledge.axpd
```

### Certificates: Rust → Lean

Runtime operations can emit versioned JSON certificates (reachability, normalization, reconciliation, migrations).
Lean verifies certificates deterministically (fixed-point probabilities).

See:
- `docs/reference/CERTIFICATES.md`
- `docs/howto/FORMAL_VERIFICATION.md`

## FFI Policy

FFI is allowed **only** for:

✅ **Allowed**:
- PDF text extraction
- Image OCR
- Network I/O for data ingestion
- Database connections for warehouse bindings

❌ **Not Allowed**:
- Semantics (paths/groupoid laws, normalization, reconciliation policy)
- Certificate verification (must be done in Lean)
- Any “hidden” inference that cannot be expressed as a certificate + checked

All **logical meaning** must be representable as:

- canonical `.axi` inputs, and
- explicit certificates that Lean can validate.

## Extending the System

### Adding a New Ingestion Source

1. Create `rust/crates/axiograph-ingest-foo/`
2. Parse/extract into the **generic evidence plane**:
   - `chunks.json` (optional)
   - `proposals.json` (required)
4. Add CLI command in `axiograph-cli`
5. If needed, add a **promotion mapping** from proposals → candidate `.axi` for explicit review
6. No hidden inference/semantics in Rust (certificates only for certified steps)

### Adding a New Checked Semantics / Certificate Kind

1. Add the operation to the Rust engine (untrusted) and define a certificate payload
2. Add the certificate kind to:
   - Rust: `rust/crates/axiograph-pathdb/src/certificate.rs`
   - Lean: `lean/Axiograph/Certificate/Format.lean` + `lean/Axiograph/Certificate/Check.lean`
3. Add fixtures + e2e verification:
   - `examples/certificates/*.json`
   - `make verify-lean-certificates` / `make verify-lean-e2e-suite`
