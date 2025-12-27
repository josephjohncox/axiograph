#!/bin/bash
set -euo pipefail

# Ontology rewrite rules demo: first-class `.axi` rules, meta-plane inspection,
# and simple “both-sides” queries for definitional equivalences.
#
# Run:
#   ./scripts/ontology_rewrites_axi_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/ontology_rewrites_axi_demo"
mkdir -p "$OUT_DIR"

echo "== Ontology rewrite rules demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) import canonical .axi, inspect rules, run queries (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/ontology/OntologyRewrites.axi" \
  --cmd "schema OrgFamily" \
  --cmd "constraints OrgFamily" \
  --cmd "validate_axi" \
  --cmd "rules" \
  --cmd "rules OrgFamilySemantics grandparent_def" \
  --cmd "q select ?gc where name(\"Alice\") -Parent/Parent-> ?gc limit 10" \
  --cmd "q select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10" \
  --cmd "q select ?m where name(\"Eve\") -ReportsTo-> ?m limit 10" \
  --cmd "q select ?e where name(\"Bob\") -ManagerOf-> ?e limit 10" \
  --cmd "export_axi $OUT_DIR/ontology_rewrites_axi_export_v1.axi" \
  --cmd "save $OUT_DIR/ontology_rewrites_axi.axpd"

echo ""
echo "-- B) viz (meta + data)"
"$AXIOGRAPH" tools viz "$OUT_DIR/ontology_rewrites_axi.axpd" \
  --out "$OUT_DIR/ontology_rewrites_meta.html" \
  --format html \
  --plane meta \
  --focus-name OrgFamily \
  --hops 3 \
  --max-nodes 420

"$AXIOGRAPH" tools viz "$OUT_DIR/ontology_rewrites_axi.axpd" \
  --out "$OUT_DIR/ontology_rewrites_data.html" \
  --format html \
  --plane data \
  --hops 2 \
  --max-nodes 420

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/ontology_rewrites_axi_export_v1.axi"
echo "  $OUT_DIR/ontology_rewrites_axi.axpd"
echo "  $OUT_DIR/ontology_rewrites_meta.html"
echo "  $OUT_DIR/ontology_rewrites_data.html"
