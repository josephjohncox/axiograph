# Behavior Cases

Behavior cases are JSON-first BDD/DDD examples over typed ontology scopes. They
compile into read-only reports with receipts, CQ status, trust classification,
semantic-slice selectors, and Rust/TypeScript test skeleton previews.

Run the industrial case:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover behavior-case examples/industrial/RegulatedProductionLine.axi \
  --request examples/behavior_cases/regulated_ship_release.json \
  --out build/examples/regulated_ship_release_behavior_case_report.json
```

These are examples of ontology-driven engineering workflows. They should cite
canonical `.axi` scopes and implementation surfaces without directly mutating
accepted ontology state.
