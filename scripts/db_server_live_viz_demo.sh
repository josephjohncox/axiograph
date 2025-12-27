#!/usr/bin/env bash
set -euo pipefail

# Live visualization demo:
# - start `axiograph db serve` in master mode
# - open `/viz?...&refresh_secs=N` in a browser
# - then apply a few updates via HTTP admin endpoints so the visualization changes over time
#
# Run from repo root:
#   ./scripts/db_server_live_viz_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/db_server_live_viz_demo"
PLANE_DIR="$OUT_DIR/plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== PathDB server live viz demo =="
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
echo "-- A) Seed accepted plane with OntologyRewrites"
"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/ontology/OntologyRewrites.axi" \
  --dir "$PLANE_DIR" \
  --message "demo: seed OntologyRewrites"

echo ""
echo "-- B) Start master server (ephemeral port)"
ADMIN_TOKEN="demo-admin-token"
READY="$OUT_DIR/ready.json"

"$AXIOGRAPH" db serve \
  --dir "$PLANE_DIR" \
  --layer accepted \
  --snapshot head \
  --role master \
  --admin-token "$ADMIN_TOKEN" \
  --listen 127.0.0.1:0 \
  --ready-file "$READY" \
  >"$OUT_DIR/server.log" 2>&1 &
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
echo "admin token (for /admin/* endpoints): $ADMIN_TOKEN"
echo ""
echo "Open this in a browser to watch changes (auto-refresh):"
echo "  http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420&refresh_secs=2"
echo ""
echo "This script will now apply a few updates (every ~3s)."

TICK0="$OUT_DIR/OntologyRewrites_tick0.axi"
TICK1="$OUT_DIR/OntologyRewrites_tick1.axi"
TICK2="$OUT_DIR/OntologyRewrites_tick2.axi"

cp "$ROOT_DIR/examples/ontology/OntologyRewrites.axi" "$TICK0"

python3 - "$TICK0" "$TICK1" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace("Person = {Alice, Bob, Carol, Eve}", "Person = {Alice, Bob, Carol, Eve, Zoe}")
text = text.replace("(parent=Bob, child=Carol)\n  }", "(parent=Bob, child=Carol),\n    (parent=Carol, child=Zoe)\n  }")
open(dst, "w", encoding="utf-8").write(text)
PY

python3 - "$TICK1" "$TICK2" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace("(employee=Eve, manager=Bob)", "(employee=Eve, manager=Bob),\n    (employee=Zoe, manager=Eve)")
open(dst, "w", encoding="utf-8").write(text)
PY

promote() {
  local path="$1"
  local msg="$2"
  python3 - "$ADDR" "$ADMIN_TOKEN" "$path" "$msg" <<'PY' >>"$OUT_DIR/promote.log"
import json, sys, urllib.request
addr = sys.argv[1]
token = sys.argv[2]
axi_path = sys.argv[3]
msg = sys.argv[4]
axi_text = open(axi_path, "r", encoding="utf-8").read()
payload = {"axi_text": axi_text, "message": msg, "quality": "off"}
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{addr}/admin/accept/promote",
  data=data,
  headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
  method="POST",
)
resp = urllib.request.urlopen(req, timeout=10)
print(resp.read().decode("utf-8"))
PY
}

sleep 3
echo "-- tick1: add Zoe + new Parent edge (Carol→Zoe)"
promote "$TICK1" "demo: tick1 (add Zoe + new Parent edge)"
sleep 3

echo "-- tick2: add new ReportsTo (Zoe→Eve)"
promote "$TICK2" "demo: tick2 (add Zoe reports to Eve)"
sleep 3

echo ""
echo "-- final: capture viz artifacts"
if command -v curl >/dev/null 2>&1; then
  curl -sS "http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
    >"$OUT_DIR/final_viz_alice.html"
  curl -sS "http://$ADDR/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
    >"$OUT_DIR/final_viz_alice.json"
  curl -sS "http://$ADDR/snapshots?layer=accepted&limit=25" \
    >"$OUT_DIR/snapshots.json"
else
  echo "skip: curl not found; not capturing final viz artifacts"
fi

echo ""
echo "Wrote:"
echo "  $OUT_DIR/promote.log"
echo "  $OUT_DIR/OntologyRewrites_tick1.axi"
echo "  $OUT_DIR/OntologyRewrites_tick2.axi"
echo "  $OUT_DIR/final_viz_alice.html"
echo "  $OUT_DIR/final_viz_alice.json"
echo "  $OUT_DIR/snapshots.json"
echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo "Note: this script stops the server when it exits."
  echo "Tip: keep it running by setting KEEP_RUNNING=1."
fi
