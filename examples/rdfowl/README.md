# RDF/SHACL Boundary Examples

These fixtures demonstrate RDF-family interop as a boundary layer. They are not
the ontology kernel: import produces evidence/review artifacts that must lower
through canonical `.axi` and typed review before they can become accepted
meaning.

## Named Graphs To Contexts

`named_graphs_minimal/graphs.trig` demonstrates named graphs mapped to
Axiograph contexts/worlds.

```bash
./scripts/rdf_named_graph_context_demo.sh
```

The script runs `axiograph ingest dir ... --domain rdfowl`, drafts candidate
`.axi`, validates it, and emits inspection artifacts under `build/`.

## SHACL Boundary Fixture

`w3c_shacl_minimal/` pairs an RDF data graph with a compact SHACL shapes graph.
The current flow checks RDF/SHACL ingestion and proposal generation; SHACL is a
validation gate, not the internal kernel.

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest dir examples/rdfowl/w3c_shacl_minimal \
  --out-dir build/examples/rdfowl/w3c_shacl_minimal \
  --domain rdfowl
```

Use `examples/backend_projection/` for TypeDB/TerminusDB native-read projection
contracts. Use this directory for RDF/SHACL import-boundary behavior.
