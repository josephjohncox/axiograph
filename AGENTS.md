## Axiograph — Agent Notes (where we are so far)

This file is a shared context note for humans/agents working in this repo. It captures the current technical reality and the direction we’ve committed to.

### Harness engineering guidance (adapted for this repo)

- Treat `AGENTS.md` as a map, not an encyclopedia.
  - The durable system of record should live in versioned docs, specs, roadmaps,
    references, examples, schemas, and executable plans under `docs/`,
    `examples/`, and the typed runtime/Lean surfaces.
  - When adding new durable policy or theory, prefer adding or updating the
    canonical doc and then linking it from here instead of growing this file
    indefinitely.
  - We should progressively slim this file over time by moving detailed,
    domain-specific truth into better-indexed docs rather than preserving a
    monolithic agent manual.
- Repository-local knowledge is the system of record.
  - If an important design constraint, business rule, ontology assumption,
    migration caveat, world-model lesson, or backend limitation only exists in
    chat, memory, or a human’s head, it effectively does not exist for agents.
  - Encode important operational knowledge into the repo in typed/runtime-usable
    form where possible:
    docs, `.axi`, compiled IR, CQ fixtures, semantic coverage reports, trust
    contracts, backend capability profiles, tests, or generated references.
- When agents fail, fix the harness, not just the prompt.
  - Prefer adding missing capability over retrying vague instructions:
    better typed diagnostics, richer query elaboration, hole-driven repair,
    better docs, stronger examples, CQ fixtures, backend smoke tests, business
    rule surfaces, semantic coverage, typed migration/reconciliation builders,
    and clearer trust reports.
  - In this repo, harness engineering means making the ontology/type/query/CQ
    stack operationally legible to agents, not only mathematically elegant.
- Optimize for agent legibility.
  - Favor repo structure, APIs, docs, examples, and abstractions that are easy
    for an agent to navigate and reason over locally.
  - Prefer boring, explicit, inspectable mechanisms over opaque magic when the
    latter hurts typed reasoning or semantic traceability.
  - Stable ids, typed anchors, explicit lifecycle states, deterministic IR
    lowering, and reviewable preview objects are all part of the harness.
- Plans are first-class artifacts.
  - Complex work should leave behind an execution plan or roadmap entry with
    progress and decisions captured in-repo.
  - Typed ontology evolution work should not rely on ephemeral conversational
    context alone.
- Mechanical enforcement beats aspirational prose.
  - Prefer CI/tests/lints/checkers/generators that enforce:
    doc freshness,
    example validity,
    CQ behavior,
    trust-contract shape,
    typed anchor propagation,
    backend capability/profile invariants,
    and runtime/Lean alignment.
  - If a rule matters, try to make it machine-checkable.
- Agent-facing feedback loops should be rich.
  - The harness should expose not just code but also typed evidence about
    behavior: query elaboration, typed holes, refinement handles, semantic
    coverage, CQ status, migration previews, reconciliation previews, logs,
    metrics, traces, and backend pushdown plans.
  - “Useful to agents” in Axiograph means the agent can discover what is
    missing, what is weak vs strong, what changed, and what code/ontology/test
    action should come next.
- Throughput changes merge/review philosophy.
  - Prefer small, typed, reviewable deltas and explicit semantic previews over
    large opaque patches.
  - Reconciliation, migration, promotion, and ontology discovery should all
    converge on shared typed preview/apply surfaces.
- Entropy must be removed aggressively.
  - Greenfield rule: do not preserve old compatibility harnesses unless they are
    clearly worth the cost right now.
  - Remove stale docs, obsolete protocol versions, dead fallback paths, and
    historical interchange surfaces when they no longer serve the current
    semantic/kernel direction.
  - Backward compatibility is not a default goal in this repo.

### Current state (as-is in `axiograph_v6/`)

- **Build system**: `axiograph_v6/Makefile` builds **Rust + Lean** (Idris/FFI compatibility removed for the initial Rust+Lean release).
- **Runtime / system backbone (Rust)**: `axiograph_v6/rust/` is a workspace with production crates:
  - `axiograph-dsl` (canonical `.axi` parsing)
  - `axiograph-pathdb` (binary storage + certificate / verified-support scaffolding)
  - ingestion crates (`axiograph-ingest-*`), `axiograph-llm-sync`, `axiograph-storage`
- **Snapshot store (Accepted plane + PathDB WAL)**:
  - `.axi` accepted snapshots are canonical (meaning plane).
  - `.axpd` is derived and can be rebuilt from accepted snapshots.
  - `axiograph db accept pathdb-commit` stores **evidence-plane** overlays (currently: `proposals.json` + `chunks.json`) as append-only WAL ops with checkpoints.
- **Proof/verification layer (Lean)**: `axiograph_v6/lean/Axiograph/` contains:
  - **Rewrite/groupoid semantics** (mathlib-backed): `Axiograph.HoTT.*`
  - **Certificate format + checking**: `Axiograph.Certificate.*`
  - `.axi` parsing parity: `Axiograph.Axi.*`
- **Historical Idris prototype**: Idris was used early as a proof-layer prototype. The initial Rust+Lean release removes Idris/FFI compatibility; refer to git history if you need to consult the old Idris modules while porting remaining theory into Lean.
- **Rust already has “certificate-like” witnesses** we can reuse:
  - `axiograph_v6/rust/crates/axiograph-pathdb/src/verified.rs` defines `ReachabilityProof` and `ProvenQueryResult` (explicit witness chains + confidence composition), plus `VerifiedProb` invariants.
  - `axiograph_v6/rust/crates/axiograph-llm-sync/src/path_verification.rs` implements a typed `VerifiedGraph`, `Path`, conflict detection.
- **Rust formal verification tooling is already present (partial)**:
  - `axiograph_v6/rust/verus/` exists as a verification crate (Verus-oriented).
- **Viz frontend (TS)**: `frontend/viz/` is a Vite + TypeScript app; servers/tools load HTML from `frontend/viz/dist` (no embedded template fallback).

### Trust snapshot (current truth)

- Axiograph today is a **proof-carrying ontology workbench**:
  - canonical `.axi` modules are the reviewable meaning plane,
  - PathDB / `.axpd` is a derived execution/query substrate,
  - accepted-plane snapshots + PathDB WAL are the live storage backbone,
  - Lean checks a narrow but real slice of certificates and ontology gates.
- The trusted boundary is the import closure of `lean/Axiograph/VerifyMain.lean`, not “all Lean code”.
  - Use `docs/reference/TRUSTED_KERNEL.md` for current trust claims.
  - Treat HoTT/topos/presheaf material outside that boundary as design/spec support unless it is wired into the verifier.
- Rust provides **dependent-type effects / runtime guardrails**, not a dependently typed kernel:
  - typed anchors,
  - `Module<Validated>` / `Module<Reviewed>`,
  - checked builders,
  - limited typestate,
  - compiled-semantics hooks.
- Trust contracts on user-facing outputs are currently a language for
  `trust_class` / soundness / coverage / scope under stated anchors.
  - They are not completeness claims.
  - They are not ontology-closure claims.
- The next high-value move remains a canonical schema/category IR plus APIs indexed by lifecycle state, anchor, and schema.
- PathDB is a strong engine, but it is not yet the ontology kernel.
  - The intended kernel is accepted `.axi` plus compiled schema/category IR.
  - Keep relation-as-object + projection arrows canonical internally; treat RDF / LPG / quad stores as lowerings.
  - Backend compatibility should target only advanced graph engines with enough
    structure to preserve typed projections:
    RDF/quad stores with named graphs, or property-graph systems with
    constraints/transactions/stable query surfaces.
  - Current backend priority:
    - `TypeDB` is the primary high-fidelity typed backend target,
    - `TerminusDB` is the preferred RDF/VCS-shaped secondary target,
    - and property-graph backends remain experimental rather than first-class support targets.
  - Pushdown guidance:
    - push the richest runtime typing, n-ary relation/role structure, and
      typed-query validation into `TypeDB`,
    - import the useful TypeDB ideas semantically, not syntactically:
      first-class role interfaces, subtype-inherited admissible players,
      explicit constrained schema evolution, and typed constraints,
    - push native history/branch/diff workspace mirroring into `TerminusDB`
      only as a projected collaborator view, never as semantic authority,
    - make projected backends usable through their own native read/query
      surfaces so outside tools can understand a reduced view of the ontology,
      but treat those surfaces as read-only projected lenses rather than the
      full Axiograph semantic/query contract,
    - and keep semantic VCS, olog authoring, CQ gates, trust contracts,
      lifecycle state, and denotational meaning in Axiograph above all
      backends.
  - Semantic VCS, authoring, CQ gates, trust contracts, and lifecycle state
    remain Axiograph-native even when data is materialized into external graph
    databases.
  - Mutation authority for projected graph backends remains Axiograph-only:
    external graph engines may be queried directly for partial understanding,
    but semantic mutation, promotion, review, and lifecycle transitions must
    still flow through Axiograph.
  - Keep Docker-backed smoke coverage for the currently prioritized external
    backends:
    `TypeDB` and `TerminusDB` should have runnable container
    checks in-repo so projection compatibility does not remain doc-only.
  - Runtime adapter work should consume capability profiles directly rather than
    re-encoding backend assumptions:
    generate typed pushdown plans from `CompiledSchemaIr` plus
    `BackendCapabilityProfileV1` / `ProjectionCapabilityProfileV1`, keep those
    plans explicit about preserved tuple/role/context semantics, native
    read-only query dialect, preserved lower-tier interfaces
    (native query / RDF / SHACL where real), lifting contracts back into the
    higher typed/meta layer, and trust caveats.
- Semantic VCS is still a first slice, not the universal lifecycle backbone.
  - `sem/commits`, `sem/refs`, `sem/validations`, and `sem/world_model_runs` exist in the accepted-plane layer.
  - accepted-plane promotion and evidence-plane overlay commits still do not flow through one universal ancestry-driven semantic-commit path.
- Important mismatch still open:
  - `axiograph-pathdb` contains a sectioned “verified” `.axpd` v2 story in `verified.rs`,
  - but the live runtime still serializes/deserializes a simpler v1 blob in `lib.rs`.
- Practical framing to keep using:
  - “Axiograph has a Lean-checked core for typed rewrite/path witnesses and ontology gates, with a roadmap toward categorical ontology semantics.”
  - Do not describe the current system as a full HoTT/topos ontology engine.

### What “making it one” means operationally

- Reserve the phrase “full dependently typed ontology engine” for a concrete repo state, not for aspiration-level theory.
- In this repo, the stronger claim becomes honest only when all of the following are true:
  - the Lean verifier checks more than replay-shaped witnesses:
    - canonical accepted `.axi` typing,
    - checked rewrite-rule admissibility,
    - and certifiable schema/category/query/migration obligations for explicitly stated fragments;
  - one compiled schema/category IR is the common semantic currency for:
    - authoring,
    - query elaboration,
    - migration,
    - certification,
    - and semantic diff / merge;
  - Rust public APIs make the semantic indices difficult to ignore:
    - lifecycle state,
    - accepted snapshot / module anchors,
    - schema / theory / context ids,
    - prepared query handles,
    - migration preview handles,
    - and certified-answer handles;
  - typed authoring surfaces (including olog-oriented ones) emit canonical `.axi` deltas over stable object / arrow / role ids with provenance, CQ attachments, and trust metadata;
  - semantic VCS history records state anchors plus explicit semantic deltas, so review / merge / promotion operate over typed ontology objects rather than opaque storage diffs.
- This does **not** mean “all runtime logic moves into Lean”.
  - It means every semantics-bearing seam reduces to typed, anchored artifacts with a small checked kernel boundary and explicit soundness scope.
- Until those conditions hold, keep using the current workbench framing above.

### Active priorities (2026-04)

- Keep docs and agent notes aligned with the actual trusted kernel boundary; avoid present-tense claims for roadmap-only HoTT/topos/query semantics.
- Highest-value TODOs while the repo is still a proof-carrying ontology workbench:
  - [ ] compile one canonical schema/category IR with deterministic ids for schema/theory/instance/context objects, relation-objects, projection arrows, rewrite rules, and anchors shared by Rust, Lean, semantic diff, migration preview, and certificates.
  - [ ] make typed authoring and ontology exploration return one preview contract:
    - canonical `.axi` deltas over stable ids,
    - provenance/evidence/context anchors,
    - CQ attachments and trust metadata,
    - explicit structural evolution primitives (`reify_relation_object`, `introduce_dependent_relation_family`, `transport_along_schema_morphism`, `introduce_subtype`, `generalize_to_supertype`, `specialize_to_subtype`, `push_relation_role_to_subtype`, `pull_relation_role_to_supertype`, `factor_common_structure_to_supertype`, `split_type_into_subtypes`, `merge_types_under_supertype`, `lift_relation_to_carrier`, `add_path_equation`, `add_rewrite_rule`, `resolve_conflict_by_decision`),
    - directed exploration next-actions keyed to those primitives,
    - and fix-oriented repair suggestions instead of ad hoc notes or raw strings.
  - [ ] keep typed query and certification first-class across REPL, server, and tooling:
    - `query_ir_v1` / prepared query handles as the common execution currency,
    - structured elaboration diagnostics and repair data,
    - anchor-aware answers and uniform trust contracts,
    - and certification language scoped to row soundness under explicit anchors.
  - [ ] treat migration as typed transport plus explicit reindexing over stable semantic ids:
    - previews should name the transport basis,
    - preserved/reindexed/split/merged/dropped ids,
    - and residual comparability obligations across anchors.
  - [ ] treat reconciliation as typed evolution over conflict sets and decisions:
    - merge is not file concatenation,
    - conflict sets, operator/policy decisions, CQ/trust consequences, and unresolved obligations should all remain first-class preview/history objects.
  - [~] make type inference / hole-driven exploration a first-class shared service over the compiled IR:
    - current implemented slice:
      inferred types, structured typed holes, variable-centric exploration suggestions, elaborated IR, and focused exploration payloads are available over prepared queries / REPL / server / tool-loop surfaces,
      typed olog authoring now also returns structured runtime holes for missing relation-role bindings, role/type mismatches, projection holes, and path endpoint mismatches,
      query exploration and typed olog authoring now share a first common runtime refinement-handle/candidate protocol, while keeping query ops and authoring ops surface-specific,
    - keep inferring schema/type/context/result-shape indices conservatively from accepted/review-state semantics rather than guessing ontology meaning,
    - next implementation step:
      extend the same shared refinement protocol into migration authoring, reconciliation review, CQ repair, and implementation-surface mapping,
    - extend that same typed-hole/refinement model beyond the current query + typed-olog slice into CQ authoring, migration authoring, and implementation-surface mapping,
    - continue replacing hard failures for recoverable unknowns with typed repair sites where semantically defensible,
    - and keep docs/UX explicit that this is runtime dependent-type usefulness, not a replacement for the Lean-checked semantic kernel.
  - [ ] make CQ-gated evolution a shared primitive rather than a proposal-preview-only slice:
    - one preview object for proposal review, accepted-plane promotion, migration preview, and semantic merge,
    - schema/theory/instance/context deltas plus before/after CQ status and expected answer-shape refs,
    - rich structural primitives so review can distinguish subtype/generalization/role-movement/factor/split/merge/lift moves from generic add/remove churn,
    - and fail-closed policy hooks before accepted-plane mutation.
  - [x] first slice: treat migration as typed transport plus reindexing over stable semantic ids:
    - migration preview builders now emit transport-along-morphism, transported path-equation, subtype-collapse, and merge-image primitives,
    - previews should keep citing the transport basis and preserve semantic comparability explicitly,
    - and the next step is to route real migration-authoring/preview entrypoints through these builders rather than leaving them helper-level.
  - [x] first slice: treat reconciliation as typed evolution over conflict sets and decisions:
    - conflict/decision records now lower into the same preview language as authoring and migration,
    - persisted reconciliation can now emit a stored preview report plus a `SemCommitKindV1::Merge` commit carrying typed delta/trust/rule/coverage summaries,
    - unresolved conflicts remain explicit residual obligations,
    - and the next step is to route CLI/server merge flows through this path rather than treating reconciliation as a raw side file.
  - [x] first slice: treat compiled-IR exploration as a general service beyond olog authoring:
    - compiled IR now emits typed exploration candidates for relation-object reification, dependent families, carrier lifts, rewrite candidates, and subtype factoring,
    - `/discover/draft-axi` now returns this exploration preview,
    - and the next step is to drive query refinement, migration authoring, reconciliation review, backend projection review, and implementation-surface mapping through the same refinement/typed-hole protocol.
  - [ ] make semantic VCS the reviewable unit of ontology change without overstating what exists today:
    - semantic commits / refs / validations should carry typed deltas and anchors,
    - review/evidence/world-model branches should be explicit,
    - and ancestry/reconciliation should become the default path for high-value changes.
  - [ ] make the typed lifecycle useful for co-evolving real engineering systems, not only ontology artifacts:
    - implementation surfaces must grow beyond endpoints/jobs into simulators, optimizers, PLC logic, HMI views, reports, and document/SOP sections,
    - agent-facing reports should answer "what applies here, what is weak, what drifted, and what code/test/CQ/ontology action comes next?",
    - and the canonical stress case should remain a hard industrial system where physics, process, ERP/MRP, certification, pricing, delivery, and code all evolve together.
  - [ ] require AI/world-model outputs to remain evidence-plane but richly typed:
    - typed run/proposal/snapshot anchors,
    - proposal-set digests and grounded evidence links,
    - candidate schema/theory/olog deltas,
    - and explicit preview failures instead of silent coercion.
  - [ ] keep verification language pinned to the live-byte truth boundary:
    - only claim what is checked against accepted `.axi` anchors and the actual production `.axpd` bytes/checkpoints,
    - and keep the verified-v2 `.axpd` story clearly separate until production serialization converges.
- Current slices to build on rather than re-invent:
  - [x] typed anchors/lifecycle wrappers and an initial `kernel_ir.rs` / compiled-semantics scaffold already exist in Rust.
  - [x] `/query`, query-result certification, and proposal preview validation already expose a first trust-language slice with explicit non-claims around completeness and ontology closure.
  - [x] CQ-gated proposal preview is real today for evidence-plane overlays.
  - [x] accepted-plane code already knows about `sem/commits`, `sem/refs`, `sem/validations`, and `sem/world_model_runs`; the gap is universal workflow use, not directory invention.
- Keep AI/world-model tooling in evidence-plane, but make lineage first-class where usefulness is consumed:
  - typed run/proposal/snapshot anchors,
  - proposal digests,
  - grounded source/context links,
  - explicit preview failure modes rather than silent coercion.
  - Treat “LLM-assisted” as concrete typed integration surfaces:
    plugin protocols, API-backed runners, tool-loop services, and future
    MCP/skill-style adapters, not as free-form rewrite-only behavior.
  - Prefer agent-facing typed reports and APIs over prose-only assistance:
    rule applicability, semantic coverage, drift, typed hole exploration, and
    engineering next-actions should be available as structured services.
- Raise semantic-VCS/world-model lifecycle coupling to high priority:
  - persist proposal/review bundles under `sem/validations/` with stable proposal-set and run anchors,
  - require CQ-gated merge/review transitions from review branches before accepted-plane promotion,
  - and emit previewable semantic diffs/cq-impact reports before any `accepted` snapshot mutation.
- Continue to align the runtime with the `.axpd` live-byte truth boundary:
  - only claim what is checked against live serialized bytes,
  - scope verification language to that seam until verified-v2 `axpd` emission is live.

High-level service direction (not exhaustive):

- compiler/typechecker should be an ontology-service interface, not a validation-only phase;
- compiler output should include classification (`certifiable`, `mixed`, `execution-only`) plus repair guidance;
- schema-aware completion should drive authoring and proposal drafting before execution.
- the same compiled schema/category IR should be shared by authoring, query, migration, certification, and semantic diff rather than duplicated across tools.

### Runtime usefulness bar

- Treat the Rust runtime checker as the default useful surface for ordinary ontology work outside the Lean-certified slice.
- Near-term surfaces should converge on a small artifact family instead of inventing bespoke review JSON:
  - one evolution-preview report for proposal / promotion / migration / merge with typed `schema` / `theory` / `instance` / `context` deltas, rich structural primitives, CQ status, trust contract, residual obligations, and directed exploration next-actions;
  - one business-rule applicability report naming matched theory/rule/CQ objects, accepted/review anchors, world/context scope, trust strength, and suggested next actions;
  - one semantic-coverage / drift report showing which ontology objects, rules, and CQs are mapped into code/tests/docs/interop artifacts and where gaps remain;
  - one agent-facing semantic report family that can compose ontology-backed results, CQ assets, and retrieved evidence without collapsing strong and weak claims.
- Prefer extending shared preview / trust / report families over creating new one-off payloads per CLI, server, viz, or agent surface.
- Keep business-rule usefulness concrete:
  - the question is not only “can Lean certify this?”;
  - it is also “what can the runtime checker say now, under explicit anchors, about applicability, scope, residual gaps, and required next actions?”

### Change policy (greenfield default)

- Backward compatibility is **not** a default goal in this repo.
  - Prefer the cleaner semantic/runtime design when a change improves the canonical IR, typed authoring/query surfaces, semantic VCS, or trusted-boundary clarity.
- Keep compatibility only when there is a concrete reason, for example:
  - trust/soundness continuity across certificate or verifier boundaries,
  - the live-byte `.axpd` truth boundary,
  - accepted-plane anchor semantics,
  - or an explicit operational/runtime contract we have chosen to preserve.
- Examples, demos, and tests should be updated intentionally to match the current design.
  - They are not required to remain unchanged.
  - What matters is that the repo keeps a coherent, truthful set of examples and verification checks for the surfaces we currently claim.
- When a stronger typed/runtime surface exists, remove the old compatibility harness in the same slice instead of dual-running both.
  - Do not keep legacy request/response shapes, parser branches, or plugin payload variants around “just in case”.
- Current high-priority compatibility cuts after the raw-AxQL removal:
  - collapse `query_ir_v1` execution so it stops routing through AxQL as the effective runtime currency,
  - keep `query_result_v3` as the only supported query certificate family; do not reintroduce `query_result_v1` / `query_result_v2`,
  - then remove the remaining `/anchor.axi`-style surfaces and any residual `PathDBExportV1` certification paths rather than treating those exports as primary anchors,
  - and collapse workflow-specific wrappers so `EvolutionPreviewV1` becomes the single mutation-review currency.
  - when strengthening ontology evolution, prefer exact semantic primitives over compatibility summaries.
    - subtype/supertype moves, role push/pull, split/merge, and carrier lifting should be explicit preview/history fields rather than inferred from old bucket-only reports.

### Docs (canonical references)

- `docs/README.md` is the navigation entrypoint.
- `docs/reference/TRUSTED_KERNEL.md` is the source of truth for trust-boundary claims.
- `docs/reference/KERNEL_IR.md` is the source of truth for the compiled IR scope.
- `docs/reference/RUST_LIFECYCLE_TYPES.md` is the source of truth for Rust lifecycle/anchor design.
- `docs/reference/SEMANTIC_VCS.md` is the source of truth for semantic versioning/lifecycle targets.
- `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md` is the focused theory/usefulness note for typing, denotational semantics, ologs, and AI-assisted ontology discovery.

### Pointers for new agents

- **Rewrite/groupoid semantics are in Lean**:
  - `axiograph_v6/lean/Axiograph/HoTT/KnowledgeGraph.lean`
  - `axiograph_v6/lean/Axiograph/HoTT/PathAlgebraProofs.lean`
- **Rust “witness” starting points**:
  - `axiograph_v6/rust/crates/axiograph-pathdb/src/verified.rs` (`ReachabilityProof`, `VerifiedProb`)
  - `axiograph_v6/rust/crates/axiograph-llm-sync/src/path_verification.rs`
- **Current target specs / execution seams**:
  - `docs/reference/TRUSTED_KERNEL.md`
  - `docs/reference/KERNEL_IR.md`
  - `docs/reference/RUST_LIFECYCLE_TYPES.md`
  - `docs/reference/SEMANTIC_VCS.md`
  - `docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md`
- **Build entrypoint**: `axiograph_v6/Makefile`
- **Competency questions + world model demos**:
  - CLI: `axiograph discover competency-questions` (schema-driven or NL→AxQL with LLM)
  - Physics-scale demos: `scripts/world_model_mpc_physics_*_demo.sh` and `examples/competency_questions/physics_cq.json`

### Migration checklist (living)

**Lean (spec + checker)**
- [x] Lake project scaffold (`lean/`) + mathlib dependency pinned.
- [x] Targets: `make lean`, `make verify-lean*`, `make verify-lean-e2e*`.
- [x] Port: historical Idris `Axiograph.HoTT.Core` → `lean/Axiograph/HoTT/Core.lean`.
- [x] Port (partial): historical Idris `Axiograph.HoTT.KnowledgeGraph` → `lean/Axiograph/HoTT/KnowledgeGraph.lean` (`KGPath`, `KGPathEquiv`, facts).
- [ ] Port (remaining): `KnowledgeGraph.idr` transport, quotient, `KGEquiv` / migration scaffolding (Lean `Quot`-based).
- [x] Port (partial): historical Idris `Axiograph.HoTT.PathAlgebraProofs` → `lean/Axiograph/HoTT/PathAlgebraProofs.lean` (length + confidence).
- [ ] Port (remaining): inverse paths, equivalence congruence, normalization, functoriality proofs (prefer mathlib groupoid/free groupoid where applicable).
- [x] Port: historical Idris `Axiograph.Prob.Verified` → `lean/Axiograph/Prob/Verified.lean` (fixed-point `VProb`, combination laws, Bayes update, etc).
- [ ] Port: historical Idris `Axiograph.Prob.PathVerificationVerified` → `lean/Axiograph/Prob/PathVerificationVerified.lean`.
- [ ] Port: historical Idris `Axiograph.Prob.ReconciliationVerified` → `lean/Axiograph/Prob/ReconciliationVerified.lean`.
- [ ] Port: remaining theory modules (modal/temporal/tacit, reconciliation) into Lean as needed for the trusted checker.
- [x] Parse canonical `.axi` in Lean: unified `axi_v1` parser (`lean/Axiograph/Axi/AxiV1.lean`).
- [x] Unified `.axi` entrypoint in Lean: `axi_v1` dialect detection (`lean/Axiograph/Axi/AxiV1.lean`).
- [ ] Converge parsers to a shared `Axiograph.ModuleAST` (schema/theory/instance), anchored to `examples/canonical/corpus.json`.
  - Canonical corpus manifest: `examples/canonical/corpus.json`
  - Current corpus uses the unified `axi_v1` entrypoint (dialect-detecting); the versioned parsers remain for clarity.
- [ ] Keep the trusted-scope docs honest:
  - clearly separate Lean-checked kernel claims from explanation-level HoTT/topos roadmap material,
  - keep “not a full HoTT implementation” explicit in agent/docs framing,
  - keep univalence isolated and non-essential to currently shipped certificate checks.

**Certificates**
- [x] Historical cert v1 (JSON): reachability witness remains understood by Lean for verifier continuity, but the Rust runtime no longer treats the old float-wrapper as an active emission surface.
- [x] Cert v2 (JSON): fixed-point reachability witness (`rel_confidence_fp`) + Lean checker.
- [x] Cert v2: reconciliation decision (`resolution_v2`) + Lean checker.
- [x] Cert v2: normalization (`normalize_path_v2`) + Lean checker.
- [x] Cert v2: path equivalence (`path_equiv_v2`) + Lean checker (shared normal form + optional derivations).
- [x] Extend `normalize_path_v2` to groupoid rewrite derivations (assoc/inv/cancel + explicit step list).
- [ ] Extend cert v2 rewrite derivations beyond normalization (reconciliation proofs, domain rewrites).
- [ ] Anchor certificates to canonical `.axi` inputs (stable module hash + extracted facts).
- [ ] Continue removing legacy certificate surfaces unless they are still needed for trusted-checker continuity or explicit fixture coverage.
  - Current state: Rust-side standalone v1 reachability emission is gone, the DB server now emits canonical `.axi`-anchored `reachability_v3`, and Rust-side query certificate emission routes only through the `.axi`-anchored `query_result_v3` / typed-query-witness path; verifier-side historical support remains explicit in `docs/reference/CERTIFICATES.md`.
  - Highest-value next removal: cut any remaining `/anchor.axi`-style compatibility surfaces and retire residual `PathDBExportV1`-anchored verifier-only fixtures when the canonical anchored replacements cover them.
- [ ] Keep certificate scope explicit and conservative:
  - certify soundness of returned rows / derivations / migrations,
  - do not imply completeness of query results or full ontology-theoretic closure unless explicitly proved.

**Rust runtime / emitters**
- [x] Rust e2e: emit reachability cert (`make verify-lean-e2e`).
- [x] Rust e2e: emit fixed-point reachability cert v2 (`make verify-lean-e2e-v2`).
- [x] Rust e2e: emit resolution cert v2 (`make verify-lean-e2e-resolution-v2`).
- [x] Rust e2e: emit normalize_path cert v2 (`make verify-lean-e2e-normalize-path-v2`).
- [x] Rust proof-mode scaffolding: `ProofMode` generic + proof-producing optimizer (normalize/resolution/Δ_F).
- [x] PathDB snapshot export/import: `.axpd` ↔ `.axi` (`PathDBExportV1`) + CLI (`axiograph db pathdb export-axi|import-axi`).
- [x] DB server wrapper: `axiograph db serve` (read-only replica + optional write master) serving `/query` (`query_ir_v1`) + `/status` + admin endpoints.
- [x] DB server viz endpoints: `GET /viz` (HTML), `GET /viz.json` (graph JSON), `GET /viz.dot` + `refresh_secs=N` for live-ish auto-refresh.
- [x] DB server certificate endpoints:
  - `POST /cert/reachability` (canonical `.axi`-anchored `reachability_v3` cert from relation-id chains),
  - `/query` supports `certify/verify` over canonical anchor digests and optional default `contexts`.
- [x] DB server can optionally verify certificates server-side:
  - verifier discovery: `--verify-bin`, `AXIOGRAPH_VERIFY_BIN`, `bin/axiograph_verify`, repo dev fallback,
  - exposed in `/status.certificates.*`.
- [x] DB server can enforce a “fail closed” certificate gate for high-value answers:
  - `/llm/agent` supports `require_query_certs` / `require_verified_queries`,
  - `/viz` LLM tab exposes “require verified query certificates (fail closed)”.
- [x] DB server demos (scripts): `scripts/db_server_api_demo.sh`, `scripts/db_server_distributed_demo.sh`, `scripts/db_server_live_viz_demo.sh`.
- [x] Rust query certification emits fixed-point and canonical-module-anchored certificates for the current certifiable query subset (`/query certify`, AxQL query-result certificates, `reachability_v3` witnesses).
- [ ] Wire runtime normalization / path-equivalence / reconciliation certificate emitters into user-facing CLI/server workflows rather than keeping them mostly as optimizer/runtime helpers.
- [x] Canonical `.axi` parser in Rust: unified `axi_v1` entrypoint (`rust/crates/axiograph-dsl/src/axi_v1.rs`).
- [x] Unified `.axi` entrypoint in Rust: `axi_v1` dialect detection (`rust/crates/axiograph-dsl/src/axi_v1.rs`).
- [x] Rust↔Lean parsing parity (canonical corpus): `make verify-axi-parse-e2e`.
- [x] Rust↔Lean parsing parity (PathDB snapshot export): `make verify-pathdb-export-axi-v1`.
- [ ] Keep Rust/Lean parsers in lockstep (surface grammar + AST) for the canonical corpus.
- [ ] Converge the live `.axpd` format and the “verified” `.axpd` story:
  - either promote the sectioned verified binary format into the production serializer,
  - or explicitly scope `verified.rs` as non-production and keep all verification claims targeted at the actual live bytes/checkpoints.
- [ ] Add end-to-end tests over the **actual** production `.axpd` checkpoint format, not only over exported anchors / synthetic examples.
- [ ] Treat `axiograph-storage` as non-foundational until placeholder endpoint resolution and other prototype shortcuts are removed; keep accepted-plane snapshots + PathDB/WAL as the real storage backbone.

**Ingestion / promotion (evidence → candidates)**
- [x] `proposals.json` → candidate domain `.axi` modules (entity resolution + schema mapping + promotion trace) via `axiograph discover promote-proposals`.
- [x] Proto ingestion emits semantic annotation edges (auth scopes/idempotency/stability/tags + field required/PII/units/examples).
- [x] Optional chunk overlay: `axiograph db pathdb import-chunks` + `fts(...)` for doc search and LLM grounding (extension layer; not certifiable).
- [x] Promote reviewed candidates into the accepted `.axi` plane (append-only log + snapshot ids) and rebuild PathDB from snapshots.

**Rust formal verification (Verus, additive)**
- [x] Optional Verus target: `make verify-verus` (runs `rust/verus/src/lib.rs` if Verus is installed).
- [x] Align Verus probability model with Lean fixed-point `VProb` (avoid floats in verified core).
- [ ] Prove certificate invariants for runtime witnesses (reachability, normalization, reconciliation).

**Rust “dependent type” encodings (design)**
- [x] Documented typestate/branding/witness patterns: `docs/explanation/RUST_DEPENDENT_TYPES.md`.
- [x] Documented Rust↔Lean “Topos view” correspondence for type-directed execution (`docs/explanation/TOPOS_THEORY.md`, `docs/explanation/RUST_DEPENDENT_TYPES.md`).
- [x] Added stable semantic anchor newtypes in `axiograph-pathdb`:
  - `AxiDigest`,
  - `AcceptedSnapshotId`,
  - `PathdbSnapshotId`,
  - `ProposalDigest`,
  - `WorldModelRunId`,
  - `SchemaId`,
  - `TheoryId`,
  - `ContextId`,
  - `StableFactId`.
- [x] Threaded typed anchors through accepted-plane metadata, certificate anchors, PathDB index sidecars, world-model request/response lineage, and the store-backed DB server snapshot cache.
- [x] Introduced lifecycle-typed module wrappers and transitions in Rust:
  - `Module<Validated>`,
  - `Module<Reviewed>`,
  - `validate_axi_v1_module`,
  - `review_axi_v1_module`,
  - and importer / constraint-check entrypoints that require well-typed module states.
- [x] Added an initial kernel IR scaffold plus compiled-semantics parity hooks:
  - `kernel_ir.rs`,
  - subtype-aware context/temporal axis classification,
  - AST-vs-meta-plane compiled relation semantics parity,
  - checked-db rewrite endpoint derivation rebased on compiled carrier semantics instead of field-name heuristics.
- [ ] Make lifecycle + anchor typestate universal for major artifacts:
  - `Parsed`, `Validated`, `Reviewed`, `Accepted`, `Certified`,
  - plus stable semantic anchors on modules/proposals/snapshots/answers.
- [ ] Promote schema-scoped / snapshot-scoped wrappers to the default public APIs; de-emphasize raw `u32` / `String` surfaces in core execution paths.
- [ ] Distinguish process-local identity from persistent identity:
  - keep `DbToken` for in-memory mismatch protection,
  - add first-class stable anchors such as `AcceptedSnapshotId`, `AxiDigest`, `ProposalDigest`, `WorldModelRunId`, `SchemaId`, `TheoryId`, `ContextId`.
- [ ] Introduce first-class Rust types for ontology workflows:
  - `FactId<A>`,
  - `TypedFact<S, R, A>`,
  - `ProposalSet<Validated, A>`,
  - `WorldState<A>`,
  - `WorldModelRun<A>`,
  - `CertifiedAnswer<A>`.
- [ ] Lift module kind into the Rust type surface:
  - distinguish `CanonicalAxiModule` vs `PathdbExportModule`,
  - stop relying on runtime-only `.axi` kind classification at core import/commit boundaries.
- [ ] Expose a first-class prepared/typechecked query handle:
  - e.g. `PreparedQuery<S, A>` / `TypecheckedLoweredQuery`,
  - so REPL / server / LLM tooling stop executing from raw query strings and raw ASTs.
- [x] `db serve /query` now accepts structured `query_ir_v1` as the machine-facing query contract, and can echo canonical compiled query IR alongside elaboration output.
- [ ] Keep the structured query/compiler surface first-class across all entrypoints:
  - `query_ir_v1` should be accepted uniformly by REPL, DB server, and tool-loop entrypoints,
  - elaboration/typecheck results should include structured diagnostics and repair suggestions, not only text notes,
  - and prepared query handles should become the common execution currency across REPL/server/tooling.
- [ ] Replace schema/type/relation string lookups in common typed workflows with stable refs derived from the compiled schema/category IR.
- [ ] Make typed execution operate over a compiled schema/category IR instead of only stringly meta-plane indexes.
- [ ] Keep docs / code comments precise: describe these Rust surfaces as “dependent-type effects / runtime guardrails,” not as a dependently typed kernel.

**Compiler / typechecker as ontology services**
- [ ] Turn the compiler/typechecker into first-class ontology-engineering services:
  - structural classification (`canonical` vs derived snapshot/export),
  - typed completion / hole-filling for roles and fields,
  - ambiguity reporting + repair suggestions,
  - elaboration output (typed query IR, inferred types, rewrite/normalization traces),
  - semantic diff / migration preview / CQ regression hooks.
- [ ] Surface certifiability and trust class in user/tool APIs:
  - label query forms and answers as `certifiable`, `execution-only`, or `mixed`,
  - expose certificate kind / anchor / soundness-only caveats in review and exploration surfaces,
  - and avoid implying completeness where only soundness is checked.
- [x] Add a first runtime trust-contract slice on DB server query/certificate responses:
  - `/query` now returns `trust = { trust_class, soundness, coverage, scope }`,
  - `/cert/reachability` returns the same shape for certificate payloads,
  - and mixed/certifiable/runtime-only query classes are surfaced as API data instead of only documentation.
- [x] Add a first runtime trust-contract slice on proposal preview validation:
  - `ProposalsValidationV1` now includes `trust = { trust_class, soundness, coverage, scope }`,
  - CQ-gated preview validation reports before/after coverage and per-question trust metadata,
  - and proposal authoring tools can request CQ gating directly.
- [ ] Extend the trust contract beyond the current DB server slice:
  - align REPL query output, LLM/tool-loop query output, migration preview, and proposal review responses on the same fields,
  - attach anchor ids / certificate kinds where applicable,
  - and keep “soundness/coverage/scope, not completeness/closure” explicit in all user-facing trust narratives.
- [x] Add a first runtime semantic-summary slice for ontology/business-rule usefulness:
  - proposal and promotion previews now attach a runtime semantic summary with explicit non-claims (`completeness_claim = not_claimed`, `ontology_closure_claim = not_claimed`),
  - the summary inventories visible rule surfaces (structured constraints, rewrite rules, named blocks), typed coverage, CQ coverage, and current review/runtime gaps,
  - and semantic commits / semantic refs persist a compact gate summary so promotion trust/coverage survives beyond preview JSON.
- [x] Add first typed runtime business-rule report objects:
  - `RuntimeRuleCatalogV1`, `RuntimeRuleReportV1`, and `BusinessRuleApplicabilityReportV1` now give the runtime checker stable rule ids/scope ids, rule classes, runtime/advisory/review-only trust classes, and agent-facing applicability/next-action summaries,
  - relation/theory scoped reports are deterministic over the current meta-plane,
  - and they make “what applies here, how strong is it, and what is still missing?” a typed runtime question rather than ad hoc UI logic.
- [ ] Upgrade query/proposal authoring diagnostics from strings/counts to typed repair objects:
  - ambiguous schema/relation cases should surface candidate fixes,
  - missing role/field/default-carrier issues should surface concrete next moves,
  - and `query_ir_v1`, proposal authoring, and draft `.axi` review should share the same repair language family.
- [x] Replace string-only query refinements with typed apply/refine handles:
  - query exploration now emits `AxqlRefinementHandleV1` / `AxqlRefinementOpV1` rather than only string patches,
  - `QueryIrV1` / `PreparedQueryV1` can apply those handles directly and return a refined typed query plus updated trust/introspection payloads,
  - and refinement ids are stable over the handle payload rather than ad hoc UI strings.
- [x] Add the first shared refinement-protocol slice across query + typed olog authoring:
  - `typed_refinement.rs` now carries one runtime handle/candidate envelope over query and olog-authoring domains,
  - query exploration now exports shared runtime refinement candidates in addition to query-local candidates,
  - typed olog holes now carry machine-applicable role-binding/refinement handles where the repair is deterministic,
  - and typed olog checking can apply shared refinement handles by id and re-run the compiled-IR checker on the refined fragment.
- [x] Extend the shared refinement protocol into migration preview, reconciliation review, and CQ repair:
  - `EvolutionPreviewV1` now carries shared `refinement_candidates` rather than only prose next-actions/residual obligations,
  - migration previews emit typed transport-obligation refinement handles,
  - reconciliation previews emit typed conflict-resolution handles and reconciliation refinement can now upsert explicit decision records,
  - CQ evaluation emits shared runtime repair handles over the prepared typed query surface,
  - and CQ repair now intentionally filters out elaboration-internal lookup-variable moves so the surfaced candidates remain machine-usable for agents.
- [x] Make the first runtime theory-obligation seam addressable in the compiled IR:
  - `TheoryIr` now exports `TheoryObligationRefIr` / `TheoryObligationKindIr` plus `TheorySubjectRefIr` / `TheorySubjectKindIr`,
  - the compiled IR now exposes `obligation_refs()`, `subject_refs()`, `subject_refs_for_obligation(...)`, and `obligation_refs_for_subject(...)`,
  - constraints now retain compiled relation ids plus field/param role ids, and parseable path equations / rewrite rules retain compiled relation ids,
  - previews/repairs can now cite typed theory obligations and semantic subjects rather than bottoming out only in strings,
  - and this is the next stepping stone toward fuller runtime-addressable higher-order/dependent theory obligations.
- [x] Add the first meta-aware query trust slice across query/repl/LLM/server/CQ surfaces:
  - query trust now carries `semantic_coverage`, `semantic_claims`, and explicit `gaps` when meta-plane data is available,
  - `PreparedQueryV1` now carries its trust/semantic profile as typed runtime state rather than forcing downstream callers to recompute it from raw query text,
  - and CQ evaluation / REPL / tool-loop / `/query` all surface the same “soundness/coverage/scope, not completeness/closure” framing.
- [ ] Deepen meta-aware query trust from “in-scope ontology surface” to “actually used semantic surface”:
  - distinguish runtime-used rules from merely in-scope declarations,
  - persist that distinction in CQ/preview/review artifacts,
  - and keep unsupported/review-only rule surfaces explicit.
- [x] Cut the old raw-AxQL compatibility harnesses from the active LLM/server machine boundaries:
  - `/query`, `/llm/to_query`, command-plugin query generation, and the LLM tool-call surfaces now require structured `query_ir_v1`,
  - raw `axql` remains a human/debug representation only, not a wire/plugin contract,
  - and old fallback request/response shapes should be removed rather than preserved unless they are explicitly needed for trusted migration.
- [ ] Make type-driven ontology usefulness explicit in user tooling:
  - schema-first exploration,
  - olog-fragment synthesis,
  - candidate constraint / path-equation / rewrite-rule discovery,
  - and previewable `.axi` deltas before promotion.
- [x] Deepen the runtime module checker beyond schema/instance-only validation:
  - `validate_axi_v1_module` now checks structured theory constraints against declared relation/field/param surfaces,
  - runtime-checked rewrite rules and parseable path equations now typecheck against compiled carrier semantics,
  - and `kernel_ir::compile_theory_ir(...)` assigns deterministic ids to constraints/equations/rewrite rules instead of leaving theory objects as stringly metadata.
- [x] Add a first typed-authoring draft signal in the LLM/tool-loop draft path:
  - `draft_axi_from_proposals` there can distinguish `draft_only` from `validated` drafts,
  - carries a Rust-side `axi_well_typed_proof_v1` summary when the generated module passes the well-typed gate,
  - and makes the review/promotion boundary explicit in the returned trust narrative.
- [x] Bring the same typed-authoring response contract to `/discover/draft-axi`:
  - the DB-server draft-authoring endpoint now returns the same lifecycle/trust summary as the LLM/tool-loop draft path,
  - backed by a shared `typed_authoring` helper rather than duplicated ad hoc JSON logic.
- [ ] Bring the same typed-authoring response contract to viz review flows and other non-LLM authoring entrypoints.
- [ ] Treat competency-question gap discovery as a first-class compiler service:
  - identify unsupported or under-specified CQs,
  - preserve expected answer shapes as review assets,
  - and route missing-typing failures into candidate schema/theory extensions instead of coercing data into the current schema.

**Mathlib targets to reuse (preferred over re-inventing)**
- `Mathlib.CategoryTheory.Groupoid.FreeGroupoid` (free groupoid on a quiver; good fit for “paths up to groupoid laws”).
- `Mathlib.CategoryTheory.Quotient` + `Quiver.Paths` (quotiented path semantics and rewriting relations).
- `Mathlib.CategoryTheory.Sites.Grothendieck` + `Mathlib.CategoryTheory.Sites.Sheaf` (sites/sheaves/subtopoi; good fit for “contexts/worlds + modalities”).
- `Mathlib.CategoryTheory.FintypeCat` (finite-set semantics for `.axi` instances).
- `Mathlib.MeasureTheory.Measure.GiryMonad` (a reference point for probability semantics; not yet in the trusted checker).

**Topos / sheaf semantics (knowledge, contexts, modalities)**
- [ ] Define a schema presentation category in Lean (relations as objects + projection arrows), then treat instances as functors into `FintypeCat`.
- [ ] Define context/world scoping semantics in Lean as a presheaf/sheaf story (avoid implicit closed-world assumptions; keep “unknown ≠ false” explicit).
- [ ] Express modal operators (`□`, `◇`) via Lawvere–Tierney / Grothendieck topologies (proof-only; not in `axiograph_verify` imports).
- [x] Add a concrete, readable explanation doc: `docs/explanation/TOPOS_THEORY.md` (ties `.axi` + PathDB + certificates to the topos view).
- [ ] Make the categorical core explicit and canonical:
  - compile `.axi` schemas into a schema/category IR with objects, subtype inclusions, relation-objects, projection arrows, path equations, and rewrite rules,
  - treat PathDB / RDF / property-graph backends as lowerings from that IR rather than as the ontology kernel.
- [ ] Keep relation-as-object + projection arrows as the canonical internal model (olog-compatible), rather than privileging binary edges.
- [ ] Add an explicit olog authoring surface that lowers into `.axi` / the schema/category IR.
  - The useful target is a review bundle, not only an editor canvas:
    - canonical delta,
    - stable object/arrow ids,
    - provenance/evidence links,
    - CQ attachments,
    - trust metadata.

**Probabilistic / approximate semantics (extension layer; certifiable subsets later)**
- [x] Add untrusted analysis tools to measure “semantic drift” between contexts/snapshots (KL/JS divergence over distributions of relations/types).
- [ ] Define a discrete distribution interface in Lean (finite, fixed-point) and document how to keep probability analytics outside the trusted checker.
- [ ] Explore a future “probability in a topos” track (Giry monad / Markov categories) for semantics-level documentation and later certificate-bounded checks.

**Objective-driven AI / JEPA / SSL (implementation plan)**
- [x] Define a canonical training export (context/target pairs) from full `.axi` modules (schema + theory + instance) anchored to accepted snapshot ids.
- [x] Add a world-model interface (JEPA-style predictor) in Rust with a pluggable backend.
- [x] Add a proposal emitter: world-model outputs -> `proposals.json` (evidence plane) with provenance.
- [x] Implement guardrail costs from constraint checks + rewrite consistency; expose task costs as configurable weights.
- [x] Add a basic MPC-style planner loop (receding horizon) that consumes world-model rollouts + cost module.
  - Runtime surfaces now include `/world_model/plan`, planner reporting, and the rollout+select loop.
- [ ] Improve planner usefulness beyond the basic loop:
  - better policy/cost shaping,
  - stronger review/certification hooks for plan steps,
  - and tighter semantic-VCS integration for accepted/review branches.
- [x] Add evaluation harness: precision/recall on held-out facts, constraint-violation rates, certificate success.
- [x] Add CLI + server endpoints for JEPA/world-model proposal generation (untrusted).
- [x] Thread world-model lineage through typed semantic anchors rather than bare strings:
  - `WorldModelRunId`,
  - `AxiDigest`,
  - `PathdbSnapshotId`,
  - `AcceptedSnapshotId`.
- [x] Added store-backed `/world_model/propose` server e2e coverage:
  - returned proposal metadata carries typed lineage fields,
  - `auto_commit` is admin-gated,
  - proposal metadata stays anchored to the pre-commit snapshot while the server reloads the committed PathDB head.
- [x] Docs: keep `docs/explanation/JEPA_INTEGRATION.md`, `OBJECTIVE_DRIVEN_AI.md`, `SELF_SUPERVISED_LEARNING.md` in sync with implementation.
  - Reference: `docs/reference/WORLD_MODEL_PLUGIN.md` (protocol schema + examples).
  - Tutorial: `docs/tutorials/WORLD_MODEL_LOOP.md` (baseline + transformer stub).
  - CLI (`rust/crates/axiograph-cli`): `axiograph discover jepa-export`, `axiograph ingest world-model`, `axiograph ingest world-model-plugin-llm`.
  - REPL (`rust/crates/axiograph-cli/src/repl.rs`): `wm` subcommand (configure backend + emit proposals + optional WAL commit).
  - Server (`rust/crates/axiograph-cli/src/db_server.rs`): `POST /world_model/propose` (evidence-plane only; optional WAL commit).
- [ ] Keep heuristic AI tooling firmly in the evidence plane:
  - `axiograph-llm-sync` path/reconciliation heuristics remain non-semantic tooling,
  - LLM and world-model outputs never write directly to canonical truth,
  - high-value answers/promotions must still pass typed validation + quality gates + certificates.
- [ ] Add a rejection policy for AI/LLM proposal bundles that fail grounding:
  - block direct review-tooling intake unless proposals include source links, context/snapshot anchors, and at least one typed acceptance path (`validated` draft or `.axi`-schema mapping).
- [ ] Improve AI usefulness through typed grounding rather than looser generation:
  - every proposal should carry source/context/snapshot anchors plus grounded evidence links,
  - proposal clusters should synthesize candidate schema/theory/olog fragments,
  - out-of-schema evidence should produce candidate schema/theory extensions rather than being coerced into the current model,
  - AI-suggested axioms should be previewed as typed `.axi`/IR deltas with CQ and quality impact before review.
- [ ] Add first-class world-model lineage objects:
  - model/plugin version,
  - input accepted snapshot id,
  - training/export digest,
  - evaluation metrics,
  - emitted proposals digest(s),
  - resulting promoted snapshot/tag (if any).
- [x] Thread proposal-set digests into world-model lineage metadata and helpers so proposal streams have a stable typed identity in addition to run/snapshot anchors.
- [x] Persist store-backed world-model run manifests under `sem/world_model_runs/<run_id>.json` for `/world_model/propose` and `wm propose --commit-dir` flows, including typed run/snapshot/proposal anchors and committed PathDB lineage when present.
- [x] Collapse world-model semantic input onto canonical `.axi` plus typed semantic layers:
  - `axi_module_text` is the primary meaning input,
  - snapshot ids are lineage metadata inside `semantic_input`,
  - JEPA/training export and guardrails are optional semantic layers,
  - and old `snapshot` / `export_path`-style first-class plugin fields are removed from the active request contract.
- [ ] Close the remaining world-model lifecycle gaps:
  - REPL `wm propose/plan --commit-dir` should persist the same run-record object,
  - `/llm/agent` auto-commit should attach lifecycle objects instead of writing only overlays,
  - and committed runs should eventually link to semantic commit ids / refs once `sem/commits` is live.
- [ ] Extend persisted world-model run manifests beyond the current phase-0 slice:
  - model/plugin/config digests,
  - evaluation refs / planner outcomes,
  - promotion commit/tag linkage,
  - and semantic-VCS branch refs once `sem/commits` is live.
- [ ] Make the world-model lifecycle explicit in tooling:
  - accepted snapshot anchor → proposal emission → preview validation against cloned snapshot → reconcile into review branch → promote selected deltas → tag new accepted baseline.

**Semantic VCS / ontology lifecycle**
- [ ] Evolve the accepted-plane snapshot store into a first-class semantic VCS, not just `HEAD` + logs.
- [x] Create the initial `sem/` store layout in the accepted-plane directory, including `sem/world_model_runs/` for persisted run manifests.
- Current state (implementation fact):
  - `sem/commits`, `sem/refs`, `sem/validations`, and `sem/world_model_runs` exist in the accepted-plane layer,
  - `refs/heads/main` and gate-summary persistence are real first slices,
  - semantic commits now persist compact `semantic_delta` / `trust_summary` / `rule_summary` / `coverage_summary` sidecars copied from `EvolutionPreviewV1`,
  - semantic commits are still evolving toward fuller explicit state+delta objects with ancestry/policy metadata,
  - and accepted-plane mutators do not yet use one universal semantic-commit / reconciliation flow.
- [ ] Add refs beyond `HEAD`:
  - `refs/heads/main`,
  - `refs/heads/review/<name>`,
  - `refs/heads/evidence/<source>`,
  - `refs/heads/wm/<experiment>`,
  - `refs/tags/<release>`.
- [ ] Add semantic commit objects with explicit parentage / ancestry:
  - author,
  - timestamp,
  - message,
  - policy/reconciliation metadata,
  - pointers to accepted modules, evidence overlays, certificates, and optional world-model runs.
- [ ] Add semantic diffs, not just text/snapshot set diffs:
  - schema diff,
  - theory diff,
  - instance diff,
  - context/world diff,
  - certificate diff.
- [ ] Define merge as reconciliation rather than file concatenation:
  - explicit conflict sets,
  - choose/merge/supersede/retract decisions,
  - optional certificates for policy evaluation / merge results.
- [ ] Track lifecycle states on facts/modules/artifacts:
  - `proposed`,
  - `validated`,
  - `reviewed`,
  - `accepted`,
  - `certified`,
  - `superseded`,
  - `retracted`.
- [ ] Add semantic VCS CLI verbs:
  - `axiograph sem init`
  - `axiograph sem branch <name>`
  - `axiograph sem checkout <ref>`
  - `axiograph sem status`
  - `axiograph sem diff <a> <b> --semantic`
  - `axiograph sem log`
  - `axiograph sem merge <source> --policy <policy>`
  - `axiograph sem tag <name>`
  - `axiograph sem promote`
  - `axiograph sem supersede`
  - `axiograph sem retract`
- [ ] Make world-model / semantic-engineering branches first-class:
  - `wm/<experiment>` for proposal streams,
  - `review/<topic>` for reconciled candidate ontology changes,
  - `main` for accepted ontology/world state,
  - release tags for production/eval/training baselines.
- [ ] Make model evolution useful to ontology engineers, not only auditable:
  - attach semantic diffs and CQ regressions to each evolution step,
  - preview migration and reconciliation impact before merge/promotion,
  - preserve lineage from evidence -> review -> accepted -> superseded/retracted artifacts.
- [ ] Make structural ontology evolution primitives first-class in semantic history and review:
  - `reify_relation_object`,
  - `introduce_dependent_relation_family`,
  - `introduce_subtype`,
  - `generalize_to_supertype`,
  - `specialize_to_subtype`,
  - `push_relation_role_to_subtype`,
  - `pull_relation_role_to_supertype`,
  - `factor_common_structure_to_supertype`,
  - `split_type_into_subtypes`,
  - `merge_types_under_supertype`,
  - `lift_relation_to_carrier`,
  - `add_path_equation`,
  - `add_rewrite_rule`.
  - These should drive directed exploration, typed olog authoring, migration preview, semantic merge/reconciliation, and theory/dependent-family evolution rather than living only in notes or inferred diffs.

### Literature-driven roadmap deltas (Appendix C of `docs/explanation/BOOK.md`)

These items are “best practices” backed by the related work list in Appendix C
(CQL/functorial migration, proof-carrying code, Semantic Web interop, Rust verification tooling).

**Category / migration semantics (CQL / functorial data migration)**
- [x] `Δ_F` (pullback) runtime scaffold (Rust) + functoriality test.
- [ ] `Δ_F` certificate + Lean checker (recompute-and-compare first; tighten later).
- [ ] `Σ_F` (left Kan extension) runtime scaffold (Rust) + certificate/checker.
- [ ] `Π_F` (right Kan extension) runtime scaffold (Rust) + certificate/checker.
- [ ] Natural transformations between schema functors (2-cells) as first-class (for “composition up to iso”).
- [ ] Relations as edge-objects + projection arrows in the core schema semantics (so migration treats relations uniformly).

**Rewrite/groupoid tightening (HoTT/groupoids/rewrite literature + mathlib)**
- [x] Normalization certificate with optional explicit rewrite derivation replay.
- [ ] Prove rewrite-step soundness against the mathlib free-groupoid denotation (move from “recompute” to “sound-by-theory”).
- [x] First-class rewrite rules in canonical `.axi` theories (structured `rewrite ...` blocks), imported/exported via the PathDB meta-plane.
- [x] `rewrite_derivation_v3`: derivations can reference either builtin rules or `.axi` rules by `(axi_digest_v1, theory, rule)` (anchor-scoped).

**Interop layering (Semantic Web / SHACL / provenance)**
- [x] RDF import boundary using Sophia (TriG/N-Quads/Turtle + RDF/XML), mapping named graphs → `Context`/world scopes (adapter emits `Context` entities and `context`-scoped relation proposals).
- [x] Public dataset fixtures + demos for RDF/OWL/SHACL ingestion:
  - committed: `examples/rdfowl/w3c_shacl_minimal/`
  - optional fetch: `scripts/fetch_public_rdfowl_datasets.sh` (W3C data-shapes + LUBM)
  - demo: `scripts/rdfowl_public_datasets_demo.sh`
- [ ] Lower RDF/OWL/SHACL and property-graph adapters through the same canonical schema/category IR rather than letting any interop format become the kernel.
- [ ] RDF-star / statement-level metadata support in the adapter layer (quoted triples → fact/evidence/provenance objects) so we can attach attribution to individual claims.
- [ ] OWL/RDFS mapping layer (`TBox` → `.axi` schema+theory, `ABox` → `.axi` instance) with an explicit “unknown ≠ false” story (no silent closed-world assumptions).
- [ ] SPARQL SELECT subset as an interop dialect (compile into AxQL IR; keep SPARQL as an untrusted boundary language).
- [ ] SHACL validation as a promotion/ingestion gate (prefer best-in-class Rust libs like `oxirs-shacl` or `rudof`), emitting:
  - a stored validation report (evidence plane), and
  - optional `.axi` constraint proposals derived from shapes (reviewable before acceptance).
- [ ] SHACL subset → certificate-checked ingestion step (“raw → validated”), keeping “unknown” explicit (start with core constraints that map to our `.axi` constraint vocabulary).
- [ ] Dataset/named-graph validation support (SHACL-DS-style semantics) for multi-context snapshots.
- [ ] Shape learning / constraint discovery stage (heuristics + optional LLM assist) to propose shapes/constraints from data, then reconcile/promote into the accepted plane.
- [ ] PROV-inspired provenance conventions for contexts (source/authority/time/policy/conversation), plus docs/examples that emphasize:
  - “certificate-checked” ≠ “true”, and
  - no implicit unique-name assumption (identity must be asserted/reconciled).
- [ ] Treat property-graph compatibility as a projection layer:
  - binary relations may emit direct edges where appropriate,
  - n-ary relations emit fact nodes / relationship entities,
  - keep the olog / relation-object model canonical internally.
- [ ] Optional tooling integration for ontology-engineering UX: `rudof` conversions (SHACL/ShEx/DCTAP) + useful visualization outputs.
- [ ] Certificate kind(s) for “validated import”: anchor the canonical `.axi` digest + (optional) adapter inputs, then prove the adapter produced the claimed `.axi` snapshot/module.

**Rust hardening (RustBelt/Oxide + verification ecosystem)**
- [ ] Fuzz untrusted surfaces (PathDB bytes, certificate JSON, `.axi` parsing, FFI entrypoints).
- [ ] Run Miri on core crates in CI/dev loops (UB detection for tests).
- [ ] Add Kani harnesses for small critical kernels (fixed-point arithmetic, bounds-checked parsing, “no panic” invariants).
- [ ] Add Loom/Shuttle tests once we introduce concurrency kernels (async ingestion, background indexing).
- [ ] Consider Aeneas later for *tiny* byte→AST→certificate kernels (not the whole runtime).

**Semantics-relevant test suite**
- [x] Focused target: `make verify-semantics` (runs Rust+Lean semantics checks without requiring full workspace compilation).
- [x] CI workflow runs `make rust-test` + `make verify-semantics` (Lean installed via elan).
- [x] CI validates Helm render + raw k8s manifests with `kubeconform`.
- [x] Added deep Rust type/semantic workflow tests for the new typed seams:
  - compile-fail doctests for anchor misuse and invalid lifecycle-state usage,
  - subtype-aware module typecheck tests,
  - validated vs reviewed importer parity tests,
  - kernel IR negative-policy and subtype-axis tests,
  - AST-vs-meta compiled semantics parity tests,
  - checked-db rewrite typing against compiled carrier semantics,
  - store-backed world-model lineage / auto-commit server e2e tests.
- [ ] Keep expanding `verify-semantics` as the “must pass” suite while other crates are mid-migration.

**CI/CD + deployment**
- [x] Release workflow builds multi-OS binaries and publishes GitHub release assets.
- [x] GHCR container build/push with multi-arch image.
- [x] Dockerfile defaults to `axiograph db serve` for `/viz` + `/query`.
- [x] K8s manifests + Helm chart for a StatefulSet + PVC-backed storage.
- [x] CI builds the Dockerfile (no push) to catch container regressions.
- [x] Add smoke tests for container startup + `/status` in CI.
- [x] Provide chart values profiles for replicas/read-only fan-out and ingress examples.

**Indexes / caches (PathDB)**
- [x] Durable index sidecar (`.axpd.idx.cbor`) for fact/text caches + path LRU, keyed by snapshot id.
- [x] `db serve` loads sidecar on startup (direct `.axpd` or store checkpoint) and persists updates via a debounced background writer.
- [x] FactIndex/TextIndex build asynchronously when an async source is attached; queries fall back to scans until ready; invalidated on mutation.
- [x] PathIndex LRU updates on a dedicated worker thread; query threads only enqueue updates; LRU persisted in sidecar; clears on mutation.
- [x] Perf harness: `axiograph tools perf indexes` + `scripts/perf_index_caches.sh` (profiles cache/index behavior, optional verify/mutation checks, multi-scale growth/shrink runs).

**AxQL performance (planner/runtime)**
- [x] Keep simple path chains as RPQ and route to PathIndex when beneficial; add fast single-path execution.
- [x] Add selectivity heuristics (relation-type counts + candidate sizes) and atom ordering; reduce RPQ clone overhead.
- [x] Add shared prepared-query cache keyed by snapshot + query IR (REPL uses).
- [ ] Add a perf harness for cache wins (repeat queries) and report hit/miss + timing deltas.
- [ ] Calibrate `PATH_INDEX_MIN_LEN` by workload; consider adaptive switching based on relation fan-out.
- [ ] Extend cache usage to `db serve` and non-REPL query paths where safe; keep LRU bound configurable.
- [ ] Add per-relation degree stats (avg/median/out-degree histograms) to improve selectivity estimates.

**REPL ergonomics (discovery-first UX)**
- [x] Add REPL discovery primitives:
  - `describe <entity>` (attrs + contexts + equivalences + grouped in/out edges),
  - `open chunk|doc|evidence|entity ...` (evidence navigation),
  - `diff ctx ...` (context/world diffs),
  - `neigh ...` (REPL-driven viz export).
- [x] Add `q --explain` plan output (join order + candidate domains + FactIndex hints).
- [x] Add a typed JSON query IR for tooling/LLMs (`query_ir_v1`) that compiles into the same AxQL core (REPL `llm query` prints this; the LLM tool-loop, DB server, and LLM/plugin query-generation surfaces use it as the machine-facing contract; raw AxQL is now a human/debug surface only).
- [x] Add schema-qualified AxQL for multi-schema “one universe” snapshots:
  - `?x is Fam.Person` / `?x -Fam.Parent-> ?y` / `?f = Fam.Parent(child=..., parent=...)`,
  - ambiguous unqualified edge labels elaborate to either a chosen schema (when inferred) or a union alternation.
- [x] Add first-class disjunction (`or`) in AxQL with a certifiable subset:
  - execution: UCQ semantics (union of conjunctive branches),
  - certificates: `query_result_v3` (Lean checks each row against the chosen branch under the canonical `.axi` anchor).
- [x] Add an LLM “tool loop” (lookup + elaborate + run + propose) so `llm ask`/`llm answer` are multi-step and models don’t emit raw AxQL by default.
- [ ] Make REPL and AxQL completion position-aware and schema-aware:
  - complete valid schema-qualified symbols, tuple roles, contexts, and rewrite-rule names from the compiled IR.
- [ ] Make typecheck/elaboration output a default UX surface:
  - inferred types,
  - ambiguity resolution,
  - rewrite/normalization explanation,
  - and suggested fixes should be shown routinely, not only in debug-oriented commands.
- [ ] Add fix-oriented diagnostics for authoring and proposal repair:
  - missing role/field suggestions,
  - schema-qualification suggestions,
  - canonical-constraint rewrite suggestions,
  - context/parameter-carrier repair suggestions.

**Ontology exploration / visualization (tooling)**
- [ ] Extend the self-contained HTML explorer (`axiograph tools viz --format html`) with ontology-explorer-style ergonomics:
  - node/edge export (CSV/JSON) for external tools (Gephi),
  - basic graph metrics table (degree, in/out degree; optional betweenness),
  - quick “compare” views for snapshots/contexts (diff/union/intersection overlays).
- [x] Add plane-aware filters in the HTML explorer (accepted / evidence / data) to support ontology-engineering loops over multiple planes.
- [x] Make confidence + evolution exploration easier in the viz UI:
  - confidence slider (hide/attenuate low-confidence edges),
  - time-travel snapshot selector when served via `axiograph db serve` (`GET /snapshots`, `GET /viz?...&snapshot=<id>`).
- [x] Add certified exploration hooks in `/viz` (server mode):
  - AxQL query panel (`run`, `certify`, `certify+verify`) calling `POST /query` and highlighting result ids,
  - path certification (`Certify path`, `Verify path`) calling `POST /cert/reachability` (directed relation-only paths).
- [x] Add UI-driven evidence-plane mutation in `/viz` (server mode):
  - generate a reviewable `proposals.json` overlay (and optional `DocChunk` evidence),
  - commit it to the PathDB WAL via the master-only admin endpoint, producing a new PathDB snapshot id.
- [x] Make “add data” proposals schema-aware + validated:
  - infer `(axi_schema, axi_source_field, axi_target_field)` from the meta-plane (when available),
  - default common required fields (`ctx`, `time`) deterministically,
  - import relation proposals as typed tuple facts (field edges + `axi_fact_of`) instead of opaque edges,
  - support n-ary facts via `extra_fields` and the LLM tool `propose_fact_proposals` (field map),
  - treat `axi_fact_in_context` as uniform scoping metadata (even when a relation signature lacks a `ctx` field),
  - return a preview validation report (axi typecheck + delta quality findings) from `/proposals/relation(s)` and LLM proposal tools.
- [x] Speed up PathDB WAL replay by caching derived CBOR sidecars for `chunks.json` and `proposals.json` blobs (JSON remains the human-readable source of record).
- [x] Add a network analysis CLI surface (untrusted tooling; evidence-plane friendly) that can emit JSON/text summaries:
  - connected components (weak/strong), “giant component” ratio,
  - degree distribution + top hubs/authorities (PageRank),
  - bridge detection (approx betweenness), and
  - community detection (Louvain; Leiden later) for “topic islands” in discovered graphs.
- [ ] Wire network-analysis results into viz as optional overlays (color by community/centrality; facet/filter by component/context).
- [ ] Add a snapshot diff + “set ops” CLI surface (graph union/intersection/difference) over `.axpd` and/or accepted-plane snapshots.
- [ ] Add “schema-first” exploration modes: tree views for object/subtype/relation signatures, plus theory panels (constraints + rewrite rules).
- [ ] Make contexts/worlds first-class in viz/query UX (facets/filters, per-context overlays, provenance display).
- [ ] Add a “competency questions” runner: executable query suites attached to examples + CI checks (ontology engineering best practice).
- [ ] Render ontology structure as a typed program in viz / explorer surfaces:
  - relation tuples with named roles,
  - subtype lattice,
  - carrier/fiber structure,
  - lifecycle/anchor state,
  - and certificate availability.
- [ ] Add migration preview UX that surfaces semantic impact:
  - renamed objects/arrows,
  - dropped information,
  - CQ/query rewrites,
  - certificate availability,
  - and explicit review checkpoints before promotion.
- [ ] Use accepted-plane promotion as the next concrete CQ-gated evolution seam:
  - compare the current accepted snapshot with the would-be accepted snapshot,
  - persist the resulting preview under `sem/validations/`,
  - and fail closed on CQ regressions before promotion mutates accepted state.

**Web/GitHub ingestion (tooling; evidence plane)**
- [x] GitHub importer: `axiograph ingest github import` (repo index + optional proto ingest) emits merged `chunks.json` + `proposals.json`.
- [x] Web importer: `axiograph ingest web ingest` (URL list or crawl) emits `manifest.jsonl` + `chunks.json` + `facts.json` + `proposals.json` (rate limits + robots.txt + size caps).
- [x] Large-ish scrape demos (networked scripts):
  - `scripts/web_mixed_sources_demo.sh`
  - `scripts/web_wikipedia_crawl_demo.sh`
- [x] Offline “all source types” ontology-engineering demo:
  - `scripts/ontology_engineering_all_sources_offline_demo.sh`

**Quality checks (ontology + data)**
- [x] Add a first-class `axiograph check quality` command family that produces a structured report (JSON + human-readable).
  Initial checks:
  - meta-plane lint: detect “missing meta-plane” and subtyping cycles (warning),
  - data-plane lint: dangling references (error),
  - schema constraints (best-effort): key/functional violations (error),
  - context scoping coverage (info, strict profile only).
- [x] Add a surgical `.axi` formatter for constraint lines:
  - `axiograph check fmt --write <file.axi>` canonicalizes `constraint ...` syntax while preserving comments and non-constraint lines verbatim.
- [ ] Expand quality checks to cover more ontology/data hygiene:
  - unused/near-duplicate symbols, ambiguous n-ary field naming,
  - symmetric/transitive closure checks (as info/warn, not “truth”),
  - rewrite-rule lint (unreachable rules, non-terminating orientations heuristics).
- [x] Make `axiograph db accept promote` optionally run a quality profile (fast/strict) and attach the report to the accepted-plane commit metadata.
- [x] Make accepted-plane promotion fail-closed on unknown constraints and require the certifiable core constraint gate:
  - reject `ConstraintV1.unknown` in canonical modules,
  - require `axi_constraints_ok_v1` and store it in the accepted-plane log + `certs/`.
- [x] Add a certifiable subset of quality gates:
  - “well-typed module” (already exists),
  - “constraint-satisfied for core constraints” (`axi_constraints_ok_v1`: key/functional + symmetry (incl. guarded) + transitivity (closure-compatibility) + builtin typing rules), and
  - “rewrite derivation validates” (already exists; extend to rule-set soundness later).
  - closure carrier fields: support optional `... on (field0, field1)` for `symmetric`/`transitive` constraints (Rust+Lean + cert check).
  - closure parameter fields: support optional `... param (field0, field1, ...)` to interpret symmetry/transitivity as fibered closure (e.g. `ctx/time`), and allow key constraints to mention carriers + params.

**Docs system (Diataxis)**
- [ ] Re-organize docs into Diataxis quadrants (Tutorials / How-to / Reference / Explanation), keeping existing content but improving navigation.
- [ ] Add “Start here” tutorial flows for: `.axi` authoring, accepted-plane promotion, PathDB snapshots, AxQL querying, certificates + Lean verification, and ontology-engineering loops.
