#!/bin/bash
set -euo pipefail

# End-to-end world model MPC flow:
# - plan proposals
# - merge proposals
# - draft .axi
# - promote
# - rebuild PathDB + viz
#
# Run:
#   ./scripts/world_model_mpc_flow_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_flow_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
PLAN_REPORT="$OUT_DIR/wm_plan.json"
MERGED_PROPOSALS="$OUT_DIR/wm_plan_proposals.json"
DRAFT_AXI="$OUT_DIR/wm_plan_draft.axi"
AXPD_OUT="$OUT_DIR/wm_plan.axpd"
VIZ_OUT="$OUT_DIR/wm_plan_viz.html"

mkdir -p "$OUT_DIR"

echo "== World model MPC flow demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

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
echo "-- B) MPC plan (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/Family.axi" \
  --cmd "wm use command scripts/axiograph_world_model_plugin_baseline.py --strategy oracle" \
  --cmd "wm plan $PLAN_REPORT --steps 2 --rollouts 2 --goal \"predict missing parent links\" --axi examples/Family.axi --cq \"has_parent=select ?p where ?p is Person limit 1\""

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
    "source": {"source_type": "world_model_plan", "locator": report.get("trace_id", "wm_plan")},
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
  --module FamilyWM \
  --schema Fam \
  --instance WMPlan \
  --infer-constraints

echo ""
echo "-- E) Promote + rebuild PathDB"
"$AXIOGRAPH" db accept promote "$DRAFT_AXI" --dir "$PLANE_DIR" --message "wm plan draft" --quality fast
"$AXIOGRAPH" db accept build-pathdb --dir "$PLANE_DIR" --snapshot head --out "$AXPD_OUT"

echo ""
echo "-- F) Viz"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
  --out "$VIZ_OUT" \
  --format html \
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
