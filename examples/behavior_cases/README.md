# Behavior Cases

Behavior cases are versioned BDD/DDD tool requests over typed ontology scopes.
Keep executable CQs in adjacent `.cq` files when the questions are authored by
humans; the JSON request should describe the behavior scenario, not bury random
query strings. Reports include receipts, CQ status, trust classification,
semantic-slice selectors, and Rust/TypeScript test skeleton previews.

Run the industrial case:

```bash
./examples/behavior_cases/run_regulated_release_flow.sh
```

Or call the behavior-case command directly:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover behavior-case examples/industrial/RegulatedProductionLine.axi \
  --request examples/behavior_cases/regulated_ship_release.json \
  --cq-file examples/behavior_cases/regulated_ship_release.cq \
  --overlay examples/behavior_cases/regulated_ship_release_overlay.json \
  --out build/examples/regulated_ship_release_behavior_case_report.json
```

These are examples of ontology-driven engineering workflows. They should cite
canonical `.axi` scopes and implementation surfaces without directly mutating
accepted ontology state.
