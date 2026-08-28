# W3C SHACL Boundary Example

This folder contains a **focused SHACL + RDF** example used for:

- sanity-checking the RDF/OWL ingestion boundary adapter (`axiograph ingest dir`),
- future SHACL validation experiments (as an ingestion gate).

The content is intentionally compact and human-readable.

Files:

- `data.ttl`: RDF data graph (`schema:Person` instances).
- `shapes.ttl`: SHACL shapes graph (`sh:NodeShape`).

Run it directly:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest dir examples/rdfowl/w3c_shacl_minimal \
  --out-dir build/examples/rdfowl/w3c_shacl_minimal \
  --domain rdfowl
```

Expected outputs include typed proposal/evidence artifacts under the output
directory. Treat them as boundary-layer review material, not accepted ontology
truth.
