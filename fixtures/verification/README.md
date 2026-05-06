# Verification Anchors

These files are verification fixtures, not teaching examples.

- `rewrite_rules_anchor_v1.axi` anchors rewrite/path certificate checks.
- `pathdb_export_anchor_v1.axi` anchors explicit PathDB snapshot parser-parity
  checks only.

This directory is deliberately outside `examples/` so the public examples stay
focused on authoring, querying, semantic VCS, software coverage, and backend
projection flows. Do not use these fixtures as ontology-authoring inputs.
