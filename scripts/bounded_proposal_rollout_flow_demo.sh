#!/bin/bash
set -euo pipefail

# End-to-end bounded proposal rollout flow:
# - plan proposals
# - merge proposals
# - draft .axi
# - promote
# - rebuild PathDB + viz
#
# Run:
#   ./scripts/bounded_proposal_rollout_flow_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/bounded_proposal_rollout_flow_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
PLAN_REPORT="$OUT_DIR/proposal_rollout_plan.json"
MERGED_PROPOSALS="$OUT_DIR/proposal_rollout_proposals.json"
DRAFT_AXI="$OUT_DIR/proposal_rollout_draft.axi"
AXPD_OUT="$OUT_DIR/proposal_rollout.axpd"
VIZ_OUT="$OUT_DIR/proposal_rollout_viz.json"

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$PLANE_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== Bounded proposal rollout flow demo =="
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
ADAPTER_MODEL="${PREDICTIVE_PROPOSAL_MODEL:-${OPENAI_MODEL:-${ANTHROPIC_MODEL:-${OLLAMA_MODEL:-}}}}"

if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "baseline" ]; then
  ADAPTER_REPL_USE="proposal use command scripts/axiograph_predictive_proposal_plugin_baseline.py --strategy oracle"
  ADAPTER_DESC="baseline"
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
  if [ -z "$ADAPTER_MODEL" ]; then
    echo "error: PREDICTIVE_PROPOSAL_MODEL (or OPENAI_MODEL / ANTHROPIC_MODEL / OLLAMA_MODEL) is required"
    exit 2
  fi
  export PREDICTIVE_PROPOSAL_MODEL="$ADAPTER_MODEL"
fi

echo ""
echo "-- Predictive proposal backend: $PREDICTIVE_PROPOSAL_BACKEND (mode=$ADAPTER_DESC model=$ADAPTER_MODEL)"

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Init accepted plane + seed snapshot"
"$AXIOGRAPH" db accept init --dir "$PLANE_DIR"
"$AXIOGRAPH" db accept promote examples/Family.axi --dir "$PLANE_DIR" --message "seed family"

echo ""
echo "-- B) Planning pass (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/Family.axi" \
  --cmd "$ADAPTER_REPL_USE" \
  --cmd "proposal model $ADAPTER_MODEL" \
  --cmd "proposal plan $PLAN_REPORT --steps 2 --rollouts 2 --goal \"predict missing parent links\" --axi examples/Family.axi --cq \"has_parent=select ?p where ?p is Person limit 1\""

if [ ! -f "$PLAN_REPORT" ]; then
  echo "error: expected plan report at $PLAN_REPORT"
  exit 2
fi

echo ""
echo "-- C) Merge plan proposals"
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
echo "-- D) Draft canonical module"
"$AXIOGRAPH" discover draft-module \
  "$MERGED_PROPOSALS" \
  --out "$DRAFT_AXI" \
  --module FamilyProposalDraft \
  --schema Fam \
  --instance ProposalRollout \
  --infer-constraints

echo ""
echo "-- E) Promote + rebuild PathDB"
"$AXIOGRAPH" db accept promote "$DRAFT_AXI" --dir "$PLANE_DIR" --message "bounded proposal rollout draft" --quality fast
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_OUT"

echo ""
echo "-- F) Viz"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
  --out "$VIZ_OUT" \
  --format json \
  --plane data \
  --focus-name Carol

echo ""
echo "Done."
echo "Outputs:"
echo "  $PLAN_REPORT"
echo "  $MERGED_PROPOSALS"
echo "  $DRAFT_AXI"
echo "  $AXPD_OUT"
echo "  $VIZ_OUT"
