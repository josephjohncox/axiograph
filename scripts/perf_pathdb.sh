#!/bin/bash
set -euo pipefail

# PathDB perf harness (synthetic ingestion + indexing + path queries).
#
# Run:
#   ./scripts/perf_pathdb.sh
#
# Tunables (env vars):
#   PERF_NATIVE=1 ENTITIES=200000 EDGES_PER_ENTITY=8 REL_TYPES=8 INDEX_DEPTH=3 PATH_LEN=3 QUERIES=50000 SEED=1
#   OUT_AXPD=/path/to/out.axpd OUT_AXI=/path/to/out.axi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

PERF_NATIVE="${PERF_NATIVE:-1}"
if [ "$PERF_NATIVE" = "1" ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native"
fi
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_PANIC="${CARGO_PROFILE_RELEASE_PANIC:-abort}"

ENTITIES="${ENTITIES:-200000}"
EDGES_PER_ENTITY="${EDGES_PER_ENTITY:-8}"
REL_TYPES="${REL_TYPES:-8}"
INDEX_DEPTH="${INDEX_DEPTH:-3}"
PATH_LEN="${PATH_LEN:-3}"
QUERIES="${QUERIES:-50000}"
SEED="${SEED:-1}"
OUT_AXPD="${OUT_AXPD:-}"
OUT_AXI="${OUT_AXI:-}"

echo "== Axiograph perf (pathdb) =="
echo "root: $ROOT_DIR"

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

ARGS=(
  --entities "$ENTITIES"
  --edges-per-entity "$EDGES_PER_ENTITY"
  --rel-types "$REL_TYPES"
  --index-depth "$INDEX_DEPTH"
  --path-len "$PATH_LEN"
  --queries "$QUERIES"
  --seed "$SEED"
)

if [ -n "$OUT_AXPD" ]; then
  ARGS+=(--out-axpd "$OUT_AXPD")
fi
if [ -n "$OUT_AXI" ]; then
  ARGS+=(--out-axi "$OUT_AXI")
fi

echo ""
echo "-- Run: tools perf pathdb"
"$AXIOGRAPH" tools perf pathdb "${ARGS[@]}"

echo ""
echo "Done."
