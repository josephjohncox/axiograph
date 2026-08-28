# Industrial Engineering Example

This tutorial demonstrates how a larger domain example can exercise Axiograph
without becoming part of the core ontology kernel.

The scenario is a regulated production line embedded in a business context:
orders, work orders, recipes, material lots, certificates, PLC/HMI/SOP seams,
inspection decisions, delivery commitments, and pricing terms.

## 1. Validate The Canonical Module

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/industrial/RegulatedProductionLine.axi
```

This checks the canonical `.axi` module through the Rust parser/typechecker.

## 2. Inspect The Theory Graph

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover theory-graph examples/industrial/RegulatedProductionLine.axi \
  --out build/examples/industrial/theory_graph.json
```

Use this when teaching the type-theory/category seam: relation objects, role
projections, constraints, equations, and obligations should be runtime-visible
before agents try to repair or extend the ontology.

## 3. Run The Example Scenario

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

The example crate imports canonical `.axi` through PathDB, computes an accepted-style
anchor from the `.axi` digest, and writes cache-only teaching artifacts under the
regulated production line cache directory.

## 4. Inspect The Run

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-industrial \
  --bin axiograph-industrial-example \
  -- inspect \
  --cache-root build/examples/industrial \
  --run-id regulated-demo-001
```

The inspection report checks that all artifacts agree on campaign id, run id,
and anchor. This is the minimum artifact discipline needed before agentic
engineering loops can trust a report enough to plan follow-up code or ontology
work.

## What This Teaches

- Canonical `.axi` is the meaning-bearing input.
- Domain example crates belong outside the core CLI.
- PathDB is an execution substrate used by the example, not the ontology kernel.
- CQ and coverage artifacts are useful for coding agents, but their trust
  contract matters.
- The industrial scenario is a library/teaching package that can evolve without
  polluting the trusted kernel or core CLI.

## Next Extensions

- Add a typed behavior case for one release-review scenario.
- Add a semantic slice selector for the PLC/HMI/SOP bounded context.
- Add a migration preview that splits material certification into a separate
  bounded context.
- Promote accepted deltas through the semantic VCS instead of mutating example
  cache artifacts.
