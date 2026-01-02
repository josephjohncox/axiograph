#!/bin/bash
set -euo pipefail

# Fibered closure constraints demo:
# - `param (...)` for symmetric/transitive constraints
# - `axi_constraints_ok_v1` certificate emission
# - Lean verification of the emitted certificate
# - typed-overlay visualization
#
# Run:
#   ./scripts/fibered_closure_constraints_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/fibered_closure_constraints_demo"
mkdir -p "$OUT_DIR"

echo "== Fibered closure constraints demo =="
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

INPUT_AXI="$ROOT_DIR/examples/demo_data/FiberedClosureConstraints.axi"

echo ""
echo "-- A) Validate canonical .axi"
"$AXIOGRAPH" check validate "$INPUT_AXI"

echo ""
echo "-- B) Emit axi_constraints_ok_v1 certificate"
CERT="$OUT_DIR/axi_constraints_ok_v1.json"
"$AXIOGRAPH" cert constraints "$INPUT_AXI" --out "$CERT"
echo "wrote $CERT"

echo ""
echo "-- C) Verify certificate in Lean"
make verify-lean-cert AXI="$INPUT_AXI" CERT="$CERT"

echo ""
echo "-- D) Render typed-overlay viz (HTML)"
VIZ="$OUT_DIR/fibered_closure_constraints.html"
"$AXIOGRAPH" tools viz "$INPUT_AXI" \
  --out "$VIZ" \
  --format html \
  --plane both \
  --typed-overlay \
  --all \
  --max_nodes 400 \
  --max_edges 6000

echo ""
echo "Done."
echo "Outputs:"
echo "  $CERT"
echo "  $VIZ"

