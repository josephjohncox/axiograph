#!/usr/bin/env bash
set -euo pipefail

# Distributed-ish demo: write-master + read-replica using snapshot-store replication,
# plus HTTP APIs on top via `axiograph db serve`.
#
# Run from repo root:
#   ./scripts/db_server_distributed_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/db_server_distributed_demo"
MASTER_DIR="$OUT_DIR/master_plane"
REPLICA_DIR="$OUT_DIR/replica_plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== PathDB distributed demo (master/replica + HTTP APIs) =="
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
echo "-- A) Create master accepted-plane (bootstrap)"
"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/economics/EconomicFlows.axi" \
  --dir "$MASTER_DIR" \
  --message "demo: bootstrap EconomicFlows"

echo ""
echo "-- B) Sync master → replica (filesystem-only)"
"$AXIOGRAPH" db accept sync \
  --from "$MASTER_DIR" \
  --dir "$REPLICA_DIR" \
  --layer accepted \
  --include-checkpoints \
  --include-logs

echo ""
echo "-- C) Start master + replica servers (ephemeral ports)"
ADMIN_TOKEN="demo-admin-token"

MASTER_READY="$OUT_DIR/master_ready.json"
REPLICA_READY="$OUT_DIR/replica_ready.json"

"$AXIOGRAPH" db serve \
  --dir "$MASTER_DIR" \
  --layer accepted \
  --snapshot head \
  --role master \
  --admin-token "$ADMIN_TOKEN" \
  --listen 127.0.0.1:0 \
  --ready-file "$MASTER_READY" \
  >"$OUT_DIR/master_server.log" 2>&1 &
MASTER_PID=$!

"$AXIOGRAPH" db serve \
  --dir "$REPLICA_DIR" \
  --layer accepted \
  --snapshot head \
  --role replica \
  --listen 127.0.0.1:0 \
  --ready-file "$REPLICA_READY" \
  >"$OUT_DIR/replica_server.log" 2>&1 &
REPLICA_PID=$!

trap 'kill "$MASTER_PID" "$REPLICA_PID" 2>/dev/null || true' EXIT

python3 - "$MASTER_READY" "$REPLICA_READY" <<'PY' >"$OUT_DIR/addrs.txt"
import json, time, sys
paths = sys.argv[1:]
def wait(path):
  deadline = time.time() + 30
  while time.time() < deadline:
    try:
      with open(path) as f:
        j = json.load(f)
      if "addr" in j:
        return j["addr"]
    except Exception:
      time.sleep(0.05)
  raise RuntimeError(f"server did not write ready file: {path}")

master = wait(paths[0])
replica = wait(paths[1])
print(master)
print(replica)
PY

MASTER_ADDR="$(sed -n '1p' "$OUT_DIR/addrs.txt")"
REPLICA_ADDR="$(sed -n '2p' "$OUT_DIR/addrs.txt")"

echo "master:  http://$MASTER_ADDR"
echo "replica: http://$REPLICA_ADDR"
echo "admin token (for master /admin/* endpoints): $ADMIN_TOKEN"

echo ""
echo "-- D) Query both servers (EconomicFlows)"
curl -sS -X POST "http://$MASTER_ADDR/query" \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?to where name(\"Household_A\") -Flow-> ?to limit 5","show_elaboration":true}' \
  >"$OUT_DIR/master_query_econ.json"

curl -sS -X POST "http://$REPLICA_ADDR/query" \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?to where name(\"Household_A\") -Flow-> ?to limit 5","show_elaboration":true}' \
  >"$OUT_DIR/replica_query_econ.json"

echo ""
echo "-- E) Promote a second module on the master via HTTP admin API"
python3 - "$MASTER_ADDR" "$ADMIN_TOKEN" "$ROOT_DIR/examples/ontology/OntologyRewrites.axi" <<'PY' >"$OUT_DIR/master_promote_response.json"
import json, sys, urllib.request
master_addr = sys.argv[1]
token = sys.argv[2]
axi_path = sys.argv[3]
axi_text = open(axi_path, "r", encoding="utf-8").read()

payload = {
  "axi_text": axi_text,
  "message": "demo: promote OntologyRewrites via HTTP",
  "quality": "off",
}
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{master_addr}/admin/accept/promote",
  data=data,
  headers={
    "Content-Type": "application/json",
    "Authorization": f"Bearer {token}",
  },
  method="POST",
)
resp = urllib.request.urlopen(req, timeout=10)
print(resp.read().decode("utf-8"))
PY

curl -sS "http://$MASTER_ADDR/status" >"$OUT_DIR/master_status_after_promote.json"

echo ""
echo "-- F) Sync master → replica again and wait for replica reload"
"$AXIOGRAPH" db accept sync \
  --from "$MASTER_DIR" \
  --dir "$REPLICA_DIR" \
  --layer accepted \
  --include-checkpoints \
  --include-logs

sleep 3

echo ""
echo "-- G) Query replica for data introduced by the promoted module"
curl -sS -X POST "http://$REPLICA_ADDR/query" \
  -H 'Content-Type: application/json' \
  -d '{"query":"select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10","show_elaboration":true}' \
  >"$OUT_DIR/replica_query_grandparent.json"

echo ""
echo "-- H) Viz over HTTP (replica)"
curl -sS "http://$REPLICA_ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
  >"$OUT_DIR/replica_viz_alice.html"

curl -sS "http://$REPLICA_ADDR/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
  >"$OUT_DIR/replica_viz_alice.json"

echo ""
echo "-- I) Snapshot list (time travel)"
curl -sS "http://$REPLICA_ADDR/snapshots?layer=accepted&limit=25" >"$OUT_DIR/replica_snapshots.json"
echo "wrote: $OUT_DIR/replica_snapshots.json"

echo ""
echo "Open a live view of the replica (auto-refresh):"
echo "  http://$REPLICA_ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420&refresh_secs=2"
echo ""
echo "Time travel: pick a snapshot id from $OUT_DIR/replica_snapshots.json and open:"
echo "  http://$REPLICA_ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420&snapshot=<snapshot_id>"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/master_query_econ.json"
echo "  $OUT_DIR/replica_query_econ.json"
echo "  $OUT_DIR/master_promote_response.json"
echo "  $OUT_DIR/replica_query_grandparent.json"
echo "  $OUT_DIR/replica_viz_alice.html"
echo "  $OUT_DIR/replica_viz_alice.json"
echo "  $OUT_DIR/replica_snapshots.json"

echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the servers running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$MASTER_PID" "$REPLICA_PID"
else
  echo "Note: this script stops both servers when it exits."
  echo "Tip: keep them running by setting KEEP_RUNNING=1."
fi
