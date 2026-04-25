# REPL Scripts

These scripts are smoke demos for the interactive REPL. They are not the
canonical teaching surface for new features.

Many scripts still end with `export_axi build/*_export_v1.axi` to preserve
parser/storage parity checks. Treat those exports as debug/interchange
artifacts, not semantic anchors. New examples should prefer canonical `.axi`,
typed reports, certificates, behavior cases, semantic slice manifests, and
evolution previews.

Preferred scripts for current workflows:

- `ontology_rewrites_axi_demo.repl`
- `schema_evolution_axi_demo.repl`
- `supply_chain_hott_axi_demo.repl`
- `regulated_production_line_axi_demo.repl`
- `sql_schema_discovery_axi_demo.repl`
