## Axiograph — Agent Notes (where we are so far)

This file is a shared context note for humans/agents working in this repo. It captures the current technical reality and the direction we’ve committed to.

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
- The next high-value move remains a canonical schema/category IR plus APIs indexed by lifecycle state, anchor, and schema.
- PathDB is a strong engine, but it is not yet the ontology kernel.
  - The intended kernel is accepted `.axi` plus compiled schema/category IR.
  - Keep relation-as-object + projection arrows canonical internally; treat RDF / LPG / quad stores as lowerings.
- Important mismatch still open:
  - `axiograph-pathdb` contains a sectioned “verified” `.axpd` v2 story in `verified.rs`,
  - but the live runtime still serializes/deserializes a simpler v1 blob in `lib.rs`.
- Practical framing to keep using:
  - “Axiograph has a Lean-checked core for typed rewrite/path witnesses and ontology gates, with a roadmap toward categorical ontology semantics.”
  - Do not describe the current system as a full HoTT/topos ontology engine.

### Active priorities (2026-04)

- Keep docs and agent notes aligned with the actual trusted kernel boundary; avoid present-tense claims for roadmap-only HoTT/topos/query semantics.
- Make the compiler/typechecker a first-class ontology service:
  - compiled schema/category IR,
  - prepared/typechecked query handles,
  - elaboration/type info/rewrite traces,
  - certifiability classification (`certifiable`, `execution-only`, `mixed`),
  - repair-oriented diagnostics.
- Make typed ontology authoring/exploration useful to operators:
  - schema-aware completion,
  - competency-question regression and gap discovery,
  - semantic diff / migration preview,
  - context/world/provenance visibility everywhere,
  - olog-fragment and candidate-constraint synthesis from grounded evidence.
- Add high-priority usefulness backlog:
  - [ ] add a typed olog authoring/edit surface that emits reviewable `.axi` deltas with explicit anchors and trusted provenance fields,
  - [ ] add CQ-driven migration previews and CQ regression gating for schema proposals,
  - [ ] require all proposal previews and suggestion bundles to carry `AxiDigest`, source/snapshot/context anchors, and trust-class labels,
  - [ ] expose a single trust contract in API responses (`soundness`, `coverage`, `scope`, `trust class`) across query/certify/migrate paths,
  - [ ] add a concrete backlog item for proof-carrying migration witnesses (not only path/query certificates) before broader CQL migration rollout.
- Keep AI and world-model tooling evidence-plane only, but make lineage first-class:
  - typed run/proposal/snapshot anchors,
  - proposal digests,
  - grounded evidence links,
  - previewable typed deltas before review/promotion.
- Converge the `.axpd` production format and the “verified” format story, and keep verification claims scoped to the real live bytes until that lands.

### Migration constraints (non-negotiable)

- **No feature loss**: all existing functionality remains available during the transition.
- **Examples keep working**: `axiograph_v6/examples/` and the Rust+Lean test suites must continue to pass.

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
- [x] Cert v1 (JSON): reachability witness + Lean checker.
- [x] Cert v2 (JSON): fixed-point reachability witness (`rel_confidence_fp`) + Lean checker.
- [x] Cert v2: reconciliation decision (`resolution_v2`) + Lean checker.
- [x] Cert v2: normalization (`normalize_path_v2`) + Lean checker.
- [x] Cert v2: path equivalence (`path_equiv_v2`) + Lean checker (shared normal form + optional derivations).
- [x] Extend `normalize_path_v2` to groupoid rewrite derivations (assoc/inv/cancel + explicit step list).
- [ ] Extend cert v2 rewrite derivations beyond normalization (reconciliation proofs, domain rewrites).
- [ ] Anchor certificates to canonical `.axi` inputs (stable module hash + extracted facts).
- [x] Keep backward compatibility: accept v1 certificates during transition.
  - Spec notes: `docs/reference/CERTIFICATES.md`
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
- [x] DB server wrapper: `axiograph db serve` (read-only replica + optional write master) serving `/query` (AxQL) + `/status` + admin endpoints.
- [x] DB server viz endpoints: `GET /viz` (HTML), `GET /viz.json` (graph JSON), `GET /viz.dot` + `refresh_secs=N` for live-ish auto-refresh.
- [x] DB server certificate endpoints:
  - `GET /anchor.axi` (export PathDBExportV1 anchor),
  - `POST /cert/reachability` (reachability cert from relation-id chains),
  - `/query` supports `certify/verify/include_anchor` and optional default `contexts`.
- [x] DB server can optionally verify certificates server-side:
  - verifier discovery: `--verify-bin`, `AXIOGRAPH_VERIFY_BIN`, `bin/axiograph_verify`, repo dev fallback,
  - exposed in `/status.certificates.*`.
- [x] DB server can enforce a “fail closed” certificate gate for high-value answers:
  - `/llm/agent` supports `require_query_certs` / `require_verified_queries`,
  - `/viz` LLM tab exposes “require verified query certificates (fail closed)”.
- [x] DB server demos (scripts): `scripts/db_server_api_demo.sh`, `scripts/db_server_distributed_demo.sh`, `scripts/db_server_live_viz_demo.sh`.
- [x] Rust query certification emits v2 reachability/confidence certificates for the current certifiable query subset (`/query certify`, AxQL query-result certificates, anchored reachability witnesses).
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
- [x] `db serve /query` now accepts structured `query_ir_v1` in addition to raw AxQL, and can echo canonical compiled query IR alongside elaboration output.
- [ ] Keep the structured query/compiler surface first-class across all entrypoints:
  - `query_ir_v1` should be accepted by REPL/tooling/server APIs, not only by the LLM loop,
  - elaboration/typecheck results should be available as API data, not only REPL text,
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
- [ ] Make type-driven ontology usefulness explicit in user tooling:
  - schema-first exploration,
  - olog-fragment synthesis,
  - candidate constraint / path-equation / rewrite-rule discovery,
  - and previewable `.axi` deltas before promotion.
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
- [x] Add a typed JSON query IR for tooling/LLMs (`query_ir_v1`) that compiles into the same AxQL core (REPL `llm query` prints this; the LLM tool-loop emits it; raw AxQL is fallback only).
- [x] Add schema-qualified AxQL for multi-schema “one universe” snapshots:
  - `?x is Fam.Person` / `?x -Fam.Parent-> ?y` / `?f = Fam.Parent(child=..., parent=...)`,
  - ambiguous unqualified edge labels elaborate to either a chosen schema (when inferred) or a union alternation.
- [x] Add first-class disjunction (`or`) in AxQL with a certifiable subset:
  - execution: UCQ semantics (union of conjunctive branches),
  - certificates: `query_result_v2` (Lean checks each row against the chosen branch).
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
