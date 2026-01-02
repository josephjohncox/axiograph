#!/bin/bash
set -euo pipefail

# Physics-scale world model MPC demo (REPL script).
#
# Run:
#   ./scripts/world_model_mpc_physics_repl_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_physics_repl_demo"
mkdir -p "$OUT_DIR"

echo "== Physics world model MPC REPL demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

echo ""
echo "-- World model backend (real)"
if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  if [ -n "${OPENAI_API_KEY:-}" ]; then
    export WORLD_MODEL_BACKEND="openai"
  elif [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    export WORLD_MODEL_BACKEND="anthropic"
  elif [ -n "${OLLAMA_HOST:-}" ] || [ -n "${OLLAMA_MODEL:-}" ]; then
    export WORLD_MODEL_BACKEND="ollama"
  else
    echo "error: no world model backend configured."
    echo "Set WORLD_MODEL_BACKEND=openai|anthropic|ollama and configure API keys."
    echo "Examples:"
    echo "  export WORLD_MODEL_BACKEND=openai OPENAI_API_KEY=... WORLD_MODEL_MODEL=gpt-4o-mini"
    echo "  export WORLD_MODEL_BACKEND=ollama OLLAMA_HOST=http://127.0.0.1:11434 WORLD_MODEL_MODEL=llama3.1"
    exit 2
  fi
fi
echo "world model backend: $WORLD_MODEL_BACKEND"

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
"$AXIOGRAPH" repl --quiet --script examples/repl_scripts/world_model_mpc_physics_demo.repl

if [ -f "$ROOT_DIR/build/world_model_mpc_physics_plan.json" ]; then
  cp "$ROOT_DIR/build/world_model_mpc_physics_plan.json" "$OUT_DIR/"
fi

echo ""
echo "Done."
echo "Outputs:"
if [ -f "$OUT_DIR/world_model_mpc_physics_plan.json" ]; then
  echo "  $OUT_DIR/world_model_mpc_physics_plan.json"
fi
