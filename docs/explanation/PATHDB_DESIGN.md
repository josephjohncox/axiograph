# PathDB: Authenticated SQLite Execution Substrate

**Diataxis:** Explanation  
**Audience:** contributors

PathDB is Axiograph's derived query engine. Accepted `.axi` modules and compiled
`KernelSnapshotIr` define meaning; a `.axpd` file is an immutable, disposable
SQLite materialization of that accepted state. It is not an ontology kernel,
review surface, semantic history, or source-recovery format.

## Ownership and trust boundary

There is one persistence family: `axiograph_store::AxiStore`.

- AxiStore owns accepted objects, snapshots, trees, manifests, semantic commits,
  refs, audit records, and materializations.
- `AxiStore::publish_axpd` is the only public `.axpd` writer and rejects anchors
  not found in a manifest that previously reached protected accepted main.
- `AxiStore::open_axpd` rechecks accepted-manifest membership, an immutable
  receipt, and the exact SQLite image before exposing rows. Same-repository
  review/evidence candidates do not qualify.
- `axiograph_pathdb::materialization::load_verified_pathdb` hydrates the in-memory
  indexes only after store verification succeeds.
- PathDB never reconstructs or exports accepted `.axi` text.

The intended flow is:

```text
exact accepted .axi closure
        |
        v
KernelSnapshotIr + AcceptedBuildManifest + ordered overlays
        |
        v
AxpdBuildSpec --canonicalize/validate--> SQLite backup image
        |
        v
exact-image + logical digests + semantic anchors
        |
        v
AxiStore/materializations/<MaterializationIdV2>.axpd
        |
        v
verified read-only open --> PathDB hydration --> rebuildable indexes
```

## SQLite format

`.axpd` is ordinary SQLite with fixed invariants:

- SQLite `application_id` is `AXPD`;
- `user_version` is the greenfield Axiograph V2 schema version; V1 rejects;
- page size and materializer configuration are pinned;
- the table set is exact (unknown or missing tables reject);
- every relation is represented by declared foreign keys and explicit row
  validation;
- `quick_check` must pass before hydration;
- the connection opens read-only with bounded SQLite limits; and
- image and receipt paths must be regular files, never symlinks.

The logical schema stores:

- `materialization_meta`: format, versions, configuration digest, semantic
  anchors, logical digest, exact-image digest, and materialization id;
- `module_closure`: exact ordered revision digests;
- `kernel_refs`: canonical compiled-IR references;
- `entities`: instance/object/value rows with stable keys;
- `relation_facts`: stable fact and relation ids;
- `projections`: ordered role values and typed targets;
- `contexts`: context/world/temporal axes;
- `equivalences`: reversible generator mappings;
- `overlays`: ordered, typed, content-digested extension layers.

Interned strings, entity ordinals, LRU state, fact indexes, text indexes, and path
indexes are process-local implementation details. They are never persisted as
semantic state.

## Identity and authentication

A materialization carries two different digests:

1. **Logical digest** — framed SHA-256 over canonical semantic rows and anchors.
   It is independent of insertion order and SQLite page layout.
2. **Exact-image digest** — SHA-256 over the final SQLite bytes.

`MaterializationIdV2` binds both digests plus:

- repository id;
- accepted snapshot id;
- accepted tree id;
- ordered module closure;
- kernel IR digest;
- canonical fact-log digest;
- overlay order and digests;
- materializer and SQLite versions;
- configuration digest.

The store-family receipt is named by the materialization id and is itself
revalidated against the image. Changing the receipt and image together does not
help an attacker: open recomputes the exact digest, logical digest, anchors, and
materialization id before returning `VerifiedAxpd`.

## Determinism

The materializer sorts and validates every logical row family before writing.
Equivalent semantic input in a different insertion order produces the same
logical digest and, under the pinned SQLite/materializer configuration, the same
exact image. A semantic row change must change the logical digest.

Determinism is version-scoped. A SQLite or materializer version change yields a
new materialization identity even if the logical rows are unchanged.

## Bounded validation

Both build and open are bounded. Limits cover:

- file bytes and SQLite pages;
- module, kernel-reference, entity, fact, projection, context, equivalence, and
  overlay counts;
- UTF-8 string bytes;
- projection fanout;
- SQLite SQL length, expression depth, column count, compound selects, attached
  databases, and parameter counts.

Counts are checked at exact N/N+1 boundaries. Arbitrary bytes, old bincode files,
old sectioned `AXPD` files, truncation, table substitution, anchor mutation,
logical-row mutation, and digest substitution all fail closed.

## Publication and recovery

AxiStore publication builds in memory, backs up to a private temporary SQLite
file, fsyncs it, computes both digests, reopens and verifies it, then performs
immutable image and receipt renames. Failure injection covers populate, backup,
validation, image publication, receipt write/fsync, and receipt publication.
Restart sees no openable materialization until both immutable files validate;
a completed pair remains reusable even if the caller failed after publication.

`AxiStore::recover_axpd` handles a missing or corrupt derived image:

1. deterministically rebuild a private candidate from accepted inputs;
2. verify any existing image and receipt against the candidate identity;
3. reuse a valid image;
4. quarantine corrupt image/receipt files;
5. publish the rebuilt pair without mutating accepted state.

Deleting and rebuilding a materialization preserves the logical digest and
finite query answers. Cache loss is therefore a performance event, not semantic
loss.

## In-memory query structures

After verified hydration, PathDB preserves each finite n-ary fact as a fact
object with ordered typed role projections. Context/world/temporal rows remain
ordinary role projections and also gain the derived `axi_fact_in_context` scope
edge used by the runtime fact index. Reversible generators hydrate into the
process-local equivalence index. These structures preserve the checked finite
rows; they do not establish univalence, arbitrary higher paths, general
transport, or a Lean theorem about the Rust hydrator.

PathDB then uses:

- compact string interning;
- column-oriented entity attributes;
- forward/backward relation indexes;
- Roaring bitmaps for set operations;
- bounded precomputed path indexes;
- lazy fact and text indexes;
- optional process-local LRU entries for deep paths;
- an equivalence index for reversible mappings.

All of these structures are rebuildable. No index sidecar is durable.

### Fact index

Canonical n-ary tuples are represented as fact nodes with stable fact ids and
ordered role edges. The in-memory `FactIndex` accelerates:

- relation and schema/relation lookup;
- key lookup from declared constraints;
- context/world-scoped fact lookup.

AxQL uses these indexes automatically. Index invalidation occurs on in-memory
mutation; no index content enters the `.axpd` identity.

### Path index

Short relation paths can be precomputed into Roaring-bitmap reachability maps.
Longer paths may use a bounded process-local LRU. The path index depth and LRU
capacity affect runtime performance only; semantic rows and certificates remain
anchored to stable ids.

## Operational rule

Never open a bare `.axpd` by path in a query service. Require an AxiStore root and
`MaterializationIdV2`, call `load_verified_pathdb`, and only then serve queries.
If exact accepted `.axi` bytes are needed for review or certificate checking,
read them from the accepted AxiStore object closure—not from PathDB.
