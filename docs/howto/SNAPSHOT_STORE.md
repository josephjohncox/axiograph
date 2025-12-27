# Snapshot Store (Accepted Plane + PathDB WAL)

**Diataxis:** How-to  
**Audience:** users (and operators)

This repo treats **canonical `.axi`** as the source of truth, and treats **PathDB**
(`.axpd`) as a derived, rebuildable index for fast query/REPL workflows.

To make continuous ingest and discovery practical (without rebuilding from
scratch every time), we store an **append-only PathDB WAL** *under* the accepted
plane directory.

## Goals

- `.axi` is canonical, reviewable, diffable.
- Promotion into the accepted plane is explicit and append-only.
- PathDB snapshots are derived from accepted snapshots and can be checked out by
  id.
- Extension-layer mutations (doc chunks, heuristic links, etc.) are stored as
  WAL ops and are **not** part of the certified core unless explicitly promoted
  into canonical `.axi`.

## Directory layout

An accepted-plane directory (default: `build/accepted_plane`) contains:

```
build/accepted_plane/
  modules/<ModuleName>/<digest>.axi
  snapshots/<accepted_snapshot_id>.json
  accepted_plane.log.jsonl
  HEAD

  pathdb/
    blobs/<digest>.chunks.json
    blobs/<digest>.proposals.json
    snapshots/<pathdb_snapshot_id>.json
    checkpoints/<pathdb_snapshot_id>.axpd
    pathdb_wal.log.jsonl
    HEAD
```

### Accepted-plane snapshots

- `HEAD` points to the latest accepted snapshot id.
- Each accepted snapshot manifest lists the set of modules (by digest + stored path).
- Snapshot ids are **content-derived** (stable): change the module set and the
  snapshot id changes.

### PathDB WAL snapshots

- `pathdb/HEAD` points to the latest **PathDB** snapshot id.
- A PathDB snapshot manifest records:
  - the accepted snapshot id it is derived from, and
  - a cumulative list of WAL ops (currently: `ImportChunksV1`, `ImportProposalsV1`).
- A checkpoint `.axpd` is stored per snapshot id for fast checkout.

## CLI usage

For “git-like” inspection, you can also use:

```bash
cd rust
axiograph db accept init --dir ../build/accepted_plane
axiograph db accept status --dir ../build/accepted_plane
axiograph db accept list --dir ../build/accepted_plane --layer accepted --limit 20
axiograph db accept list --dir ../build/accepted_plane --layer pathdb --limit 20
axiograph db accept show --dir ../build/accepted_plane --layer accepted --snapshot head
axiograph db accept show --dir ../build/accepted_plane --layer pathdb --snapshot head
axiograph db accept log --dir ../build/accepted_plane --layer accepted --limit 20
axiograph db accept log --dir ../build/accepted_plane --layer pathdb --limit 20
```

Snapshot ids can be passed as:
- `head` / `latest`
- a full id (`fnv1a64:...`)
- or a unique prefix (e.g. `a05581cb` or `fnv1a64:a05581cb`)

## Serving snapshots (HTTP)

If you want a long-running process that keeps a snapshot loaded and serves
queries/viz over HTTP, use:

- `axiograph db serve` (documented in `docs/howto/DB_SERVER.md`).

### 1) Promote canonical `.axi` into the accepted plane

```bash
cd rust
axiograph db accept promote ../examples/economics/EconomicFlows.axi \
  --dir ../build/accepted_plane \
  --message "reviewed: initial economics module"
```

This prints the new accepted snapshot id.

### 2) Build a base `.axpd` from an accepted snapshot

```bash
cd rust
axiograph db accept build-pathdb \
  --dir ../build/accepted_plane \
  --snapshot latest \
  --out ../build/accepted_base.axpd
```

### 3) Commit extension-layer overlays into the PathDB WAL (optional)

This is useful for discovery workflows: you can import doc/code chunks as graph
nodes to enable `fts(...)` / `contains(...)`, viz, and LLM grounding.

```bash
cd rust
axiograph db accept pathdb-commit \
  --dir ../build/accepted_plane \
  --accepted-snapshot latest \
  --chunks ../build/ingest_chunks.json \
  --message "import chunks for discovery"
```

You can also preserve cross-domain extracted structure by importing a
`proposals.json` file (Evidence/Proposals schema) into the WAL:

```bash
cd rust
axiograph db accept pathdb-commit \
  --dir ../build/accepted_plane \
  --accepted-snapshot latest \
  --proposals ../build/proposals.json \
  --message "preserve evidence-plane proposals"
```

This prints the new PathDB snapshot id (distinct from the accepted snapshot id).

### 4) Check out a `.axpd` from the PathDB WAL snapshot

```bash
cd rust
axiograph db accept pathdb-build \
  --dir ../build/accepted_plane \
  --snapshot latest \
  --out ../build/accepted_with_chunks.axpd
```

## Trust boundary (important)

- The accepted plane is the **canonical meaning plane**.
- The PathDB WAL is a **derived query substrate** that can include non-certified
  overlays.

Rule of thumb:

- **Certified answers** should be anchored to accepted `.axi` inputs (digest +
  extracted fact ids) and checked by Lean.
- WAL overlays are for:
  - discovery,
  - evidence navigation,
  - retrieval/grounding,
  - interactive ontology engineering,
  - and performance (incremental ingest).

If/when an overlay becomes “accepted knowledge”, it should be **explicitly
promoted** into canonical `.axi` (and thus becomes part of the accepted snapshot).

## Master/replica (read replicas)

The simplest distributed deployment is:

- a **single write master** that runs promotion + WAL commits, and
- one or more **read replicas** that sync the snapshot store directory and serve queries/viz/REPL.

Because the store is “mostly immutable objects + small HEAD pointers”, replication can be done by
copying missing objects and then updating HEAD.

### Filesystem sync (v1)

Use the built-in sync command:

```bash
cd rust
axiograph db accept sync \
  --from ../build/master_plane \
  --dir  ../build/replica_plane \
  --layer both \
  --include-checkpoints
```

Notes:

- `--no-update-head` copies immutable objects only (safer for staged rollouts).
- `--include-checkpoints` is optional: replicas can rebuild `.axpd` checkpoints from manifests + blobs.
