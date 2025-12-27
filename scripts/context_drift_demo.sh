#!/usr/bin/env bash
set -euo pipefail

# Context drift demo (KL/JS divergence) over a small `.axi` example.
#
# Run from repo root:
#   ./scripts/context_drift_demo.sh

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/context_drift_demo"
mkdir -p "$OUT_DIR"

AXI="$ROOT_DIR/examples/Family.axi"
AXPD="$OUT_DIR/family.axpd"

echo "== Context drift demo =="
echo "axi:  $AXI"
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

echo "== A) Import .axi → .axpd =="
"$AXIOGRAPH" db pathdb import-axi "$AXI" --out "$AXPD"

echo ""
echo "== B) Analyze drift between contexts =="
echo "(CensusData vs FamilyTree)"
"$AXIOGRAPH" tools analyze context-drift "$AXPD" \
  --ctx-a CensusData \
  --ctx-b FamilyTree \
  --metric js \
  --alpha 1.0 \
  --top 25

echo ""
echo "Done."
