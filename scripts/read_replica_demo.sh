#!/usr/bin/env bash
set -euo pipefail

# Write-master / read-replica demo using the snapshot store.
#
# This is filesystem-only and does not require any network/LLM access.
#
# Run from repo root:
#   ./scripts/read_replica_demo.sh

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/read_replica_demo"
MASTER_DIR="$OUT_DIR/master_plane"
REPLICA_DIR="$OUT_DIR/replica_plane"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== Axiograph read-replica demo =="
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

echo "== A) Build master snapshot store =="
"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/economics/EconomicFlows.axi" \
  --dir "$MASTER_DIR" \
  --message "demo: promote EconomicFlows"

CHUNKS_JSON="$OUT_DIR/docs_chunks.json"
PROPOSALS_JSON="$OUT_DIR/docs_proposals.json"
"$AXIOGRAPH" ingest repo index "$ROOT_DIR/docs" \
  --out "$PROPOSALS_JSON" \
  --chunks "$CHUNKS_JSON" \
  --max-files 200

"$AXIOGRAPH" db accept pathdb-commit \
  --dir "$MASTER_DIR" \
  --accepted-snapshot head \
  --chunks "$CHUNKS_JSON" \
  --message "demo: import docs chunks overlay"

echo ""
"$AXIOGRAPH" db accept status --dir "$MASTER_DIR"

echo ""
echo "== B) Sync master → replica =="
"$AXIOGRAPH" db accept sync \
  --from "$MASTER_DIR" \
  --dir "$REPLICA_DIR" \
  --layer both \
  --include-checkpoints \
  --include-logs

echo ""
"$AXIOGRAPH" db accept status --dir "$REPLICA_DIR"

echo ""
echo "== C) Query from replica =="
REPLICA_AXPD="$OUT_DIR/replica_head.axpd"
"$AXIOGRAPH" db accept pathdb-build --dir "$REPLICA_DIR" --snapshot head --out "$REPLICA_AXPD"

# A small sanity query that should work from the replica without talking to the master.
"$AXIOGRAPH" repl --axpd "$REPLICA_AXPD" --quiet \
  --cmd 'q select ?a where ?a is Agent limit 10'

echo ""
echo "Done."
echo "Outputs:"
echo "  master store:   $MASTER_DIR"
echo "  replica store:  $REPLICA_DIR"
echo "  replica axpd:   $REPLICA_AXPD"
