#!/bin/bash
set -euo pipefail

# World model MPC demo with server + viz.
#
# Run:
#   ./scripts/world_model_mpc_server_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/world_model_mpc_server_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
READY_FILE="$OUT_DIR/server_ready.json"
VIZ_OUT="$OUT_DIR/viz.html"
PLAN_OUT="$OUT_DIR/plan_response.json"
ADMIN_TOKEN="demo-token"

mkdir -p "$OUT_DIR"

echo "== World model MPC server demo =="
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
echo "-- B) Start server (master)"
"$AXIOGRAPH" db serve \
  --dir "$PLANE_DIR" \
  --layer pathdb \
  --snapshot head \
  --role master \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --world-model-plugin-arg --strategy oracle \
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
print(data["listen"].split(":")[-1])
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
