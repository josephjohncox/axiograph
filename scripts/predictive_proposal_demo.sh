#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

mkdir -p build

echo "== Masked-tuple training export"
bin/axiograph discover training-export examples/Family.axi \
  --out build/family_training_export.json \
  --mask-fields 1

if [ -z "${PREDICTIVE_PROPOSAL_BACKEND:-}" ]; then
  export PREDICTIVE_PROPOSAL_BACKEND="baseline"
fi

ADAPTER_MODEL="default"
ADAPTER_BACKEND_ARGS="--proposal-adapter-llm"

if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "baseline" ]; then
  ADAPTER_BACKEND_ARGS="--proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py --proposal-adapter-plugin-arg=--strategy --proposal-adapter-plugin-arg=oracle"
  ADAPTER_MODEL="baseline_oracle"
elif [ "$PREDICTIVE_PROPOSAL_BACKEND" = "onnx" ]; then
  echo "error: PREDICTIVE_PROPOSAL_BACKEND=onnx is not supported in this demo (use physics demos)"
  exit 2
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

echo "== Predictive proposal adapter output ($PREDICTIVE_PROPOSAL_BACKEND)"
bin/axiograph ingest predictive-proposal examples/Family.axi \
  --out build/family_proposals.json \
  $ADAPTER_BACKEND_ARGS \
  --proposal-adapter-model "$ADAPTER_MODEL"

echo "== planning/eval harness (3 steps, 2 rollouts)"
bin/axiograph tools perf proposal-rollout \
  --input examples/Family.axi \
  $ADAPTER_BACKEND_ARGS \
  --proposal-adapter-model "$ADAPTER_MODEL" \
  --horizon-steps 3 \
  --rollouts 2 \
  --holdout-frac 0.2 \
  --out-json build/predictive_proposal_perf.json

echo "wrote build/predictive_proposal_perf.json"
