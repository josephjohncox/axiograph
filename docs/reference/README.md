# Reference

Reference docs are for lookup and should stay stable.

## Start by task

| Task | Primary reference | Adjacent checks/docs |
| --- | --- | --- |
| Determine what is trusted | `docs/reference/TRUSTED_KERNEL.md` | `docs/reference/CERTIFICATES.md`, `docs/howto/FORMAL_VERIFICATION.md` |
| Work on the canonical semantic spine | `docs/howto/CANONICAL_SEMANTIC_SPINE.md`, `docs/reference/KERNEL_IR.md` | `docs/howto/TESTING.md` (`make verify-canonical-spine`) |
| Emit or verify certificates | `docs/reference/CERTIFICATES.md` | `docs/howto/FORMAL_VERIFICATION.md` |
| Use runtime theory judgments or typed holes | `docs/reference/RUNTIME_THEORY_CHECKER.md` | `docs/reference/KERNEL_IR.md` |
| Build software authoring/codegen workflows | `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` | `examples/software_authoring/README.md` |
| Track semantic branches, reconciliation, or backend projections | `docs/reference/SEMANTIC_VCS.md` | `docs/reference/KERNEL_IR.md`, `docs/howto/TESTING.md` |
| Use embeddings/RAG/vector search safely | `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` | `docs/howto/KNOWLEDGE_INGESTION.md` |
| Query through AxQL/SQL-ish/certified querying | `docs/reference/QUERY_LANG.md` | `docs/reference/CERTIFICATES.md` |

Semantic authority stays with accepted canonical `.axi`, compiled IR, typed
runtime reports, and explicitly checked certificates. Storage/debug formats are
documented only where they are needed for engine tests.

## Reference files

- `docs/reference/CERTIFICATES.md` — active certificate families and Lean checker validation.
- `docs/reference/TRUSTED_KERNEL.md` — current trusted boundary and non-claims.
- `docs/reference/KERNEL_IR.md` — canonical schema/category/theory IR, runtime surfaces, and backend projection lowering.
- `docs/reference/RUNTIME_THEORY_CHECKER.md` — runtime theory judgments, closure tiers, typed holes, and non-claims.
- `docs/reference/LEAN_THEORY_EVALUATION.md` — Lean-encoded theory status and gaps.
- `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` — embedding/vector sidecars, RAG evidence, promotion boundaries.
- `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` — authoring/codegen crates, CLI, MCP, server, and LSP surfaces.
- `docs/reference/AGENT_CONTEXT.md` — current agent-facing architecture and greenfield policy.
- `docs/reference/RUST_LIFECYCLE_TYPES.md` — Rust lifecycle and anchor-aware artifact model.
- `docs/reference/QUERY_LANG.md` — AxQL, SQL-ish elaboration, typed query metadata, and certified querying.
- `docs/reference/SEMANTIC_VCS.md` — semantic refs, commits, merge/rebase, reconciliation, and projection manifests.
- `docs/reference/LLM_REPL_PLUGIN.md` — LLM REPL plugin protocol.
- `docs/reference/WORLD_MODEL_PLUGIN.md` — world-model plugin protocol.
