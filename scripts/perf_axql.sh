#!/bin/bash
set -euo pipefail

# AxQL perf harness (synthetic querying over a generated PathDB).
#
# Run:
#   ./scripts/perf_axql.sh
#
# Tunables (env vars):
#   PERF_NATIVE=1 ENTITIES=200000 EDGES_PER_ENTITY=8 REL_TYPES=8 INDEX_DEPTH=3 MODE=star PATH_LEN=3
#   LIMIT=200 QUERIES=2000 SEED=1

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
MODE="${MODE:-star}"
PATH_LEN="${PATH_LEN:-3}"
LIMIT="${LIMIT:-200}"
QUERIES="${QUERIES:-2000}"
SEED="${SEED:-1}"

echo "== Axiograph perf (axql) =="
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

echo ""
echo "-- Run: tools perf axql"
"$AXIOGRAPH" tools perf axql \
  --entities "$ENTITIES" \
  --edges-per-entity "$EDGES_PER_ENTITY" \
  --rel-types "$REL_TYPES" \
  --index-depth "$INDEX_DEPTH" \
  --mode "$MODE" \
  --path-len "$PATH_LEN" \
  --limit "$LIMIT" \
  --queries "$QUERIES" \
  --seed "$SEED"

echo ""
echo "Done."
