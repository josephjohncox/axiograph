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

if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  export WORLD_MODEL_BACKEND="openai"
fi
if [ "$WORLD_MODEL_BACKEND" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
  echo "error: OPENAI_API_KEY is required for WORLD_MODEL_BACKEND=openai"
  exit 2
fi
if [ "$WORLD_MODEL_BACKEND" = "anthropic" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
  echo "error: ANTHROPIC_API_KEY is required for WORLD_MODEL_BACKEND=anthropic"
  exit 2
fi
if [ "$WORLD_MODEL_BACKEND" = "ollama" ] && [ -z "${OLLAMA_HOST:-}" ] && [ -z "${OLLAMA_MODEL:-}" ]; then
  echo "error: OLLAMA_HOST or OLLAMA_MODEL is required for WORLD_MODEL_BACKEND=ollama"
  exit 2
fi
WM_MODEL="${WORLD_MODEL_MODEL:-${OPENAI_MODEL:-${ANTHROPIC_MODEL:-${OLLAMA_MODEL:-}}}}"
if [ -z "$WM_MODEL" ]; then
  echo "error: WORLD_MODEL_MODEL (or OPENAI_MODEL / ANTHROPIC_MODEL / OLLAMA_MODEL) is required"
  exit 2
fi
export WORLD_MODEL_MODEL="$WM_MODEL"

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
