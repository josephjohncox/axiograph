#!/usr/bin/env bash
set -euo pipefail

# Proto business fleet + live server + LLM + viz demo.
#
# - generates a large synthetic proto/gRPC API surface (scenario=proto_api_business)
# - includes DocChunks for grounding (service/rpc docs + an order/checkout narrative)
# - starts `axiograph db serve --axpd ...` with the LLM panel enabled
# - prints useful `/viz` URLs and (optionally) calls `/llm/agent`
#
# Run from repo root:
#   ./scripts/proto_business_server_llm_viz_demo.sh
#
# Examples:
#   LLM_BACKEND=mock KEEP_RUNNING=1 ./scripts/proto_business_server_llm_viz_demo.sh
#   LLM_BACKEND=ollama LLM_MODEL=nemotron-3-nano KEEP_RUNNING=1 ./scripts/proto_business_server_llm_viz_demo.sh
#   LLM_BACKEND=openai LLM_MODEL=gpt-5.2 OPENAI_API_KEY=... KEEP_RUNNING=1 ./scripts/proto_business_server_llm_viz_demo.sh
#   LLM_BACKEND=anthropic LLM_MODEL=claude-3-7-sonnet-20250219 ANTHROPIC_API_KEY=... KEEP_RUNNING=1 ./scripts/proto_business_server_llm_viz_demo.sh
#
# Tunables:
#   SCALE=48 INDEX_DEPTH=3 SEED=1 ./scripts/proto_business_server_llm_viz_demo.sh
#
# Notes:
# - This demo serves a single `.axpd` snapshot (no snapshot store / WAL writes).
# - Use `scripts/graph_explorer_full_demo.sh` or `scripts/graph_explorer_deep_knowledge_demo.sh`
#   for the full “accepted plane + WAL + promotion” lifecycle.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

SCALE="${SCALE:-48}"
INDEX_DEPTH="${INDEX_DEPTH:-3}"
SEED="${SEED:-1}"

OUT_DIR="$ROOT_DIR/build/proto_business_server_llm_viz_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

AXPD="$OUT_DIR/proto_api_business_scale${SCALE}_depth${INDEX_DEPTH}_seed${SEED}.axpd"

# LLM defaults (can be overridden by env vars).
LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-nemotron-3-nano}"
LLM_HTTP_TIMEOUT_SECS="${LLM_HTTP_TIMEOUT_SECS:-240}"
SKIP_SERVER="${SKIP_SERVER:-0}"
ADMIN_TOKEN="${ADMIN_TOKEN:-demo-admin-token}"

RUN_SAMPLES="${RUN_SAMPLES:-}"
if [ -z "$RUN_SAMPLES" ]; then
  if [ "${KEEP_RUNNING:-0}" = "1" ]; then
    RUN_SAMPLES=0
  else
    RUN_SAMPLES=1
  fi
fi

echo "== Proto business fleet + server + LLM viz demo =="
echo "root:  $ROOT_DIR"
echo "out:   $OUT_DIR"
echo "axpd:  $AXPD"
echo "scale: SCALE=$SCALE INDEX_DEPTH=$INDEX_DEPTH SEED=$SEED"
echo "llm:   backend=$LLM_BACKEND model=$LLM_MODEL"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Generate PathDB snapshot (.axpd) (scenario=proto_api_business)"
"$AXIOGRAPH" tools perf scenario \
  --scenario proto_api_business \
  --scale "$SCALE" \
  --index-depth "$INDEX_DEPTH" \
  --seed "$SEED" \
  --path-queries 0 \
  --axql-queries 0 \
  --out-axpd "$AXPD" \
  --out-json "$OUT_DIR/perf.json"

if [ "$SKIP_SERVER" = "1" ]; then
  echo ""
  echo "SKIP_SERVER=1: skipping server start."
  echo "Tip: start the server manually:"
  echo "  $AXIOGRAPH db serve --axpd \"$AXPD\" --listen 127.0.0.1:8089 --llm-mock"
  exit 0
fi

echo ""
echo "-- B) Start server (ephemeral port) with LLM enabled"
READY="$OUT_DIR/ready.json"

LLM_FLAGS=()
if [ "$LLM_BACKEND" = "ollama" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=ollama (e.g. nemotron-3-nano)"
    exit 2
  fi
  echo "note: requires: ollama serve  (and: ollama pull $LLM_MODEL)"
  LLM_FLAGS+=(--llm-ollama --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "openai" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=openai (example: gpt-5.2)"
    exit 2
  fi
  LLM_FLAGS+=(--llm-openai --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "anthropic" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=anthropic (example: claude-3-7-sonnet-20250219)"
    exit 2
  fi
  LLM_FLAGS+=(--llm-anthropic --llm-model "$LLM_MODEL")
else
  LLM_FLAGS+=(--llm-mock)
fi

"$AXIOGRAPH" db serve \
  --role standalone \
  --axpd "$AXPD" \
  --listen 127.0.0.1:0 \
  --admin-token "$ADMIN_TOKEN" \
  --ready-file "$READY" \
  "${LLM_FLAGS[@]}" \
  >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

sleep 0.2
if ! kill -0 "$SERVER_PID" 2>/dev/null; then
  echo "error: server exited early; see $OUT_DIR/server.log"
  tail -n 120 "$OUT_DIR/server.log" || true
  exit 2
fi

python3 - "$READY" <<'PY' >"$OUT_DIR/addr.txt"
import json, time, sys
path = sys.argv[1]
deadline = time.time() + 60
while time.time() < deadline:
  try:
    with open(path) as f:
      j = json.load(f)
    if "addr" in j:
      print(j["addr"])
      sys.exit(0)
  except Exception:
    time.sleep(0.05)
print("error: server did not write ready file", file=sys.stderr)
sys.exit(2)
PY

ADDR="$(cat "$OUT_DIR/addr.txt")"

echo "server: http://$ADDR"
echo "admin token: $ADMIN_TOKEN"
echo ""
echo "Open in a browser:"
echo "  http://$ADDR/viz?focus_name=acme.orders.v1.OrderService&plane=both&typed_overlay=true&hops=2&max_nodes=900"
echo "  http://$ADDR/viz?focus_name=acme.payments.v1.PaymentService&plane=both&typed_overlay=true&hops=2&max_nodes=900"
echo "  http://$ADDR/viz?focus_name=doc_proto_business_checkout_0&plane=data&hops=2&max_nodes=900"
echo ""
echo "Try in the Query tab (AxQL):"
echo "  select ?svc where ?svc is ProtoService limit 30"
echo "  select ?rpc where name(\"acme.orders.v1.OrderService\") -proto_service_has_rpc-> ?rpc limit 50"
echo "  select ?dst where name(\"acme.orders.v1.OrderService\") -calls-> ?dst limit 50"
echo "  select ?ep where name(\"acme.payments.v1.PaymentService.AuthorizePayment\") -proto_rpc_http_endpoint-> ?ep limit 10"
echo "  select ?c where ?c is DocChunk, fts(?c, \"text\", \"Checkout\") limit 10"
echo ""
echo "Ask in the LLM panel:"
echo "  - what services exist in this snapshot?"
echo "  - what RPCs does acme.orders.v1.OrderService have?"
echo "  - what is the HTTP endpoint for acme.payments.v1.PaymentService.AuthorizePayment?"
echo "  - summarize the checkout narrative and cite evidence chunks"
echo ""

echo "-- C) Optional: call /llm/agent once (RUN_SAMPLES=$RUN_SAMPLES)"
if [ "$RUN_SAMPLES" = "1" ]; then
  set +e
  python3 - "$ADDR" "$LLM_HTTP_TIMEOUT_SECS" <<'PY' >"$OUT_DIR/llm_agent_response.json"
import json, sys, urllib.request, urllib.error
addr = sys.argv[1]
timeout = int(sys.argv[2])
payload = {
  "question": "what RPCs does acme.orders.v1.OrderService have?",
  "max_steps": 10,
  "max_rows": 50,
}
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{addr}/llm/agent",
  data=data,
  headers={"Content-Type": "application/json"},
  method="POST",
)
try:
  resp = urllib.request.urlopen(req, timeout=timeout)
  print(resp.read().decode("utf-8"))
except urllib.error.HTTPError as e:
  # Preserve the server-provided JSON error body for debugging.
  body = e.read().decode("utf-8", errors="replace")
  print(body)
  sys.exit(1)
PY
  if [ $? -ne 0 ]; then
    echo "warn: /llm/agent sample failed (see $OUT_DIR/llm_agent_response.json and $OUT_DIR/server.log)"
  fi
  set -e
else
  echo "skip: RUN_SAMPLES=0"
fi

echo ""
echo "-- D) Capture viz artifacts (HTML + JSON)"
if command -v curl >/dev/null 2>&1; then
  curl -sS "http://$ADDR/viz?focus_name=acme.orders.v1.OrderService&plane=both&typed_overlay=true&hops=2&max_nodes=900" \
    >"$OUT_DIR/viz_orders_service.html"
  curl -sS "http://$ADDR/viz.json?focus_name=acme.orders.v1.OrderService&plane=both&typed_overlay=true&hops=2&max_nodes=900" \
    >"$OUT_DIR/viz_orders_service.json"
  curl -sS "http://$ADDR/status" \
    >"$OUT_DIR/status.json"
else
  echo "skip: curl not found; not capturing viz artifacts"
fi

echo ""
echo "Wrote:"
echo "  $OUT_DIR/server.log"
echo "  $OUT_DIR/perf.json"
echo "  $OUT_DIR/llm_agent_response.json"
echo "  $OUT_DIR/viz_orders_service.html"
echo "  $OUT_DIR/viz_orders_service.json"
echo "  $OUT_DIR/status.json"

echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo "Note: this script stops the server when it exits."
  echo "Tip: keep it running by setting KEEP_RUNNING=1."
fi
