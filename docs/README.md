# Documentation

Axiograph docs are organized around the current canonical semantic spine:

```text
canonical .axi
  -> KernelModuleIr
  -> SchemaCategoryIr + TheoryIr + InstanceFunctorIr
  -> KernelSurfaceV1 refs
  -> typed runtime reports
  -> optional Lean verifier
```

Use these docs as the source of truth for current behavior. Pages should
describe the current architecture directly; superseded protocol notes belong in
git history, not in the active docs path.

## Start Here

1. `README.md` — project intent, trust boundary, and quick commands.
2. `docs/howto/CANONICAL_SEMANTIC_SPINE.md` — the current end-to-end workflow.
3. `examples/README.md` — runnable teaching catalog.
4. `docs/explanation/SYSTEM_OVERVIEW.md` — conceptual architecture.
5. `docs/howto/TESTING.md` — verification and CI-style gates.

## Current Navigation Spine

| Need | Start Here |
| --- | --- |
| Canonical semantic input and compiled IR | `docs/reference/KERNEL_IR.md` |
| Trusted verification boundary | `docs/reference/TRUSTED_KERNEL.md` |
| Certificate/report formats | `docs/reference/CERTIFICATES.md` |
| Runtime theory judgments and closure tiers | `docs/reference/RUNTIME_THEORY_CHECKER.md` |
| Software authoring, DDD/fDDD, codegen, coverage | `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` |
| Semantic VCS, reconciliation, merge/rebase | `docs/reference/SEMANTIC_VCS.md` |
| Embeddings, RAG, and evidence overlays | `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` |
| Backend projection and pushdown contracts | `docs/reference/KERNEL_IR.md`, `docs/reference/SEMANTIC_VCS.md` |
| Agent working context and backlog | `docs/reference/AGENT_CONTEXT.md`, `docs/roadmaps/ROADMAP_AGENT_BACKLOG.md` |

Repo-wide language rule:

- User labels are ergonomics.
- Compiled IR ids and `KernelSurfaceV1` refs are authority.
- LLM/MCP/tool-loop output is evidence or proposal material until accepted by
  typed review.
- JEPA/MPC/world-model/control language describes external adapters or research
  interpretations only; the core runtime exposes predictive proposal adapters
  and bounded proposal rollout surfaces.
- External graph databases and PathDB are projection/execution substrates, not
  ontology kernels.

## Tutorials

- `docs/tutorials/REPL.md` — interactive querying, viz, and scripts.
- `docs/tutorials/VIZ_EXPLORER.md` — browser exploration and tool-assisted querying.
- `docs/tutorials/CERTIFIED_QUERYING_101.md` — minimal Rust to certificate to Lean walkthrough.
- `docs/tutorials/FIBERED_CLOSURE_CONSTRAINTS.md` — certifiable open-world constraints.
- `docs/tutorials/TYPE_THEORY_DEMOS.md` — paths, homotopies, dependent structures, and certificates.
- `docs/tutorials/SCHEMA_DISCOVERY.md` — automated ontology discovery loop.
- `docs/tutorials/CONTINUOUS_INGEST_AND_DISCOVERY.md` — continuous ingest/discovery loop.
- `docs/tutorials/BOUNDED_PROPOSAL_ROLLOUT.md` — bounded proposal rollout with guardrails.
- `docs/tutorials/INDUSTRIAL_ENGINEERING_EXAMPLE.md` — industrial/business/process example harness.
- `examples/software_authoring/README.md` — pure-domain DDD/fDDD authoring flow.

## How-To Guides

- `docs/howto/TESTING.md` — running test suites and demos.
- `docs/howto/CANONICAL_SEMANTIC_SPINE.md` — one current workflow across validation, theory checks, typed queries, overlays, semantic VCS, embeddings, and backend projections.
- `docs/howto/FORMAL_VERIFICATION.md` — Lean checks and certificate gates.
- `docs/howto/DB_SERVER.md` — HTTP query/viz server.
- `docs/howto/SNAPSHOT_STORE.md` — accepted-plane and WAL workflow.
- `docs/howto/KNOWLEDGE_INGESTION.md` — ingest to proposals to review.
- `docs/howto/INGEST_PROTO.md` — proto/gRPC evidence adapter to proposals/chunks.
- `docs/howto/LLM_QUERY_INTEGRATION.md` — typed tool-loop query integration.
- `docs/howto/PERFORMANCE_PROFILING.md` — profiling runtime hot paths.

## Reference

- `docs/reference/CERTIFICATES.md` — current certificate families and Lean validation.
- `docs/reference/TRUSTED_KERNEL.md` — exact trusted boundary and non-claims.
- `docs/reference/KERNEL_IR.md` — compiled schema/category/theory IR and backend lowering.
- `docs/reference/RUNTIME_THEORY_CHECKER.md` — runtime theory judgments, closure tiers, completeness claims, and non-claims.
- `docs/reference/LEAN_THEORY_EVALUATION.md` — Lean-encoded theory status, feasibility, and gaps.
- `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` — embedding sidecars and promotion boundaries.
- `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` — codegen, overlays, CLI, MCP, server, and LSP surfaces.
- `docs/reference/RUST_ARCHITECTURE_CLEANUP.md` — Rust crate boundaries, simplification targets, and standard-library/crate guidance.
- `docs/reference/AGENT_CONTEXT.md` — current architecture and working policy for agents.
- `docs/reference/AXI_STYLE.md` — canonical `.axi` authoring style.
- `docs/reference/RUST_LIFECYCLE_TYPES.md` — Rust lifecycle/anchor artifact model.
- `docs/reference/QUERY_LANG.md` — AxQL, SQL-ish elaboration, and typed query metadata.
- `docs/reference/SEMANTIC_VCS.md` — semantic refs, commits, merge/rebase, reconciliation, and projection manifests.
- `docs/reference/LLM_REPL_PLUGIN.md` — LLM REPL plugin protocol.
- `docs/reference/PREDICTIVE_PROPOSAL_ADAPTER.md` — predictive proposal adapter protocol.

## Research

- `docs/research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md` — applied category theory, dependent contexts, HoTT/groupoid paths, institutions, RDF/SHACL, and DDD/fDDD grounding.
- `docs/research/MANUFACTURING_RECOMMENDED_READINGS.md` — domain references used by manufacturing/learning examples.

## Explanation

- `docs/explanation/SYSTEM_OVERVIEW.md` — conceptual system overview.
- `docs/explanation/ARCHITECTURE.md` — runtime architecture and trust boundary.
- `docs/explanation/CONSTRAINT_SEMANTICS.md` — open-world constraints and certifiable fragments.
- `docs/explanation/PATHDB_DESIGN.md` — PathDB storage/index design.
- `docs/explanation/DISTRIBUTED_PATHDB.md` — distributed storage and replication.
- `docs/explanation/VERIFICATION_AND_GUARDRAILS.md` — verification and failure modes.
- `docs/explanation/RECONCILIATION.md` — reconciliation workflow.
- `docs/explanation/PATH_VERIFICATION.md` — path verification and witness design.
- `docs/explanation/MATHEMATICAL_FOUNDATIONS.md` — category theory, HoTT/groupoids, and denotational framing.
- `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md` — practical type theory, ologs, and AI-assisted ontology discovery.
- `docs/explanation/DDD_CONTEXT_WRAPPERS.md` — bounded contexts and fDDD overlays.
- `docs/explanation/HOTT_FOR_KNOWLEDGE_GRAPHS.md` — HoTT framing for knowledge graphs.
- `docs/explanation/TYPE_THEORY_DESIGN.md` — type-theory surface for `.axi` and runtime checking.
- `docs/explanation/QUERY_SPEC_CONVERGENCE.md` — query IR, AxQL, CQ, and refinement convergence.
- `docs/explanation/TOPOS_THEORY.md` — topos/sheaf semantics roadmap.
- `docs/explanation/RUST_DEPENDENT_TYPES.md` — Rust branding/typestate/dependent-index patterns.
- `docs/explanation/SEMANTIC_WEB_INTEROP.md` — RDF/OWL/SHACL/PROV boundary design.
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md` — learning and epistemics direction.
- `docs/explanation/JEPA_INTEGRATION.md` — JEPA-style proposal adapters.
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md` — objective-driven AI mapping.
- `docs/explanation/SELF_SUPERVISED_LEARNING.md` — self-supervised loops.
- `docs/explanation/LLM_KG_SYNC.md` — LLM to KG sync at an untrusted boundary.

## Roadmaps

- `docs/roadmaps/ROADMAP_AGENT_BACKLOG.md`
- `docs/roadmaps/ROADMAP_TOOLING_OVERLAY_SEPARATION.md`
- `docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md`
- `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`
- `docs/roadmaps/ROADMAP_MATHEMATICAL.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_MERGE_LATTICE.md`
- `docs/roadmaps/ROADMAP_RUNTIME_THEORY_AND_TYPED_WORKFLOWS.md`
