#!/usr/bin/env bash
set -euo pipefail

# Snapshot store demo: accepted plane + PathDB WAL.
#
# This is intentionally deterministic and does not require any LLM/network access.
#
# Run from repo root:
#   ./scripts/snapshot_store_demo.sh

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/snapshot_store_demo"
ACCEPTED_DIR="$OUT_DIR/accepted_plane"

mkdir -p "$OUT_DIR"

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

echo "== A) Promote canonical .axi modules into accepted plane =="
"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/economics/EconomicFlows.axi" \
  --dir "$ACCEPTED_DIR" \
  --message "demo: promote EconomicFlows"

"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/learning/MachinistLearning.axi" \
  --dir "$ACCEPTED_DIR" \
  --message "demo: promote MachinistLearning"

"$AXIOGRAPH" db accept promote "$ROOT_DIR/examples/ontology/SchemaEvolution.axi" \
  --dir "$ACCEPTED_DIR" \
  --message "demo: promote SchemaEvolution"

echo ""
"$AXIOGRAPH" db accept status --dir "$ACCEPTED_DIR"

echo ""
echo "== B) Build base PathDB (.axpd) from accepted HEAD =="
BASE_AXPD="$OUT_DIR/accepted_base.axpd"
"$AXIOGRAPH" db accept build-pathdb --dir "$ACCEPTED_DIR" --snapshot head --out "$BASE_AXPD"
echo "wrote: $BASE_AXPD"

echo ""
echo "== C) Create a chunks overlay (repo index over ./docs) =="
CHUNKS_JSON="$OUT_DIR/docs_chunks.json"
PROPOSALS_JSON="$OUT_DIR/docs_proposals.json"
"$AXIOGRAPH" ingest repo index "$ROOT_DIR/docs" --out "$PROPOSALS_JSON" --chunks "$CHUNKS_JSON" --max-files 2000
echo "wrote: $CHUNKS_JSON"

echo ""
echo "== D) Commit chunks overlay into PathDB WAL (append-only) =="
"$AXIOGRAPH" db accept pathdb-commit --dir "$ACCEPTED_DIR" --accepted-snapshot head --chunks "$CHUNKS_JSON" \
  --message "demo: import docs chunks overlay"

echo ""
"$AXIOGRAPH" db accept status --dir "$ACCEPTED_DIR"

echo ""
echo "== E) Build/check out PathDB snapshot (.axpd) from WAL HEAD =="
WITH_CHUNKS_AXPD="$OUT_DIR/accepted_with_chunks.axpd"
"$AXIOGRAPH" db accept pathdb-build --dir "$ACCEPTED_DIR" --snapshot head --out "$WITH_CHUNKS_AXPD"
echo "wrote: $WITH_CHUNKS_AXPD"

echo ""
echo "== F) Export reversible .axi snapshot (for anchoring certificates) =="
EXPORT_AXI="$OUT_DIR/accepted_with_chunks_export_v1.axi"
"$AXIOGRAPH" db pathdb export-axi "$WITH_CHUNKS_AXPD" --out "$EXPORT_AXI"
echo "wrote: $EXPORT_AXI"

echo ""
echo "== G) Inspect history (git-like logs) =="
"$AXIOGRAPH" db accept log --dir "$ACCEPTED_DIR" --layer accepted --limit 10
echo ""
"$AXIOGRAPH" db accept log --dir "$ACCEPTED_DIR" --layer pathdb --limit 10

echo ""
echo "Done."
echo "Outputs:"
echo "  accepted plane dir: $ACCEPTED_DIR"
echo "  base axpd:          $BASE_AXPD"
echo "  chunks:             $CHUNKS_JSON"
echo "  wal axpd:           $WITH_CHUNKS_AXPD"
echo "  export axi:         $EXPORT_AXI"
