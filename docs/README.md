# Documentation (Diataxis)

This repository has a lot of documentation because Axiograph spans:

- a **canonical language** (`.axi`),
- a **runtime system** (Rust + PathDB),
- a **trusted semantics/checker** (Lean + mathlib),
- and **tooling** (REPL, discovery loops, ingestion adapters, visualization).

To keep this navigable, we organize docs using the **Diataxis** framework:
**Tutorials** (learn), **How-to** (do), **Reference** (look up), **Explanation** (understand).

If you only read one thing, read the “book”:
- `docs/explanation/BOOK.md`

---

## Start here

1. `README.md` (build + quick start)
2. `docs/explanation/SYSTEM_OVERVIEW.md` (conceptual map of the system)
3. `docs/explanation/BOOK.md` (end-to-end: semantics → certificates → production readiness → use cases)

---

## Tutorials (learn by doing)

- `docs/tutorials/REPL.md` — interactive querying, viz, scripts
- `docs/tutorials/VIZ_EXPLORER.md` — browser-based exploration + LLM-assisted querying (server mode)
- `docs/tutorials/CERTIFIED_QUERYING_101.md` — minimal Rust→cert→Lean verification walkthrough
- `docs/tutorials/TYPE_THEORY_DEMOS.md` — paths, homotopies, dependent structures, certificates (examples-first)
- `docs/tutorials/SCHEMA_DISCOVERY.md` — automated ontology engineering loop (structured + LLM-assisted)
- `docs/tutorials/CONTINUOUS_INGEST_AND_DISCOVERY.md` — continuous ingest/discovery prototype loop

---

## How-to guides (task oriented)

- `docs/howto/TESTING.md` — running the test suites and demos
- `docs/howto/FORMAL_VERIFICATION.md` — running Lean checks, Rust↔Lean parity, semantics e2e
- `docs/howto/DB_SERVER.md` — serving snapshots over HTTP (query + viz; master/replica)
- `docs/howto/SNAPSHOT_STORE.md` — accepted-plane + PathDB WAL workflow (promotion, commits, sync)
- `docs/howto/KNOWLEDGE_INGESTION.md` — ingest pipelines (repo/docs/sql/json/proto/web) → proposals → candidates → accept
- `docs/howto/INGEST_PROTO.md` — proto/gRPC ingestion (Buf) details + examples
- `docs/howto/LLM_QUERY_INTEGRATION.md` — NL-ish query integration + REPL workflows

---

## Reference (formats, protocols, languages)

- `docs/reference/CERTIFICATES.md` — certificate schema, versions, and how the Lean checker validates them
- `docs/reference/QUERY_LANG.md` — AxQL + SQL-ish dialect reference (and “certified querying” roadmap)
- `docs/reference/LLM_REPL_PLUGIN.md` — plugin protocol (`axiograph_llm_plugin_v2`)

---

## Explanation (design + semantics)

- `docs/explanation/ARCHITECTURE.md` — system architecture and trust boundary
- `docs/explanation/PATHDB_DESIGN.md` — PathDB storage/index design
- `docs/explanation/DISTRIBUTED_PATHDB.md` — replication/sharding + snapshot-scoped certificates + reading list
- `docs/explanation/UNIFIED_STORAGE.md` — storage layers and artifact formats (`.axi`, `.axpd`, WAL, exports)
- `docs/explanation/VERIFICATION_AND_GUARDRAILS.md` — how guardrails/verification fit together (and how they can fail)
- `docs/explanation/RECONCILIATION.md` — reconciliation workflow + semantics direction
- `docs/explanation/PATH_VERIFICATION.md` — path verification and witness design
- `docs/explanation/MATHEMATICAL_FOUNDATIONS.md` — core math notes (category theory, HoTT/groupoids, etc.)
- `docs/explanation/HOTT_FOR_KNOWLEDGE_GRAPHS.md` — HoTT framing (paths/groupoids for KGs)
- `docs/explanation/TYPE_THEORY_DESIGN.md` — the type-theory surface for `.axi` and PathDB
- `docs/explanation/TOPOS_THEORY.md` — topos/sheaf semantics roadmap for contexts/modalities (explanation-level)
- `docs/explanation/RUST_DEPENDENT_TYPES.md` — “dependent” encodings + branding/typestate patterns in Rust
- `docs/explanation/SEMANTIC_WEB_INTEROP.md` — RDF/OWL/SHACL/PROV boundary design
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md` — learning/epistemics direction (extension semantics)
- `docs/explanation/LLM_KG_SYNC.md` — LLM ↔ KG sync direction (untrusted boundary; cert-checked core)

---

## Roadmaps (tracked plans)

- `docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md`
- `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`
- `docs/roadmaps/ROADMAP_MATHEMATICAL.md`
