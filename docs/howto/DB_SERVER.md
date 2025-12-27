# PathDB Server (`axiograph db serve`)

**Diataxis:** How-to  
**Audience:** users (and operators)

`axiograph db serve` runs a small HTTP server that keeps a PathDB snapshot loaded
in memory for low-latency querying and visualization.

This is **tooling / deployment glue**, not part of the trusted kernel.

Trusted boundary reminder:

- Rust runs queries and (optionally) emits certificates.
- Lean verifies certificates (see `docs/howto/FORMAL_VERIFICATION.md`).

---

## Start a server from a `.axpd` snapshot

```bash
make binaries

bin/axiograph db serve \
  --axpd build/my_snapshot.axpd \
  --listen 127.0.0.1:7878
```

Endpoints:

- `GET /healthz`
- `GET /status`
- `GET /contexts` (list contexts/worlds + fact counts)
- `GET /snapshots` (store-backed only; list snapshots for time travel)
- `GET /anchor.axi` (export the loaded snapshot as a PathDBExportV1 `.axi` anchor)
- `GET /entity/describe?id=<id>` (on-demand full-snapshot entity details for UIs/LLM grounding)
- `POST /query` (AxQL)
- `POST /cert/reachability` (emit a reachability certificate for a directed relation-id chain)
- `GET /viz` (HTML)
- `GET /viz.json` (JSON)
- `GET /viz.dot` (Graphviz DOT)
- `POST /llm/to_query` (LLM: question → query)
- `POST /llm/agent` (LLM: tool-loop, recommended)
- `POST /discover/draft-axi` (untrusted: draft canonical `.axi` from `proposals.json` content)

---

## Query over HTTP (AxQL)

```bash
curl -sS http://127.0.0.1:7878/status | jq .
```

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","show_elaboration":true}'
```

Default contexts/worlds (applied only when the query text has no explicit `in ...`):

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?x where ?x is Person limit 5","contexts":["123"],"show_elaboration":true}'
```

Time-travel query (store-backed only):

```bash
curl -sS http://127.0.0.1:7878/snapshots | jq .
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{"snapshot":"<snapshot_id>","query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","show_elaboration":true}'
```

Certified queries (optional)

If you request `certify:true`, the server emits a Lean-checkable certificate anchored to the current snapshot’s exported `.axi` digest.

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","certify":true}' | jq .
```

If you request `verify:true`, the server will also run the Lean checker (`axiograph_verify`) server-side and attach the result:

```bash
make lean-exe

curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","certify":true,"verify":true}' | jq .
```

---

## Visualize over HTTP

HTML (static snapshot of the currently loaded DB):

```bash
open 'http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320'
```

Live-ish HTML (auto-refresh):

```bash
open 'http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320&refresh_secs=2'
```

Time-travel HTML (render a historical snapshot):

```bash
curl -sS http://127.0.0.1:7878/snapshots | jq .
open 'http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320&snapshot=<snapshot_id>'
```

JSON (for your own frontends):

```bash
curl -sS 'http://127.0.0.1:7878/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320' > graph.json
```

---

## Export the snapshot anchor `.axi`

The Lean checker verifies certificates against a canonical `.axi` anchor. For PathDB-backed snapshots, the server can export that anchor on demand:

```bash
curl -sS http://127.0.0.1:7878/anchor.axi > anchor.axi
```

Time travel (store-backed only):

```bash
curl -sS http://127.0.0.1:7878/anchor.axi\?snapshot\=<snapshot_id> > anchor.axi
```

---

## Serve directly from a snapshot store (accepted plane + WAL)

Instead of giving the server a `.axpd`, you can point it at an accepted-plane
directory. The server will build the derived `.axpd` internally.

Serve the canonical accepted plane (`HEAD`):

```bash
bin/axiograph db serve \
  --dir build/my_plane \
  --layer accepted \
  --snapshot head \
  --listen 127.0.0.1:7878 \
  --watch-head
```

Serve the PathDB WAL layer (`pathdb/HEAD`):

```bash
bin/axiograph db serve \
  --dir build/my_plane \
  --layer pathdb \
  --snapshot head \
  --listen 127.0.0.1:7878 \
  --watch-head
```

---

## Container + Kubernetes

The Docker image (root `Dockerfile`) starts `axiograph db serve` by default:

```bash
docker run --rm -p 7878:7878 \
  -v "$(pwd)/build/accepted_plane:/data/accepted" \
  ghcr.io/axiograph/axiograph:latest
```

Kubernetes manifests live in `deploy/k8s/` and a Helm chart lives in
`deploy/helm/axiograph/`. The StatefulSet mounts a PVC at `/data` and serves
`/viz` and `/query` on port 7878.

---

## Master vs replica roles (distributed-ish mode)

The snapshot store gives you a practical “write-master / read-replica” shape.

- `--role master` enables **admin** endpoints (write operations).
- `--role replica` is read-only and defaults to `--watch-head`.

Admin endpoints (master only):

- `POST /admin/reload`
- `POST /admin/accept/promote`
- `POST /admin/accept/pathdb-commit`

If you set `--admin-token <token>`, admin requests must include:

```text
Authorization: Bearer <token>
```

For a runnable distributed demo, see:

- `scripts/db_server_distributed_demo.sh`
- `scripts/db_server_api_demo.sh` (single-node HTTP query + viz)
- `scripts/db_server_live_viz_demo.sh` (watching `/viz?...&refresh_secs=N` while promoting updates)

---

## LLM in the `/viz` UI (server mode)

The self-contained HTML explorer supports an LLM panel when served over HTTP.

Important: this is still an **untrusted** runtime convenience feature:

- the LLM proposes structured tool calls / queries,
- Rust executes them against the snapshot,
- you can later require certificates + Lean verification for high-value results.

The UI also supports:

- On-demand DB details (`DB` tab) via `GET /entity/describe` (so the neighborhood graph doesn’t need to embed all edges/attrs).
- Context filtering powered by server-provided context membership (more robust when the neighborhood graph is truncated).
- A simple lifecycle flow:
  - LLM can propose overlay changes (untrusted),
  - UI can commit them to the PathDB WAL (admin token),
  - UI can draft a candidate canonical `.axi` module from the overlay (untrusted),
  - UI can promote reviewed `.axi` into the accepted plane (admin token).

---

## “Add data” as proposals (schema-aware + validated)

The DB server exposes deterministic endpoints to generate **untrusted**
`proposals.json` overlays for “add a fact / relationship” UX.

These endpoints do **not** mutate the loaded snapshot: they return a reviewable
overlay, plus a *validation preview* (meta-plane typecheck + quality/lint delta).

Generate a single relation proposal:

```bash
curl -sS -X POST http://127.0.0.1:7878/proposals/relation \
  -H 'Content-Type: application/json' \
  -d '{
    "rel_type":"Parent",
    "source_name":"Jamison",
    "target_name":"Bob",
    "schema_hint":"Fam",
    "context":"FamilyTree",
    "validate":true,
    "quality_profile":"fast",
    "quality_plane":"both"
  }' | jq .
```

Response fields (subset):

- `proposals_json`: Evidence/Proposals schema payload
- `chunks`: optional `DocChunk` evidence (if `evidence_text` was provided)
- `validation`: preview import summary + typecheck + *delta* quality findings

To actually apply the overlay to the snapshot store (evidence-plane WAL), use
the master-only admin commit endpoint:

```bash
curl -sS -X POST http://127.0.0.1:7878/admin/accept/pathdb-commit \
  -H "Authorization: Bearer $AXIOGRAPH_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "accepted_snapshot":"head",
    "proposals": <paste proposals_json here>,
    "chunks": <paste chunks here>,
    "message":"add Jamison as Bob'\''s child"
  }' | jq .
```

This writes a WAL commit under the snapshot store’s `pathdb/` layer and (when
serving `pathdb/head`) auto-reloads so the UI can see it immediately.

### Enable LLM endpoints

Mock backend (offline, deterministic; good for demos/tests):

```bash
bin/axiograph db serve --axpd build/my_snapshot.axpd --listen 127.0.0.1:7878 --llm-mock
```

Ollama backend (local models):

```bash
ollama serve
bin/axiograph db serve --axpd build/my_snapshot.axpd --listen 127.0.0.1:7878 --llm-ollama --llm-model nemotron-3-nano
```

Tune the default tool-loop step limit (when a client does not pass `max_steps`):

```bash
export AXIOGRAPH_LLM_MAX_STEPS=12
```

Then open:

```bash
open 'http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320'
```

### Call LLM endpoints directly (optional)

Tool-loop (recommended):

```bash
curl -sS -X POST http://127.0.0.1:7878/llm/agent \
  -H 'Content-Type: application/json' \
  -d '{"question":"find the grandparents of Alice","max_steps":6,"max_rows":25}' | jq .
```

Question → query (lower-level helper):

```bash
curl -sS -X POST http://127.0.0.1:7878/llm/to_query \
  -H 'Content-Type: application/json' \
  -d '{"question":"list ProtoService"}' | jq .
```

---

## Server-side certificate verification (Lean)

`axiograph db serve` can optionally invoke the trusted Lean checker as an external process.

Preferred: build and install the verifier into `bin/`:

```bash
make lean-exe
```

The server auto-discovers `bin/axiograph_verify` when running `bin/axiograph db serve ...`.

You can also configure it explicitly:

- CLI: `axiograph db serve --verify-bin /path/to/axiograph_verify`
- Env: `AXIOGRAPH_VERIFY_BIN=/path/to/axiograph_verify`

Use `GET /status` to confirm the server sees the verifier:

```bash
curl -sS http://127.0.0.1:7878/status | jq .certificates
```

---

## RAG grounding: `DocChunk` evidence + hybrid semantic search (embeddings)

The server’s LLM “tool loop” works best when your snapshot contains **DocChunk**
evidence nodes (untrusted grounding pointers).

There are two recommended retrieval layers:

1) **Always-on deterministic retrieval** (no model):
   - token-hash vectors + an in-memory HNSW index (fast, reproducible).
2) **Optional model embeddings** (Ollama):
   - computed once and stored **snapshot-scoped** in the PathDB WAL as CBOR blobs.

### A) Import `chunks.json` into the PathDB WAL

If you serve from a snapshot store (`--dir ... --layer pathdb`), commit chunks as
an extension-layer overlay:

```bash
bin/axiograph db accept pathdb-commit \
  --dir build/my_plane \
  --accepted-snapshot head \
  --chunks build/ingest/chunks.json \
  --message "add doc chunks overlay"
```

### B) Compute snapshot-scoped embeddings (Ollama) and store them in the WAL

```bash
ollama serve
ollama pull nomic-embed-text

bin/axiograph db accept pathdb-embed \
  --dir build/my_plane \
  --snapshot head \
  --target docchunks \
  --ollama-model nomic-embed-text \
  --message "embed doc chunks"
```

The `/llm/agent` tool loop can then use:

- `fts_chunks` (fast, token-based search over DocChunks), and
- `semantic_search` (hybrid: token-hash ANN + optional Ollama embeddings)

to find relevant entities and evidence, and cite `chunk_id`s in answers.
