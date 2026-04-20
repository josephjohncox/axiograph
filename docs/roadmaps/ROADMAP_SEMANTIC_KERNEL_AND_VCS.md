# Semantic Kernel + Semantic VCS Roadmap

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap turns the current review conclusions into a concrete program of work.

Companion target specs:

- `docs/reference/TRUSTED_KERNEL.md`
- `docs/reference/KERNEL_IR.md`
- `docs/reference/RUST_LIFECYCLE_TYPES.md`
- `docs/reference/SEMANTIC_VCS.md`

It assumes the following framing:

- Axiograph today is a **proof-carrying ontology workbench**, not yet a full categorical ontology kernel.
- The trusted core lives in **Lean** and is currently strongest on:
  - typed path expressions,
  - free-groupoid denotation,
  - fixed-point probabilities,
  - anchored certificate replay.
- **Rust** should become the place where lifecycle state, schema scoping, snapshot anchoring, and typed execution are made operational and difficult to bypass.
- The accepted plane should evolve into a **semantic VCS**, not just a snapshot store with `HEAD`.

---

## 0. Program goals

By the end of this program, Axiograph should have:

1. A small, explicit, honest **Lean trusted kernel**.
2. A canonical **schema/category IR** that becomes the ontology kernel.
3. A first-class Rust workflow for **lifecycle-typed, anchor-aware artifacts**.
4. A real **semantic VCS** for ontology / semantic / world-model evolution.
5. Clean **interop layers** for RDF/OWL/SHACL/property-graph systems.
6. A disciplined **evidence plane** for AI/world-model/LLM tooling.

---

## 1. Non-negotiable framing corrections

- [ ] Keep external and internal documentation precise:
  - do not market the current Lean layer as a full HoTT ontology engine,
  - do not market Rust guardrails as a dependently typed kernel,
  - do not treat PathDB as the ontology kernel.
- [ ] Standard framing to use:
  - “Lean-checked typed rewrite/path witnesses and ontology gates, with a roadmap toward categorical ontology semantics.”
- [ ] Standard anti-framing to avoid:
  - “already a dependently typed ontology engine”
  - “already a topos-theoretic kernel”
  - “PathDB is the meaning layer”

---

## 2. Workstream A: Lean trusted kernel

### Goal

Make the trusted kernel small, stable, and explicit.

### Deliverables

- [ ] Define the boundary of the trusted kernel in docs and code:
  - what is checked,
  - what is merely recomputed,
  - what is explanatory only.
- [ ] Add a dedicated `docs/reference/TRUSTED_KERNEL.md` with a table:
  - module,
  - role,
  - trust class,
  - certificate kinds served,
  - theorem-backed vs replay-only vs recompute-only status.
- [ ] Keep `HoTT.Core` minimal and isolate any axioms that are not needed by shipped certificates.
- [ ] Continue strengthening:
  - typed path/groupoid semantics,
  - rewrite soundness,
  - fixed-point probability,
  - anchored certificates.
- [ ] Add certificate kinds only when their semantics can be clearly stated and replayed in Lean.
- [ ] Ensure certificate docs remain explicit about **soundness vs completeness**.
- [ ] Split Lean packaging into:
  - an explicit executable/runtime kernel target, and
  - a broader theory/explanation target.
- [ ] Add a CI audit that checks the import closure of `axiograph_verify` remains free of non-core axioms.
- [ ] Add a checked `.axi` rewrite-rule layer in Lean:
  - validate declared vars,
  - endpoint preservation,
  - schema scoping,
  - sort-correctness for accepted rewrite rules,
  - and make `rewrite_derivation_v3` consume these compiled checked rules instead of raw parsed rules.

### Out of scope for the trusted kernel

- full query planning
- PathDB binary format internals
- heuristic reconciliation
- AI scoring / confidence heuristics
- general RDF/OWL entailment engines

---

## 3. Workstream B: Canonical schema/category IR

### Goal

Make the ontology kernel explicit and independent of any one execution backend.

### Canonical model

- [x] Land the first kernel-IR slice in `axiograph-pathdb`:
  - `CompiledSchemaIr`,
  - `RelationSemanticsIr`,
  - `RoleKind::{Data, Context, Temporal}`,
  - `CarrierSpecIr`,
  - `WitnessViewIr`,
  - with `.axi` import/meta-plane code consulting compiled relation semantics
    for carrier/witness selection.
- [ ] Compile `.axi` schemas into a first-class schema/category IR with:
  - object types,
  - subtype inclusions,
  - relation-objects,
  - ordered roles,
  - field/projection arrows,
  - optional explicit arrows,
  - path equations,
  - rewrite rules,
  - context/world annotations.
- [ ] Keep **relation-as-object + projection arrows** canonical.
- [ ] Treat binary edges as a projection/optimization, not the semantic primitive.
- [ ] Add an olog-oriented authoring surface that lowers into the same IR.
- [ ] Define the canonical IR in three layers:
  - `SchemaCoreIR` for ontology meaning,
  - `TraversalView` for execution-friendly binary projections and query elaboration,
  - `InstanceIR` for tuple/fact carriers plus total projection functions.
- [ ] Preserve modality-bearing source structure from `.axi` instead of erasing it during lowering:
  - keep `@context` axes explicit,
  - keep `@temporal` axes explicit,
  - avoid collapsing these into ad hoc `ctx` / `time` fields with lossy re-export.
- [ ] Make role semantics explicit in the IR. Each relation role should record whether it is:
  - `Data`
  - `Context(axis)`
  - `Temporal(axis)`
  - `Parameter`
  - `Evidence`
- [ ] Introduce an explicit carrier specification for relations/facts:
  - represent which roles are carriers,
  - which are parameters/fibers,
  - and which participate in closure/equational reasoning.
- [ ] Separate theory content into a distinct `TheoryIR`:
  - certifiable constraints,
  - path equations,
  - rewrite rules,
  - opaque/external equations kept outside the certifiable kernel.
- [ ] Represent equations in two classes:
  - `PathEquation` for equations the Lean/path kernel can interpret,
  - `OpaqueEquation` for reviewed but non-kernel semantic guidance.
- [ ] Introduce concrete kernel IR modules:
  - `rust/crates/axiograph-dsl/src/schema_category_ir.rs`
  - `rust/crates/axiograph-dsl/src/instance_ir.rs`
- [ ] Consider promoting the kernel IR into a dedicated crate if reuse pressure increases:
  - `rust/crates/axiograph-kernel-ir/`
- [ ] Reuse / absorb the current migration-side category scaffold instead of inventing a second parallel IR:
  - the new canonical IR should subsume the useful parts of `rust/crates/axiograph-pathdb/src/migration.rs`.
- [ ] Make migrations and query elaboration consume the kernel IR first and only then lower to convenience projections.

### Why this matters

- It unifies:
  - Lean semantics,
  - PathDB fact-node storage,
  - RDF edge-object export,
  - SHACL-like validation,
  - property-graph projections,
  - future `Δ/Σ/Π` migrations.
- It also prevents current semantic leakage where:
  - context/temporal structure is erased too early,
  - traversal semantics depend on endpoint heuristics,
  - and relation-objects are treated inconsistently across storage, migration, and interop.

---

## 4. Workstream C: Rust lifecycle typing and anchor-aware APIs

### Goal

Make the semantic state machine explicit in the Rust layer.

### Artifact states

- [x] Introduce universal lifecycle states:
  - `Parsed`
  - `Validated`
  - `Reviewed`
  - `Accepted`
  - `Certified`

### Anchor model

- [x] Distinguish process-local identity from persistent identity:
  - keep `DbToken` for in-memory misuse prevention,
  - add stable anchors such as:
    - `AcceptedSnapshotId`
    - `AxiDigest`
    - `PathdbSnapshotId`
    - `ProposalDigest`
    - `WorldModelRunId`
    - `SchemaId`
    - `TheoryId`
    - `ContextId`
- [x] Implement anchor/lifecycle modules directly in `axiograph-pathdb`:
  - `rust/crates/axiograph-pathdb/src/anchor.rs`
  - `rust/crates/axiograph-pathdb/src/lifecycle.rs`

### First-class Rust artifact types

- [x] Introduce the first lifecycle-typed public artifact surface for canonical modules:
  - `axiograph_pathdb::axi_module_typecheck::Module<Validated>` / `Module<Reviewed>`
  - plus `ReviewStamp`.
- [ ] Introduce typed public surfaces for:
  - `Module<S>`
  - `ProposalSet<S, A>`
  - `Snapshot<S, A>`
  - `Query<S, A>`
  - `Answer<S, A>`
  - `FactId<A>`
  - `TypedFact<S, R, A>`
  - `WorldState<A>`
  - `WorldModelRun<A>`
  - `CertifiedAnswer<A>`
- [ ] Use a two-part identity model in the core APIs:
  - persistent identity via stable anchor newtypes,
  - live identity via `DbBranded<u32>`,
  - and carry both where available.
- [ ] Treat `PathdbSnapshotId` as an operational/storage anchor, not the semantic anchor; query and certificate APIs should be indexed by accepted-plane meaning anchors.

### Transition discipline

- [x] Enforce the `typecheck_axi -> Module<Validated>` and
  `review_module -> Module<Reviewed>` transitions at the canonical `.axi`
  import boundary; `axi_module_import` does not accept raw parsed modules.
- [ ] Enforce state transitions through constructors only:
  - `parse_axi -> Module<Parsed>`
  - `typecheck_axi -> Module<Validated>`
  - `review_module -> Module<Reviewed>`
  - `promote -> Snapshot<Accepted, AcceptedSnapshotId>`
  - `emit_certificate -> Answer<Validated, AcceptedSnapshotId>`
  - `verify_certificate -> Answer<Certified, AcceptedSnapshotId>`

### API shift

- [ ] Make schema-scoped / snapshot-scoped wrappers the default public API for core execution.
- [ ] Gradually demote raw `u32` / stringly APIs to adapter/internal-only surfaces.
- [ ] Make typed execution operate over the schema/category IR rather than only over stringly meta-plane indexes.
- [ ] Promote `AxiTypedEntity` / `AxiTypedFact` into anchor-aware handles rather than schema-scoped wrappers over raw ids only.
- [ ] Make builders return typed handles:
  - `CheckedDbMut::fact_builder(...).commit() -> TypedFact<..., A>`
  - not raw `u32`.
- [ ] Wrap public query execution in typed surfaces:
  - `Query<S, A>`
  - `Answer<S, A>`
  - `CertifiedAnswer<A>`
  while keeping current CLI/HTTP JSON forms as adapters.

---

## 5. Workstream D: Storage and anchors

### Goal

Align storage claims with actual runtime artifacts.

### Tasks

- [ ] Resolve the `.axpd` story:
  - either adopt the sectioned verified binary v2 as the production format,
  - or scope the verified format as experimental and keep verification claims targeted at actual live bytes.
- [ ] Prefer the pragmatic branch first:
  - keep `.axpd` v1 as the only production format until v2 is actually used end-to-end by serializers, checkpoints, sync, and server loading,
  - mark the sectioned v2 format in `verified.rs` as experimental design work until then.
- [ ] Add tests and fixtures over the **actual** production checkpoint format.
- [ ] Add production-format contract tests:
  - golden `.axpd` fixtures produced by accepted-plane / WAL flows,
  - checkpoint compatibility tests for `pathdb/checkpoints/*.axpd`,
  - a regression test proving the runtime rejects experimental v2 bytes until migration is real.
- [ ] Keep PathDB as a derived execution substrate; do not let its current shape define ontology semantics.
- [ ] Keep accepted `.axi` + schema/category IR as the meaning plane.
- [ ] Treat `axiograph-storage` as non-foundational until prototype shortcuts are removed.
- [ ] Split storage identity into two layers:
  - semantic anchor = accepted snapshot + canonical module digests + canonical fact ids,
  - runtime materialization id = accepted snapshot + overlay digests + build parameters + checkpoint/sidecar digests.
- [ ] Treat `PathDBExportV1` as a debug/interchange anchor, not the primary semantic truth anchor.
- [ ] Keep WAL overlays explicitly outside the semantic kernel:
  - queries may use chunks/proposals/embeddings for retrieval and explanation,
  - but they should not be “certified” unless the relevant facts were promoted into accepted `.axi`.
- [ ] Change store-backed certification to prefer canonical accepted-plane anchors:
  - for accepted-plane serving, certify from accepted module text / canonical anchor material,
  - for pathdb-layer serving, fail closed when a requested certified answer depends on overlay-only facts,
  - keep snapshot-export certification as a legacy/debug path rather than the default.
- [ ] Start plumbing module digests into runtime metadata and canonical fact material alongside `axi_fact_id`.

---

## 6. Workstream E: Semantic VCS

### Goal

Turn accepted-plane snapshots + WAL into a first-class semantic version control system.

### Required concepts

- [ ] Refs, not just `HEAD`:
  - `refs/heads/main`
  - `refs/heads/review/<name>`
  - `refs/heads/evidence/<source>`
  - `refs/heads/wm/<experiment>`
  - `refs/tags/<release>`
- [ ] Semantic commits with:
  - parents,
  - author,
  - timestamp,
  - message,
  - policy,
  - pointers to accepted modules, evidence overlays, certificates, and world-model runs.
- [ ] Add a dedicated semantic-history store under the accepted plane:
  - `sem/HEAD`
  - `sem/refs/heads/*`
  - `sem/refs/tags/*`
  - `sem/commits/`
  - `sem/reconciliations/`
  - `sem/world_model_runs/`
  - optional `sem/validations/`
- [ ] Make semantic commits point at materialized state rather than duplicating it:
  - `accepted_snapshot_id_before`
  - `accepted_snapshot_id_after`
  - `accepted_tree_digest`
  - `pathdb_snapshot_id_before`
  - `pathdb_snapshot_id_after`
  - `evidence_digest`
  because current snapshot ids are history-sensitive commit-like ids rather than pure state/tree hashes.
- [ ] Persist each `WorldModelRunId` once under `sem/world_model_runs/<run_id>.json` and require all wm-run commit references to resolve there.
- [ ] Semantic diff:
  - schema diff
  - theory diff
  - instance diff
  - context/world diff
  - certificate diff
- [ ] Merge as reconciliation:
  - explicit conflict sets,
  - choose/merge/supersede/retract decisions,
  - optional certificates for merge/policy results.
- [ ] Add a first-class `SemReconciliationV1` object:
  - base/left/right commit ids
  - conflict set
  - chosen decisions
  - reviewer/policy
  - optional certificate refs
  - resulting lifecycle transitions
- [ ] Lifecycle states tracked on facts/modules/artifacts:
  - `proposed`
  - `validated`
  - `reviewed`
  - `accepted`
  - `certified`
  - `superseded`
  - `retracted`
- [ ] Treat `refs/heads/wm/<experiment>` as immutable lifecycle branch roots:
  - create run by writing `WorldModelRunV1` first,
  - advance branch with semantic commits that include `branch_ref`/`run_id` provenance,
  - reject commit generation for wm branches without persisted run lineage.

### Proposed CLI surface

- [ ] `axiograph sem init`
- [ ] `axiograph sem branch <name>`
- [ ] `axiograph sem checkout <ref>`
- [ ] `axiograph sem status`
- [ ] `axiograph sem diff <a> <b> --semantic`
- [ ] `axiograph sem log`
- [ ] `axiograph sem merge <source> --policy <policy>`
- [ ] `axiograph sem tag <name>`
- [ ] `axiograph sem promote`
- [ ] `axiograph sem supersede`
- [ ] `axiograph sem retract`
- [ ] Make the first shipping slice:
  - [ ] `sem init`
  - [ ] `sem status`
  - [ ] `sem log`
  - [ ] `sem show`
  - [ ] `sem branch`
  - [ ] `sem diff`
  - [ ] `sem tag` and `sem show --object=world-model --run <id>`
  before materialized merge/retract flows.
- [ ] Add explicit high-priority TODOs for first implementation slice:
  - [ ] wire auto-emit for `db accept promote` and `db accept pathdb-commit` into `sem/commits`.
  - [ ] persist `sem/world_model_runs/<run_id>.json` with proposal digests and base/ending anchors.
  - [ ] enforce `WorldModelRunId` validation when reading commits.
  - [ ] define minimal `sem world-model` read path for status and lineage.
  - [ ] add `sem merge --dry-run` to validate semantic conflict output before materialized merge.
- [ ] Make `db accept promote` and `db accept pathdb-commit` auto-emit semantic commits when `sem/` exists, so legacy plumbing and semantic history cannot drift apart.

---

## 7. Workstream F: Interop and projection layers

### Goal

Make RDF/OWL/SHACL/property-graph interoperability strong without making any of them the kernel.

### Tasks

- [ ] Lower RDF/OWL/SHACL adapters through the canonical schema/category IR.
- [ ] Introduce boundary adapter ASTs / IRs:
  - `RdfDatasetIr`
  - `OwlOntologyIr`
  - `ShaclShapeIr`
  - `PropertyGraphIr`
- [ ] Keep RDF open-world and context-aware.
- [ ] Treat OWL axioms as:
  - constraint candidates,
  - rewrite candidates,
  - or assumptions,
  not as automatic kernel semantics.
- [ ] Add SHACL validation as:
  - an ingestion/promotion gate,
  - a stored report in the evidence plane,
  - and later a certificate-checked restricted subset.
- [ ] Compile SPARQL fragments into the typed query IR instead of creating a second semantic core.
- [ ] Treat property-graph compatibility as projection:
  - binary relations may project to edges,
  - n-ary relations project to fact nodes / relationship entities,
  - canonical internal form remains olog/relation-object based.
- [ ] Rework RDF ingest to lower through IR rather than directly into `ProposalV1::Entity` / binary `ProposalV1::Relation`.
- [ ] Add initial SHACL restricted subset support over the IR:
  - `sh:NodeShape`
  - `sh:targetClass`
  - `sh:path`
  - `sh:datatype`
  - `sh:minCount`
  with `Valid | Invalid | Unknown` results and evidence-plane report storage.
- [ ] Add a narrow SPARQL compilation slice only after the IR exists:
  - `SELECT`
  - basic graph patterns
  - `GRAPH`
  - equality filters
  - property paths
  compiling into `query_ir_v1` / AxQL rather than a second executor.

---

## 8. Workstream G: World-model and AI lifecycle

### Goal

Make AI/world-model outputs first-class evidence streams with full lineage.

### Tasks

- [ ] Keep all world-model and LLM outputs in the evidence plane by construction.
- [ ] Add first-class world-model run objects with:
  - model/plugin version,
  - input accepted snapshot id,
  - training/export digest,
  - evaluation metrics,
  - proposals digest(s),
  - promoted snapshot/tag (if any).
- [ ] Persist world-model lineage under the semantic-VCS layer:
  - `sem/world_model_runs/<run_id>.json`
  - `WorldModelRunV1`
  - base semantic commit / accepted snapshot
  - plan/request config digest
  - task costs / guardrails
  - proposal digests
  - validation/eval summaries
  - resulting promotion/tag refs
- [ ] Make the lifecycle explicit:
  - train/run from accepted snapshot anchor or pathdb-derived branch head,
  - emit proposals only,
  - snapshot anchors in run object for base state + optional end state,
  - preview validation against a review branch,
  - reconcile into a review branch,
  - promote selected deltas,
  - tag the new accepted baseline,
  - repeat.
- [ ] Use `wm/<experiment>` branches for proposal streams and `review/<topic>` branches for reconciled ontology changes.
- [ ] Persist `WorldModelRunV1` status transitions at least:
  - Pending, Running, ProposalsReady, Validated, Reconciled, Promoted/Failed/Aborted.
- [ ] On `/world_model/propose` and proposal export paths, write proposal digests to `WorldModelRunV1` and return a run identifier that is durable in `sem/world_model_runs`.
- [ ] Tie wm proposals to branches:
  - run object carries `branch_ref`
  - commits on the branch must reference `world_model_run_id`
  - `run_id` drives all proposal lineage queries.

---

## 9. Program sequencing

### Phase 1: truth in framing + anchor discipline

- [ ] tighten docs and trusted-kernel claims
- [ ] resolve `.axpd` format story
- [ ] define stable semantic anchors
- [ ] make lifecycle states explicit in Rust APIs

### Phase 2: kernel/IR split

- [ ] define schema/category IR
- [ ] define lowering from `.axi` to IR
- [ ] rebase typed execution and interop work on that IR

### Phase 3: semantic VCS

- [ ] add refs, semantic commits, parentage, semantic diff
- [ ] add reconciliation merges and lifecycle state transitions
- [ ] persist first-class world-model runs and wire run IDs into commit provenance before merge automation.

### Phase 4: interop and ologs

- [ ] add olog authoring surface
- [ ] lower RDF/OWL/SHACL/property-graph systems through the same IR

### Phase 5: world-model lifecycle integration

- [ ] bind world-model runs to semantic commits / refs / tags
- [ ] make evaluation baselines and promoted lineage explicit
- [ ] add persisted world-model run index keyed by branch with status transitions (`running`/`validated`/`promoted`/`failed`).

---

## 10. Subagent seams

The work decomposes cleanly into these parallel seams:

1. Lean trusted-kernel scope and certificate semantics
2. Schema/category IR and olog core
3. Rust lifecycle typing and anchor-aware public APIs
4. Storage / `.axpd` / snapshot-anchor alignment
5. Semantic VCS and reconciliation workflow
6. RDF/OWL/SHACL/property-graph interop through the IR
7. World-model lineage and evidence-plane lifecycle

Each seam should produce:

- a concrete target spec,
- file/module touchpoints,
- migration steps,
- dependencies on other seams,
- and a suggested first implementation slice.
