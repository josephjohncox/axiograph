# Authenticated PathDB server

**Diataxis:** How-to  
**Audience:** operators and integrators

The database server is read-only and serves one immutable SQLite `.axpd`
materialization. It never accepts a bare file, accepted-plane `HEAD`, mutable
role, custom WAL, or sidecar.

## Prerequisites

You need:

- an AxiStore with an accepted protected-main build manifest;
- a published materialization image and receipt under
  `materializations/`;
- the exact `MaterializationIdV2` returned by publication.

Publication is performed by `axiograph_store::AxiStore::publish_axpd`, the
`axiograph db materialize --dir <store> --spec <build-spec.json>` adapter, or the
kernel integration `axiograph_pathdb::materialization::publish_kernel_pathdb`.
All require an explicit `AxpdBuildSpec` whose snapshot, tree, ordered module
closure, kernel, and fact-log anchors exactly match a manifest that reached
protected accepted main. An initialized-but-empty store or review/evidence
candidate is rejected. Configuration and ordered overlay anchors remain part of
the materialization identity.

## Start the server

```bash
axiograph db serve \
  --dir build/axi_store \
  --materialization axi:materialization:v2:sha256:<digest> \
  --listen 127.0.0.1:7878
```

For an ephemeral port and a machine-readable readiness receipt:

```bash
axiograph db serve \
  --dir build/axi_store \
  --materialization "$MATERIALIZATION_ID" \
  --listen 127.0.0.1:0 \
  --ready-file build/server-ready.json
```

The readiness file is written only after receipt/image verification and listener
binding. It contains:

```json
{
  "format": "axiograph_db_server_ready_v2",
  "listen": "127.0.0.1:49152",
  "materialization_id": "axi:materialization:v2:sha256:...",
  "exact_image_digest": "axi:object-blob:v2:sha256:..."
}
```

## Startup verification

Before publishing the listener, the server checks:

1. the named receipt exists and is bounded;
2. the receipt materialization id matches the requested id;
3. the exact SQLite image exists under the store family;
4. file size, SQLite prefix, application id, schema version, page count, and
   exact table set;
5. `quick_check`, foreign-key/cross-row constraints, and configured limits;
6. accepted repository/snapshot/tree/module/kernel/fact-log anchors against a
   protected-main manifest in validated AxiStore audit history;
7. ordered overlay anchors, canonical logical digest, and exact-image digest;
8. recomputed `MaterializationIdV2` and receipt equality.

PathDB hydration and process-local index construction happen only after those
checks pass. Failure exits before the ready file or listener is visible.

## HTTP API

### Health

```bash
curl -fsS http://127.0.0.1:7878/healthz
```

Returns `ok` only for a running, already-verified process.

### Status

```bash
curl -fsS http://127.0.0.1:7878/status | jq
```

The response includes entity/relation counts and the complete authenticated
materialization receipt:

```json
{
  "format": "axiograph_authenticated_pathdb_status_v2",
  "loaded_at_unix_secs": 0,
  "entities": 0,
  "relations": 0,
  "receipt": {}
}
```

### Query

```bash
curl -fsS -X POST http://127.0.0.1:7878/query \
  -H 'content-type: application/json' \
  -d '{"query":"select ?x where ?x : Person limit 20"}' | jq
```

The body is bounded to 1 MiB and denies unknown fields. Parse or execution
failures return structured JSON with HTTP 400. This endpoint is execution-only:
it does not accept `certificate_policy` and does not claim certified answers.
Use the typed certificate/MCP surfaces when a query certificate policy is
required.

## MCP

The stdio MCP service uses the same authenticated open path:

```bash
axiograph mcp \
  --dir build/axi_store \
  --materialization "$MATERIALIZATION_ID"
```

Optional approved Lean checker flags remain available for MCP certificate tools:
`--verify-bin`, `--verify-sha256`, `--verify-build-id`, and
`--verify-timeout-secs`.

## Runtime-only path cache

A deeper-path LRU can be enabled without changing durable identity:

```bash
axiograph db serve \
  --dir build/axi_store \
  --materialization "$MATERIALIZATION_ID" \
  --path-index-lru-capacity 4096 \
  --path-index-lru-async \
  --path-index-lru-queue 1024
```

The cache is process-local, bounded, and disposable. It is not written as a
sidecar or included in accepted meaning.

## Recovery

If an image or receipt is missing or corrupt, rebuild from accepted inputs with
`AxiStore::recover_axpd`. The recovery path verifies existing files,
quarantines invalid image/receipt pairs, and publishes a deterministic rebuild.
Never repair a materialization by editing SQLite rows or copying an unbound
`.axpd` file into place.
