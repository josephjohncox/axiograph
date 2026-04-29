# REPL Scripts

These scripts are smoke demos for the interactive REPL. They now use canonical
`.axi` imports plus typed query/report commands as the teaching surface.
Some scripts end with `export_axi_module` and `save`; treat those final lines as
optional round-trip/e2e checks, not the authoring path. Reversible
`PathDBExportV1` snapshots are intentionally absent from REPL teaching scripts;
use `axiograph db pathdb export-axi` only for explicit
debug/live-byte/parser-parity checks.

Preferred scripts for current workflows:

- `ontology_rewrites_axi_demo.repl`
- `schema_evolution_axi_demo.repl`
- `supply_chain_hott_axi_demo.repl`
- `regulated_production_line_axi_demo.repl`
- `sql_schema_discovery_axi_demo.repl`

Use these as smoke tests after the canonical CLI flows, not as the first
learning path. The current path is `.axi` validation, runtime theory/query
reports, and optional module export or `.axpd` save/load only when a REPL
scenario intentionally needs a round-trip check.
