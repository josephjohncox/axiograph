# Semantic VCS and AxiStore

**Diataxis:** Reference  
**Audience:** contributors

Semantic VCS records accepted ontology evolution, review/evidence branches,
reconciliations, immutable tags, and auditable lifecycle events. Git remains the
source-control system for source files. AxiStore is the persistence authority
for accepted meaning state and semantic lineage.

## Authority

The authority has two layers:

1. the exact ordered closure of accepted UTF-8 `.axi` module bytes; and
2. one transactional `AxiStore` rooted at a cryptographically bound repository
   descriptor.

`KernelSnapshotIr` is the immutable compiled semantic representation of the
accepted closure; it is not a second persistence authority. Runtime indexes,
PathDB, `.axpd`, evidence, embeddings, and backend projections are derived or
advisory artifacts and cannot redefine accepted meaning. Projection manifests
and evidence-only readback are specified in
`docs/reference/BACKEND_PROJECTIONS.md`.

The implementation is `rust/crates/axiograph-store`. There is no accepted-state
file pointer, JSONL authority, custom write-ahead log, compatibility reader,
legacy import checkpoint, or second semantic store.

## Persistence Layout

```text
<store>/
  catalog.sqlite
  catalog.sqlite-wal
  catalog.sqlite-shm
  objects/sha256/<64-lowercase-hex>
```

The catalog is opened with:

- the `AXIS` SQLite application id, the greenfield V2 schema, and
  4096-byte pages;
- SQLite WAL mode and `synchronous=FULL`;
- foreign keys and `trusted_schema=OFF`;
- an exact all-STRICT authority-table set;
- bounded SQLite parser/runtime limits and busy timeout; and
- explicit catalog, sidecar, object, publication, module, ref, and closure caps.

Opening validates the SQLite prefix, application/schema/page identity, exact
table set, `quick_check`, foreign keys, row counts, regular-file paths, and byte
limits before object decoding. This prevents old or substituted SQLite files
from becoming an accidental compatibility path.

The immutable object directory contains exact bytes for:

- the repository descriptor;
- accepted `.axi` revisions;
- accepted trees and snapshots;
- compiled kernel IR and canonical fact logs;
- validation, competency-question, and runtime-theory reports;
- certificates and trusted-checker receipts;
- evidence referenced by a reviewed change;
- accepted build manifests;
- reconciliations; and
- semantic commits.

Existing same-id bytes must match exactly. A same-id/different-byte publication
is corruption and fails closed.

## Primary Regulated-Shipment Fixture

`make verify-regulated-shipment` exercises the accepted-state path with real
compiled canonical modules rather than synthetic empty images:

1. promote `RegulatedShipmentBaseline.axi`;
2. publish `RegulatedShipment.axi` on `heads/review/regulated-shipment`;
3. construct a reviewed `SemReconciliationV2` with exact typed payload
   keep/drop/introduce accounting;
4. materialize an exact `[current_main_tip, source_tip]` merge;
5. derive `AxpdBuildSpec` directly from the accepted `KernelSnapshotIr`;
6. publish immutable SQLite image and receipt; and
7. reopen AxiStore and hydrate PathDB only after receipt/image verification.

The adversarial test appends bytes to the immutable image and requires restart
verification to reject it. The merge claim remains finite payload accounting,
not an arbitrary categorical pushout, general dependent transport, or Lean
proof. The type/constraint receipts attached to the candidate are actual
`VerifyMain` outputs. Exact query completeness is checked separately in the
same gate through `query_result_v4`; AxiStore does not acquire proof authority
by storing adjacent receipts.

## Identity

All persistent identities use the `AXIOGRAPH-ID` family from
`axiograph-kernel`: a closed domain registry, versioned and length-framed
fields, fixed-width counts/integers, and full lowercase SHA-256.

The implementation uses strict newtypes for repository, module, revision,
schema, object, relation, role, theory, obligation, fact, tree, snapshot,
commit, reconciliation, object blob, materialization, query, answer,
certificate, proposal, run, and checker identities. Parsing one identity kind
as another is rejected.

Repository identity is derived from the exact canonical repository descriptor.
Repository name alone is not an authority anchor.

## Accepted Tree

`AcceptedTree` binds:

- repository identity; and
- a strictly sorted, duplicate-free sequence of module name, repository-scoped
  module identity, and exact-byte revision digest.

Files are never normalized or reconstructed from PathDB. Whitespace, comments,
line endings, declaration order, and import order remain identity-significant
when they change exact accepted bytes.

## Accepted Snapshot

`AcceptedSnapshot` binds:

- repository identity;
- accepted tree identity; and
- zero, one, or two ordered parent snapshots.

Normal history has zero or one parent. A merge has ordered
`[target_snapshot, source_snapshot]` parents. Reversing parents changes the
snapshot identity.

## Accepted Build Manifest

`AcceptedBuildManifest` binds:

- repository, tree, and snapshot;
- the ordered exact module closure;
- compiler and IR versions;
- compiled `KernelSnapshotIr` bytes, identified by the AxiStore `KernelIr`
  object digest (distinct from the snapshot's inner compiler `ir_digest`);
- canonical fact-log bytes;
- validation, CQ, and runtime-theory reports;
- a trusted-checker receipt; and
- explicit non-claims.

Required non-claims include that runtime validation is not a Lean proof, the
closed finite fragment is not general dependent type theory, and the result is
not open-world completeness or ontology closure.

## Semantic Commit

`SemCommitV2` binds every review-relevant field:

- repository identity;
- kind (`normal` or `merge`);
- zero, one, or two ordered commit parents;
- accepted tree, snapshot, and build-manifest identities;
- exact reconciliation identity for a merge;
- author, timestamp, message, action, policy, and provenance;
- full typed semantic/reindex delta;
- promotion gates and exact report digests;
- typed attachments; and
- lifecycle events.

The typed delta uses explicit operations:

- preserve;
- rename;
- split;
- merge;
- drop;
- add; and
- replace.

Each operation has checked source/target cardinality and uses typed semantic
identities rather than untyped names.

Normal commits have zero or one parent. Merge commits have exactly ordered
`[target_tip, source_tip]` parents and an immutable reconciliation.

## Protected Main Gates

A commit eligible for protected main contains exactly these passed gates:

1. canonical validation;
2. competency questions;
3. trust; and
4. runtime theory.

Each gate must cite the exact corresponding digest in the accepted build
manifest. The trust gate cites the configured trusted-checker receipt; changing
that receipt without rebuilding the commit is rejected. Missing, duplicate,
failed, or merely same-kind-but-different-report gates reject promotion. The
receipt applies only to the fragment it actually checks. Runtime reports do not
become Lean proofs by being stored in the same manifest.

## SemReconciliationV2

`SemReconciliationV2` is the only accepted merge record. It binds:

- repository identity and the unique maximal common ancestor;
- exact left and right parent commits;
- reviewed parent candidates, each binding its snapshot, tree, root module,
  compiled kernel IR, canonical/CQ/trust/theory reports, reviewer, and review
  time;
- one reviewed merged candidate with the same typed anchors and gate family;
- one canonical sequence of typed `keep`, `drop`, `introduce`, or `transport`
  decisions;
- a transport-witness digest on every transport decision;
- the exact preview digest, outcome, and explicit scope non-claims.

Every addressable `KernelRefV2` receives a payload fingerprint over its complete
compiled payload. Stable-looking ids therefore cannot conceal a changed role
order, role kind, indexed/refined type, equation, rewrite, instance, or fact.
For a materialized outcome, every payload occurrence from both parents must be
covered exactly once and every merged payload must be produced exactly once.
The checker caps each candidate at 131,072 payloads and the reconciliation at
393,216 decisions. An exact keep requires both ref and fingerprint equality.
Drops and introductions are explicit. A transport changes the typed payload and
requires an immutable witness commitment.

Before writing accepted state, AxiStore recompiles all three candidates from
the exact stored `.axi` closures through `CanonicalCompiler`, reproduces the
exact kernel IR bytes and payload-fingerprint index, and compares every review
anchor. This is a finite decidable operational check. It is not arbitrary
categorical-colimit completeness, general dependent transport, univalence,
higher-path equivalence, open-world completeness, or a Lean proof.

## Authenticated Merge Materialization

Generic promotion rejects merge commits. Protected-main merges go only through
`AxiStore::materialize_merge(expected_generation, source_ref,
expected_source_tip, plan)`. In the same `IMMEDIATE` transaction, AxiStore
requires:

- the current accepted main commit and snapshot as the first ordered parents;
- the exact current non-main `heads/*` source tip as the second ordered parent;
- two distinct commit and snapshot parents;
- a materialized `SemReconciliationV2` whose reviewed left/right tips equal
  those parents in that order;
- a reviewed merged candidate equal to the plan's tree, snapshot, kernel IR,
  and four exact gate reports; and
- a unique maximal common ancestor equal to the reconciliation base.

There is no single-parent merge writer and no generic promotion bypass. Reversed
parents, stale source refs, unreviewed payload indexes, substituted gate reports,
or a reconciliation for different tips reject before state advances.

## Merge Bases

Merge-base selection is order-theoretic, not distance-based:

1. compute the complete ancestor sets of the two independently named tips;
2. intersect them;
3. remove every common ancestor dominated by a newer common ancestor; and
4. inspect the resulting maximal-common-ancestor antichain.

One maximal common ancestor is the unique merge base. Zero means no base.
Multiple maximal bases are a criss-cross ambiguity and fail closed. Distance,
timestamp, or digest tie-breakers do not choose semantic authority.

## Singleton State

One `store_state` row binds:

- repository;
- generation;
- accepted snapshot;
- accepted semantic commit;
- digest of the complete ref map;
- accepted build-manifest digest; and
- audit-event tail.

Before first promotion, accepted snapshot/commit/manifest are all null. Partial
accepted tuples are forbidden.

Protected `heads/main` must equal the accepted semantic commit. It cannot move
through the branch API. Review/evidence branches and immutable tags are stored
in the same catalog and every ref mutation advances singleton generation,
ref-map digest, and audit tail in one transaction.

An authenticated `.axpd` receipt may cite only a build manifest whose commit
appears as accepted main in that validated audit history. Merely existing in the
same repository, review branch, evidence branch, or object catalog is not an
accepted anchor.

## Promotion Transaction

Promotion is objects first, state second:

1. validate all typed objects and exact module bytes;
2. write each new immutable object to a unique temporary file;
3. fsync the file;
4. rename to its content-addressed path;
5. fsync the object directory;
6. begin one SQLite `IMMEDIATE` transaction;
7. compare expected and actual singleton generations;
8. insert object metadata and the complete reachable tree/snapshot/manifest/
   reconciliation/commit closure;
9. move protected main and requested review refs;
10. recompute the complete ref-map digest;
11. append the next audit hash-chain event;
12. update the singleton state; and
13. commit.

A crash before step 13 leaves the old complete state and possibly unreachable
immutable objects. A crash after step 13 leaves the new complete state. SQLite
serializes concurrent writers; after one expected-generation writer wins, the
other receives typed `AxiStoreError::StaleState`.

## Branches and Tags

`publish_candidate` records a complete authenticated candidate commit on a
review/evidence branch without moving accepted main. It still uses generation
CAS and atomically updates ref-map digest and audit tail.

`update_branch` permits fast-forward moves only. `create_tag` creates an
immutable tag. Recreating or moving a tag is rejected.

Target branch families are:

- `heads/review/*`;
- `heads/evidence/*`;
- `heads/evidence/proposals/*`;
- protected `heads/main`; and
- `tags/*`.

## Lineage Proofs

A lineage proof contains the exact ordered commit path from subject to ancestor
and validates every identity and parent edge. It requires one of:

- the exact current `StoreState` digest, only when the subject is current
  accepted main; or
- an independently pinned subject commit.

Repository identity alone is not enough. The proof establishes runtime digest
integrity and the claimed parent path under the pin. It does not establish
signatures, author identity, non-repudiation, historical wall-clock existence,
source truth, query completeness, or ontology closure.

## Restart Validation

`AxiStore::open` validates before returning:

- repository descriptor and identity;
- every reachable immutable object's kind, path, byte length, and raw SHA-256;
- tree identities, module ordering, module identities, and exact revision
  bytes;
- snapshot identities, parent rows, closure, and acyclicity;
- commit identities, every payload field, parent rows, closure, and acyclicity;
- build-manifest closure and every referenced object;
- reconciliation identity, reviewed candidate anchors, recompiled payload fingerprints, transport witnesses, and preview;
- protected main and every branch/tag target;
- ref-map digest;
- the complete audit hash chain; and
- singleton state consistency.

Missing parents, cycles, changed ordering, partial closure, altered object
bytes, and changed catalog fields reject the store.

## Public API

`AxiStore` exposes:

- `init` / `open`;
- `promote` / `promote_with_injector` for normal protected-main commits;
- `materialize_merge` for authenticated exact-two-parent merges;
- `publish_candidate`;
- `status`;
- `branches`;
- `tags`;
- `update_branch`;
- `create_tag`;
- `lineage_proof` / `verify_lineage_proof`;
- `maximal_common_ancestors`; and
- `merge_base`.

The failure injector exists to exercise crash boundaries. It is not a second
persistence path.

## Replication

There is no direct-copy replication API and no mutable-pointer copying. A
future replication protocol must authenticate immutable objects and advance
catalog state with the same closure and generation checks as local promotion.
Copying a live SQLite directory or selected pointer files is not a supported
state transition.

## Trust Boundary and Non-Claims

AxiStore enforces persistence integrity and finite operational lineage. Rust is
still untrusted relative to the product's Lean checker. The trusted product
boundary remains the import closure of `lean/Axiograph/VerifyMain.lean`.

AxiStore authenticates local content and finite lineage under its repository
anchor. It does not provide remote signatures, author non-repudiation, encrypted
storage, protection from a hostile process or kernel after validation, or a
formal proof of SQLite/filesystem/Rust correctness.

AxiStore also does not prove:

- frontend source-to-lowering correctness;
- general dependent type theory;
- univalence or higher inductive types;
- arbitrary higher paths;
- open-world entailment;
- backend completeness;
- query completeness beyond a separately checked finite certificate;
- ontology closure; or
- correctness of Rust itself.

## Verification Gate

```bash
make verify-axi-store
# equivalent focused Rust gate:
cd rust && cargo test -p axiograph-store
```

The suite covers every object write/fsync/publication and catalog
begin/event/state/commit failure point, concurrent generation-CAS writers,
tampering, missing parents, cycles, dominated and criss-cross common ancestors,
multiple maximal bases, authenticated exact-two-parent merge materialization,
stale source refs, forged payload fingerprints, substituted trust reports,
union-accounting failures, independent subject pins, branches, tags, and
restart.
