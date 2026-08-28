# Semantic Kernel + Semantic VCS Roadmap

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap turns the current review conclusions into a concrete program of work.

Companion target specs:

- `docs/reference/TRUSTED_KERNEL.md`
- `docs/reference/KERNEL_IR.md`
- `docs/reference/RUST_LIFECYCLE_TYPES.md`
- `docs/reference/SEMANTIC_VCS.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_MERGE_LATTICE.md`

It assumes the following framing:

- Axiograph today is a **proof-carrying ontology workbench**, not yet a full categorical ontology kernel.
- The trusted core lives in **Lean** and is currently strongest on:
  - typed path expressions,
  - free-groupoid denotation,
  - fixed-point probabilities,
  - anchored certificate replay.
- **Rust** should become the place where lifecycle state, schema scoping, snapshot anchoring, and typed execution are made operational and difficult to bypass.
- The accepted plane should evolve into a **semantic VCS**, not just a snapshot store with `HEAD`.
- Roadmap planning should assume a **greenfield target surface**:
  - do not preserve superseded runtime/API/format behavior by default,
  - replace superseded defaults with one canonical semantic contract where possible,
  - and keep carry-forward support only when it preserves trusted anchors, defended
    soundness claims, or an explicit migration window.

---

## 0. Program goals

By the end of this program, Axiograph should have:

1. A small, explicit, honest **Lean trusted kernel**.
2. A canonical **schema/category IR** that becomes the ontology kernel.
3. A first-class Rust workflow for **lifecycle-typed, anchor-aware artifacts**.
4. A real **semantic VCS** for ontology / semantic / proposal-adapter evolution.
5. Clean **interop layers** for RDF/OWL/SHACL/property-graph systems.
6. A disciplined **evidence plane** for AI/proposal-adapter/LLM tooling.
7. One typed operational currency across authoring, query, migration, certification, and review.

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
- [ ] Keep cutover language equally precise:
  - do not promise long-term support for superseded public surfaces by default,
  - do not keep parallel public semantic contracts indefinitely,
  - and do not describe migration adapters as first-class long-term kernels.
- [ ] Standard cutover framing to use:
  - “greenfield target with explicit migration from older surfaces where
    justified”
  - “migration retained only for trust-preserving anchors, audited
    migration, or short-lived operational cutovers”
- [ ] Standard cutover framing to avoid:
  - “retain superseded public surfaces unless proven unsafe”
  - “support both old and new semantics indefinitely”
  - “keep the superseded path as a default until further notice”

### What counts as "making it one"

- [ ] Treat the stronger claim as a milestone with explicit entry conditions:
  - Lean checks the certifiable semantic acceptance boundary for canonical `.axi`,
    checked rewrite rules, and narrow query/migration/certificate obligations.
  - A canonical schema/category IR becomes the semantic spine shared by authoring,
    query, migration, certification, and semantic diff.
  - Rust public APIs are indexed by lifecycle, accepted anchors, and stable
    schema/theory/context ids rather than raw runtime strings alone.
  - Olog/authoring surfaces lower to canonical deltas over the same ids.
  - Semantic VCS history records explicit `schema` / `theory` / `instance` /
    `context` deltas rather than only storage mutation.
- [ ] Keep one subtle clarification explicit:
  - becoming a dependently typed ontology engine does **not** mean “all runtime
    logic moves into Lean”;
  - it means “the semantics-bearing seams are typed, anchored, and fail-closed,
    and the checked claims are explicit about soundness scope”.

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
- [ ] Add a certifiable schema/category IR boundary in Lean:
  - check the well-formedness of the compiled schema/category fragment that
    certificates and prepared queries will cite,
  - keep the certified slice narrow and explicit,
  - and treat richer HoTT/topos material as theorem support until imported by
    the verifier.
- [ ] Add narrow typed query and migration witness checkers:
  - prepared-query/result witnesses for explicitly supported query fragments,
  - migration witnesses for explicitly supported `Δ/Σ/Π`-style transports,
  - and documentation that states row/transport soundness without implying
    completeness or full semantic equivalence.

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

- [x] Land the canonical compiler in `axiograph-kernel` and keep PathDB's
  `RuntimeSchemaIndex` / relation indexes derived:
  - `CompiledKernelSnapshot`, `KernelSnapshotIr`, `SchemaPresentationIr`, and
    validated finite `InstanceModelIr`,
  - explicit data/context/world/temporal/parameter/evidence role kinds,
  - `CarrierSpecIr`,
  - `WitnessViewIr`,
  - with `.axi` import/meta-plane code consulting compiled relation semantics
    for carrier/witness selection.
- [x] Compile `.axi` schemas into canonical `SchemaPresentationIr` with:
  - object types,
  - subtype inclusions,
  - relation-objects,
  - ordered roles,
  - field/projection arrows,
  - optional explicit arrows,
  - path equations,
  - rewrite rules,
  - context/world annotations.
- [x] Keep **relation-as-object + projection arrows** canonical.
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
- [x] Keep one category presentation in `axiograph-kernel`:
  - `SchemaPresentationIr` owns object types, relation objects, ordered role
    projections, subtype/aspect/function generators, equations, congruence, and
    derived formation evidence;
  - `InstanceModelIr` owns the finite interpretation data;
  - PathDB's duplicate `SchemaCategoryIr` and `InstanceFunctorIr` views were
    removed, and its semantic index now exposes canonical citations only.
- [ ] Reuse / absorb the current migration-side category slice instead of inventing a second parallel IR:
  - the new canonical IR should subsume the useful parts of `rust/crates/axiograph-pathdb/src/migration.rs`.
- [ ] Make migrations and query elaboration consume the kernel IR first and only then lower to convenience projections.
- [~] Make theory transport runtime-addressable:
  - current slice adds `TheoryTransportPlanIr` / `TheoryTransportItemIr` over
    compiled `TheoryIr` plus `SchemaMorphismV1`,
  - migration preview now consumes these statuses before emitting resolver
    handles,
  - next step: use the same plan object in semantic rebase/merge and future
    Lean migration witnesses.
- [ ] Make all semantics-bearing artifacts cite IR-level ids:
  - authoring deltas,
  - prepared queries,
  - migration previews,
  - certificates,
  - and semantic diffs should all name the same objects/relations/roles/arrows.
- [ ] Add typed IR-level forms for more than schema compilation:
  - a typed query IR that elaborates against `SchemaCoreIr` / `TheoryIr`,
  - a migration IR that names schema morphisms and transport obligations,
  - and certificate payloads that cite canonical IR ids rather than runtime-only
    field-name heuristics.

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
    - `MaterializationIdV2`
    - `ProposalDigest`
    - `ProposalAdapterRunId`
    - `SchemaId`
    - `TheoryId`
    - `ContextId`
- [x] Implement runtime-handle/lifecycle modules directly in `axiograph-pathdb`:
  - `rust/crates/axiograph-pathdb/src/runtime_handle.rs`
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
  - `ProposalAdapterRun<A>`
  - `CertifiedAnswer<A>`
- [ ] Add first-class typed handles for the seams that currently drift apart:
  - prepared queries over accepted anchors and compiled schema ids,
  - semantic deltas / authoring drafts over stable object/arrow/role ids,
  - migration preview handles over source+target anchors,
  - and certificate-bearing answers that refine ordinary answer handles rather
    than living in a separate reporting path.
- [ ] Use a two-part identity model in the core APIs:
  - persistent identity via stable anchor newtypes,
  - live identity via `DbBranded<u32>`,
  - and carry both where available.
- [x] Replace `PathdbSnapshotId` with authenticated `MaterializationIdV2` for
  derived SQLite images; query and certificate APIs remain indexed by accepted
  meaning anchors.

### Lifecycle discipline

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
- [ ] Treat lifecycle transitions as semantic-state discipline:
  - once a typed constructor or handle exists, it becomes the default public
    path,
  - superseded JSON/report/API shapes remain adapter-only where trust or audited
    migration requires them,
  - otherwise they leave the default path.

### API shift

- [ ] Make schema-scoped / snapshot-scoped wrappers the default public API for core execution.
- [ ] Demote raw `u32` / stringly APIs to adapter/internal-only surfaces, then remove them from default public workflows.
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
- [ ] Make typed authoring/query/migration/certification share one contract:
  - every meaning-changing or meaning-claiming API should return canonical
    anchors, stable ids, trust-contract fields, and reviewable semantic deltas
  where relevant,
  - so editor, CLI, server, and agentic workflows stop speaking partially
    incompatible semantic languages.
- [ ] Treat superseded JSON/report/API shapes as adapter-only once the shared
  typed contract exists:
  - preserve only trust-relevant fields and import ability where needed,
  - prefer one canonical contract over permanent dual-surface support.

### Runtime-usable checker and authoring/query surfaces

Rust should become the place where the ontology checker is **operationally
useful**, while Lean remains the authority for the certified semantic slice.

That means the Rust layer should expose one reusable checker/report surface for:

- business-rule lookup and applicability checking,
- typed authoring validation and repair hints,
- typed query elaboration and result-shape reporting,
- CQ and migration preview execution,
- semantic coverage / drift reporting,
- and interop-layer validation over RDF/SHACL/olog-facing workflows.

- [ ] Define one Rust-side checker contract that all of those surfaces reuse:
  - accepted/review anchors,
  - stable IR ids,
  - trust class,
  - coverage,
  - findings/repairs,
  - and explicit non-claims.
- [ ] Make the checker usable from ordinary CLI/server/tool paths without
  requiring certificate mode or Lean replay for every call.
- [ ] Keep the trust split explicit in the API surface:
  - `runtime_checked`
  - `reviewed_but_uncertified`
  - `lean_certifiable`
  - `lean_certified`
  - `retrieval_only`
- [ ] Add typed prepared handles for business-rule and semantic-coverage use
  cases in addition to query execution:
  - rule handles,
  - authoring-delta handles,
  - migration-preview handles,
  - coverage-report handles.
- [ ] Ensure agent/tool-facing responses can compare strong vs weak claims across
  ontology versions/worlds without scraping prose or bespoke logs.

---

## 5. Workstream D: Storage and anchors

### Goal

Align storage claims with actual runtime artifacts.

### Tasks

- [x] Resolve `.axpd` on deterministic SQLite under AxiStore; all other
  PathDB byte formats and writers are deleted.
- [x] Enforce a greenfield cutover: old bincode/sectioned/custom-WAL bytes fail
  closed and are rebuilt from exact accepted inputs rather than migrated.
- [x] Add production-format deterministic, bounded, corruption, substitution,
  recovery, fault-injection, and arbitrary-byte tests.
- [ ] Keep PathDB as a derived execution substrate; do not let its current shape define ontology semantics.
- [ ] Keep accepted `.axi` + schema/category IR as the meaning plane.
- [x] Remove persistence shortcuts from `axiograph-storage`; it now stages
  evidence in memory only.
- [x] Split logical identity from exact image identity and bind both, plus
  accepted snapshot/tree/module/kernel/fact-log/configuration/overlay anchors,
  into `MaterializationIdV2`.
- [x] Delete derived PathDB snapshot export/import surfaces rather than treating
  them as interchange, debug round trips, or semantic anchors.
- [x] Keep evidence overlays explicitly outside the semantic kernel:
  - queries may use chunks/proposals/embeddings for retrieval and explanation,
  - but they are not certified unless the relevant facts were promoted into
    accepted `.axi`.
- [~] Add typed embedding sidecar manifests and evidence overlays:
  - `EmbeddingSidecarManifestV1` anchored to accepted ref, optional
    `MaterializationIdV2`, compiled IR digest, model version, and text digests,
  - `EmbeddingEvidenceOverlayV1` for similarity observations and candidate
    semantic relationships,
  - no vector payloads in canonical `.axi`,
  - embedding-derived relationships enter semantic VCS only as typed proposals
    or review deltas.
  - implemented so far: Rust data contracts and validation for manifest
    anchors, source model identity, target ids, trust caveats, and advisory-only
    relationship overlays.
  - remaining: CLI/tool-loop generation, persisted overlay artifacts, and
    proposal/evolution-preview plumbing.
- [ ] Complete store-backed certification from canonical accepted `.axi` anchor
  material and fail closed when a requested certified answer depends on
  overlay-only facts. No snapshot-export migration/debug path remains.
- [ ] Start plumbing module digests into runtime metadata and canonical fact material alongside `axi_fact_id`.

---

## 6. Workstream E: Semantic VCS

### Goal

Use AxiStore as the first-class semantic version-control and accepted-state
system. Evidence stays outside accepted state until typed review and promotion.

### Current implemented slice

`axiograph-store` owns immutable objects, snapshots, trees, semantic commits,
reconciliations, refs, tags, audit lineage, gate closures, and authenticated
materialization receipts in one repository-bound family. Promotion and
candidate publication use generation CAS. The CLI's `semantic_model.rs` holds
filesystem-free review/projection DTOs only; it is not a second persistence
implementation. The deleted `accepted_plane.rs`, `sem/HEAD`, and `sem/*` JSON
layout are not compatibility paths.

### Branch and ref semantics

Semantic refs should define permitted workflow, not just naming convention.

| Ref | Purpose | Normal producers | Required invariants |
| --- | --- | --- | --- |
| `refs/heads/main` | accepted ontology baseline | accepted-plane promotion, reviewed merge | commits that change state must carry before/after accepted anchors |
| `refs/heads/review/<topic>` | candidate ontology review stream | human authoring, migration preview, reconciliation output | promotion-relevant commits should cite persisted validation preview refs |
| `refs/heads/evidence/<source>` | source-aligned evidence stream | ingest/import/grounding flows | may advance without accepted-state change |
| `refs/heads/evidence/proposals/<experiment>` | proposal-adapter proposal stream | persisted `ProposalAdapterRunRecordV1` lifecycle | commit generation is illegal without `run_id` + `branch_ref` provenance |
| `refs/tags/<release>` | immutable release pointer | semantic commit on `main` | tag move must not alter accepted state payload |

- [x] Remove `sem/HEAD`; AxiStore catalog refs are the only mutable pointers.
- [x] Reject direct `evidence/proposals/* -> main` transitions; proposal-adapter output must reconcile through `review/*`.
- [x] Reserve tags for accepted/released states only; do not tag unreviewed evidence or proposal branches.
- [x] Define ref-update validation centrally so branch invariants are enforced in one place rather than by CLI convention.
  - current runtime validation covers `heads/main`, `heads/review/*`,
    `heads/evidence/*`, `heads/evidence/proposals/*`, and immutable `tags/*`
  - `heads/evidence/proposals/*` requires `PredictiveProposalRun` commits with run-id provenance,
    delta refs, and persisted run records

### Semantic state objects vs semantic delta objects

The semantic VCS should keep a hard separation between "what state this commit
points to" and "what semantic change it records".

Required state payload:

- `accepted_snapshot_id_before`
- `accepted_snapshot_id_after`
- `accepted_tree_digest`
- `materialization_id_before`
- `materialization_id_after`
- `evidence_digests`

Required delta payload:

- `schema` delta
- `theory` delta
- `instance` delta
- `context` delta
- evidence/proposal refs
- certificate refs
- validation refs
- lifecycle events
- proposal-adapter run refs

- [x] Land a first compact typed-layer sidecar on `SemDeltaV1`:
  - semantic commits can now carry `semantic_delta` copied from `EvolutionPreviewV1`
    without inlining the full preview report.
- [~] Continue expanding `SemDeltaV1` beyond the first sidecar:
  - keep layer summaries compact,
  - preserve refs for quality/validation/certs/proposal-adapter lineage,
  - and avoid copying full preview internals into commit history.
- [ ] Keep `SemStateRefV1` pointer-only; it should not inline preview payloads or copy large reports.
- [ ] Make semantic commits cite materialized state and persisted review artifacts, not duplicate them.
- [x] Add one compact semantic gate summary to commits and refs:
  - trust summary
  - CQ gate summary
  - residual obligation count
  - preview kind/candidate label
  - current runtime also copies the compact gate summary into `SemDeltaV1`
    where the delta was derived from a preview/gated overlay

### Merge and reconciliation rules

Merge should remain semantic reconciliation, not text concatenation.

- [ ] Make `sem merge --dry-run` the default first shipping merge mode.
- [~] Require dry-run merge output to materialize:
  - semantic diff by `schema` / `theory` / `instance` / `context`
  - conflict set
  - candidate reconciliation object
  - CQ/trust preview result
  - residual obligations
  - typed blocker set for quality, CQ, trust, coverage, runtime theory,
    unresolved conflicts, and unapplied resolver handles
- [~] Route merge/rebase planning through typed semantic slices and a finite
  runtime merge lattice:
  - see `docs/roadmaps/ROADMAP_SEMANTIC_MERGE_LATTICE.md`
  - current first slice adds `SemanticSliceManifestV1`,
    `SemanticMergeLatticeV1`, `SemanticMergePlanV1`, MCP-visible resolver
    steps, and typed blocker summaries over existing semantic merge dry-runs
  - auto-merge stays conservative and materialization remains fail-closed
- [x] Materialize protected-main merges only after an immutable
  `SemReconciliationV2` exists.
  - `AxiStore::materialize_merge` authenticates the current main and named
    source tips as exact ordered parents; generic promotion rejects merges.
  - reviewed parent/result candidates bind exact snapshots, trees, root module,
    compiled kernel IR, payload-fingerprint index, and canonical/CQ/trust/theory
    reports.
  - typed keep/drop/introduce/transport decisions account for every parent and
    result payload; transport carries an immutable witness digest.
  - AxiStore recompiles all candidates from exact stored `.axi` bytes before
    advancing state. The check is finite payload-union preservation, not a
    general categorical-colimit or dependent-transport theorem.

### Lifecycle states tracked in history

- [ ] Track lifecycle transitions explicitly on facts/modules/artifacts:
  - `proposed`
  - `validated`
  - `reviewed`
  - `accepted`
  - `certified`
  - `superseded`
  - `retracted`
- [ ] Make lifecycle transitions part of commit/reconciliation payloads, not inferred from branch names alone.

### Operational adapter slice

The broad `axiograph sem` command family was removed. Add narrow adapters only
for typed AxiStore operations that operators actually need: status, semantic
diff, candidate publication, promotion, merge preview, and immutable tagging.
Adapters must not reintroduce filesystem refs, `sem/*` JSON, or dual writes.

### Implementation-first actions

- [x] Remove `db accept promote` and `sem/commits`; typed
  `AxiStore::promote` publishes the semantic commit and protected ref atomically.
- [ ] Attach immutable `MaterializationIdV2` receipts to AxiStore semantic
  commits without granting derived rows mutation authority.
- [ ] Validate `ProposalAdapterRunId` resolution whenever reading proposal-annotated commits.
- [ ] Add one semantic ref read/write path for `main`, `review/*`, `evidence/*`, `evidence/proposals/*`, and tags rather than branch-specific helpers.
- [ ] Center persistence in `axiograph-store`; CLI semantic commands should be
  typed adapters over AxiStore rather than a second filesystem authority.

---

## 7. Workstream F: Uniform Evolution Previews and CQ-Gated Semantic Operations

### Goal

Make one preview object the normal review currency for proposal validation,
migration preview, semantic merge, and accepted-plane promotion.

### Current implemented slice

The implementation is already ahead of the roadmap text:

- `proposals_validate.rs` produces `ProposalsValidationV1`
- `AxiStore::promote` consumes a typed `PromotionPlan` and immutable gate objects
- `evolution_preview.rs` already defines `EvolutionPreviewV1` with:
  - `typed_change`
  - `semantic_delta`
  - `quality_delta`
  - `competency_gate`
  - `trust`
  - `trust_summary`
  - `runtime_semantics`
  - `rule_summary`
  - `coverage_summary`
  - `trust_delta`
  - `residual_obligations`
  - `ok`
- compact summaries can already be derived via `SemGateSummaryV1`

The roadmap task is to converge these producers on one review object, not to
invent a new preview vocabulary.

### Standard preview kinds

- `proposal_preview`
- `promotion_preview`
- `migration_preview`
- `merge_preview`
- `review_branch_preview`

- [ ] Make every ontology-changing workflow emit `EvolutionPreviewV1` directly or embed it as the canonical sub-object.
- [ ] Treat `ProposalsValidationV1` and `PromotionPreviewReportV1` as adapters around `EvolutionPreviewV1`, not divergent peer schemas.
- [ ] Persist all full preview reports as immutable AxiStore objects attached to
  candidate or promotion plans.
- [ ] Persist only compact gate summaries in commits, reconciliations, and refs.

### CQ-gated workflow policy

One CQ policy object should govern every semantic operation that can change
accepted meaning.

Required policy fields:

- `fail_on_regression`
- `fail_on_unsatisfied_after`
- future:
  - `require_certifiable_questions`
  - `allow_execution_only_questions`
  - `minimum_coverage_after`

- [ ] Use the same CQ policy for:
  - proposal preview
  - migration preview
  - semantic merge dry-run
  - accepted-plane promotion
- [ ] Require per-question reporting to include:
  - before/after rows
  - before/after satisfied
  - before/after trust class
  - trust-change reasons
  - expected answer-shape reference where available
- [ ] Fail closed on `review/* -> main` when the configured CQ gate fails.
- [ ] Fail closed on `evidence/proposals/* -> review/*` when the configured CQ gate fails for required suites.

### Typed change summaries and residual obligations

- [ ] Standardize `typed_change.schema/theory/instance/context` as the cross-surface delta summary.
- [ ] Require every preview to surface residual obligations explicitly rather than hiding them in prose.
- [ ] Classify residual obligations at least as:
  - quality/lint failure
  - unsupported migration obligation
  - CQ regression
  - trust downgrade
  - unresolved reconciliation conflict

### Concrete implementation slice

- [ ] Make `sem merge --dry-run` emit `EvolutionPreviewV1` plus a candidate reconciliation object.
- [ ] Make accepted-plane promotion persist the `EvolutionPreviewV1` path and cite it from the emitted semantic commit.
- [x] Make persisted reconciliations capable of emitting a stored `EvolutionPreviewV1`-backed
  review report and a `SemCommitKindV1::Merge` commit with compact semantic/trust/rule/coverage summaries.
- [ ] Make future migration preview emit the same object shape and store it in the same location.
- [ ] Add one preview-read surface in CLI/server tooling so reviewers can inspect stored preview objects without scraping logs.

---

## 8. Workstream G: Trust-Contract Surfacing and Typed Query Certifiability

### Goal

Make trust semantics and query certifiability explicit across query, preview,
merge, and promotion paths.

### Current implemented slice

- `query_ir_v1` and `CompiledFiniteQuery` already expose trust and certifiability;
  `PreparedQueryMetadataV1` packages the prepared-query id, input/elaborated IR
  ids, inferred types, trust, explicit non-claims, and refinement handles for
  downstream reports.
- `trust_contract.rs` already defines:
  - `TrustContractV1`
  - `QueryTrustContractV1`
  - `semantic_coverage`
  - `semantic_claims`
  - `gaps`
- query trust already distinguishes:
  - `certifiable`
  - `mixed`
  - `execution_only`
- query trust states:
  - `claim_scope = finite_query_denotation_within_exact_accepted_module`
  - `completeness_claim = exact_for_declared_finite_decidable_fragment` only
    after an accepted bound Lean receipt; otherwise `not_claimed`
  - `ontology_closure_claim = not_claimed`
- single-context typed queries are certifiable today; multi-context unions and approximate operators remain execution-only.
- theory checking now has a separate runtime report family:
  - `RuntimeTheoryCheckReportV1`
  - `RuntimeTheoryClosureTierV1`
  - `CompletenessClaimV1`
  - `OntologyClosureClaimV1`
  - `axiograph check theory`
  - `semantic_theory_check`
  These claims are scoped to compiled theory obligations and declared
  world/evidence/ref assumptions. They do not extend V4's finite exact theorem
  beyond its declared fragment or imply full ontology closure.

### Trust-contract unification

- [ ] Use one trust-contract family across:
  - query results
  - prepared-query introspection
  - evolution previews
  - reconciliation summaries
  - proposal-adapter validation summaries
  - semantic-commit gate summaries
- [ ] Keep explicit claim scope and non-claims present everywhere:
  - finite completeness only with a bound V4 Lean receipt
  - no ontology-closure claim
  - no global semantic-equivalence claim unless explicitly checked
  - Runtime-theory CLI output surfaces module digest, admissibility scope,
    declared world, evidence policy, admissibility trace, the explicit
    `closure_engine_not_implemented` residual, and next action; continue
    threading the same boundary into every report.
- [ ] Make semantic coverage and semantic claims available to preview/reporting paths, not only to query execution.
- [ ] Attach runtime theory-check reports to semantic merge/rebase and
  reconciliation previews so merge gates can distinguish checked, review-only,
  residual, and blocking theory obligations.

### Typed query certifiability actions

- [x] Make `CompiledFiniteQuery` the sole query execution currency across:
  - REPL
  - server `/query`
  - CQ evaluation
  - agent/tool surfaces
  - Current bridge: CQ evaluation now cites `PreparedQueryMetadataV1` per
    question, and refinement apply reports carry base/refined prepared-query
    metadata instead of only raw query text.
- [ ] Persist certifiability metadata for CQs and preview reports:
  - whole-query `trust_class`
  - `certifiable_disjuncts`
  - `execution_only_disjuncts`
  - query-level reasons for unsupported fragments
  - Current bridge: CQ reports carry this via `PreparedQueryMetadataV1`; preview
    report persistence still needs the same envelope.
- [ ] Require CQ reports to say when a regression is:
  - semantic answer regression
  - trust downgrade
  - or both
- [ ] Add branch/policy hooks that can require:
  - only certifiable CQs on `main`
  - mixed CQs allowed on `review/*`
  - execution-only CQs allowed only for exploratory evidence branches unless explicitly waived
- [ ] Add a narrow "prepared query + certificate" persistence seam for high-value review assets:
  - prepared query digest
  - accepted snapshot anchor
  - trust contract
  - optional certificate ref

### Concrete file touchpoints

- [ ] `rust/crates/axiograph-cli/src/query_ir.rs`
- [ ] `rust/crates/axiograph-cli/src/trust_contract.rs`
- [ ] `rust/crates/axiograph-cli/src/repl.rs`
- [ ] `rust/crates/axiograph-cli/src/db_server.rs`
- [ ] `rust/crates/axiograph-cli/src/competency_questions.rs`

---

## 9. Workstream H: Interop and Projection Layers

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
- [ ] Preserve relation-object and projection-arrow structure when lowering RDF
  and property-graph inputs rather than normalizing everything to binary-edge
  assumptions too early.
- [ ] Make olog authoring, RDF adapters, and SHACL validation agree on the same
  semantic objects:
  - ologs are the human-facing authoring surface,
  - RDF is a boundary-layer interchange/query format,
  - SHACL is a boundary-layer validation format,
  - but all three must compile to the same canonical object/arrow/role ids.
- [ ] Treat OWL axioms as:
  - constraint candidates,
  - rewrite candidates,
  - or assumptions,
  not as automatic kernel semantics.
- [ ] Add SHACL validation as:
  - an ingestion/promotion gate,
  - a stored report in the evidence plane,
  - and later a certificate-checked restricted subset.
- [ ] Make SHACL reports runtime-usable engineering artifacts:
  - they should cite accepted/review anchors,
  - reference canonical IR ids where resolution is possible,
  - participate in semantic coverage reporting,
  - and stay explicit about `Valid | Invalid | Unknown` rather than pretending to
    close the world.
- [ ] Keep semantic-VCS summaries compact and persistent:
  - promotion/evidence commits and refs should carry compact gate summaries,
  - previews remain the rich working object,
  - and commit/ref payloads should preserve trust/CQ/rule/coverage state without
    copying float-heavy preview internals into long-lived semantic history.
- [x] Remove the old single-parent semantic-commit/reconciliation write model.
  `SemCommitV2` and `SemReconciliationV2` are the only accepted AxiStore
  formats; no default legacy reader or compatibility writer is retained.
  CLI merge summaries are explicitly untrusted previews and cannot be supplied
  to the accepted-state API.
- [ ] Compile SPARQL fragments into the typed query IR instead of creating a second semantic core.
- [x] Treat property-graph support as an experimental projection: canonical
  relation objects and n-ary facts remain explicit nodes, role projections are
  typed edges, and no generic binary-edge completeness claim is made.
- [ ] Add an explicit **advanced graph backend projection profile** instead of
  a vague "generic graph DB" promise.
  - Tier-1 targets should be only graph engines that expose enough structure to
    preserve typed projections and scoped execution, for example:
    - RDF/quad systems with named graphs and SPARQL dataset semantics,
    - property-graph systems with constraints, transactions, and stable query
      surfaces (`Cypher`/`openCypher`/`Gremlin`),
    - and graph engines with explicit schema/index management rather than
      purely ad hoc edge stores.
  - Initial reference backends to design against:
    - `TypeDB` as the primary high-fidelity typed backend target because it is
      schema-first, strongly typed, relation/role-native, and n-ary by design,
    - `TerminusDB` as the preferred RDF/VCS-shaped secondary target because it
      exposes schema/instance graph separation plus commit/branch graphs,
    - `Neo4j`, `Neptune`, `JanusGraph`, and `Memgraph` as lower-level
      projection/execution targets,
    - and `Apache AGE` as an experimental property-graph option rather than a
      first-class projection target.
- [x] Define a closed `BackendCapabilityDeclarationV1` contract for PathDB,
  TypeDB, TerminusDB, RDF/OWL, and property graphs. Every profile classifies
  relation objects, n-ary relations, typed roles, subtype inclusions,
  dependent indexes, refinements, context/world axes, evidence/provenance,
  constraints, path equations, rewrites, higher paths, finite instances, and
  native readback as `Native | Encoded | Sidecar | Unsupported`.
- [x] Encode backend projection asymmetrically instead of pretending every
  engine should host every semantic layer:
  - `TypeDB` should be the primary pushdown target for schema typing, relation
    roles, n-ary structure, typed query validation, and safe runtime rule/query
    fragments.
  - `TerminusDB` should be the primary pushdown target for projected branch /
    history / diff workspaces and context-aware RDF/document collaboration.
  - property-graph execution should remain experimental until a backend clears
    the same long-term support and typed-projection bar.
  - No backend becomes the ontology kernel just because it can host one of
    these layers well.
- [x] Keep semantic authority above the backend:
  - authoring, ologs, semantic VCS refs/commits, CQ gates, trust contracts,
    and lifecycle states remain Axiograph-native artifacts,
  - backends store **materialized projections** anchored to accepted snapshot /
    semantic commit ids,
  - native backend query interfaces stay usable as read-only projection lenses
    for outside tools and graph users,
  - backend mutation remains non-authoritative and must flow through
    Axiograph review/promotion paths,
  - and no backend-local schema or branch model becomes the ontology kernel.
- [ ] Be explicit about backend-native VCS limits:
  - exploit native commit/branch/diff features when a backend has them,
  - but keep schema/theory review and promotion in Axiograph,
  - especially where backend-native branch synchronization does not transport
    schema evolution together with instance data.
- [x] Add `ProjectionManifestV1` for every backend projection, anchored by
  repository id, accepted snapshot id, and compiled-IR digest, with finite
  `KernelRefV2` records, native artifact, closed capability declaration,
  coverage, semantic-loss report, and Axiograph-only mutation authority.
- [x] Support output-side backend-specific lowering without semantic surrender:
  - RDF backends preserve relation-objects and context/world structure through
    named graphs / reified fact objects where needed,
  - property-graph backends project binary carrier relations to edges only when
    semantics are not lost,
  - n-ary relations, evidence objects, and provenance remain explicit nodes /
    relationship entities,
  - and query pushdown is limited to fragments whose semantics are understood by
    the adapter profile.
- [~] Make backend round-tripping explicit rather than implicit. Output and
  readback are now versioned, anchor-aware, and report missing/drifted/extra
  records as evidence-only `ReadbackReportV1`; external backend imports still
  need to lower through typed proposals and canonical IR.
- [ ] Treat backend query pushdown as an optimization layer, not a second
  semantic core:
  - typed query elaboration still happens against the canonical IR,
  - pushdown plans must record which predicates/paths were executed remotely,
  - and trust contracts must distinguish anchored Axiograph results that used
    remote execution from plain backend retrieval.
- [ ] Rework RDF ingest to lower through IR rather than directly into `ProposalV1::Entity` / binary `ProposalV1::Relation`.
- [ ] Rework RDF export/query support to preserve anchor-aware trust reporting:
  - accepted ontology-backed answers stay distinct from retrieval-only graph
    hits,
  - review-state mappings stay distinct from accepted mappings,
  - and version/world scoping remains visible in the response contract.
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

## 10. Workstream I: Predictive Proposal Adapter Lifecycle

### Goal

Make AI/proposal-adapter outputs first-class evidence streams with full lineage.

### Current implemented slice

- `ProposalAdapterRunRecordV1` is already specified and persisted under `sem/evidence/proposal_adapter_runs/`.
- proposal/promotion flows already have typed run and snapshot anchors available.
- semantic commits already have a `proposal_adapter_run_id` seam.

### Required lifecycle

The lifecycle should be explicit and branch-resolved:

1. create `ProposalAdapterRunRecordV1`
2. bind it to `refs/heads/evidence/proposals/<experiment>`
3. record base semantic commit / accepted snapshot
4. emit proposal digests only
5. run preview validation
6. reconcile onto `review/<topic>`
7. merge/promote to `main`
8. tag release if appropriate
9. close the run with final status and resulting refs

The run lifecycle should prefer one canonical write path. Once a branch-aware
lifecycle path exists, delete parallel proposal/run surfaces instead of keeping
wrappers.

### Branch and status invariants

- [ ] Create the run record before the first proposal-branch semantic commit.
- [ ] Require every commit on `evidence/proposals/*` to carry:
  - `branch_ref`
  - `proposal_adapter_run_id`
  - proposal digests or status-update provenance
- [ ] Persist status transitions at least:
  - `Pending`
  - `Running`
  - `ProposalsReady`
  - `Validated`
  - `Reconciled`
  - `Promoted`
  - `Failed`
  - `Aborted`
- [ ] Require `finished_utc` for terminal states.
- [ ] Record final `promotion_commit` and resulting tag ref when promotion succeeds.
- [ ] Record `run_error` and retain lineage when the run fails or is aborted.

### Persisted run payload

- [ ] Persist:
  - model/plugin version
  - config digest
  - cost/guardrail digest
  - base semantic commit
  - base accepted/pathdb anchors
  - proposal digests
  - validation refs
  - reconciliation refs
  - promotion commit / tag refs
- [ ] Add a minimal proposal status/read surface before adding more write automation:
  - `sem show --object=proposal-adapter --run <id>`
  - `sem log --with-runs`
  - `sem show <proposal-ref>`

### Implementation-first actions

- [ ] Auto-materialize `ProposalAdapterRunRecordV1` on `/evidence/proposals/predict` and planner entrypoints before emitting proposals.
- [ ] Return durable `run_id` values from CLI/server proposal-adapter commands.
- [ ] Write proposal digests back to the run object atomically with proposal emission.
- [ ] Require validation and reconciliation refs to be added to the run before promotion to `main`.

---

## 11. Program sequencing

### Phase 1: truth in framing + anchor discipline

- [ ] tighten docs and trusted-kernel claims
- [ ] resolve `.axpd` format story
- [ ] define stable semantic anchors
- [ ] make lifecycle states explicit in Rust APIs
- [ ] define which superseded APIs/formats survive only as migration shims and which
  are removed from default public use

### Phase 2: kernel/IR split

- [ ] define schema/category IR
- [ ] define lowering from `.axi` to IR
- [ ] rebase typed execution and interop work on that IR
- [ ] stop adding new semantics-bearing features to pre-IR adapter paths except
  where needed for audited migration

### Phase 3: typed operational surfaces

- [ ] make prepared queries and typed answers first-class across CLI/server/tooling
- [ ] make authoring deltas and olog edits lower to canonical IR ids
- [ ] add migration preview handles and trust-contract output indexed by source/target anchors
- [ ] connect certificates to the same IR-level object ids used by authoring and query tooling
- [ ] remove superseded default report/API paths once typed surfaces cover the
  same trust-preserving use cases
- [ ] make `EvolutionPreviewV1` the shared preview artifact across proposal, promotion, migration, and merge dry-run

### Phase 4: semantic VCS

- [ ] add refs, semantic commits, parentage, semantic diff
- [ ] add branch/ref invariants for `main`, `review/*`, `evidence/*`, `evidence/proposals/*`, and tags
- [ ] add reconciliation merges and lifecycle state transitions
- [ ] persist validation previews and compact gate summaries in semantic history
- [ ] persist first-class proposal-adapter runs and wire run IDs into commit provenance before merge automation

### Phase 5: interop and ologs

- [ ] add olog authoring surface
- [ ] lower RDF/OWL/SHACL/property-graph systems through the same IR

### Phase 6: proposal-adapter lifecycle integration

- [ ] bind proposal-adapter runs to semantic commits / refs / tags
- [ ] make evaluation baselines and promoted lineage explicit
- [ ] add persisted proposal-adapter run index keyed by branch with status transitions (`running`/`validated`/`promoted`/`failed`).
- [ ] enforce `evidence/proposals/* -> review/* -> main` as the only promotion ladder for proposal-adapter outputs

### Phase 7: CQ-gated semantic review

- [ ] require CQ gate policy on merge/promotion surfaces
- [ ] persist per-question trust deltas in stored previews
- [ ] allow stricter certifiability requirements on `main` than on `review/*`

---

## 12. Subagent seams

The work decomposes cleanly into these parallel seams:

1. Lean trusted-kernel scope and certificate semantics
2. Schema/category IR and olog core
3. Rust lifecycle typing and anchor-aware public APIs
4. Storage / `.axpd` / snapshot-anchor alignment
5. Semantic VCS refs/commits/reconciliations
6. Evolution-preview, CQ-gate, and trust-contract unification
7. Typed query certifiability and prepared-query persistence
8. RDF/OWL/SHACL/property-graph interop through the IR
9. World-model lineage and evidence-plane lifecycle

Each seam should produce:

- a concrete target spec,
- file/module touchpoints,
- migration steps,
- dependencies on other seams,
- and a suggested first implementation slice.
