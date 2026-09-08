# Visualize canonical ontology modules

**Diataxis:** Tutorial  
**Audience:** users

Visualization is an inspection aid, not semantic authority. The current CLI
visualizer accepts exact canonical `.axi` input and builds process-local PathDB
state for neighborhood extraction. It does not accept bare `.axpd` files.

## Build a graph artifact

From the repository root:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools viz examples/manufacturing/SupplyChainHoTT.axi \
  --out build/supply-chain.dot \
  --format dot \
  --plane both \
  --focus-name RawMetal_A \
  --hops 2
```

Available formats are `dot`, `json`, and `html`. Plane selection is `data`,
`meta`, or `both`.

HTML output requires the runtime assets from this checkout (exact Node/npm pins
remain required for release acceptance):

```bash
(cd frontend/viz && npm ci --ignore-scripts && npm run build)
```

For a bounded whole-module view:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools viz examples/Family.axi \
  --out build/family.html \
  --format html \
  --plane both \
  --all \
  --max-nodes 250 \
  --max-edges 4000
```

## Evidence chunks

Document chunks remain typed evidence. Load them only into process-local query
state or include them as an explicitly ordered, content-digested materialization
overlay. Do not append them through a PathDB WAL and do not treat `DocChunk`
relationships as accepted ontology truth.

## Authenticated server distinction

`axiograph db serve` serves one immutable AxiStore materialization and exposes
`GET /healthz`, `GET /status`, `GET /capabilities`, `POST /query`, and optional
fixed `GET /viz`. It does not expose the old mutable explorer/admin API. Start it
with:

```bash
axiograph db serve \
  --dir build/axi_store \
  --materialization "$MATERIALIZATION_ID"
```

After building assets, open `http://127.0.0.1:7878/viz`. The page is cached at
startup after image authentication. Missing/unbuilt/over-budget assets return
503 with `ui_available=false` in capabilities; health/status/query still work.
There is no static file-directory route. Offline HTML exports instead open at
`build/family/index.html`; assets are read at export time, not embedded by Rust
compilation.

Use the QueryIrV1 JSON editor, not an AxQL string. It sends only `{query}` and
leaves resolution, typing and execution to Rust. The capability schema describes
an explicit profile, not total Serde/semantic equivalence. Full output retains
trust, non-claims and truncation; result IDs are server-image-local and cannot
highlight the local graph without atomic source binding. Local graph/draft
inspection and prefill remain, but remote mutation, LLM, proposals, draft
generation, evidence lookup, describe and certification are unavailable.

See [DB Server](../howto/DB_SERVER.md) for startup and query commands and
[Testing](../howto/TESTING.md#read-only-database-client-workflow) for the explicit
production-client workflow. The server `/query` endpoint is execution-only and
does not accept `certificate_policy`; use the typed certificate/MCP path when a
query certificate policy is required. An image receipt is not HTTP
authentication, a verified query certificate or ontology closure.

## Trust interpretation

- canonical `.axi` is the reviewable meaning input;
- graph JSON/DOT/HTML is derived inspection output;
- SQLite `.axpd` is authenticated execution state;
- visualization does not prove query completeness, ontology closure, or a Lean
  theorem.
