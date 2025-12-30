#!/usr/bin/env bash
set -euo pipefail

# Perf harness for cache/index layers (fact/text caches + path LRU).
#
# Run from repo root:
#   ./scripts/perf_index_caches.sh
#
# Scaling knobs (single run or multiple via SCALES):
#   ENTITIES=50000 EDGES_PER_ENTITY=8 REL_TYPES=8 INDEX_DEPTH=2 PATH_LEN=4 \
#     PATH_QUERIES=20000 FACT_QUERIES=20000 TEXT_QUERIES=20000 \
#     ./scripts/perf_index_caches.sh
#
#   SCALES=20000,80000,20000 ./scripts/perf_index_caches.sh
#
# Cache controls:
#   LRU_CAPACITY=256 LRU_ASYNC=1 LRU_QUEUE=1024 INDEX_MODE=async ASYNC_WAIT_SECS=20 \
#     VERIFY=1 MUTATIONS=0 ./scripts/perf_index_caches.sh
#
# Profiling (CPU+mem on by default):
#   PROFILE_ENABLED=0 ./scripts/perf_index_caches.sh
#   PROFILE_FORMAT=pprof PROFILE_HZ=199 PROFILE_INTERVAL=10 ./scripts/perf_index_caches.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${OUT_DIR:-$ROOT_DIR/build/perf_index_caches}"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

PERF_NATIVE="${PERF_NATIVE:-1}"
if [ "$PERF_NATIVE" = "1" ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native"
fi
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_PANIC="${CARGO_PROFILE_RELEASE_PANIC:-abort}"

PROFILE_ENABLED="${PROFILE_ENABLED:-1}"
PROFILE_CPU="${PROFILE_CPU:-1}"
PROFILE_MEM="${PROFILE_MEM:-1}"
PROFILE_FORMAT="${PROFILE_FORMAT:-all}"
PROFILE_HZ="${PROFILE_HZ:-99}"
PROFILE_INTERVAL="${PROFILE_INTERVAL:-10}"
PROFILE_SIGNAL="${PROFILE_SIGNAL:-0}"
PROFILE_DIR="$OUT_DIR/profiles"
PROFILE_SEQ=0

if [ "$PROFILE_ENABLED" = "0" ]; then
  PROFILE_CPU=0
  PROFILE_MEM=0
fi

ENTITIES="${ENTITIES:-50000}"
EDGES_PER_ENTITY="${EDGES_PER_ENTITY:-8}"
REL_TYPES="${REL_TYPES:-8}"
INDEX_DEPTH="${INDEX_DEPTH:-2}"
PATH_LEN="${PATH_LEN:-4}"
PATH_QUERIES="${PATH_QUERIES:-20000}"
FACT_QUERIES="${FACT_QUERIES:-20000}"
TEXT_QUERIES="${TEXT_QUERIES:-20000}"
SEED="${SEED:-1}"

LRU_CAPACITY="${LRU_CAPACITY:-256}"
LRU_ASYNC="${LRU_ASYNC:-1}"
LRU_QUEUE="${LRU_QUEUE:-1024}"
INDEX_MODE="${INDEX_MODE:-async}"
ASYNC_WAIT_SECS="${ASYNC_WAIT_SECS:-20}"
VERIFY="${VERIFY:-1}"
MUTATIONS="${MUTATIONS:-0}"

if [ "$LRU_ASYNC" = "1" ]; then
  LRU_ASYNC_STR="true"
else
  LRU_ASYNC_STR="false"
fi

SCALES="${SCALES:-}"

echo "== Axiograph perf (index caches) =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"
if [ "$PROFILE_ENABLED" = "1" ]; then
  echo "profiling: cpu=$PROFILE_CPU mem=$PROFILE_MEM format=$PROFILE_FORMAT hz=$PROFILE_HZ interval=${PROFILE_INTERVAL}s signal=$PROFILE_SIGNAL dir=$PROFILE_DIR"
fi

mkdir -p "$PROFILE_DIR"
TIME_MODE="none"
TIME_FLAG=""
if [ "$PROFILE_MEM" = "1" ] && [ -x /usr/bin/time ]; then
  if /usr/bin/time -o "$PROFILE_DIR/.time_probe" -v true >/dev/null 2>&1; then
    TIME_MODE="file"
    TIME_FLAG="-v"
  elif /usr/bin/time -o "$PROFILE_DIR/.time_probe" -l true >/dev/null 2>&1; then
    TIME_MODE="file"
    TIME_FLAG="-l"
  elif /usr/bin/time -v true >/dev/null 2>&1; then
    TIME_MODE="tee"
    TIME_FLAG="-v"
  elif /usr/bin/time -l true >/dev/null 2>&1; then
    TIME_MODE="tee"
    TIME_FLAG="-l"
  fi
  rm -f "$PROFILE_DIR/.time_probe"
fi

profile_label() {
  PROFILE_SEQ=$((PROFILE_SEQ + 1))
  printf "%03d_%s" "$PROFILE_SEQ" "$1"
}

axiograph_profiled() {
  local label
  label="$(profile_label "$1")"
  shift

  local cmd=("$AXIOGRAPH")
  if [ "$PROFILE_CPU" = "1" ]; then
    cmd+=(--profile "$PROFILE_FORMAT" --profile-out "$PROFILE_DIR/$label" --profile-hz "$PROFILE_HZ")
    if [ -n "${PROFILE_INTERVAL:-}" ] && [ "$PROFILE_INTERVAL" -gt 0 ]; then
      cmd+=(--profile-interval "$PROFILE_INTERVAL")
    fi
    if [ "${PROFILE_SIGNAL:-0}" = "1" ]; then
      cmd+=(--profile-signal)
    fi
  fi

  if [ "$PROFILE_MEM" = "1" ] && [ "$TIME_MODE" != "none" ] && [ -n "$TIME_FLAG" ]; then
    local time_log="$PROFILE_DIR/${label}.time.txt"
    if [ "$TIME_MODE" = "file" ]; then
      /usr/bin/time -o "$time_log" "$TIME_FLAG" "${cmd[@]}" "$@"
    else
      { /usr/bin/time "$TIME_FLAG" "${cmd[@]}" "$@"; } 2> >(tee "$time_log" >&2)
    fi
  else
    "${cmd[@]}" "$@"
  fi
}

echo ""
echo "-- Build (via Makefile)"
if [ "$PROFILE_CPU" = "1" ]; then
  make binaries CARGO_FEATURES="--features profiling"
else
  make binaries
fi

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

if [ -n "$SCALES" ]; then
  SCALES="${SCALES// /}"
  IFS=',' read -r -a SCALE_LIST <<< "$SCALES"
else
  SCALE_LIST=("$ENTITIES")
fi

run_idx=0
for SCALE in "${SCALE_LIST[@]}"; do
  run_idx=$((run_idx + 1))
  echo ""
  echo "-- Run $run_idx: entities=$SCALE"

  ARGS=(
    tools perf indexes
    --entities "$SCALE"
    --edges-per-entity "$EDGES_PER_ENTITY"
    --rel-types "$REL_TYPES"
    --index-depth "$INDEX_DEPTH"
    --path-len "$PATH_LEN"
    --path-queries "$PATH_QUERIES"
    --fact-queries "$FACT_QUERIES"
    --text-queries "$TEXT_QUERIES"
    --lru-capacity "$LRU_CAPACITY"
    --lru-async "$LRU_ASYNC_STR"
    --lru-queue "$LRU_QUEUE"
    --index-mode "$INDEX_MODE"
    --async-wait-secs "$ASYNC_WAIT_SECS"
    --seed "$SEED"
    --out-json "$OUT_DIR/report_${run_idx}_${SCALE}.json"
  )

  if [ "$VERIFY" = "1" ]; then
    ARGS+=(--verify)
  fi

  if [ "$MUTATIONS" -gt 0 ]; then
    ARGS+=(--mutations "$MUTATIONS")
  fi

  axiograph_profiled "indexes_${SCALE}" "${ARGS[@]}"
done

echo ""
echo "Done."
