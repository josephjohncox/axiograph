#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

mkdir -p build

echo "== JEPA export"
bin/axiograph discover jepa-export examples/Family.axi \
  --out build/family_jepa.json \
  --mask-fields 1

if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  export WORLD_MODEL_BACKEND="openai"
fi

WM_MODEL="default"
WM_BACKEND_ARGS="--world-model-llm"

if [ "$WORLD_MODEL_BACKEND" = "baseline" ]; then
  WM_BACKEND_ARGS="--world-model-plugin scripts/axiograph_world_model_plugin_baseline.py --world-model-plugin-arg --strategy --world-model-plugin-arg oracle"
  WM_MODEL="baseline_oracle"
elif [ "$WORLD_MODEL_BACKEND" = "onnx" ]; then
  echo "error: WORLD_MODEL_BACKEND=onnx is not supported in this demo (use physics demos)"
  exit 2
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

echo "== World model proposals ($WORLD_MODEL_BACKEND)"
bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals.json \
  $WM_BACKEND_ARGS \
  --world-model-model "$WM_MODEL"

echo "== MPC/eval harness (3 steps, 2 rollouts)"
bin/axiograph tools perf world-model \
  --input examples/Family.axi \
  $WM_BACKEND_ARGS \
  --world-model-model "$WM_MODEL" \
  --horizon-steps 3 \
  --rollouts 2 \
  --holdout-frac 0.2 \
  --out-json build/world_model_perf.json

echo "wrote build/world_model_perf.json"
