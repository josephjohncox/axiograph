# Industrial Engineering Example

`RegulatedProductionLine.axi` is a domain example, not part of the core
semantic kernel. It models a regulated production line with business,
industrial, and operational-control artifacts:

- ERP/MRP order and shipment lineage,
- material lots and supplier certificates,
- PLC/HMI/SOP process-control artifacts,
- release review and inspection decisions,
- delivery commitments and pricing terms,
- competency questions and semantic coverage targets supplied by adjacent
  tooling overlays.

The `.axi` module intentionally does not model code refs, test specs, coverage
policy, or generated implementation surfaces. Those are tooling concepts that
consume the ontology through behavior cases and overlays.

The example is intentionally linked to the core through public runtime
contracts:

- canonical `.axi` import,
- typed meta-plane and fact metadata in PathDB,
- accepted-style anchors over `.axi` digests,
- CQ/coverage/agent-report artifacts,
- cache-only materialization.

It is not linked through private `axiograph-cli` modules or core MCP tools.

## Run

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-industrial \
  --bin axiograph-industrial-example \
  -- run-regulated-seed \
  --axi examples/industrial/RegulatedProductionLine.axi \
  --cache-root build/examples/industrial \
  --run-id regulated-demo-001 \
  --created-at-unix-secs 1713810000 \
  --json
```

Inspect the run:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-industrial \
  --bin axiograph-industrial-example \
  -- inspect \
  --cache-root build/examples/industrial \
  --run-id regulated-demo-001
```

## What To Look For

- `run.json` carries the accepted-style `.axi` anchor used by every artifact.
- `cq_results.json` shows the competency-question coverage gates for the imported
  canonical instance.
- `coverage.json` maps ontology relations to overlay-defined software/business
  surfaces.
- `agent_report.json` summarizes gaps and next actions for coding agents.
- `distill.json` is the short handoff summary for review or agent planning.

The CQ evaluator in this example is intentionally pedagogical: it checks
relation presence over imported `.axi` fact metadata and does not claim full
AxQL completeness. Full typed query elaboration and certification stay in the
core query surfaces.
