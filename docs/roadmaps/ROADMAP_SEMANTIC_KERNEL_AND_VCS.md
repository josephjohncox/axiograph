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
  - replace old defaults with one canonical semantic contract where possible,
  - and keep compatibility only when it preserves trusted anchors, defended
    soundness claims, or an explicit migration window.

---

## 0. Program goals

By the end of this program, Axiograph should have:

1. A small, explicit, honest **Lean trusted kernel**.
2. A canonical **schema/category IR** that becomes the ontology kernel.
3. A first-class Rust workflow for **lifecycle-typed, anchor-aware artifacts**.
4. A real **semantic VCS** for ontology / semantic / world-model evolution.
5. Clean **interop layers** for RDF/OWL/SHACL/property-graph systems.
6. A disciplined **evidence plane** for AI/world-model/LLM tooling.
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
- [~] Introduce concrete runtime schema-category and instance-functor IR:
  - current slice lives in `rust/crates/axiograph-pathdb/src/kernel_ir.rs`
    rather than a separate DSL crate,
  - `SchemaCategoryIr` exposes object types, relation objects, role projection
    arrows, and subtype inclusion arrows,
  - `InstanceFunctorIr` interprets object memberships, relation fact-id sets,
    role projections, and subtype transport,
  - next step: decide whether these stay in `axiograph-pathdb` or move to a
    dedicated kernel-IR crate once Lean/export/query/migration reuse increases.
- [ ] Consider promoting the kernel IR into a dedicated crate if reuse pressure increases:
  - `rust/crates/axiograph-kernel-ir/`
- [ ] Reuse / absorb the current migration-side category scaffold instead of inventing a second parallel IR:
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
- [ ] Treat `PathdbSnapshotId` as an operational/storage anchor, not the semantic anchor; query and certificate APIs should be indexed by accepted-plane meaning anchors.

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
- [ ] Treat lifecycle transitions as semantic-state discipline, not as a reason
  to preserve old public entrypoints:
  - once a typed constructor/handle exists, make it the default public path,
  - keep stringly/raw adapters only as migration shims or edge adapters,
  - and remove them when the migration window closes.

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
- [ ] Treat older JSON/report/API shapes as migration adapters once the shared
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

- [ ] Resolve the `.axpd` story:
  - choose one production `.axpd` format,
  - scope all trust claims to that one format,
  - and treat every other format as either a migration/import path or non-production design work.
- [ ] Replace default compatibility language with explicit cutover policy:
  - if sectioned verified v2 becomes production, ship a one-way audited import
    path from superseded checkpoints and stop treating v1 as the default runtime
    contract,
  - if live runtime bytes remain the production contract, remove v2 from
    production-facing claims and keep it only as design work until a real cutover
    is scheduled.
- [ ] Add tests and fixtures over the **actual** production checkpoint format.
- [ ] Add production-format contract tests:
  - golden `.axpd` fixtures produced by accepted-plane / WAL flows,
  - migration/import tests only for explicitly supported superseded bytes,
  - and a regression test proving the runtime rejects non-production bytes on
    the default path.
- [ ] Keep PathDB as a derived execution substrate; do not let its current shape define ontology semantics.
- [ ] Keep accepted `.axi` + schema/category IR as the meaning plane.
- [ ] Treat `axiograph-storage` as non-foundational until prototype shortcuts are removed.
- [ ] Split storage identity into two layers:
  - semantic anchor = accepted snapshot + canonical module digests + canonical fact ids,
  - runtime materialization id = accepted snapshot + overlay digests + build parameters + checkpoint/sidecar digests.
- [x] Treat `PathDBExportV1` as a debug/interchange anchor, not the primary semantic truth anchor.
  - Implemented: generic semantic/query/cert loading is canonical-only; `PathDBExportV1`
    remains under explicit `db pathdb` debug/live-byte/parser-parity commands.
  - Continued cleanup: REPL scripts no longer emit `*_export_v1.axi`, schema
    discovery examples no longer teach synthetic `Entity` fallback, and PathDB
    docs now frame `PathDBExportV1` as debug/live-byte/parser parity rather
    than semantic/query/certificate authority.
- [ ] Keep WAL overlays explicitly outside the semantic kernel:
  - queries may use chunks/proposals/embeddings for retrieval and explanation,
  - but they should not be “certified” unless the relevant facts were promoted into accepted `.axi`.
- [~] Add typed embedding sidecar manifests and evidence overlays:
  - `EmbeddingSidecarManifestV1` anchored to accepted ref / PathDB snapshot /
    compiled IR digest / model version / text digests,
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
- [ ] Change store-backed certification to prefer canonical accepted-plane anchors:
  - for accepted-plane serving, certify from accepted module text / canonical anchor material,
  - for pathdb-layer serving, fail closed when a requested certified answer depends on overlay-only facts,
  - keep snapshot-export certification only as an explicit migration/debug path,
    not as a default compatibility promise.
- [ ] Start plumbing module digests into runtime metadata and canonical fact material alongside `axi_fact_id`.

---

## 6. Workstream E: Semantic VCS

### Goal

Turn accepted-plane snapshots + WAL into a first-class semantic version control system.

### Current implemented slice

The roadmap should build on the semantic-history code that already exists in
`axiograph-cli`, not invent a second object model in prose.

- `accepted_plane.rs` already defines/persists:
  - `SemCommitV1`
  - `SemReconciliationV1`
  - `WorldModelRunV1`
  - `PromotionPreviewReportV1`
  - semantic ref pointers under `sem/refs/*`
- promotion and validation flows can already carry:
  - `validation_report_path`
  - `constraints_cert_path`
  - `quality_report_path`
  - `world_model_run_id`
  - `reconciliation_id`
- `sem/validations/` and `sem/world_model_runs/` already exist as real storage seams.

The missing work is to make these the normal workflow currency and to define the
branch/ref invariants around them.

### Branch and ref semantics

Semantic refs should define permitted workflow, not just naming convention.

| Ref | Purpose | Normal producers | Required invariants |
| --- | --- | --- | --- |
| `refs/heads/main` | accepted ontology baseline | accepted-plane promotion, reviewed merge | commits that change state must carry before/after accepted anchors |
| `refs/heads/review/<topic>` | candidate ontology review stream | human authoring, migration preview, reconciliation output | promotion-relevant commits should cite persisted validation preview refs |
| `refs/heads/evidence/<source>` | source-aligned evidence stream | ingest/import/grounding flows | may advance without accepted-state change |
| `refs/heads/wm/<experiment>` | world-model proposal stream | persisted `WorldModelRunV1` lifecycle | commit generation is illegal without `run_id` + `branch_ref` provenance |
| `refs/tags/<release>` | immutable release pointer | semantic commit on `main` | tag move must not alter accepted state payload |

- [ ] Treat `sem/HEAD` as a symbolic semantic-ref pointer, not a snapshot-id cache.
- [x] Reject direct `wm/* -> main` transitions; world-model output must reconcile through `review/*`.
- [x] Reserve tags for accepted/released states only; do not tag unreviewed evidence or wm branches.
- [x] Define ref-update validation centrally so branch invariants are enforced in one place rather than by CLI convention.
  - current runtime validation covers `heads/main`, `heads/review/*`,
    `heads/evidence/*`, `heads/wm/*`, and immutable `tags/*`
  - `heads/wm/*` requires `WorldModelRun` commits with run-id provenance,
    delta refs, and persisted run records

### Semantic state objects vs semantic delta objects

The semantic VCS should keep a hard separation between "what state this commit
points to" and "what semantic change it records".

Required state payload:

- `accepted_snapshot_id_before`
- `accepted_snapshot_id_after`
- `accepted_tree_digest`
- `pathdb_snapshot_id_before`
- `pathdb_snapshot_id_after`
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
- world-model run refs

- [x] Land a first compact typed-layer sidecar on `SemDeltaV1`:
  - semantic commits can now carry `semantic_delta` copied from `EvolutionPreviewV1`
    without inlining the full preview report.
- [~] Continue expanding `SemDeltaV1` beyond the first sidecar:
  - keep layer summaries compact,
  - preserve refs for quality/validation/certs/world-model lineage,
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
- [~] Only materialize a merge commit after a persisted reconciliation object exists when there are semantic conflicts.
  - current runtime persists the reconciliation and preview before a merge
    commit and rejects missing or non-materializing decisions such as
    `manual_review`
- [ ] Require `SemReconciliationV1` to reference:
  - base/left/right commit ids
  - preview report refs
  - chosen decisions
  - lifecycle transitions
  - optional certificate refs

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

### First shipping CLI slice

- [ ] `axiograph sem init`
- [ ] `axiograph sem status`
- [ ] `axiograph sem log`
- [ ] `axiograph sem show`
- [ ] `axiograph sem branch <name>`
- [ ] `axiograph sem diff <a> <b> --semantic`
- [ ] `axiograph sem tag <name>`
- [ ] `axiograph sem show --object=world-model --run <id>`
- [ ] `axiograph sem merge --dry-run`

### Implementation-first actions

- [ ] Wire auto-emit for `db accept promote` into `sem/commits`.
- [ ] Wire auto-emit for `db accept pathdb-commit` into `sem/commits`.
- [ ] Validate `WorldModelRunId` resolution whenever reading wm-annotated commits.
- [ ] Add one semantic ref read/write path for `main`, `review/*`, `evidence/*`, `wm/*`, and tags rather than branch-specific helpers.
- [ ] Keep the first implementation centered in:
  - `rust/crates/axiograph-cli/src/accepted_plane.rs`
  - `rust/crates/axiograph-cli/src/main.rs`
  before factoring to a dedicated semantic-VCS module.

---

## 7. Workstream F: Uniform Evolution Previews and CQ-Gated Semantic Operations

### Goal

Make one preview object the normal review currency for proposal validation,
migration preview, semantic merge, and accepted-plane promotion.

### Current implemented slice

The implementation is already ahead of the roadmap text:

- `proposals_validate.rs` produces `ProposalsValidationV1`
- `accepted_plane.rs` produces `PromotionPreviewReportV1`
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
- [ ] Persist all full preview reports under `sem/validations/`.
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
- [ ] Fail closed on `wm/* -> review/*` when the configured CQ gate fails for required suites.

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

- `query_ir_v1` and `PreparedQueryV1` already expose trust and certifiability;
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
- query trust already states:
  - `claim_scope = returned_rows_within_snapshot_and_context`
  - `completeness_claim = not_claimed`
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
  world/evidence/ref assumptions. They do not upgrade query-result trust
  contracts into answer-set completeness or full ontology closure.

### Trust-contract unification

- [ ] Use one trust-contract family across:
  - query results
  - prepared-query introspection
  - evolution previews
  - reconciliation summaries
  - world-model validation summaries
  - semantic-commit gate summaries
- [ ] Keep explicit non-claims present everywhere:
  - no completeness claim
  - no ontology-closure claim
  - no global semantic-equivalence claim unless explicitly checked
  - Recent runtime-theory CLI output now surfaces module digest, closure tier,
    declared world, evidence policy, closure trace, and next action in the
    human summary; continue threading the same boundary into every report.
- [ ] Make semantic coverage and semantic claims available to preview/reporting paths, not only to query execution.
- [ ] Attach runtime theory-check reports to semantic merge/rebase and
  reconciliation previews so merge gates can distinguish checked, review-only,
  residual, and blocking theory obligations.

### Typed query certifiability actions

- [ ] Make `PreparedQueryV1` the common query execution currency across:
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
- [ ] Treat older semantic-history payloads as migratable objects, not
  open-ended defaults:
  - when `SemCommitV1` / `EvolutionPreviewV1` are superseded, add explicit
    readers or migration tools only where historical audit requires them,
  - do not keep obsolete commit/preview schemas on the default write path.
- [ ] Compile SPARQL fragments into the typed query IR instead of creating a second semantic core.
- [ ] Treat property-graph compatibility as projection:
  - binary relations may project to edges,
  - n-ary relations project to fact nodes / relationship entities,
  - canonical internal form remains olog/relation-object based.
- [ ] Add an explicit **advanced graph backend compatibility profile** instead of
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
      first-class compatibility target.
- [ ] Define a `BackendCapabilityProfile` / `ProjectionCapabilityProfile`
  contract in Rust for backend adapters:
  - backend engine / support tier
  - `named_graphs`
  - `transactions`
  - `constraints`
  - `schema_management`
  - `native_type_system`
  - `native_nary_relations`
  - `typed_query_validation`
  - `logic_programming_or_functions`
  - `cypher_like_queries`
  - `gremlin_like_traversals`
  - `sparql_dataset_queries`
  - `relationship_entities`
  - `procedures_or_triggers`
  - `multi_database_or_namespace_support`
  - `immutable_history`
  - `branching_and_merge`
  - `diff_and_patch`
  - `schema_instance_separation`
- [ ] Encode backend pushdown asymmetrically instead of pretending every engine
  should host every semantic layer:
  - `TypeDB` should be the primary pushdown target for schema typing, relation
    roles, n-ary structure, typed query validation, and safe runtime rule/query
    fragments.
  - `TerminusDB` should be the primary pushdown target for projected branch /
    history / diff workspaces and context-aware RDF/document collaboration.
  - property-graph execution should remain experimental until a backend clears
    the same long-term support and typed-projection bar.
  - No backend becomes the ontology kernel just because it can host one of
    these layers well.
- [ ] Keep semantic authority above the backend:
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
- [ ] Add a typed projection manifest for every backend materialization:
  - source `AcceptedSnapshotId` / semantic ref,
  - compiled IR digest,
  - backend capability profile,
  - backend engine / support tier,
  - projected object/relation/role mappings,
  - context/world mapping strategy,
  - trust caveats about what was preserved vs flattened.
- [ ] Support backend-specific lowering without semantic surrender:
  - RDF backends preserve relation-objects and context/world structure through
    named graphs / reified fact objects where needed,
  - property-graph backends project binary carrier relations to edges only when
    semantics are not lost,
  - n-ary relations, evidence objects, and provenance remain explicit nodes /
    relationship entities,
  - and query pushdown is limited to fragments whose semantics are understood by
    the adapter profile.
- [ ] Make backend round-tripping explicit rather than implicit:
  - imports from external graph engines lower through the canonical IR,
  - exports/projected views are versioned and anchor-aware,
  - and drift between a backend projection and the accepted semantic state is
    reported as a typed coverage/drift artifact rather than silently repaired.
- [ ] Treat backend query pushdown as an optimization layer, not a second
  semantic core:
  - typed query elaboration still happens against the canonical IR,
  - pushdown plans must record which predicates/paths were executed remotely,
  - and trust contracts must distinguish `accepted_semantic_result_with_backend_pushdown`
    from plain backend retrieval.
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

## 10. Workstream I: World-Model and AI Lifecycle

### Goal

Make AI/world-model outputs first-class evidence streams with full lineage.

### Current implemented slice

- `WorldModelRunV1` is already specified and persisted under `sem/world_model_runs/`.
- proposal/promotion flows already have typed run and snapshot anchors available.
- semantic commits already have a `world_model_run_id` seam.

### Required lifecycle

The lifecycle should be explicit and branch-resolved:

1. create `WorldModelRunV1`
2. bind it to `refs/heads/wm/<experiment>`
3. record base semantic commit / accepted snapshot
4. emit proposal digests only
5. run preview validation
6. reconcile onto `review/<topic>`
7. merge/promote to `main`
8. tag release if appropriate
9. close the run with final status and resulting refs

The run lifecycle should prefer one canonical write path. Older world-model
entrypoints may survive as import/adaptation layers for a time, but the roadmap
should not assume indefinite compatibility between multiple proposal/run
surfaces once one branch-aware lifecycle path exists.

### Branch and status invariants

- [ ] Create the run record before the first wm-branch semantic commit.
- [ ] Require every commit on `wm/*` to carry:
  - `branch_ref`
  - `world_model_run_id`
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
- [ ] Add a minimal wm status/read surface before adding more write automation:
  - `sem show --object=world-model --run <id>`
  - `sem log --with-runs`
  - `sem show <wm-ref>`

### Implementation-first actions

- [ ] Auto-materialize `WorldModelRunV1` on `/world_model/propose` and planner entrypoints before emitting proposals.
- [ ] Return durable `run_id` values from CLI/server world-model commands.
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
- [ ] add branch/ref invariants for `main`, `review/*`, `evidence/*`, `wm/*`, and tags
- [ ] add reconciliation merges and lifecycle state transitions
- [ ] persist validation previews and compact gate summaries in semantic history
- [ ] persist first-class world-model runs and wire run IDs into commit provenance before merge automation

### Phase 5: interop and ologs

- [ ] add olog authoring surface
- [ ] lower RDF/OWL/SHACL/property-graph systems through the same IR

### Phase 6: world-model lifecycle integration

- [ ] bind world-model runs to semantic commits / refs / tags
- [ ] make evaluation baselines and promoted lineage explicit
- [ ] add persisted world-model run index keyed by branch with status transitions (`running`/`validated`/`promoted`/`failed`).
- [ ] enforce `wm/* -> review/* -> main` as the only promotion ladder for world-model outputs

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
