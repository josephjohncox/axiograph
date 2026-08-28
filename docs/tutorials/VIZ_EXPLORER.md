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
`/healthz`, `/status`, and `/query`. It does not expose the old mutable visual
explorer/admin API. Start it with:

```bash
axiograph db serve \
  --dir build/axi_store \
  --materialization "$MATERIALIZATION_ID"
```

See `docs/howto/DB_SERVER.md` for the authenticated startup contract. The
server `/query` endpoint is execution-only and does not accept
`certificate_policy`; use the typed certificate/MCP path when a query
certificate policy is required.

## Trust interpretation

- canonical `.axi` is the reviewable meaning input;
- graph JSON/DOT/HTML is derived inspection output;
- SQLite `.axpd` is authenticated execution state;
- visualization does not prove query completeness, ontology closure, or a Lean
  theorem.
