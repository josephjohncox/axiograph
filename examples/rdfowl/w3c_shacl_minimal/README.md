# W3C SHACL Minimal Example (Fixture)

This folder contains a **small SHACL + RDF** example used for:

- sanity-checking the RDF/OWL ingestion boundary adapter (`axiograph ingest-dir`),
- future SHACL validation experiments (as an ingestion gate).

The content is intentionally tiny and human-readable.

Files:

- `data.ttl`: RDF data graph (`schema:Person` instances).
- `shapes.ttl`: SHACL shapes graph (`sh:NodeShape`).

