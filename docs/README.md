# Documentation (Diataxis)

This repository has a lot of documentation because Axiograph spans:

- a **canonical language** (`.axi`),
- a **runtime system** (Rust + PathDB),
- a **trusted semantics/checker** (Lean + mathlib),
- and **tooling** (REPL, discovery loops, ingestion adapters, visualization).

Repo-wide terminology note:

- "LLM-assisted" means typed plugin/API/tool-loop/MCP-skill-style integration
  surfaces that emit `query_ir_v1`, typed exploration payloads, or
  evidence-plane proposal artifacts. It does not mean free-form semantic
  authority over accepted ontology state.

To keep this navigable, we organize docs using the **Diataxis** framework:
**Tutorials** (learn), **How-to** (do), **Reference** (look up), **Explanation** (understand).

If you only read one thing, read the “book”:
- `docs/explanation/BOOK.md`

---

## Start here

1. `README.md` (build + quick start)
2. `docs/explanation/SYSTEM_OVERVIEW.md` (conceptual map of the system)
3. `docs/explanation/BOOK.md` (end-to-end: semantics → certificates → production readiness → use cases)
4. `examples/README.md` (pedagogical example catalog and runnable teaching path)

---

## Tutorials (learn by doing)

- `docs/tutorials/REPL.md` — interactive querying, viz, scripts
- `docs/tutorials/VIZ_EXPLORER.md` — browser-based exploration + LLM-assisted querying (server mode)
- `docs/tutorials/CERTIFIED_QUERYING_101.md` — minimal Rust→cert→Lean verification walkthrough
- `docs/tutorials/FIBERED_CLOSURE_CONSTRAINTS.md` — certifiable symmetry/transitivity (`param (...)`) in open world
- `docs/tutorials/TYPE_THEORY_DEMOS.md` — paths, homotopies, dependent structures, certificates (examples-first)
- `docs/tutorials/SCHEMA_DISCOVERY.md` — automated ontology engineering loop (structured + LLM-assisted)
- `docs/tutorials/CONTINUOUS_INGEST_AND_DISCOVERY.md` — continuous ingest/discovery prototype loop
- `docs/tutorials/WORLD_MODEL_LOOP.md` — JEPA/world-model loop with guardrails + promotion
- `docs/tutorials/INDUSTRIAL_ENGINEERING_EXAMPLE.md` — industrial/business/process example harness outside the core CLI

---

## How-to guides (task oriented)

- `docs/howto/TESTING.md` — running the test suites and demos
- `docs/howto/FORMAL_VERIFICATION.md` — running Lean checks, Rust↔Lean parity, semantics e2e
- `docs/howto/DB_SERVER.md` — serving snapshots over HTTP (query + viz; master/replica)
- `docs/howto/SNAPSHOT_STORE.md` — accepted-plane + PathDB WAL workflow (promotion, commits, sync)
- `docs/howto/KNOWLEDGE_INGESTION.md` — ingest pipelines (repo/docs/sql/json/proto/web) → proposals → candidates → accept
- `docs/howto/INGEST_PROTO.md` — proto/gRPC ingestion (Buf) details + examples
- `docs/howto/LLM_QUERY_INTEGRATION.md` — NL-ish query integration + REPL workflows
- `docs/howto/PERFORMANCE_PROFILING.md` — phase timings + flamegraphs (PathDB/WAL hot paths)

---

## Reference (formats, protocols, languages)

- `docs/reference/CERTIFICATES.md` — certificate schema, versions, and how the Lean checker validates them
- `docs/reference/TRUSTED_KERNEL.md` — exact boundary of the Lean trusted kernel and certificate trust classes
- `docs/reference/KERNEL_IR.md` — canonical schema/category IR target spec
- `docs/reference/RUNTIME_THEORY_CHECKER.md` — Rust runtime theory judgments, closure tiers, completeness claims, and non-claims
- `docs/reference/AGENT_CONTEXT.md` — current agent-facing architecture, trust, type-system, and compatibility context
- `docs/reference/AXI_STYLE.md` — canonical `.axi` authoring/style guide for examples and reviews
- `docs/reference/RUST_LIFECYCLE_TYPES.md` — Rust lifecycle state + anchor-aware artifact model
- `docs/reference/QUERY_LANG.md` — AxQL + SQL-ish dialect reference (and “certified querying” roadmap)
- `docs/reference/SEMANTIC_VCS.md` — semantic refs/commits/reconciliation/world-model lineage spec
- `docs/reference/LLM_REPL_PLUGIN.md` — plugin protocol (`axiograph_llm_plugin_v2`)
- `docs/reference/WORLD_MODEL_PLUGIN.md` — world model plugin protocol (`axiograph_world_model_v1`)

## Research (design sources)

- `docs/research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md` — applied category theory, dependent contexts, HoTT/groupoid paths, institutions, RDF/SHACL, and DDD/fDDD grounding for runtime theory checking

---

## Explanation (design + semantics)

- `docs/explanation/ARCHITECTURE.md` — system architecture and trust boundary
- `docs/explanation/CONSTRAINT_SEMANTICS.md` — open-world constraints and what can be certified
- `docs/explanation/PATHDB_DESIGN.md` — PathDB storage/index design
- `docs/explanation/DISTRIBUTED_PATHDB.md` — replication/sharding + snapshot-scoped certificates + reading list
- `docs/explanation/UNIFIED_STORAGE.md` — storage layers and artifact formats (`.axi`, `.axpd`, WAL, exports)
- `docs/explanation/VERIFICATION_AND_GUARDRAILS.md` — how guardrails/verification fit together (and how they can fail)
- `docs/explanation/RECONCILIATION.md` — reconciliation workflow + semantics direction
- `docs/explanation/PATH_VERIFICATION.md` — path verification and witness design
- `docs/explanation/MATHEMATICAL_FOUNDATIONS.md` — core math notes (category theory, HoTT/groupoids, etc.)
- `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md` — practical type theory + denotational semantics + ologs + AI-assisted ontology discovery/axiomitization
- `docs/explanation/DDD_CONTEXT_WRAPPERS.md` — how bounded contexts / fDDD / wrapper vocabulary fit over the existing ontology/query/report seams
- `docs/explanation/HOTT_FOR_KNOWLEDGE_GRAPHS.md` — HoTT framing (paths/groupoids for KGs)
- `docs/explanation/TYPE_THEORY_DESIGN.md` — the type-theory surface for `.axi` and PathDB
- `docs/explanation/QUERY_SPEC_CONVERGENCE.md` — how query IR, AxQL, CQ gating, refinement, and spec/scenario work may converge without a greenfield language rewrite
- `docs/explanation/TOPOS_THEORY.md` — topos/sheaf semantics roadmap for contexts/modalities (explanation-level)
- `docs/explanation/RUST_DEPENDENT_TYPES.md` — “dependent” encodings + branding/typestate patterns in Rust
- `docs/explanation/SEMANTIC_WEB_INTEROP.md` — RDF/OWL/SHACL/PROV boundary design
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md` — learning/epistemics direction (extension semantics)
- `docs/explanation/JEPA_INTEGRATION.md` — JEPA-style architectures and how they map to Axiograph
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md` — objective-driven AI (world model + costs + MPC) mapping
- `docs/explanation/SELF_SUPERVISED_LEARNING.md` — self-supervised learning loops on anchored snapshots
- `docs/explanation/LLM_KG_SYNC.md` — LLM ↔ KG sync direction (untrusted boundary; cert-checked core)

---

## Roadmaps (tracked plans)

- `docs/roadmaps/ROADMAP_AGENT_BACKLOG.md`
- `docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md`
- `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`
- `docs/roadmaps/ROADMAP_MATHEMATICAL.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_MERGE_LATTICE.md`
