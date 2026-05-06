#!/bin/bash
set -euo pipefail

# Physics-scale bounded proposal rollout demo with server + viz.
#
# Run:
#   ./scripts/physics_bounded_proposal_rollout_server_demo.sh
#   KEEP_RUNNING=0 ./scripts/physics_bounded_proposal_rollout_server_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/physics_bounded_proposal_rollout_server_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
READY_FILE="$OUT_DIR/server_ready.json"
VIZ_OUT="$OUT_DIR/viz.json"
VIZ_FULL_OUT="$OUT_DIR/viz_full.json"
PLAN_OUT="$OUT_DIR/plan_response.json"
AXPD_OUT="$OUT_DIR/server_snapshot.axpd"
ADMIN_TOKEN="${ADMIN_TOKEN:-demo-token}"
MODEL_PATH="${PREDICTIVE_PROPOSAL_MODEL_PATH:-models/predictive_proposal_small.onnx}"
PYTHON="${PYTHON:-python}"
if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
  PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
fi
if [ -z "${PREDICTIVE_PROPOSAL_BACKEND:-}" ]; then
  export PREDICTIVE_PROPOSAL_BACKEND="baseline"
fi
ADAPTER_MODEL="default"
ADAPTER_BACKEND_FLAG="--proposal-adapter-llm"

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$PLANE_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== Physics bounded proposal rollout server demo =="
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
    if [ "${ALLOW_ONNX_PIP_INSTALL:-0}" = "1" ]; then
      "$ROOT_DIR/scripts/setup_onnx_runtime.sh"
      if [ -x "$ROOT_DIR/.venv-onnx/bin/python" ]; then
        PYTHON="$ROOT_DIR/.venv-onnx/bin/python"
      fi
    else
      echo "error: PREDICTIVE_PROPOSAL_BACKEND=onnx requires onnxruntime/onnx in $PYTHON" >&2
      echo "hint: run ALLOW_ONNX_PIP_INSTALL=1 ./scripts/setup_onnx_runtime.sh, or set PYTHON to an environment that already has onnxruntime and onnx" >&2
      exit 2
    fi
  fi

  if [ ! -f "$MODEL_PATH" ]; then
    echo "note: building ONNX predictive proposal adapter at $MODEL_PATH"
    "$PYTHON" "$ROOT_DIR/scripts/build_predictive_proposal_onnx.py" --out "$MODEL_PATH"
  fi
  export PREDICTIVE_PROPOSAL_MODEL_PATH="$MODEL_PATH"
  ADAPTER_BACKEND_FLAG="--proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_onnx.py"
  ADAPTER_MODEL="onnx_v1"
elif [ "$PREDICTIVE_PROPOSAL_BACKEND" = "baseline" ]; then
  ADAPTER_BACKEND_FLAG="--proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py --proposal-adapter-plugin-arg=--strategy --proposal-adapter-plugin-arg=oracle"
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
echo "-- Predictive proposal backend: $PREDICTIVE_PROPOSAL_BACKEND (model=$ADAPTER_MODEL)"

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
EMPTY_PROPOSALS="$OUT_DIR/empty_proposals_fixture.json"
EMPTY_PROPOSALS="$EMPTY_PROPOSALS" python - <<'PY'
import json
import os

fixture = {
    "version": 1,
    "generated_at": "0",
    "source": {"source_type": "init", "locator": "empty"},
    "schema_hint": None,
    "proposals": [],
}
with open(os.environ["EMPTY_PROPOSALS"], "w") as f:
    json.dump(fixture, f, indent=2)
print("wrote {}".format(os.environ["EMPTY_PROPOSALS"]))
PY
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
  $ADAPTER_BACKEND_FLAG \
  --proposal-adapter-model "$ADAPTER_MODEL" \
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
CQ_FILE="$ROOT_DIR/examples/competency_questions/physics.cq" \
PLAN_REQ="$OUT_DIR/plan_request.json" python - <<'PY'
import json
import os

def load_cq_text(path):
    questions = []
    current = None
    with open(path) as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("#") or line.startswith("version "):
                continue
            if line.startswith("question ") and line.endswith(":"):
                if current:
                    questions.append(current)
                current = {
                    "name": line[len("question "):-1].strip(),
                    "min_rows": 1,
                    "weight": 1.0,
                    "contexts": [],
                }
                continue
            if current is None:
                raise ValueError("CQ field before question header: {}".format(line))
            key, value = line.split(":", 1)
            key = key.strip()
            value = value.strip()
            if key in ("asks", "question"):
                current["question"] = value
            elif key == "axql":
                current["query"] = value
            elif key == "min_rows":
                current["min_rows"] = int(value)
            elif key == "weight":
                current["weight"] = float(value)
            elif key == "context":
                current["contexts"].append(value)
            elif key == "contexts":
                current["contexts"].extend(
                    [item.strip() for item in value.split(",") if item.strip()]
                )
            else:
                raise ValueError("unsupported CQ field {}".format(key))
    if current:
        questions.append(current)
    return questions

cqs = load_cq_text(os.environ["CQ_FILE"])

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
echo "-- D) Run planning pass (stepwise commit)"
curl -sS -X POST "$BASE_URL/planning/proposal-rollout" \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  --data-binary @"$OUT_DIR/plan_request.json" >"$PLAN_OUT"

echo ""
echo "-- E) Build local PathDB snapshot for offline viz"
"$AXIOGRAPH" db accept pathdb-build \
  --dir "$PLANE_DIR" \
  --snapshot head \
  --out "$AXPD_OUT"

echo ""
echo "-- F) Viz JSON (offline)"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
  --out "$VIZ_OUT" \
  --format json \
  --plane both \
  --typed-overlay \
  --max-nodes 1200 \
  --max-edges 12000

echo ""
echo "-- G) Full viz bundle (all nodes, all planes)"
"$AXIOGRAPH" tools viz "$AXPD_OUT" \
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
echo "  $OUT_DIR/server.log"
echo "  $PLAN_OUT"
echo "  $AXPD_OUT"
echo "  $VIZ_OUT"
echo "  $VIZ_FULL_OUT"
echo "admin token: $ADMIN_TOKEN"

echo ""
echo "Server URL:"
echo "  $BASE_URL/viz"
echo ""
echo "=== Viz UI demo playbook ==="
echo "Server URL:"
echo "  $BASE_URL/viz?focus_name=PositionX&plane=both&typed_overlay=true&hops=3&max_nodes=600"
cat <<'TXT'

Explore tab:
  - Search for PositionX or Unit_Meter; shift-click PositionX then Unit_Meter to highlight a path.
  - Toggle plane/meta/data to see ontology vs. data edges.

Query tab (AxQL):
  select ?q ?u where
    ?q is PhysicsMeasurements.Quantity,
    ?q -PhysicsMeasurements.QuantityHasCanonicalUnit-> ?u
  limit 10

LLM tab (tool loop):
  - "List the quantities and their canonical units."
  - "Show me relationships involving DifferentialForm in the Physics schema."

Predictive Proposal tab:
  - Goals: "add missing quantity descriptions"
  - Max new proposals: 50
  - Steps: 2, Rollouts: 2, Guardrail: strict
  - Click "plan" -> review proposals in the Review tab.

Review tab:
  - Inspect proposals, deselect any you don't want, then commit (requires admin token).

Add tab (manual overlay):
  - Relation type: QuantityDescription
  - Source: AccelerationX
  - Target: Text_1
  - Generate -> review -> commit

Note: Auto-commit in Predictive Proposal tab requires the same admin token used by Review/Add.
TXT

if [ "${KEEP_RUNNING:-1}" = "1" ]; then
  echo ""
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo ""
  echo "Tip: keep it running (default) or exit by setting KEEP_RUNNING=0."
fi
