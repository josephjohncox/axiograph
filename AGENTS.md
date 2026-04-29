## Axiograph Agent Map

This file is a short map for agents working in this repo. Keep durable truth in
versioned docs, specs, examples, tests, and typed runtime surfaces. Do not turn
this file back into a backlog or session transcript.

### Harness Rules

- Treat this file as a table of contents, not an encyclopedia.
- If context matters after the current turn, put it in `docs/`, examples, tests,
  `.axi` artifacts, or runtime schemas.
- If an agent repeatedly fails, improve the harness: diagnostics, examples,
  tests, typed reports, refinement handles, or docs.
- Prefer small, typed, reviewable changes with explicit semantic previews.
- Backward compatibility is not a default goal. Keep it only for an explicit
  trust, live-byte, accepted-plane, or operational contract.
- Avoid preserving stale protocol versions, fallback paths, historical exports,
  or old examples when a cleaner typed surface exists.
- Prefer maintained, widely used Rust crates for protocol/core infrastructure
  over custom implementations unless Axiograph semantics require custom logic.
- Keep `AGENTS.md` below 150 lines unless there is a specific reason.
- Do not add living checkbox backlogs to `AGENTS.md`; put them in roadmaps.

### Current Truth

- Axiograph is currently a proof-carrying ontology workbench, not yet a full
  HoTT/topos/dependently typed ontology engine.
- Canonical accepted `.axi` modules are the reviewable meaning plane.
- PathDB and `.axpd` are derived execution/query substrates, not the ontology
  kernel.
- The trusted checker is the import closure of `lean/Axiograph/VerifyMain.lean`,
  not all Lean code.
- Rust provides runtime type discipline: typed anchors, lifecycle wrappers,
  checked builders, typed query/authoring surfaces, and compiled-IR hooks.
- Rust is not the trusted dependently typed kernel; Lean certifies the strongest
  supported fragment.
- Trust contracts are soundness/scope/coverage statements under explicit
  anchors. They are not completeness or ontology-closure claims.
- The intended semantic kernel is accepted `.axi` plus one compiled
  schema/category IR with stable ids.
- Keep relation-as-object plus projection arrows canonical internally.
- RDF, OWL, SHACL, property graphs, TypeDB, TerminusDB, and PathDB are
  adapters/projections/execution substrates unless explicitly lowered through
  the canonical IR.
- AI, LLM, and world-model outputs stay in the evidence plane until typed
  validation, review, CQ gates, reconciliation, and promotion accept them.
- The live `.axpd` format and the sectioned verified `.axpd` story are still not
  fully converged; keep verification claims pinned to the actual checked path.

### Source Of Truth

- Trust boundary: `docs/reference/TRUSTED_KERNEL.md`
- Compiled semantic IR: `docs/reference/KERNEL_IR.md`
- Runtime theory checker: `docs/reference/RUNTIME_THEORY_CHECKER.md`
- Lean theory status: `docs/reference/LEAN_THEORY_EVALUATION.md`
- Embeddings/evidence sidecars: `docs/reference/EMBEDDINGS_AND_EVIDENCE.md`
- Software authoring/codegen tools: `docs/reference/SOFTWARE_AUTHORING_TOOLS.md`
- Rust lifecycle and anchor types: `docs/reference/RUST_LIFECYCLE_TYPES.md`
- Semantic VCS and reconciliation: `docs/reference/SEMANTIC_VCS.md`
- Current agent context: `docs/reference/AGENT_CONTEXT.md`
- Active agent backlog: `docs/roadmaps/ROADMAP_AGENT_BACKLOG.md`
- Typed ontology engineering theory: `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md`
- Rust dependent-type effects: `docs/explanation/RUST_DEPENDENT_TYPES.md`
- Topos/sheaf roadmap: `docs/explanation/TOPOS_THEORY.md`
- Formal verification how-to: `docs/howto/FORMAL_VERIFICATION.md`
- Canonical V1 user workflow: `docs/howto/CANONICAL_SEMANTIC_SPINE.md`
- Test commands and suites: `docs/howto/TESTING.md`
- Documentation index: `docs/README.md`

### Where To Look First

| Task | Start Here |
| --- | --- |
| Understand current trust claims | `docs/reference/TRUSTED_KERNEL.md` |
| Work on schema/category IR | `docs/reference/KERNEL_IR.md` |
| Work on runtime theory checking | `docs/reference/RUNTIME_THEORY_CHECKER.md` |
| Work on Lean theory status | `docs/reference/LEAN_THEORY_EVALUATION.md` |
| Work on embeddings/RAG evidence | `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` |
| Work on software authoring/codegen | `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` |
| Work on Rust type/lifecycle surfaces | `docs/reference/RUST_LIFECYCLE_TYPES.md` |
| Work on ontology usefulness and ologs | `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md` |
| Work on semantic history or merge | `docs/reference/SEMANTIC_VCS.md` |
| Work on active priorities | `docs/roadmaps/ROADMAP_AGENT_BACKLOG.md` |
| Run verification | `docs/howto/FORMAL_VERIFICATION.md` |
| Run tests | `docs/howto/TESTING.md` |

### Implementation Direction

- Make one compiled schema/category IR the common currency for authoring,
  query elaboration, migration, certification, semantic diff, reconciliation,
  and backend projection.
- Make runtime type checking useful to users and agents: inferred types, typed
  holes, refinement handles, repair suggestions, semantic coverage, CQ status,
  trust contracts, and next actions.
- Make theory objects runtime-addressable: constraints, equations, rewrite
  rules, obligations, subjects, transports, paths, contexts, and worlds.
- Prefer shared report families over bespoke JSON: `EvolutionPreviewV1`,
  business-rule applicability, semantic coverage/drift, trust contracts, and
  agent-facing semantic reports.
- Treat semantic VCS as the lifecycle backbone for accepted ontology state,
  review branches, evidence branches, world-model runs, reconciliation, tags,
  supersession, and retraction.
- Keep projected graph backends readable through their native interfaces, but
  keep mutation authority and semantic authority in Axiograph.
- TypeDB is the primary high-fidelity typed backend target. TerminusDB is the
  preferred RDF/VCS-shaped secondary target. Other graph backends are
  experimental unless their capability profiles justify promotion.

### Quality Checks For This File

Run these after changing `AGENTS.md`:

```bash
wc -l AGENTS.md
rg "\\- \\[[ x~]\\]" AGENTS.md
rg "AGENT_CONTEXT|ROADMAP_AGENT_BACKLOG" docs/README.md docs/reference/README.md docs/roadmaps/README.md
git diff --check
```

Acceptance criteria:

- `AGENTS.md` stays below 150 lines unless intentionally justified.
- `AGENTS.md` has no living checkbox backlog markers.
- Durable context and todos are discoverable through the docs indexes.
