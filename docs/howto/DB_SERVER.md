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
- `GET /entity/describe?id=<id>` (on-demand full-snapshot entity details for UIs/LLM grounding)
- `POST /query` (structured `query_ir_v1`)
- `GET /viz` (HTML)
- `GET /viz.json` (JSON)
- `GET /viz.dot` (Graphviz DOT)

Note: the `/viz` HTML is served from the Vite frontend in `frontend/viz/dist`.
Build it first with `make viz-build` (or `cd frontend/viz && npm install && npm run build`).

CLI HTML exports now write a directory with `index.html`, `graph.json`, and
`assets/`. Open `index.html?data=graph.json`.
- `POST /llm/to_query` (LLM: question -> structured `query_ir_v1`)
- `POST /llm/agent` (LLM: tool-loop, recommended)
- `POST /evidence/proposals/predict` (predictive proposal adapter -> evidence-plane `proposals.json`)
- `POST /planning/proposal-rollout` (bounded proposal rollout -> proposals + costs)
- `POST /discover/draft-axi` (untrusted draft canonical `.axi` from `proposals.json` content, now with a typed-authoring lifecycle/trust summary so callers can distinguish `draft_only` from `validated` drafts)
- `POST /discover/check-olog` (check a typed olog fragment against canonical `.axi`; subtype-aware, relation-object aware, and explicit about fragment-only soundness)
- `POST /semantic/business-rule` (compute a typed business-rule applicability report for one relation/theory scope under the current loaded snapshot/meta-plane)
- `POST /semantic/coverage` (compute agent-facing semantic coverage over implementation surfaces, rules, and typed coverage edges under the current loaded snapshot/meta-plane)
- `POST /semantic/agent-report` (compose an agent-facing engineering report over a task, mapped implementation surfaces, rule applicability, semantic coverage, and next actions under the current loaded snapshot/meta-plane)

---

## Query over HTTP (`query_ir_v1`)

```bash
curl -sS http://127.0.0.1:7878/status
```

Pipe curl output to `jq` or another JSON viewer only when you want pretty
printing.

Structured query IR is the machine-facing query contract for HTTP/tooling:

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{
        "lang":"query_ir_v1",
        "query_ir_v1":{
          "version":1,
          "select_vars":["?gc"],
          "where_atoms":[
            {"kind":"edge","left":"Alice","path":"Grandparent","right":"?gc"}
          ],
          "limit":10
        },
        "show_elaboration":true
      }'
```

Default contexts/worlds (applied only when the query text has no explicit `in ...`):

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{
        "lang":"query_ir_v1",
        "query_ir_v1":{
          "version":1,
          "select_vars":["?x"],
          "where_atoms":[
            {"kind":"type","term":"?x","type":"Person"}
          ],
          "limit":5
        },
        "contexts":["123"],
        "show_elaboration":true
      }'
```

Time-travel query (store-backed only):

```bash
curl -sS http://127.0.0.1:7878/snapshots
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{
        "snapshot":"<snapshot_id>",
        "lang":"query_ir_v1",
        "query_ir_v1":{
          "version":1,
          "select_vars":["?gc"],
          "where_atoms":[
            {"kind":"edge","left":"Alice","path":"Grandparent","right":"?gc"}
          ],
          "limit":10
        },
        "show_elaboration":true
      }'
```

When `show_elaboration:true`, the response includes:

- `compiled_query_ir_v1`: the normalized structured query surface,
- `elaborated_query_ir_v1`: the structured query IR the runtime actually
  prepared after elaboration,
- `elaborated_query`: the best-effort elaborated AxQL text,
- `inferred_types`, `notes`, `typed_holes`, `exploration_suggestions`, and
  `plan` when available.
- `trust`: a uniform trust contract for the response:
  - `trust_class`: `certifiable`, `mixed`, or `execution_only`
  - `soundness`: whether row soundness is only available, emitted as a certificate, or Lean-verified
  - `coverage`: whether the response trust applies to the whole query or only a mixed/runtime-only execution mode
  - `scope`: currently always snapshot-scoped, with explicit context mode (`unscoped`, `single_context`, `multi_context`)
- `support_summary` may appear for accepted-anchor certifiable queries even when
  `certificate_policy` is `none`:
  - the wire field stays `support_summary`,
  - `basis.certificate_emitted_to_client` tells you whether the support basis was
    returned as the top-level `certificate` or kept internal to the runtime,
  - `basis.certificate_kind` is currently `query_result_v3`,
  - and `contexts` / `evidence` are supplemental attachments, not the support basis.

Certified queries (optional)

Use `certificate_policy` for all query certificate behavior. The supported
values are `none`, `emit`, `verify`, and `require_verified`; boolean-style
request fields are not part of the public contract.

If you request `"certificate_policy":"emit"`, the server emits a Lean-checkable
typed query witness anchored to the current canonical `.axi` digest.

```bash
curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{
        "lang":"query_ir_v1",
        "query_ir_v1":{
          "version":1,
          "select_vars":["?gc"],
          "where_atoms":[
            {"kind":"edge","left":"Alice","path":"Grandparent","right":"?gc"}
          ],
          "limit":10
        },
        "certificate_policy":"emit"
      }'
```

If you request `"certificate_policy":"verify"`, the server will also run the
Lean checker (`axiograph_verify`) server-side and attach the result:

```bash
make lean-exe

curl -sS -X POST http://127.0.0.1:7878/query \
  -H 'Content-Type: application/json' \
  -d '{
        "lang":"query_ir_v1",
        "query_ir_v1":{
          "version":1,
          "select_vars":["?gc"],
          "where_atoms":[
            {"kind":"edge","left":"Alice","path":"Grandparent","right":"?gc"}
          ],
          "limit":10
        },
        "certificate_policy":"verify"
      }'
```

The response still remains a **soundness-oriented** surface:

- `trust.soundness = lean_verified_row_soundness` means the returned rows are backed by a verified certificate,
- it does **not** mean the server has proved query completeness or full ontology closure,
- and mixed/runtime-only query forms continue to report their caveats explicitly in `trust`.

Raw AxQL remains a human-facing REPL/debug surface, but it is no longer the
HTTP wire contract for `POST /query`.

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

Time-travel HTML (render a prior snapshot):

```bash
curl -sS http://127.0.0.1:7878/snapshots
open 'http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320&snapshot=<snapshot_id>'
```

JSON (for your own frontends):

```bash
curl -sS 'http://127.0.0.1:7878/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=2&max_nodes=320' > graph.json
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
    "source_field":"child",
    "target_field":"parent",
    "schema_hint":"Fam",
    "context":"FamilyTree",
    "validate":true,
    "quality_profile":"fast",
    "quality_plane":"both"
  }'
```

## Typed Olog Check

Use `POST /discover/check-olog` when you want to check a human-authored olog
fragment against canonical `.axi` without mutating the snapshot.

The check is:

- subtype-aware,
- relation-object aware,
- explicit about context/time roles,
- and conservative about direct arrows: if a relation has extra business data
  roles beyond a carrier pair, the checker pushes you toward a relation box plus
  explicit projections instead of silently collapsing it to a binary edge.

```bash
curl -sS -X POST http://127.0.0.1:7878/discover/check-olog \
  -H 'Content-Type: application/json' \
  -d '{
        "axi_text": "module Demo\n\nschema S:\n  object Person\n  relation Parent(parent: Person, child: Person)\n\ninstance I of S:\n  Person = {Alice, Bob}\n  Parent = {(parent=Bob, child=Alice)}\n",
        "schema_name": "S",
        "fragment": {
          "boxes": [
            {"box_id": "parent", "object_type": "Person"},
            {"box_id": "child", "object_type": "Person"}
          ],
          "aspects": [
            {
              "aspect_id": "parent_of",
              "from_box": "parent",
              "to_box": "child",
              "kind": {"kind": "relation_carrier", "relation": "Parent"}
            }
          ]
        }
      }'
```

The response includes a `checked_olog` object with:

- lifecycle/trust summary,
- explicit diagnostics,
- the selected schema id,
- and, when available, the Rust-side well-typed module proof for the base
  canonical draft.

If the response includes `checked_olog.refinement_candidates[*].handle.id`, you
can pass one of those ids back as `apply_refinement_handle_id` to apply the
typed authoring repair against the same canonical `.axi` draft and fragment:

```bash
curl -sS -X POST http://127.0.0.1:7878/discover/check-olog \
  -H 'Content-Type: application/json' \
  -d '{
        "axi_text": "module Demo\n\nschema S:\n  object Person\n  object Team\n  object Context\n  relation WorksFor(employee: Person, employer: Team, ctx: Context)\n\ntheory SRules on S:\n  constraint key WorksFor(employee, employer, ctx)\n",
        "schema_name": "S",
        "fragment": {
          "boxes": [
            {"box_id": "employee", "object_type": "Person"},
            {"box_id": "team", "object_type": "Team"},
            {"box_id": "ctx", "object_type": "Context"}
          ],
          "relation_boxes": [
            {
              "box_id": "works_for_fact",
              "relation": "WorksFor",
              "role_bindings": [
                {"role": "employee", "target_box": "employee"},
                {"role": "ctx", "target_box": "ctx"}
              ]
            }
          ]
        },
        "apply_refinement_handle_id": "olog_refine_v1:fnv1a64:..."
      }'
```

When a refinement handle is applied the response also includes
`applied_refinement`, which records:

- the handle that was applied,
- the base fragment,
- the refined fragment,
- and the updated typed check for the refined fragment.

## Semantic Coverage

Use `POST /semantic/coverage` when you want an agent-usable report of which
runtime rules are covered by concrete implementation surfaces such as endpoints,
jobs, reports, or migrations.

This is not generic retrieval coverage. It is a typed report over:

- ontology/business-rule scope ids,
- implementation surfaces,
- coverage edges (`tested`, `implemented`, `documented_only`, `drifted`, etc.),
- and missing obligations under the currently loaded snapshot.

```bash
curl -sS -X POST http://127.0.0.1:7878/semantic/coverage \
  -H 'Content-Type: application/json' \
  -d '{
        "surfaces": [
          {
            "surface_id": "endpoint:parent_lookup",
            "kind": "endpoint",
            "label": "GET /parent",
            "scopes": [
              {
                "scope_id": "schema/s/relation/parent",
                "schema": "S",
                "scope_class": "relation",
                "relation": "Parent"
              }
            ],
            "code_refs": ["src/parent.rs"]
          }
        ],
        "edges": [
          {
            "surface_id": "endpoint:parent_lookup",
            "rule_id": "schema/s/relation/parent/rule/functional/0",
            "status": "tested",
            "notes": ["covered by endpoint integration test"]
          }
        ]
      }'
```

The response includes a typed `coverage` object with:

- per-surface rule applicability,
- per-rule best coverage status,
- uncovered rule ids,
- missing obligations,
- and suggested next actions.

## Business-Rule Applicability

Use `POST /semantic/business-rule` when you want a typed answer to:

- which accepted/review-state rules apply to this relation or theory scope?
- are those rules runtime-enforced, advisory, or review-only?
- what obligations are still missing before a strong engineering claim is justified?

```bash
curl -sS -X POST http://127.0.0.1:7878/semantic/business-rule \
  -H 'Content-Type: application/json' \
  -d '{
        "scope": {
          "scope_id": "schema/s/relation/parent",
          "schema": "S",
          "scope_class": "relation",
          "relation": "Parent"
        }
      }'
```

The response includes a typed `report` object with:

- relation/theory scope identity,
- accepted snapshot anchor and lifecycle state,
- trust class and claim strength,
- matched rule ids,
- missing obligations,
- and suggested next actions.

## Agent-Facing Engineering Report

Use `POST /semantic/agent-report` when a coding/operations agent needs one
typed report for a concrete engineering task rather than separate rule and
coverage calls.

This is the current API-facing contract for questions like:

- what business rules apply to this endpoint/job/report/workflow?
- which of those rules are accepted, runtime-enforced, review-only, or only weakly mapped?
- what semantic coverage/drift exists across the supplied implementation surfaces?
- what should the agent do next in code, tests, docs, CQ assets, or ontology review?

```bash
curl -sS -X POST http://127.0.0.1:7878/semantic/agent-report \
  -H 'Content-Type: application/json' \
  -d '{
        "task": {
          "task_id": "task:family_endpoint_alignment",
          "label": "Align family endpoint",
          "objective": "check whether endpoint and docs match accepted ontology",
          "languages": ["rust", "typescript"],
          "artifact_refs": ["src/family.rs", "ui/family.tsx"]
        },
        "surfaces": [{
          "surface_id": "endpoint:family_tree",
          "kind": "endpoint",
          "label": "GET /family/tree",
          "scopes": [{
            "scope_id": "schema/s/relation/parent",
            "schema": "S",
            "scope_class": "relation",
            "relation": "Parent"
          }],
          "code_refs": ["src/family.rs"]
        }],
        "edges": [{
          "surface_id": "endpoint:family_tree",
          "rule_id": "schema/s/relation/parent/rule/functional/0",
          "status": "implemented",
          "notes": ["checked in endpoint handler"]
        }]
      }'
```

The response includes a typed `report` object with:

- task identity and artifact refs,
- accepted snapshot anchor and lifecycle state,
- matched scope ids and rule ids,
- per-surface applicability reports,
- aggregated semantic coverage,
- residual unknowns,
- and suggested next actions.

Like the rest of the runtime semantic surfaces, this is an anchor-scoped
engineering report. It does not claim full ontology closure or exhaustive
implementation completeness.

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
  }'
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
  -d '{"question":"find the grandparents of Alice","max_steps":6,"max_rows":25}'
```

Question -> query (lower-level helper):

```bash
curl -sS -X POST http://127.0.0.1:7878/llm/to_query \
  -H 'Content-Type: application/json' \
  -d '{"question":"list ProtoService"}'
```

---

### Enable predictive proposal adapters

Stub backend (no proposals; good for wiring/tests):

```bash
bin/axiograph db serve \
  --axpd build/my_snapshot.axpd \
  --listen 127.0.0.1:7878 \
  --proposal-adapter-stub
```

Command plugin backend:

```bash
bin/axiograph db serve \
  --axpd build/my_snapshot.axpd \
  --listen 127.0.0.1:7878 \
  --proposal-adapter-plugin /path/to/proposal_adapter \
  --proposal-adapter-plugin-arg=--some-flag \
  --proposal-adapter-model my_adapter
```

Use command plugins only when you need a local/offline adapter boundary. Normal
HTTP clients call the typed server endpoints directly; they should not copy a
Python adapter or custom JSON-RPC layer.

Call the endpoint:

```bash
curl -sS -X POST http://127.0.0.1:7878/evidence/proposals/predict \
  -H 'Content-Type: application/json' \
  -d '{"goals":["predict missing parent links"],"max_new_proposals":50}'
```

The server derives the predictive proposal request from the canonical `.axi`
module stored in the current snapshot, then attaches typed lineage anchors
(`axi_digest_v1`, `pathdb_snapshot_id`, `accepted_snapshot_id`) in the request
metadata. It does not send derived snapshot text to predictive proposal
adapters.

Bounded rollout endpoint (multi-step proposal search/evaluation):

```bash
curl -sS -X POST http://127.0.0.1:7878/planning/proposal-rollout \
  -H 'Content-Type: application/json' \
  -d '{"horizon_steps":3,"rollouts":2,"max_new_proposals":50,"goals":["fill missing parent links"]}'
```

Stepwise auto-commit (commit each step and reload between steps):

```bash
curl -sS -X POST http://127.0.0.1:7878/planning/proposal-rollout \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <token>' \
  -d '{"horizon_steps":3,"rollouts":2,"max_new_proposals":50,"auto_commit":true,"commit_stepwise":true}'
```

The response includes `commit_steps` (one WAL commit per step).

Competency questions are typed review-gate objects. For human-authored suites,
prefer `.cq` files and the CLI/REPL loaders; the HTTP API still receives JSON
because it is a wire protocol. See `examples/competency_questions/physics.cq`
and `scripts/physics_bounded_proposal_rollout_server_demo.sh` for a complete
request builder.

```bash
curl -sS -X POST http://127.0.0.1:7878/planning/proposal-rollout \
  -H 'Content-Type: application/json' \
  --data @build/physics_bounded_proposal_rollout_server_demo/plan_request.json
```

To auto-commit the resulting proposals into the PathDB WAL, include:

```bash
curl -sS -X POST http://127.0.0.1:7878/evidence/proposals/predict \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <token>' \
  -d '{"goals":["predict missing parent links"],"auto_commit":true,"quality":"fast","quality_plane":"both"}'
```

Guardrail weights + task costs:

```bash
curl -sS -X POST http://127.0.0.1:7878/evidence/proposals/predict \
  -H 'Content-Type: application/json' \
  -d '{"guardrail_weights":{"quality_error":20,"rewrite_rule_error":8},"task_costs":[{"name":"latency","value":3.2,"weight":0.5,"unit":"ms"}]}'
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
curl -sS http://127.0.0.1:7878/status
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

### A) Import `EvidenceChunkBundleV1` into the PathDB WAL

If you serve from a snapshot store (`--dir ... --layer pathdb`), commit typed
chunk evidence as an extension-layer overlay. The file is a
`EvidenceChunkBundleV1`, not canonical ontology truth and not a bare chunk
array:

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
  --embed-model nomic-embed-text \
  --message "embed doc chunks"
```

The `/llm/agent` tool loop can then use:

- `fts_chunks` (fast, token-based search over DocChunks), and
- `semantic_search` (hybrid: token-hash ANN + optional Ollama embeddings)

to find relevant entities and evidence, and cite `chunk_id`s in answers.
