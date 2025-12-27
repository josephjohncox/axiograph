#!/bin/bash
set -euo pipefail

# Network + quality tooling demo (ontology engineering helpers).
#
# This script shows:
# - `axiograph tools analyze network` graph metrics (components, hubs, PageRank, etc.)
# - `axiograph check quality` report generation (meta + data linting)
# - promotion gating: `axiograph db accept promote --quality strict` attaching a report
#
# Run:
#   ./scripts/network_quality_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/network_quality_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph network + quality demo =="
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

INPUT_AXI="$ROOT_DIR/examples/ontology/OntologyRewrites.axi"

echo ""
echo "-- A) Network analysis over canonical .axi"
"$AXIOGRAPH" tools analyze network "$INPUT_AXI" \
  --plane both \
  --skip-facts \
  --communities \
  --format json \
  --out "$OUT_DIR/network_axi.json"

echo ""
echo "-- B) Quality report over canonical .axi"
"$AXIOGRAPH" check quality "$INPUT_AXI" \
  --plane both \
  --profile strict \
  --format json \
  --no-fail \
  --out "$OUT_DIR/quality_axi.json"

echo ""
echo "-- C) Promotion gating: accept promote --quality strict (attaches report)"
ACCEPTED_DIR="$OUT_DIR/accepted_plane"
"$AXIOGRAPH" db accept promote "$INPUT_AXI" \
  --dir "$ACCEPTED_DIR" \
  --quality strict \
  --message "demo: accept promote with quality strict"

echo ""
echo "-- D) Rebuild PathDB from accepted snapshot, then analyze"
"$AXIOGRAPH" db accept build-pathdb \
  --dir "$ACCEPTED_DIR" \
  --snapshot latest \
  --out "$OUT_DIR/accepted_plane.axpd"

"$AXIOGRAPH" tools analyze network "$OUT_DIR/accepted_plane.axpd" \
  --plane both \
  --skip-facts \
  --communities \
  --format json \
  --out "$OUT_DIR/network_accepted_axpd.json"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/network_axi.json"
echo "  $OUT_DIR/quality_axi.json"
echo "  $OUT_DIR/accepted_plane.axpd"
echo "  $OUT_DIR/network_accepted_axpd.json"
