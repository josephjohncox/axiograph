#!/usr/bin/env bash
set -euo pipefail

# PathDB server API demo: start `axiograph db serve`, query over HTTP,
# and render server-side viz.
#
# Run from repo root:
#   ./scripts/db_server_api_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/db_server_api_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== PathDB server API demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
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
echo "-- A) Build a small .axpd from a canonical .axi"
AXPD="$OUT_DIR/ontology_rewrites.axpd"
"$AXIOGRAPH" db pathdb import-axi "$ROOT_DIR/examples/ontology/OntologyRewrites.axi" --out "$AXPD"

echo ""
echo "-- B) Start server (ephemeral port)"
READY="$OUT_DIR/ready.json"
"$AXIOGRAPH" db serve --axpd "$AXPD" --listen 127.0.0.1:0 --ready-file "$READY" >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

python3 - "$READY" <<'PY' >"$OUT_DIR/addr.txt"
import json, time, sys
path = sys.argv[1]
deadline = time.time() + 30
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

echo ""
echo "-- C) Status + query over HTTP"
curl -sS "http://$ADDR/status" >"$OUT_DIR/status.json"

curl -sS -X POST "http://$ADDR/query" \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","show_elaboration":true}' \
  >"$OUT_DIR/query_grandparent.json"

echo ""
echo "-- D) Server-side viz (HTML + JSON)"
curl -sS "http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=360" \
  >"$OUT_DIR/viz_alice.html"

curl -sS "http://$ADDR/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=360" \
  >"$OUT_DIR/viz_alice.json"

echo ""
echo "Open in a browser:"
echo "  http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=360"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/status.json"
echo "  $OUT_DIR/query_grandparent.json"
echo "  $OUT_DIR/viz_alice.html"
echo "  $OUT_DIR/viz_alice.json"
echo "  $OUT_DIR/server.log"
