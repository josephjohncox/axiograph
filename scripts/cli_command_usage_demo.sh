#!/usr/bin/env bash
set -euo pipefail

# Command-surface demo:
# - captures `--help` output for key command families
# - runs a small set of representative commands on a canonical example
#
# Run from repo root:
#   ./scripts/cli_command_usage_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/cli_command_usage_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== CLI command usage demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
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
echo "-- A) Capture help output"
"$AXIOGRAPH" --help >"$OUT_DIR/help_root.txt"
"$AXIOGRAPH" db --help >"$OUT_DIR/help_db.txt"
"$AXIOGRAPH" db accept --help >"$OUT_DIR/help_db_accept.txt"
"$AXIOGRAPH" db pathdb --help >"$OUT_DIR/help_db_pathdb.txt"
"$AXIOGRAPH" tools --help >"$OUT_DIR/help_tools.txt"
"$AXIOGRAPH" ingest --help >"$OUT_DIR/help_ingest.txt"
"$AXIOGRAPH" discover --help >"$OUT_DIR/help_discover.txt"
"$AXIOGRAPH" cert --help >"$OUT_DIR/help_cert.txt"
"$AXIOGRAPH" check --help >"$OUT_DIR/help_check.txt"
"$AXIOGRAPH" repl --help >"$OUT_DIR/help_repl.txt"

echo ""
echo "-- B) Run representative commands on a canonical module"
INPUT_AXI="$ROOT_DIR/examples/ontology/OntologyRewrites.axi"

"$AXIOGRAPH" check validate "$INPUT_AXI" >"$OUT_DIR/check_validate.txt"
"$AXIOGRAPH" check quality "$INPUT_AXI" --plane both --profile strict --format json --no-fail --out "$OUT_DIR/quality_report.json"
"$AXIOGRAPH" tools analyze network "$INPUT_AXI" --plane both --skip-facts --communities --format json --out "$OUT_DIR/network_report.json"

echo ""
echo "-- C) Import to .axpd, query, and viz"
AXPD="$OUT_DIR/ontology_rewrites.axpd"
"$AXIOGRAPH" db pathdb import-axi "$INPUT_AXI" --out "$AXPD"

"$AXIOGRAPH" repl --axpd "$AXPD" --quiet \
  --cmd 'q select ?gc where name("Alice") -Grandparent-> ?gc limit 10' \
  >"$OUT_DIR/repl_query.txt"

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_alice.html" \
  --format html \
  --plane both \
  --focus-name Alice \
  --hops 3 \
  --max-nodes 420 \
  --typed-overlay

echo ""
echo "Done."
echo "Outputs (selected):"
echo "  $OUT_DIR/help_root.txt"
echo "  $OUT_DIR/quality_report.json"
echo "  $OUT_DIR/network_report.json"
echo "  $OUT_DIR/viz_alice.html"
