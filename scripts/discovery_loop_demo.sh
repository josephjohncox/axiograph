#!/bin/bash
set -euo pipefail

# A small end-to-end “evidence → discovery → candidates” demo.
#
# This stays entirely in the evidence plane until the final step (promotion),
# where we emit *candidate* `.axi` modules for human review.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

OUT_DIR="$ROOT_DIR/build/discovery_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph discovery loop demo =="
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
echo "-- ingest repo index"
"$AXIOGRAPH" ingest repo index "$ROOT_DIR" \
  --out "$OUT_DIR/repo_proposals.json" \
  --chunks "$OUT_DIR/repo_chunks.json" \
  --edges "$OUT_DIR/repo_edges.json"

echo ""
echo "-- discover suggest-links"
"$AXIOGRAPH" discover suggest-links \
  "$OUT_DIR/repo_chunks.json" \
  "$OUT_DIR/repo_edges.json" \
  --out "$OUT_DIR/repo_discovery_trace.json" \
  --max-proposals 2000

echo ""
echo "-- discover augment-proposals (heuristics only)"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/repo_proposals.json" \
  --out "$OUT_DIR/repo_proposals.aug.json" \
  --trace "$OUT_DIR/repo_proposals.aug.trace.json"

echo ""
echo "-- discover promote-proposals (emit candidates)"
"$AXIOGRAPH" discover promote-proposals \
  "$OUT_DIR/repo_proposals.aug.json" \
  --out-dir "$OUT_DIR/candidates" \
  --min-confidence 0.5

echo ""
echo "Done."
echo "Candidates: $OUT_DIR/candidates"
