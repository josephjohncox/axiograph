#!/bin/bash
set -euo pipefail

# Large-scale PathDB/.axpd performance demo (scenario-based).
#
# This is deterministic and does not require any network/LLM access.
#
# Run from repo root:
#   ./scripts/perf_large_proto_api_axpd.sh
#
# Tunables (env vars):
#   PERF_NATIVE=1 SCALE=10000 INDEX_DEPTH=3 SEED=1
#   PATH_QUERIES=100000 AXQL_QUERIES=10000 AXQL_LIMIT=25

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

SCALE="${SCALE:-10000}"
INDEX_DEPTH="${INDEX_DEPTH:-3}"
SEED="${SEED:-1}"
PATH_QUERIES="${PATH_QUERIES:-100000}"
AXQL_QUERIES="${AXQL_QUERIES:-10000}"
AXQL_LIMIT="${AXQL_LIMIT:-25}"

OUT_DIR="$ROOT_DIR/build/perf_large_proto_api_axpd"
mkdir -p "$OUT_DIR"

AXPD_OUT="$OUT_DIR/proto_api_scale${SCALE}_depth${INDEX_DEPTH}_seed${SEED}.axpd"
JSON_OUT="$OUT_DIR/report_scale${SCALE}_depth${INDEX_DEPTH}_seed${SEED}.json"

echo "== Axiograph perf (proto_api scenario) =="
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
echo "-- Run: tools perf scenario (ingest + build_indexes + axpd roundtrip + workload)"
"$AXIOGRAPH" tools perf scenario \
  --scenario proto_api \
  --scale "$SCALE" \
  --index-depth "$INDEX_DEPTH" \
  --seed "$SEED" \
  --path-queries "$PATH_QUERIES" \
  --axql-queries "$AXQL_QUERIES" \
  --axql-limit "$AXQL_LIMIT" \
  --out-axpd "$AXPD_OUT" \
  --out-json "$JSON_OUT"

echo ""
echo "Done."
echo "Outputs:"
echo "  $AXPD_OUT"
echo "  $JSON_OUT"
