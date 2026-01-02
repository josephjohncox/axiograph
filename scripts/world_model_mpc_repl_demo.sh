#!/bin/bash
set -euo pipefail

# World model MPC demo (REPL script).
#
# Run:
#   ./scripts/world_model_mpc_repl_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_repl_demo"
mkdir -p "$OUT_DIR"

echo "== World model MPC REPL demo =="
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
echo "-- Run REPL script"
"$AXIOGRAPH" repl --quiet --script examples/repl_scripts/world_model_mpc_demo.repl

if [ -f "$ROOT_DIR/build/world_model_mpc_demo_plan.json" ]; then
  cp "$ROOT_DIR/build/world_model_mpc_demo_plan.json" "$OUT_DIR/"
fi

echo ""
echo "Done."
echo "Outputs:"
if [ -f "$OUT_DIR/world_model_mpc_demo_plan.json" ]; then
  echo "  $OUT_DIR/world_model_mpc_demo_plan.json"
fi
