#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/regulated_production_line_flow_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
AXPD_OUT="$OUT_DIR/regulated_line.axpd"
CQ_OUT="$OUT_DIR/regulated_line_cq_generated.json"
VIZ_OUT="$OUT_DIR/regulated_line_viz"

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$OUT_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== Regulated production line flow demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

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

echo "-- A) Init accepted plane + seed regulated production module"
"$AXIOGRAPH" db accept init --dir "$PLANE_DIR"
"$AXIOGRAPH" db accept promote examples/industrial/RegulatedProductionLine.axi --dir "$PLANE_DIR" --message "seed regulated production line" --quality fast

echo "-- B) Build accepted PathDB snapshot"
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_OUT"

echo "-- C) Generate competency questions from the promoted snapshot"
"$AXIOGRAPH" discover competency-questions "$AXPD_OUT" --out "$CQ_OUT" --max-questions 40

echo "-- D) Run the canonical REPL demo on the promoted module"
"$AXIOGRAPH" repl --script examples/repl_scripts/regulated_production_line_axi_demo.repl --quiet

echo "-- E) Build focused typed-overlay visualization"
"$AXIOGRAPH" tools viz "$AXPD_OUT" --out "$VIZ_OUT" --format html --plane both --typed-overlay --max-nodes 800 --max-edges 8000

echo "done"
echo "accepted plane: $PLANE_DIR"
echo "pathdb snapshot: $AXPD_OUT"
echo "generated cq: $CQ_OUT"
