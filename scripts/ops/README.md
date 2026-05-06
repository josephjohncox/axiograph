# Operational Scripts

These scripts are reference and smoke-test workflows for maintainers. They are
not the first teaching path for Axiograph users.

Use them when you need to exercise broader CLI surfaces, ingestion loops, or
offline integration flows. For teaching examples, start in `examples/README.md`
and prefer canonical `.axi`, `.cq`, typed reports, semantic previews, and
purpose-built example crates.

| Script | Purpose |
| --- | --- |
| `cli_help_smoke_demo.sh` | Captures current CLI help and runs a few representative command smoke checks. |
| `discovery_loop_reference_demo.sh` | Exercises repo evidence discovery into review-plane candidate modules. |
| `continuous_ingest_sql_cli_demo.sh` | SQL evidence-ingest reference loop. |
| `continuous_ingest_proto_cli_demo.sh` | Proto evidence-ingest reference loop. |
| `ontology_engineering_all_sources_offline_demo.sh` | Large offline all-source ontology-engineering integration path. |
| `ontology_engineering_proto_evolution_ollama_demo.sh` | Proto evidence evolution with optional Ollama augmentation. |
