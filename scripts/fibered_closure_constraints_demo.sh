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

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  echo "note: $AXIOGRAPH not found; building debug binary"
  make rust-debug
  AXIOGRAPH="$ROOT_DIR/rust/target/debug/axiograph"
fi

if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph or $ROOT_DIR/rust/target/debug/axiograph"
  echo "hint: run: make binaries"
  exit 2
fi

INPUT_AXI="$ROOT_DIR/examples/demo_data/FiberedClosureConstraints.axi"

echo ""
echo "-- A) Validate canonical .axi"
INPUT_BAD_AXI="$ROOT_DIR/examples/demo_data/FiberedTransitivityNoParam.axi"
INPUT_PARAM_AXI="$ROOT_DIR/examples/demo_data/FiberedTransitivityParam.axi"

echo "Validating:"
echo "  - $INPUT_BAD_AXI"
echo "  - $INPUT_PARAM_AXI"
echo "  - $INPUT_AXI"

"$AXIOGRAPH" check validate "$INPUT_BAD_AXI"
"$AXIOGRAPH" check validate "$INPUT_PARAM_AXI"
"$AXIOGRAPH" check validate "$INPUT_AXI"

echo ""
echo "-- B) Show why param (...) matters (expected failure without it)"
NO_PARAM_ERR="$OUT_DIR/fibered_transitivity_no_param_constraints_err.txt"
if "$AXIOGRAPH" cert constraints "$INPUT_BAD_AXI" --out "$OUT_DIR/should_not_exist.json" 2>"$NO_PARAM_ERR"; then
  echo "error: expected constraints cert to fail for $INPUT_BAD_AXI"
  exit 2
fi
echo "ok: constraints cert failed as expected"
echo "  wrote stderr: $NO_PARAM_ERR"
echo "  (first line) $(head -n 1 "$NO_PARAM_ERR" || true)"

echo ""
echo "-- C) Emit + verify axi_constraints_ok_v1 (fibered transitivity with param)"
CERT_PARAM="$OUT_DIR/fibered_transitivity_param_constraints_ok_v1.json"
"$AXIOGRAPH" cert constraints "$INPUT_PARAM_AXI" --out "$CERT_PARAM"
echo "wrote $CERT_PARAM"
make verify-lean-cert AXI="$INPUT_PARAM_AXI" CERT="$CERT_PARAM"

echo ""
echo "-- D) Emit + verify axi_constraints_ok_v1 (full demo module)"
CERT_FULL="$OUT_DIR/fibered_closure_constraints_ok_v1.json"
"$AXIOGRAPH" cert constraints "$INPUT_AXI" --out "$CERT_FULL"
echo "wrote $CERT_FULL"
make verify-lean-cert AXI="$INPUT_AXI" CERT="$CERT_FULL"

echo ""
echo "-- E) Render typed-overlay viz (HTML)"
VIZ="$OUT_DIR/fibered_closure_constraints.html"
"$AXIOGRAPH" tools viz "$INPUT_AXI" \
  --out "$VIZ" \
  --format html \
  --plane both \
  --typed-overlay \
  --all \
  --max-nodes 400 \
  --max-edges 6000

echo ""
echo "Done."
echo "Outputs:"
echo "  $NO_PARAM_ERR"
echo "  $CERT_PARAM"
echo "  $CERT_FULL"
echo "  $VIZ"
