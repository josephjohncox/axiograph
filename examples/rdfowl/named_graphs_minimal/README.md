# RDF Named Graphs As Contexts

This folder is a focused **TriG** dataset intended to exercise:

- **named graphs** → Axiograph **contexts/worlds**
- open-world modeling (“missing is unknown, not false”)
- schema discovery + draft `.axi` generation from `proposals.json`

Run the deterministic ingest:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest dir examples/rdfowl/named_graphs_minimal \
  --out-dir build/examples/rdfowl/named_graphs_minimal \
  --domain rdfowl
```
