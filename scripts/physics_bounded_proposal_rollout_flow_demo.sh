#!/bin/bash
set -euo pipefail

# Physics-scale bounded proposal rollout flow:
# - build accepted plane
# - generate competency questions
# - plan proposals
# - draft + promote
# - rebuild PathDB + viz
#
# Run:
#   ./scripts/physics_bounded_proposal_rollout_flow_demo.sh
#
# Examples:
#   PREDICTIVE_PROPOSAL_BACKEND=baseline ./scripts/physics_bounded_proposal_rollout_flow_demo.sh
#   PREDICTIVE_PROPOSAL_BACKEND=openai OPENAI_API_KEY=... PREDICTIVE_PROPOSAL_MODEL=gpt-4o-mini \
#     ./scripts/physics_bounded_proposal_rollout_flow_demo.sh
#   PREDICTIVE_PROPOSAL_BACKEND=anthropic ANTHROPIC_API_KEY=... PREDICTIVE_PROPOSAL_MODEL=claude-3-5-sonnet-latest \
#     ./scripts/physics_bounded_proposal_rollout_flow_demo.sh
#   PREDICTIVE_PROPOSAL_BACKEND=ollama OLLAMA_MODEL=llama3.1:8b \
#     ./scripts/physics_bounded_proposal_rollout_flow_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/physics_bounded_proposal_rollout_flow_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
PLAN_REPORT="$OUT_DIR/proposal_rollout_plan.json"
MERGED_PROPOSALS="$OUT_DIR/proposal_rollout_proposals.json"
DRAFT_AXI="$OUT_DIR/proposal_rollout_draft.axi"
AXPD_BASE="$OUT_DIR/physics_base.axpd"
AXPD_OUT="$OUT_DIR/physics_proposal_rollout.axpd"
AXPD_WAL="$OUT_DIR/physics_proposal_rollout_full.axpd"
CQ_OUT="$OUT_DIR/physics_cq.json"
VIZ_OUT="$OUT_DIR/physics_proposal_rollout_viz.json"
VIZ_FULL_OUT="$OUT_DIR/physics_proposal_rollout_viz_full.json"
MODEL_PATH="${PREDICTIVE_PROPOSAL_MODEL_PATH:-models/predictive_proposal_small.onnx}"
PYTHON="${PYTHON:-python3}"
if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
  PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
fi

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$PLANE_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== Physics bounded proposal rollout flow demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

if [ -z "${PREDICTIVE_PROPOSAL_BACKEND:-}" ]; then
  export PREDICTIVE_PROPOSAL_BACKEND="baseline"
fi

ADAPTER_REPL_USE="proposal use llm"
ADAPTER_DESC="llm"
ADAPTER_MODEL="default"

if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "onnx" ]; then
  if ! "$PYTHON" - <<'PY' >/dev/null 2>&1
import importlib
importlib.import_module("onnxruntime")
importlib.import_module("onnx")
PY
  then
    if [ "${ALLOW_ONNX_PIP_INSTALL:-0}" = "1" ]; then
      "$ROOT_DIR/scripts/setup_onnx_runtime.sh"
    else
      echo "error: PREDICTIVE_PROPOSAL_BACKEND=onnx requires onnxruntime/onnx in $PYTHON" >&2
      echo "hint: run ALLOW_ONNX_PIP_INSTALL=1 ./scripts/setup_onnx_runtime.sh, or set PYTHON to an environment that already has onnxruntime and onnx" >&2
      exit 2
    fi
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
echo "Predictive Proposal Using: $ADAPTER_MODEL"

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Init accepted plane + seed snapshots"
"$AXIOGRAPH" db accept init --dir "$PLANE_DIR"
"$AXIOGRAPH" db accept promote examples/physics/PhysicsOntology.axi --dir "$PLANE_DIR" --message "seed physics ontology"
"$AXIOGRAPH" db accept promote examples/physics/PhysicsMeasurements.axi --dir "$PLANE_DIR" --message "seed physics measurements"

echo ""
echo "-- B) Build base PathDB snapshot"
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_BASE"

echo ""
echo "-- C) Generate competency questions (schema-driven)"
"$AXIOGRAPH" discover competency-questions \
  "$AXPD_BASE" \
  --out "$CQ_OUT" \
  --max-questions 120

echo ""
echo "-- D) Planning pass (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/physics/PhysicsOntology.axi" \
  --cmd "import_axi examples/physics/PhysicsMeasurements.axi" \
  --cmd "$ADAPTER_REPL_USE" \
  --cmd "proposal model $ADAPTER_MODEL" \
  --cmd "proposal plan $PLAN_REPORT --steps 2 --rollouts 2 --max 200 --guardrail strict --plane both --goal \"expand physics ontology coverage\" --axi examples/physics/PhysicsOntology.axi --cq-file $CQ_OUT"

if [ ! -f "$PLAN_REPORT" ]; then
  echo "error: expected plan report at $PLAN_REPORT"
  exit 2
fi

echo ""
echo "-- E) Merge plan proposals"
PLAN_REPORT="$PLAN_REPORT" MERGED_PROPOSALS="$MERGED_PROPOSALS" python - <<'PY'
import json
import os
import time

report = json.load(open(os.environ["PLAN_REPORT"]))
proposals = []
for step in report.get("steps", []):
    proposals.extend(step["proposals"]["proposals"])
out = {
    "version": 1,
    "generated_at": str(int(time.time())),
    "source": {"source_type": "proposal_rollout_plan", "locator": report.get("trace_id", "proposal_rollout_plan")},
    "schema_hint": None,
    "proposals": proposals,
}
json.dump(out, open(os.environ["MERGED_PROPOSALS"], "w"), indent=2)
print("wrote {}".format(os.environ["MERGED_PROPOSALS"]))
PY

echo ""
echo "-- F) Draft canonical module"
"$AXIOGRAPH" discover draft-module \
  "$MERGED_PROPOSALS" \
  --out "$DRAFT_AXI" \
  --module PhysicsProposalDraft \
  --schema Physics \
  --instance ProposalRollout \
  --infer-constraints

echo ""
echo "-- G) Promote + rebuild PathDB (accepted plane)"
"$AXIOGRAPH" db accept promote "$DRAFT_AXI" --dir "$PLANE_DIR" --message "bounded proposal rollout draft (physics)" --quality fast
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_OUT"

echo ""
echo "-- H) Commit evidence (WAL) + full PathDB"
COMMIT_OUT="$("$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot head --proposals "$MERGED_PROPOSALS" --message "bounded proposal rollout proposals (physics)")"
echo "$COMMIT_OUT"
WAL_SNAPSHOT="$(echo "$COMMIT_OUT" | grep -oE 'fnv1a64:[0-9a-f]+' | tail -n1)"
if [ -z "$WAL_SNAPSHOT" ]; then
  echo "error: failed to parse WAL snapshot id from pathdb-commit output"
  exit 2
fi
"$AXIOGRAPH" db accept pathdb-build --dir "$PLANE_DIR" --snapshot "$WAL_SNAPSHOT" --out "$AXPD_WAL"

echo ""
echo "-- I) Viz (focused)"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
  --out "$VIZ_OUT" \
  --format json \
  --plane both \
  --typed-overlay \
  --max-nodes 1200 \
  --max-edges 12000

echo ""
echo "-- J) Viz (full graph, all planes)"
"$AXIOGRAPH" tools viz "$AXPD_WAL" \
  --out "$VIZ_FULL_OUT" \
  --format json \
  --plane both \
  --typed-overlay \
  --all \
  --max-nodes 200000 \
  --max-edges 400000

echo ""
echo "Done."
echo "Outputs:"
echo "  $PLAN_REPORT"
echo "  $MERGED_PROPOSALS"
echo "  $DRAFT_AXI"
echo "  $AXPD_OUT"
echo "  $AXPD_WAL"
echo "  $VIZ_OUT"
echo "  $VIZ_FULL_OUT"
