#!/bin/bash
set -euo pipefail

# World model MPC demo with server + viz.
#
# Run:
#   ./scripts/world_model_mpc_server_demo.sh
#   KEEP_RUNNING=0 ./scripts/world_model_mpc_server_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_server_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
READY_FILE="$OUT_DIR/server_ready.json"
VIZ_OUT="$OUT_DIR/viz.html"
PLAN_OUT="$OUT_DIR/plan_response.json"
ADMIN_TOKEN="demo-token"

if [ -z "${AXIOGRAPH_DEMO_KEEP:-}" ]; then
  rm -rf "$PLANE_DIR"
fi
mkdir -p "$OUT_DIR"

echo "== World model MPC server demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

if [ -z "${WORLD_MODEL_BACKEND:-}" ]; then
  export WORLD_MODEL_BACKEND="openai"
fi

WM_MODEL="default"
WM_BACKEND_FLAG="--world-model-llm"

if [ "$WORLD_MODEL_BACKEND" = "baseline" ]; then
  WM_BACKEND_FLAG="--world-model-plugin scripts/axiograph_world_model_plugin_baseline.py --world-model-plugin-arg --strategy --world-model-plugin-arg oracle"
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
echo "-- A) Init accepted plane + seed snapshot"
"$AXIOGRAPH" db accept init --dir "$PLANE_DIR"
"$AXIOGRAPH" db accept promote examples/Family.axi --dir "$PLANE_DIR" --message "seed family"

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
echo "-- C) Run MPC plan (stepwise commit)"
cat >"$OUT_DIR/plan_request.json" <<'JSON'
{
  "horizon_steps": 2,
  "rollouts": 2,
  "max_new_proposals": 50,
  "auto_commit": true,
  "commit_stepwise": true,
  "competency_questions": [
    {"name": "has_parent", "query": "select ?p where ?p is Person limit 1", "min_rows": 1, "weight": 5.0}
  ]
}
JSON

curl -sS -X POST "$BASE_URL/world_model/plan" \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  --data-binary @"$OUT_DIR/plan_request.json" >"$PLAN_OUT"

echo ""
echo "-- D) Fetch viz"
curl -sS "$BASE_URL/viz" >"$VIZ_OUT"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/server.log"
echo "  $PLAN_OUT"
echo "  $VIZ_OUT"

echo ""
echo "=== Viz UI demo playbook ==="
echo "Open:"
echo "  $BASE_URL/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420"
cat <<'TXT'

Explore tab:
  - Search for Alice, Bob, or Carol; shift‑click to highlight Parent paths.

Query tab (AxQL):
  select ?child ?parent where
    ?child Parent ?parent
  limit 10

LLM tab (tool loop):
  - "Who are the parents of Carol?"
  - "Show me all Parent relations."

World Model tab:
  - Goals: "predict missing parent links"
  - Max new proposals: 50
  - Steps: 2, Rollouts: 2, Guardrail: fast
  - Click "plan" → review proposals in the Review tab.

Review tab:
  - Inspect proposals, deselect any you don't want, then commit (requires admin token).

Add tab (manual overlay):
  - Relation type: Parent
  - Source: Carol
  - Target: Alice
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
