# Reference

Reference docs are for lookup and should stay stable.

## Start by task

| Task | Primary reference | Adjacent checks/docs |
| --- | --- | --- |
| Determine what is trusted | `docs/reference/TRUSTED_KERNEL.md` | `docs/reference/CERTIFICATES.md`, `docs/howto/FORMAL_VERIFICATION.md` |
| Audit untrusted I/O, resource limits, or privileged mutation | `docs/reference/SECURITY_BOUNDARIES.md` | `docs/howto/TESTING.md`, `rust/crates/axiograph-security` |
| Work on the canonical semantic spine | `docs/howto/CANONICAL_SEMANTIC_SPINE.md`, `docs/reference/KERNEL_IR.md` | `examples/regulated_shipment/README.md`, `docs/howto/TESTING.md` (`make verify-regulated-shipment`, `make verify-canonical-spine`) |
| Emit or verify certificates | `docs/reference/CERTIFICATES.md` | `docs/howto/FORMAL_VERIFICATION.md` |
| Use runtime theory admissibility judgments or typed holes | `docs/reference/RUNTIME_THEORY_CHECKER.md` | `docs/reference/LEAN_THEORY_EVALUATION.md`, `docs/reference/KERNEL_IR.md` |
| Simplify Rust architecture or crate factoring | `docs/reference/RUST_ARCHITECTURE_CLEANUP.md` | `rust/Cargo.toml`, `docs/howto/TESTING.md` |
| Build software authoring/codegen workflows | `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` | `examples/software_authoring/README.md` |
| Emit backend projections or audit semantic loss/readback | `docs/reference/BACKEND_PROJECTIONS.md` | `docs/reference/KERNEL_IR.md`, `docs/reference/SEMANTIC_VCS.md`, `docs/howto/TESTING.md` |
| Track semantic branches, reconciliation, or accepted-state history | `docs/reference/SEMANTIC_VCS.md` | `docs/reference/KERNEL_IR.md`, `docs/howto/TESTING.md` |
| Use embeddings/RAG/vector search safely | `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` | `docs/howto/KNOWLEDGE_INGESTION.md` |
| Author/query through CQs, typed query metadata, or certified lowerings | `docs/reference/QUERY_LANG.md` | `docs/reference/CERTIFICATES.md`, `examples/competency_questions/README.md` |

Semantic authority stays with accepted canonical `.axi`, compiled IR, and
explicitly checked certificates. Typed runtime reports cite, scope, and explain
that authority; storage/debug formats are documented only where they are needed
for engine tests.

## Reference files

- [Engineering audit at 70c568b](ENGINEERING_AUDIT_70C568B.md) — baseline usability, correctness, theory, retrieval, API, and code findings. Current work is tracked in the [engineering-quality roadmap](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md).
- [Released baseline v20260908.0.0](RELEASE_BASELINE_V20260908.md) — parent-accepted release identity, publication evidence, and scope.
- `docs/reference/CERTIFICATES.md` — active certificate families and Lean checker validation.
- `docs/reference/TRUSTED_KERNEL.md` — current trusted boundary and non-claims.
- `docs/reference/SECURITY_BOUNDARIES.md` — bounded files, processes, networks, parsers, servers, stores, and privileged mutation.
- `docs/reference/KERNEL_IR.md` — canonical schema/category/theory IR and runtime surfaces.
- `docs/reference/BACKEND_PROJECTIONS.md` — capability-declared PathDB/TypeDB/TerminusDB/RDF/property-graph projections, semantic loss, and evidence-only readback.
- `docs/reference/RUNTIME_THEORY_CHECKER.md` — runtime admissibility judgments, finite-saturation boundary, typed holes, residuals, and non-claims.
- `docs/reference/RUNTIME_TYPECHECKER_AUDIT_2026_04.md` — implementation audit for the runtime checker and typed Rust surface.
- `docs/reference/RUST_ARCHITECTURE_CLEANUP.md` — crate roles, simplification rules, dependency guidance, and refactor checklist.
- `docs/reference/LEAN_THEORY_EVALUATION.md` — Lean-encoded theory status and gaps.
- `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` — embedding/vector sidecars, RAG evidence, promotion boundaries.
- `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` — authoring/codegen crates, CLI, MCP, server, and LSP surfaces.
- `docs/reference/AGENT_CONTEXT.md` — current agent-facing architecture and greenfield policy.
- `docs/reference/RUST_LIFECYCLE_TYPES.md` — Rust lifecycle and anchor-aware artifact model.
- `docs/reference/QUERY_LANG.md` — CQ/question-first authoring, typed query metadata, SQL-ish/AxQL lowerings, and certified querying.
- `docs/reference/SEMANTIC_VCS.md` — semantic refs, commits, merge/rebase, reconciliation, and projection manifests.
- `docs/reference/LLM_REPL_PLUGIN.md` — LLM REPL plugin protocol.
- `docs/reference/PREDICTIVE_PROPOSAL_ADAPTER.md` — predictive proposal adapter protocol.
