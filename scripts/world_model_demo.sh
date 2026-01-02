#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

mkdir -p build

echo "== JEPA export"
bin/axiograph discover jepa-export examples/Family.axi \
  --out build/family_jepa.json \
  --mask-fields 1

echo "== World model proposals (baseline)"
bin/axiograph discover world-model-propose \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --world-model-plugin-arg --strategy oracle

echo "== MPC/eval harness (3 steps, 2 rollouts)"
bin/axiograph tools perf world-model \
  --input examples/Family.axi \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --world-model-plugin-arg --strategy oracle \
  --horizon-steps 3 \
  --rollouts 2 \
  --holdout-frac 0.2 \
  --out-json build/world_model_perf.json

echo "wrote build/world_model_perf.json"
