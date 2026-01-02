#!/bin/bash
set -euo pipefail

# Physics-scale world model MPC demo (REPL script).
#
# Run:
#   ./scripts/world_model_mpc_physics_repl_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_physics_repl_demo"
MODEL_PATH="${WORLD_MODEL_MODEL_PATH:-models/world_model_small.onnx}"
PYTHON="${PYTHON:-python}"
if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
  PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
fi
if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  export WORLD_MODEL_BACKEND="openai"
fi
WM_REPL_USE="wm use llm"
WM_DESC="llm"
WM_MODEL="default"
mkdir -p "$OUT_DIR"

echo "== Physics world model MPC REPL demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

if [ "$WORLD_MODEL_BACKEND" = "onnx" ]; then
  if ! "$PYTHON" - <<'PY' >/dev/null 2>&1
import importlib
importlib.import_module("onnxruntime")
importlib.import_module("onnx")
PY
  then
    "$ROOT_DIR/scripts/setup_onnx_runtime.sh"
    if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
      PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
    fi
  fi

  if [ ! -f "$MODEL_PATH" ]; then
    echo "note: building ONNX world model at $MODEL_PATH"
    "$PYTHON" "$ROOT_DIR/scripts/build_world_model_onnx.py" --out "$MODEL_PATH"
  fi
  export WORLD_MODEL_MODEL_PATH="$MODEL_PATH"
  WM_REPL_USE="wm use command scripts/axiograph_world_model_plugin_onnx.py"
  WM_DESC="onnx"
  WM_MODEL="onnx_v1"
elif [ "$WORLD_MODEL_BACKEND" = "baseline" ]; then
  WM_REPL_USE="wm use command scripts/axiograph_world_model_plugin_baseline.py --strategy oracle"
  WM_DESC="baseline"
  WM_MODEL="baseline_oracle"
else
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
fi

echo ""
echo "-- World model backend: $WORLD_MODEL_BACKEND (mode=$WM_DESC model=$WM_MODEL)"

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- Run REPL commands"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/physics/PhysicsOntology.axi" \
  --cmd "import_axi examples/physics/PhysicsMeasurements.axi" \
  --cmd "$WM_REPL_USE" \
  --cmd "wm model $WM_MODEL" \
  --cmd "wm plan build/world_model_mpc_physics_plan.json --steps 2 --rollouts 2 --max 150 --guardrail strict --plane both --goal \"expand physics ontology coverage\" --axi examples/physics/PhysicsOntology.axi --cq-file examples/competency_questions/physics_cq.json"

if [ -f "$ROOT_DIR/build/world_model_mpc_physics_plan.json" ]; then
  cp "$ROOT_DIR/build/world_model_mpc_physics_plan.json" "$OUT_DIR/"
fi

echo ""
echo "Done."
echo "Outputs:"
if [ -f "$OUT_DIR/world_model_mpc_physics_plan.json" ]; then
  echo "  $OUT_DIR/world_model_mpc_physics_plan.json"
fi
