#!/bin/bash
set -euo pipefail

# Physics-scale world model MPC flow:
# - build accepted plane
# - generate competency questions
# - plan proposals
# - draft + promote
# - rebuild PathDB + viz
#
# Run:
#   ./scripts/world_model_mpc_physics_flow_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_physics_flow_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
PLAN_REPORT="$OUT_DIR/wm_plan.json"
MERGED_PROPOSALS="$OUT_DIR/wm_plan_proposals.json"
DRAFT_AXI="$OUT_DIR/wm_plan_draft.axi"
AXPD_BASE="$OUT_DIR/physics_base.axpd"
AXPD_OUT="$OUT_DIR/physics_wm.axpd"
CQ_OUT="$OUT_DIR/physics_cq.json"
VIZ_OUT="$OUT_DIR/physics_wm_viz.html"

mkdir -p "$OUT_DIR"

echo "== Physics world model MPC flow demo =="
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
echo "-- D) MPC plan (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/physics/PhysicsOntology.axi" \
  --cmd "import_axi examples/physics/PhysicsMeasurements.axi" \
  --cmd "wm use command scripts/axiograph_world_model_plugin_real.py" \
  --cmd "wm plan $PLAN_REPORT --steps 2 --rollouts 2 --max 200 --guardrail strict --plane both --goal \"expand physics ontology coverage\" --axi examples/physics/PhysicsOntology.axi --cq-file $CQ_OUT"

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
    "source": {"source_type": "world_model_plan", "locator": report.get("trace_id", "wm_plan")},
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
  --module PhysicsWM \
  --schema Physics \
  --instance WMPlan \
  --infer-constraints

echo ""
echo "-- G) Promote + rebuild PathDB"
"$AXIOGRAPH" db accept promote "$DRAFT_AXI" --dir "$PLANE_DIR" --message "wm plan draft (physics)" --quality fast
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_OUT"

echo ""
echo "-- H) Viz"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
  --out "$VIZ_OUT" \
  --format html \
  --plane data \
  --focus-name MinkowskiSpacetime_M4

echo ""
echo "Done."
echo "Outputs:"
echo "  $PLAN_REPORT"
echo "  $MERGED_PROPOSALS"
echo "  $DRAFT_AXI"
echo "  $AXPD_OUT"
echo "  $VIZ_OUT"
