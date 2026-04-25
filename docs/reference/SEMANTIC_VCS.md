# Semantic VCS

**Diataxis:** Reference  
**Audience:** contributors

This document specifies the target git-style semantic workflow for Axiograph.

The goal is not to replace Git for source code. The goal is to give ontology,
semantic, evidence, and world-model evolution a first-class history model.

This document is the storage contract seam: typed anchors already exist in the code;
this doc defines how they must be persisted as semantic VCS objects.

## Design Rules

1. The accepted plane is not enough by itself; it needs refs and DAG history.
2. Merges are semantic reconciliation, not text concatenation.
3. World-model and LLM outputs stay in evidence/review branches until promoted.
4. Artifact lifecycle changes are preserved as history, never hidden mutation.
5. World-model runs must be persisted with explicit lineage and status.
6. Anchor references are the source of truth for history, not inferred UI state.
7. Review-critical runtime reports should be stored as machine-readable objects
   and referenced from history, not left as transient console prose.

## Relationship To Existing Stores

Current stores remain useful:

- accepted-plane module/snapshot store
- PathDB WAL and checkpoints
- certs/
- quality/

The semantic VCS sits above them and points at them.

It should not duplicate large artifacts; it should reference them.

## Review Artifacts

`sem/validations/` should persist the review objects that actually explain
semantic change.

That includes, as the implementation matures:

- evolution previews
- CQ gate reports
- business-rule applicability reports used in review
- semantic coverage / drift reports
- agent-facing semantic reports when they justify a merge or promotion decision

Commits, reconciliations, and refs should point at these persisted artifacts
rather than copying their full payloads inline.

## Store Layout

Target layout under the accepted-plane directory:

```text
sem/
  HEAD
  refs/
    heads/
      main
      review/
      evidence/
      wm/
    tags/
  commits/
  reconciliations/
  projections/
  world_model_runs/
  validations/
```

`commits/`, `reconciliations/`, `projections/`, and `world_model_runs/` are canonical persistence
locations for phase-1 objects.

## Refs

Required symbolic refs:

- `refs/heads/main`
- `refs/heads/review/<topic>`
- `refs/heads/evidence/<source>`
- `refs/heads/wm/<experiment>`
- `refs/tags/<release>`

`HEAD` points to the currently checked-out semantic ref, not directly to an
accepted snapshot.

`refs/heads/wm/*` must be reserved for proposal-generation branches. Branches
must move by semantic commits only (no ad-hoc branch files), and every run
observed through these branches must have a persisted `WorldModelRun` object.

## Semantic Commit

```rust
pub struct SemCommitV1 {
    pub version: String,
    pub commit_id: SemCommitId,
    pub parent_commit_id: Option<SemCommitId>,
    pub author: String,
    pub created_at_unix_secs: u64,
    pub message: Option<String>,
    pub kind: SemCommitKind,
    pub action: String,
    pub gate_summary: Option<SemGateSummaryV1>,
    pub policy: String,
    pub provenance: SemCommitProvenanceV1,
    pub state: SemStateRefV1,
    pub delta: SemDeltaV1,
    pub reconciliation_id: Option<ReconciliationId>,
    pub accepted_snapshot_id: AcceptedSnapshotId,
    pub accepted_parent_snapshot_id: Option<AcceptedSnapshotId>,
    pub pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub validation_report_path: Option<String>,
    pub validation_ok: Option<bool>,
}

pub struct SemCommitProvenanceV1 {
    pub source: String,
    pub command: Option<String>,
    pub source_commit: Option<SemCommitId>,
    pub world_model_run_id: Option<WorldModelRunId>,
}
```

The current runtime stores a single `parent_commit_id` on `SemCommitV1`. Merge
ancestry is still preserved, but it lives in the paired reconciliation object
(`base_commit_id`, `left_commit_id`, `right_commit_id`) rather than in a
materialized multi-parent commit DAG node.

### Commit kinds

```rust
pub enum SemCommitKind {
    Promote,
    EvidenceCommit,
    ProjectionMaterialization,
    Merge,
    Validation,
    WorldModelRun,
    TagMove,
    Admin,
}
```

### State ref

The commit points at existing materialized stores.

```rust
pub struct SemStateRefV1 {
    pub accepted_snapshot_id_before: Option<AcceptedSnapshotId>,
    pub accepted_snapshot_id_after: Option<AcceptedSnapshotId>,
    pub accepted_tree_digest: Option<AxiDigest>,
    pub pathdb_snapshot_id_before: Option<PathdbSnapshotId>,
    pub pathdb_snapshot_id_after: Option<PathdbSnapshotId>,
    pub evidence_digests: Vec<ProposalDigest>,
}
```

Semantically meaningful ontology commits (`Promote`, `Merge` into main, `TagMove`)
must provide before/after accepted anchors when changed state is expected.

### Delta

```rust
pub struct SemDeltaV1 {
    pub module_digests_added: Vec<AxiDigest>,
    pub module_digests_removed: Vec<AxiDigest>,
    pub semantic_delta: Option<EvolutionSemanticDeltaV1>,
    pub trust_summary: Option<SemTrustSummaryV1>,
    pub rule_summary: Option<SemRuleSummaryV1>,
    pub coverage_summary: Option<EvolutionCoverageSummaryV1>,
    pub evidence_blobs_added: Vec<ProposalDigest>,
    pub certificate_refs_added: Vec<String>,
    pub quality_report_refs_added: Vec<String>,
    pub validation_report_refs_added: Vec<String>,
    pub projection_manifest_refs_added: Vec<AxiDigest>,
    pub lifecycle_events: Vec<SemLifecycleEventV1>,
    pub world_model_run_refs: Vec<WorldModelRunId>,
}
```

`semantic_delta`, `trust_summary`, `rule_summary`, and `coverage_summary` are the
compact typed sidecars copied from a stored `EvolutionPreviewV1` when a commit
is derived from a proposal or promotion preview. They are intentionally small:
enough for semantic history and diffing, not a duplicate of the full preview
report.

```rust
pub struct EvolutionSemanticDeltaV1 {
    pub delta_kind: String,
    pub subject_refs: Vec<String>,
    pub primitives: Vec<EvolutionPrimitiveV1>,
    pub changed_layers: Vec<String>,
    pub schema: TypedChangeBucketV1,
    pub theory: TypedChangeBucketV1,
    pub instance: TypedChangeBucketV1,
    pub context: TypedChangeBucketV1,
    pub total_added: usize,
    pub total_reused: usize,
    pub total_removed: usize,
    pub notes: Vec<String>,
}
```

`primitives` is where semantic history stops pretending that ontology
evolution is only bucket arithmetic. It carries explicit reviewable structural
moves such as:

- `reify_relation_object`
- `introduce_dependent_relation_family`
- `introduce_subtype`
- `generalize_to_supertype`
- `specialize_to_subtype`
- `push_relation_role_to_subtype`
- `pull_relation_role_to_supertype`
- `factor_common_structure_to_supertype`
- `split_type_into_subtypes`
- `merge_types_under_supertype`
- `lift_relation_to_carrier`
- `add_path_equation`
- `add_rewrite_rule`

These are intended to be the machine-readable currency for typed directed
exploration, olog refinement, migration review, and semantic merge. The coarse
`schema` / `theory` / `instance` / `context` buckets stay useful for compact
history summaries, but they are no longer sufficient on their own to explain
what kind of ontology move actually happened.

The intended rule is:

- keep full previews in `sem/validations/`
- keep compact gate/delta/trust/rule/coverage summaries in commits, refs, and reconciliations
- do not copy full quality/CQ/runtime-semantic payloads into commit history

`primitives` is the compact semantic summary that matters most for ontology
review. Generic add/remove counts are not enough to explain whether a
change:

- introduced a new subtype rather than an unrelated object,
- generalized two local concepts into a reusable supertype,
- specialized a previously overloaded type,
- pushed or pulled a role across a relation-object boundary,
- factored common structure out of sibling relations,
- split or merged concepts,
- or lifted a binary edge into a first-class relation carrier.

Those distinctions are what reviewers, migration tooling, and exploration
surfaces need to preserve across preview, commit history, merge, and promotion.

### Backend projection manifests

Backend materialization is tracked explicitly rather than being inferred from a
backend-local schema or dataset.

```rust
pub struct ProjectionManifestV1 {
    pub projection_id: AxiDigest,
    pub accepted_snapshot_id: AcceptedSnapshotId,
    pub source_sem_ref_name: Option<String>,
    pub source_sem_commit_id: Option<SemCommitId>,
    pub compiled_ir_digest: AxiDigest,
    pub materialization_ref: String,
    pub backend: BackendCapabilityProfileV1,
    pub projection: ProjectionCapabilityProfileV1,
    pub object_mappings: Vec<ProjectionObjectMappingV1>,
    pub relation_mappings: Vec<ProjectionRelationMappingV1>,
    pub context_mapping: ProjectionContextMappingV1,
    pub trust_caveats: Vec<String>,
    pub round_trip_limitations: Vec<String>,
}

pub struct ProjectionCapabilityProfileV1 {
    pub preserves_relation_objects: bool,
    pub preserves_context_world_axes: bool,
    pub supports_anchor_scoped_query_pushdown: bool,
    pub supports_context_scoped_query_pushdown: bool,
    pub native_query_access: ProjectionNativeQueryAccessV1,
    pub mutation_authority: ProjectionMutationAuthorityV1,
}

pub enum ProjectionNativeQueryAccessV1 {
    None,
    ReadOnlyPartial,
    ReadOnlyAnchorScoped,
}

pub enum ProjectionMutationAuthorityV1 {
    AxiographOnly,
    BackendWritableMirror,
}
```

The current implementation now has a planner layer above this manifest model:

- `axiograph_cli::backend_pushdown::build_backend_pushdown_plan(...)`
- `TypeDbPushdownPlanV1`
- `TerminusDbPushdownPlanV1`
- `BackendPushdownOperationalSurfaceV1`

Those pushdown plans are generated directly from `CompiledSchemaIr` plus the
backend/projection capability profiles. They are intentionally explicit about:

- tuple encoding (`relationship_entity` for `TypeDB`, `reified_fact` for
  `TerminusDB`),
- role preservation and context-axis handling,
- whether carrier edges are materialized only as lossless convenience views,
- the native read-only query dialect,
- preserved lower-tier interfaces such as native query, RDF dataset, and SHACL
  validation surfaces where the backend actually supports them,
- the lifting contracts that carry those lower-tier surfaces back into
  anchor-scoped Axiograph semantics,
- and which semantics remain Axiograph-only even when backend-native querying is
  allowed.

`BackendPushdownOperationalSurfaceV1` is the compact operational summary over
those plans. It is the current agent-facing transport/reindexing seam: the place
where tooling can inspect relation transport, context-axis handling, residual
obligations, and the explicit reconciliation boundary without parsing the full
backend plan or confusing backend-native history with semantic merge state.

The current implementation direction is:

- `TypeDB` as the primary high-fidelity typed backend target,
- `TerminusDB` as the strongest RDF/VCS-shaped secondary target,
- and property-graph engines such as `Apache AGE`, `Neo4j`, and related
  backends as experimental projection targets rather than first-class support.

The capability profile is expected to distinguish at least:

- typed-schema / relation-role / n-ary support,
- typed query validation and logic/function pushdown,
- immutable history / branch / merge / diff support,
- and schema-vs-instance separation.

That split matters because the recommended pushdown is intentionally asymmetric:

- `TypeDB` is where we should push the richest runtime type/constraint/query
  surface.
- `TerminusDB` is where we should exploit backend-native history/branch/diff
  features for projected collaboration views.
- property-graph engines are where we may eventually push execution/indexing
  and hybrid SQL/openCypher workloads, but only after they clear the same
  typed projection bar.

Projected backend usability still matters. The intended contract is:

- native backend interfaces should remain queryable so outside tools can read a
  reduced, backend-shaped view of accepted semantic state,
- those native interfaces are read-only projected lenses rather than the full
  Axiograph query/type/certificate surface,
- and mutation authority stays in Axiograph unless a future manifest
  deliberately opts into some weaker mirror mode.

Even where a backend has native VCS-like features, semantic authority stays in
Axiograph. TerminusDB is the clearest example: its git-for-data branch model is
useful, but its own transport docs say schema operations are not pushed/pulled
with ordinary branch synchronization. That means backend-native history can
mirror semantic workspaces, but it cannot replace Axiograph's schema/theory
review history.

## Lifecycle Events

```rust
pub struct SemLifecycleEventV1 {
    pub artifact: ArtifactRefV1,
    pub from: Option<LifecycleStage>,
    pub to: LifecycleStage,
    pub reason: Option<String>,
}

pub enum LifecycleStage {
    Proposed,
    Validated,
    Reviewed,
    Accepted,
    Certified,
    Superseded,
    Retracted,
}
```

These events are the semantic audit trail for facts, modules, proposals,
certificates, and world-model outputs.

## Reconciliation Objects

Merges that require actual semantic choice should emit a first-class
reconciliation object.

```rust
pub struct SemReconciliationV1 {
    pub reconciliation_id: ReconciliationId,
    pub base_commit_id: SemCommitId,
    pub left_commit_id: SemCommitId,
    pub right_commit_id: SemCommitId,
    pub policy: String,
    pub source_ref_name: Option<String>,
    pub target_ref_name: Option<String>,
    pub resolved_ref_name: Option<String>,
    pub outcome_commit_id: Option<SemCommitId>,
    pub conflicts: Vec<ConflictRecordV1>,
    pub decisions: Vec<DecisionRecordV1>,
    pub certificate_refs: Vec<String>,
}
```

The reconciliation object is where “merge” becomes ontology review rather than
filesystem merge.

The current runtime also persists a paired `ReconciliationPreviewReportV1`
under `sem/validations/`. That report stores the `EvolutionPreviewV1` used to
fail closed on unresolved conflicts before a `SemCommitKindV1::Merge` commit is
materialized.

Current operator surface:

```bash
axiograph db accept reconciliation-show \
  --dir build/accepted_plane \
  --reconciliation fnv1a64:...

axiograph db accept reconciliation-apply \
  --dir build/accepted_plane \
  --reconciliation fnv1a64:... \
  --handle-id typed_refine_v1:...
```

`reconciliation-show` returns the stored typed preview, including any compiled-IR
refinement handles. `reconciliation-apply` persists the selected decision back
into the reconciliation object and returns the typed before/after apply result.

## World-Model Run Objects

World-model lineage is persisted as first-class, branch-resolved objects, not as
ephemeral server state.

```rust
pub struct WorldModelRunV1 {
    pub run_id: WorldModelRunId,
    pub branch_ref: String,
    pub status: WorldModelRunStatus,
    pub base_commit: Option<SemCommitId>,
    pub start_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub start_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub end_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub end_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub model_backend: String,
    pub model_version: Option<String>,
    pub plugin_version: Option<String>,
    pub config_digest: Option<String>,
    pub cost_profile_digest: Option<String>,
    pub started_utc: String,
    pub finished_utc: Option<String>,
    pub run_error: Option<String>,
    pub proposal_digests: Vec<ProposalDigest>,
    pub proposal_meta: Vec<WorldModelProposalMetaV1>,
    pub evaluation_refs: Vec<String>,
    pub generated_by: String,
    pub promotion_commit: Option<SemCommitId>,
}

pub enum WorldModelRunStatus {
    Pending,
    Running,
    ProposalsReady,
    Validated,
    Reconciled,
    Promoted,
    Failed,
    Aborted,
}

pub struct WorldModelProposalMetaV1 {
    pub proposal_digest: ProposalDigest,
    pub source: String,
    pub generated_utc: String,
    pub quality_score: Option<f64>,
    pub proposal_file_ref: Option<String>,
}
```

Persistence invariants for world-model runs:

1. Exactly one file exists under `sem/world_model_runs/<run_id>.json` for each
   run.
2. A run must include `branch_ref`, `model_backend`, `started_utc`, and at least
   one `start_*` anchor (`start_accepted_snapshot_id` or
   `start_pathdb_snapshot_id`) before it can be materialized.
3. Proposal emission must append immutable `proposal_digests` and may append
   optional metadata in `proposal_meta`.
4. A run must be closed with `status = Promoted`, `Failed`, or `Aborted`, and
   `finished_utc` set.

## Branch Model

Target branch model:

- `wm/<experiment>` for proposal streams
- `review/<topic>` for reconciled candidate ontology changes
- `main` for accepted ontology/world state

`WorldModelRunV1` is expected to be recorded on `refs/heads/wm/<experiment>` and
any commit emitted in that branch that creates proposal commitments must include
its `world_model_run_id` in commit provenance.

## CLI Surface

Target commands:

```text
axiograph sem init
axiograph sem branch <name>
axiograph sem checkout <ref>
axiograph sem status
axiograph sem show [<ref>]
axiograph sem log
axiograph sem diff <a> <b> --semantic
axiograph sem merge <source> --into <target> --policy <policy>
axiograph sem tag <name> [<ref>]
axiograph sem promote ...
axiograph sem supersede <artifact>
axiograph sem retract <artifact>
```

CLI must expose persisted run objects as read-only records:

- `sem show --object=world-model --run <id>` should render the run payload and linked commits.
- `sem log --with-runs` should surface active and failed `WorldModelRunV1` entries.

## Diff Semantics

`sem diff` should report at least four layers:

1. schema/module diff
2. theory/rewrite/constraint diff
3. instance/context diff by stable fact ids
4. artifact diff:
   - evidence blobs
   - certificates
   - lifecycle transitions
   - world-model runs

It should not be limited to raw text diff.

## Merge Semantics

`sem merge` means:

1. find merge base,
2. compute semantic diffs,
3. auto-merge disjoint changes,
4. create an explicit conflict set when both sides changed the same semantic
   object incompatibly,
5. write a reconciliation object when review is required,
6. only then materialize a merge commit.

This is the core semantic distinction from Git's file merge model.

## Materialization Rules

The semantic VCS should not replace current accepted/WAL storage immediately.

Instead:

- accepted-plane promotion can emit semantic commits,
- PathDB WAL commits can emit evidence-branch semantic commits,
- `sem checkout --materialize` can later synchronize legacy `HEAD` views to the
  selected semantic ref.

## Validation and Persistence Rules

- Every `SemCommitV1` that references a `WorldModelRunId` must have a
  corresponding file in `sem/world_model_runs/<run_id>.json`.
- `WorldModelRunV1` must persist `proposal_digests` as proposal anchors, not inlined
  proposals.
- Branch commits for `refs/heads/wm/*` must only carry commits of kind
  `WorldModelRun`, `EvidenceCommit`, `Merge`, or `Validation` so semantic history
  stays auditable and machine-parseable.
- `SemCommitV1` should include stable accepted/pathdb anchor pairs before/after whenever it changes state.
- Reconciliations must be explicit `SemReconciliationV1` objects.
- Slice manifests persisted under `sem/slices/` should be built from accepted
  canonical modules compiled to `KernelModuleIr` whenever the accepted snapshot
  is available. This keeps semantic VCS merge/rebase planning attached to
  schema/category refs, theory obligations, and instance-functor refs instead
  of commit-summary metadata alone.

## Evolution Preview Sidecars

`EvolutionPreviewV1` is the canonical review artifact currently shared by
proposal validation and accepted-plane promotion. In addition to the full
quality/CQ/runtime-semantic payloads, the object now carries compact sidecars
meant for semantic-history use:

- `semantic_delta`: compact schema/theory/instance/context change summary
- `trust_summary`: compact trust/non-claim summary
- `rule_summary`: compact runtime-visible/review-only rule inventory
- `coverage_summary`: compact CQ/semantic-coverage surface summary
- `exploration_next_actions`: directed follow-on review suggestions derived from structural evolution primitives

The rule for semantic history is:

- persist the full `EvolutionPreviewV1` under `sem/validations/`
- copy the compact `semantic_delta` / `trust_summary` / `rule_summary` /
  `coverage_summary` sidecars into `SemDeltaV1`
- copy only the compact gate summary into commits/refs
- Review-critical reports should be stored under `sem/validations/` and cited by
  path/ref from semantic commits or reconciliations rather than reconstructed
  from logs.

## First Implementation Slice

The first shipping slice should minimize surface area and lock down persistence
before full UX polish:

1. Add `sem/` storage with refs and commit logs; make `sem/world_model_runs/` required.
2. Add persisted schemas for:
   - `SemCommitV1`
   - `SemStateRefV1`
   - `WorldModelRunV1`
   - `WorldModelProposalMetaV1`
3. Auto-emit commits when:
   - `db accept promote` succeeds,
   - `db accept pathdb-commit` succeeds.
4. Persist run manifests from world-model proposal/planning flows, including:
   - `run_id`
   - `branch_ref`
   - anchors (`start_accepted_snapshot_id`, `start_pathdb_snapshot_id`)
   - `proposal_digests`
5. Implement run-linked validation checks:
   - unknown `WorldModelRunId` in commit references is an error,
   - wm branch commits require a valid `branch_ref`.
6. Ship read-only first:
   - `sem status`
   - `sem log`
   - `sem show`
   - `sem diff`
   - `sem branch`
7. Add `sem merge --dry-run` before full merge materialization.
