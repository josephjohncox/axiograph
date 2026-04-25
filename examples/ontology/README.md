# Ontology Examples

This directory teaches ontology-engineering features over canonical `.axi`.

- `OntologyRewrites.axi` is the starting point for constraints, rewrite rules,
  path equations, and runtime theory-obligation graphs.
- `SchemaEvolution.axi` is the starting point for schema/category evolution,
  migration preview, typed transport, and semantic VCS planning.

Useful command:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover theory-graph examples/ontology/OntologyRewrites.axi \
  --out build/examples/ontology_rewrites_theory_graph.json
```
