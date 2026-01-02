#!/bin/bash
set -euo pipefail

# Physics-scale world model MPC demo with server + viz.
#
# Run:
#   ./scripts/world_model_mpc_physics_server_demo.sh
#   KEEP_RUNNING=0 ./scripts/world_model_mpc_physics_server_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_physics_server_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
READY_FILE="$OUT_DIR/server_ready.json"
VIZ_OUT="$OUT_DIR/viz.html"
VIZ_FULL_OUT="$OUT_DIR/viz_full.html"
VIZ_FULL_JSON="$OUT_DIR/viz_full.json"
PLAN_OUT="$OUT_DIR/plan_response.json"
ADMIN_TOKEN="demo-token"
MODEL_PATH="${WORLD_MODEL_MODEL_PATH:-models/world_model_small.onnx}"
PYTHON="${PYTHON:-python}"
if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
  PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
fi
if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  export WORLD_MODEL_BACKEND="openai"
fi
WM_MODEL="default"
WM_BACKEND_FLAG="--world-model-llm"

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$PLANE_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== Physics world model MPC server demo =="
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
  WM_BACKEND_FLAG="--world-model-plugin scripts/axiograph_world_model_plugin_onnx.py"
  WM_MODEL="onnx_v1"
elif [ "$WORLD_MODEL_BACKEND" = "baseline" ]; then
  WM_BACKEND_FLAG="--world-model-plugin scripts/axiograph_world_model_plugin_baseline.py --world-model-plugin-arg --strategy --world-model-plugin-arg oracle"
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
echo "-- World model backend: $WORLD_MODEL_BACKEND (model=$WM_MODEL)"

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
echo "-- A.1) Seed PathDB WAL snapshot (empty overlay)"
EMPTY_PROPOSALS="$OUT_DIR/empty_proposals.json"
cat >"$EMPTY_PROPOSALS" <<'JSON'
{
  "version": 1,
  "generated_at": "0",
  "source": {"source_type": "init", "locator": "empty"},
  "schema_hint": null,
  "proposals": []
}
JSON
"$AXIOGRAPH" db accept pathdb-commit \
  --dir "$PLANE_DIR" \
  --accepted-snapshot head \
  --proposals "$EMPTY_PROPOSALS" \
  --message "init pathdb wal"

echo ""
echo "-- B) Start server (master)"
"$AXIOGRAPH" db serve \
  --dir "$PLANE_DIR" \
  --layer pathdb \
  --snapshot head \
  --role master \
  $WM_BACKEND_FLAG \
  --world-model-model "$WM_MODEL" \
  --admin-token "$ADMIN_TOKEN" \
  --listen 127.0.0.1:0 \
  --ready-file "$READY_FILE" \
  >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

for _ in {1..50}; do
  if [ -f "$READY_FILE" ]; then
    break
  fi
  sleep 0.1
done

if [ ! -f "$READY_FILE" ]; then
  echo "error: server did not write ready file"
  exit 2
fi

PORT=$(READY_FILE="$READY_FILE" python - <<'PY'
import json
import os
with open(os.environ["READY_FILE"]) as f:
    data = json.load(f)
addr = data.get("addr", "")
if addr.startswith("[") and "]" in addr:
    host, _, port = addr[1:].partition("]")
    if ":" in port:
        port = port.split(":")[-1]
else:
    port = addr.split(":")[-1]
print(port)
PY
)

BASE_URL="http://127.0.0.1:${PORT}"

echo ""
echo "-- C) Build plan request (competency questions subset)"
CQ_FILE="$ROOT_DIR/examples/competency_questions/physics_cq.json" \
PLAN_REQ="$OUT_DIR/plan_request.json" python - <<'PY'
import json
import os

with open(os.environ["CQ_FILE"]) as f:
    cqs = json.load(f)

req = {
    "horizon_steps": 2,
    "rollouts": 2,
    "max_new_proposals": 80,
    "auto_commit": True,
    "commit_stepwise": True,
    "competency_questions": cqs[:8],
}

with open(os.environ["PLAN_REQ"], "w") as f:
    json.dump(req, f, indent=2)
print("wrote {}".format(os.environ["PLAN_REQ"]))
PY

echo ""
echo "-- D) Run MPC plan (stepwise commit)"
curl -sS -X POST "$BASE_URL/world_model/plan" \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  --data-binary @"$OUT_DIR/plan_request.json" >"$PLAN_OUT"

echo ""
echo "-- E) Fetch viz"
curl -sS "$BASE_URL/viz" >"$VIZ_OUT"

echo ""
echo "-- F) Fetch full viz (all nodes, all planes)"
curl -sS "$BASE_URL/viz?plane=both&typed_overlay=true&all=1&max_nodes=200000&max_edges=400000" >"$VIZ_FULL_OUT"
curl -sS "$BASE_URL/viz.json?plane=both&typed_overlay=true&all=1&max_nodes=200000&max_edges=400000" >"$VIZ_FULL_JSON"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/server.log"
echo "  $PLAN_OUT"
echo "  $VIZ_OUT"
echo "  $VIZ_FULL_OUT"
echo "  $VIZ_FULL_JSON"

echo ""
echo "=== Viz UI demo playbook ==="
echo "Open:"
echo "  $BASE_URL/viz?focus_name=PositionX&plane=both&typed_overlay=true&hops=3&max_nodes=600"
cat <<'TXT'

Explore tab:
  - Search for PositionX or Unit_Meter; shift‑click PositionX then Unit_Meter to highlight a path.
  - Toggle plane/meta/data to see ontology vs. data edges.

Query tab (AxQL):
  select ?q ?u where
    ?q is PhysicsMeasurements.Quantity,
    ?q -PhysicsMeasurements.QuantityHasCanonicalUnit-> ?u
  limit 10

LLM tab (tool loop):
  - "List the quantities and their canonical units."
  - "Show me relationships involving DifferentialForm in the Physics schema."

World Model tab:
  - Goals: "add missing quantity descriptions"
  - Max new proposals: 50
  - Steps: 2, Rollouts: 2, Guardrail: strict
  - Click "plan" → review proposals in the Review tab.

Review tab:
  - Inspect proposals, deselect any you don't want, then commit (requires admin token).

Add tab (manual overlay):
  - Relation type: QuantityDescription
  - Source: AccelerationX
  - Target: Text_1
  - Generate → review → commit

Note: Auto‑commit in World Model tab requires the same admin token used by Review/Add.
TXT

if [ "${KEEP_RUNNING:-1}" = "1" ]; then
  echo ""
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo ""
  echo "Tip: keep it running (default) or exit by setting KEEP_RUNNING=0."
fi
