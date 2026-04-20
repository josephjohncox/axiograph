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

## Relationship To Existing Stores

Current stores remain useful:

- accepted-plane module/snapshot store
- PathDB WAL and checkpoints
- certs/
- quality/

The semantic VCS sits above them and points at them.

It should not duplicate large artifacts; it should reference them.

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
  world_model_runs/
  validations/
```

`commits/`, `reconciliations/`, and `world_model_runs/` are canonical persistence
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
    pub commit_id: SemCommitId,
    pub parent_ids: Vec<SemCommitId>,
    pub author: String,
    pub timestamp_utc: String,
    pub message: String,
    pub kind: SemCommitKind,
    pub policy: Option<String>,
    pub provenance: SemCommitProvenanceV1,
    pub state: SemStateRefV1,
    pub delta: SemDeltaV1,
    pub reconciliation_id: Option<ReconciliationId>,
}

pub struct SemCommitProvenanceV1 {
    pub source: String,
    pub command: Option<String>,
    pub source_commit: Option<SemCommitId>,
    pub world_model_run_id: Option<WorldModelRunId>,
}
```

### Commit kinds

```rust
pub enum SemCommitKind {
    Promote,
    EvidenceCommit,
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
    pub accepted_tree_digest: Option<String>,
    pub pathdb_snapshot_id_before: Option<PathdbSnapshotId>,
    pub pathdb_snapshot_id_after: Option<PathdbSnapshotId>,
    pub evidence_digest: Option<String>,
}
```

Semantically meaningful ontology commits (`Promote`, `Merge` into main, `TagMove`)
must provide before/after accepted anchors when changed state is expected.

### Delta

```rust
pub struct SemDeltaV1 {
    pub module_digests_added: Vec<AxiDigest>,
    pub module_digests_removed: Vec<AxiDigest>,
    pub evidence_blobs_added: Vec<String>,
    pub certificate_refs_added: Vec<String>,
    pub quality_report_refs_added: Vec<String>,
    pub lifecycle_events: Vec<SemLifecycleEventV1>,
    pub world_model_run_refs: Vec<WorldModelRunId>,
}
```

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
    pub base: SemCommitId,
    pub left: SemCommitId,
    pub right: SemCommitId,
    pub policy: String,
    pub conflicts: Vec<ConflictRecordV1>,
    pub decisions: Vec<DecisionRecordV1>,
    pub certificate_refs: Vec<String>,
}
```

The reconciliation object is where “merge” becomes ontology review rather than
filesystem merge.

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
