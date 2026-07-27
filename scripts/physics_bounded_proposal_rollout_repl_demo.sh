#!/bin/bash
set -euo pipefail

# Physics-scale bounded proposal rollout demo (REPL script).
#
# Run:
#   ./scripts/physics_bounded_proposal_rollout_repl_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/physics_bounded_proposal_rollout_repl_demo"
MODEL_PATH="${PREDICTIVE_PROPOSAL_MODEL_PATH:-models/predictive_proposal_small.onnx}"
PYTHON="${PYTHON:-python}"
if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
  PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
fi
if [ -z "${PREDICTIVE_PROPOSAL_BACKEND:-}" ]; then
  export PREDICTIVE_PROPOSAL_BACKEND="baseline"
fi
ADAPTER_REPL_USE="proposal use llm"
ADAPTER_DESC="llm"
ADAPTER_MODEL="default"
mkdir -p "$OUT_DIR"

echo "== Physics bounded proposal rollout REPL demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "onnx" ]; then
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
    echo "note: building ONNX predictive proposal adapter at $MODEL_PATH"
    "$PYTHON" "$ROOT_DIR/scripts/build_predictive_proposal_onnx.py" --out "$MODEL_PATH"
  fi
  export PREDICTIVE_PROPOSAL_MODEL_PATH="$MODEL_PATH"
  ADAPTER_REPL_USE="proposal use command scripts/axiograph_predictive_proposal_plugin_onnx.py"
  ADAPTER_DESC="onnx"
  ADAPTER_MODEL="onnx_v1"
elif [ "$PREDICTIVE_PROPOSAL_BACKEND" = "baseline" ]; then
  ADAPTER_REPL_USE="proposal use command scripts/axiograph_predictive_proposal_plugin_baseline.py --strategy oracle"
  ADAPTER_DESC="baseline"
  ADAPTER_MODEL="baseline_oracle"
else
  if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "error: OPENAI_API_KEY is required for PREDICTIVE_PROPOSAL_BACKEND=openai"
    exit 2
  fi
  if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "anthropic" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "error: ANTHROPIC_API_KEY is required for PREDICTIVE_PROPOSAL_BACKEND=anthropic"
    exit 2
  fi
  if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "ollama" ] && [ -z "${OLLAMA_HOST:-}" ] && [ -z "${OLLAMA_MODEL:-}" ]; then
    echo "error: OLLAMA_HOST or OLLAMA_MODEL is required for PREDICTIVE_PROPOSAL_BACKEND=ollama"
    exit 2
  fi
  ADAPTER_MODEL="${PREDICTIVE_PROPOSAL_MODEL:-${OPENAI_MODEL:-${ANTHROPIC_MODEL:-${OLLAMA_MODEL:-}}}}"
  if [ -z "$ADAPTER_MODEL" ]; then
    echo "error: PREDICTIVE_PROPOSAL_MODEL (or OPENAI_MODEL / ANTHROPIC_MODEL / OLLAMA_MODEL) is required"
    exit 2
  fi
  export PREDICTIVE_PROPOSAL_MODEL="$ADAPTER_MODEL"
fi

echo ""
echo "-- Predictive proposal backend: $PREDICTIVE_PROPOSAL_BACKEND (mode=$ADAPTER_DESC model=$ADAPTER_MODEL)"

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- Run REPL commands"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/physics/PhysicsOntology.axi" \
  --cmd "import_axi examples/physics/PhysicsMeasurements.axi" \
  --cmd "$ADAPTER_REPL_USE" \
  --cmd "proposal model $ADAPTER_MODEL" \
  --cmd "proposal plan build/physics_bounded_proposal_rollout_plan.json --steps 2 --rollouts 2 --max 150 --guardrail strict --plane both --goal \"expand physics ontology coverage\" --axi examples/physics/PhysicsOntology.axi --cq-file examples/competency_questions/physics.cq"

if [ -f "$ROOT_DIR/build/physics_bounded_proposal_rollout_plan.json" ]; then
  cp "$ROOT_DIR/build/physics_bounded_proposal_rollout_plan.json" "$OUT_DIR/"
fi

echo ""
echo "Done."
echo "Outputs:"
if [ -f "$OUT_DIR/physics_bounded_proposal_rollout_plan.json" ]; then
  echo "  $OUT_DIR/physics_bounded_proposal_rollout_plan.json"
fi
