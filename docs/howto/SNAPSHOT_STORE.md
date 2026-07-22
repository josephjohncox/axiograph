# AxiStore Accepted-State Authority

**Diataxis:** How-to  
**Audience:** operators and contributors

AxiStore is the sole persistence authority for accepted ontology state and
semantic lineage. Canonical accepted UTF-8 `.axi` bytes remain the meaning
plane. PathDB and `.axpd` remain derived query materializations.

## Layout

```text
<store>/
  catalog.sqlite
  catalog.sqlite-wal
  catalog.sqlite-shm
  objects/
    sha256/
      <64-lowercase-hex>
  materializations/
    <MaterializationIdV2>.axpd
    <MaterializationIdV2>.receipt.json
```

Do not copy this directory to replicate authority and do not copy mutable
pointers. Replication needs an authenticated object-transfer and catalog
protocol; direct filesystem replication is deliberately not provided.

The SQLite catalog uses the `AXIS` application id, the greenfield V2 schema,
a pinned 4096-byte page size, WAL mode, `synchronous=FULL`, foreign
keys, strict tables, bounded SQLite parser/runtime limits, and a bounded busy
timeout. Catalog, WAL, SHM, object, publication, module, ref, and reachable-DAG
limits are checked before unbounded reads or decoding. Catalog and immutable
object paths must be regular files under real directories; symlink substitution
fails closed.

The immutable object directory stores exact repository descriptors, `.axi`
revisions, trees, snapshots, build artifacts, reports, certificates, receipts,
reconciliations, and semantic commits.

## Initialize

Use the library API in `axiograph-store`:

```rust
use axiograph_store::{AxiStore, RepositoryDescriptor};

let descriptor = RepositoryDescriptor::new(
    "example-ontology",
    "operator-generated-genesis-nonce",
)?;
let store = AxiStore::init("build/axi-store", &descriptor)?;
```

The descriptor's exact canonical bytes create the repository genesis identity.
Reopening with a different descriptor fails.

## Build a promotion

A `PromotionPlan` contains:

- every exact accepted module byte string;
- the sorted `AcceptedTree`;
- an `AcceptedSnapshot` with zero, one, or two ordered parents;
- immutable compiled IR, canonical fact log, validation, CQ, theory, and
  checker-receipt objects;
- an `AcceptedBuildManifest` binding that complete closure and explicit
  non-claims;
- an optional immutable `Reconciliation` for a merge;
- one `SemanticCommit` binding the repository, ordered parents, typed delta,
  gates, attachments, provenance, lifecycle events, and exact accepted
  anchors; and
- optional review/evidence ref updates targeting that commit.

Protected main requires all four gate families: canonical validation,
competency questions, runtime theory, and trusted-checker receipt. A failed or
missing gate rejects promotion. Runtime validation remains a Rust finite check;
it is not relabeled as a Lean proof.

## Promote with generation CAS

```rust
let before = store.status()?;
let after = store.promote(before.state.generation, &plan)?;
assert_eq!(after.state.generation, before.state.generation + 1);
```

Promotion validates per-object and aggregate publication limits, then publishes
and fsyncs immutable objects first. It uses one `BEGIN IMMEDIATE` transaction to
validate the complete closure, insert catalog rows, move protected `heads/main`
and requested review refs, append the contiguous audit sequence/hash chain, and
CAS the singleton `store_state` row. A stale writer receives
`AxiStoreError::StaleState`; the winner cannot lose an update or split accepted
snapshot and semantic heads.

## Candidate branches and tags

`publish_candidate` publishes a complete authenticated commit to a review or
evidence branch without changing accepted main. It still advances the singleton
generation, ref-map digest, and audit tail in one transaction.

```rust
let status = store.publish_candidate(
    generation,
    "heads/review/schema-change",
    expected_tip.as_ref(),
    &candidate_plan,
)?;

let tagged = store.create_tag(
    status.state.generation,
    "tags/release-2026-01",
    &commit_id,
)?;
```

Tags are immutable. `heads/main` cannot be moved through the branch API.

## Inspect and prove lineage

```rust
let status = store.status()?;
let branches = store.branches()?;
let tags = store.tags()?;
let base = store.merge_base(&left_tip, &right_tip)?;
let proof = store.lineage_proof(
    &subject,
    &ancestor,
    &LineagePin::SubjectCommit(subject.clone()),
)?;
```

Merge-base selection computes maximal common ancestors. One candidate succeeds;
zero fails; multiple maximal candidates return
`AxiStoreError::AmbiguousMergeBase`. No distance heuristic or digest tie-breaker
chooses between criss-cross bases.

A lineage proof must pin the exact current state when its subject is accepted
main, or independently pin the subject commit. Repository identity alone is not
an acceptable proof anchor.

## Publish and open PathDB materializations

Construct `AxpdBuildSpec` from the exact accepted build manifest, compiled
kernel snapshot, canonical fact log, configuration, and ordered overlays. Then:

```rust
let receipt = store.publish_axpd(build_spec, &AxpdLimits::default())?;
let verified = store.open_axpd(
    &receipt.materialization_id,
    &AxpdLimits::default(),
)?;
```

The receipt binds accepted snapshot/tree/module/kernel/fact-log/overlay anchors,
the canonical logical digest, exact SQLite image digest, versions, and
configuration. Publication, recovery, and every open also require those anchors
to match a build manifest whose semantic commit appeared as protected accepted
main in the checksum-validated AxiStore audit history. Repository membership or
a review/evidence candidate manifest is insufficient. Opening then recomputes
all receipt and image fields before returning rows. Use `load_verified_pathdb`
to hydrate runtime indexes after verification.

## Determinism scope

Identical repository descriptors and promotion inputs produce identical object,
tree, snapshot, manifest, commit, ref-map, audit, and `StoreState` identities
and identical immutable object bytes. Mutable `catalog.sqlite`/WAL page bytes are
not an identity surface and are not claimed to be byte-for-byte deterministic;
SQLite owns their physical layout. `.axpd` has separate logical and exact-image
digests under a pinned materializer/SQLite version.

## Restart and corruption checks

`AxiStore::open` validates:

- repository descriptor identity and exact bytes;
- every reachable object hash and kind;
- accepted tree module ordering and exact revision bytes;
- snapshot and commit parent ordering, closure, and acyclicity;
- build-manifest artifacts and explicit non-claims;
- reconciliation base/tips, decisions, preview, and certificates;
- protected main, every branch/tag target, and ref-map digest;
- the complete audit hash chain; and
- the singleton accepted snapshot/commit/manifest tuple.

Missing parents, cycles, partial state, tampered catalog fields, or changed
object bytes reject the store. Unreachable immutable objects left by a crash
before catalog commit are harmless and may be garbage-collected only by a
future closure-aware maintenance operation.

## Focused gate

```bash
make verify-axi-store
# or
cd rust && cargo test -p axiograph-store
```

The gate covers failure injection after every object write/fsync/publication,
materialization image/receipt write/fsync/publication, and catalog
begin/event/state/commit step. It also covers concurrent CAS writers, catalog
identity/schema/size substitution, object-size limits, audit sequence/checksum
tampering, missing parents, cycles, ordered merge parents, maximal-base
ambiguity, independently pinned lineage subjects, branch movement, tags, and
restart.
