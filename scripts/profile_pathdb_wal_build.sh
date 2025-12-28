#!/usr/bin/env bash
set -euo pipefail

# Profile PathDB WAL "checkout" vs "rebuild" (phase timings + optional flamegraph).
#
# Why:
# - `axiograph db accept pathdb-build` should be fast when a checkpoint exists (hardlink/copy).
# - `--rebuild` forces the slow path (recompute base + apply ops + build indexes) for profiling.
#
# Run from repo root:
#   ./scripts/profile_pathdb_wal_build.sh
#
# Inputs:
#   PLANE_DIR=... SNAPSHOT=head ./scripts/profile_pathdb_wal_build.sh
#
# If the default PLANE_DIR is missing, this script will generate it by running:
#   SKIP_SERVER=1 EMBED_ENABLED=0 ./scripts/graph_explorer_deep_knowledge_demo.sh
#
# Optional:
#   FLAMEGRAPH=1 ./scripts/profile_pathdb_wal_build.sh
#   (requires `cargo flamegraph` to be installed; on macOS this may require extra privileges)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="${OUT_DIR:-$ROOT_DIR/build/profile_pathdb_wal_build}"
PLANE_DIR="${PLANE_DIR:-$ROOT_DIR/build/graph_explorer_deep_knowledge_demo/plane}"
SNAPSHOT="${SNAPSHOT:-head}"

GENERATE="${GENERATE:-1}"
DATA_POINTS="${DATA_POINTS:-100000}"
EXTRA_AXI_MODULES="${EXTRA_AXI_MODULES:-24}"

DO_CHECKOUT="${DO_CHECKOUT:-1}"
DO_REBUILD="${DO_REBUILD:-1}"
FLAMEGRAPH="${FLAMEGRAPH:-0}"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== PathDB WAL build profiling =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"
echo "plane: $PLANE_DIR"
echo "snap:  $SNAPSHOT"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

if [ ! -d "$PLANE_DIR" ]; then
  if [ "$GENERATE" = "1" ] && [ "$PLANE_DIR" = "$ROOT_DIR/build/graph_explorer_deep_knowledge_demo/plane" ]; then
    echo ""
    echo "-- Generating demo snapshot store (deep knowledge) to profile against"
    SKIP_SERVER=1 EMBED_ENABLED=0 DATA_POINTS="$DATA_POINTS" EXTRA_AXI_MODULES="$EXTRA_AXI_MODULES" \
      "$ROOT_DIR/scripts/graph_explorer_deep_knowledge_demo.sh"
  else
    echo "error: missing PLANE_DIR: $PLANE_DIR"
    echo "hint: set PLANE_DIR to an existing accepted-plane dir, or run:"
    echo "  SKIP_SERVER=1 EMBED_ENABLED=0 ./scripts/graph_explorer_deep_knowledge_demo.sh"
    exit 2
  fi
fi

CHECKOUT_AXPD="$OUT_DIR/checkout.axpd"
REBUILD_AXPD="$OUT_DIR/rebuild.axpd"

if [ "$DO_CHECKOUT" = "1" ]; then
  echo ""
  echo "-- A) pathdb-build (checkpoint fast path)"
  "$AXIOGRAPH" db accept pathdb-build \
    --dir "$PLANE_DIR" \
    --snapshot "$SNAPSHOT" \
    --out "$CHECKOUT_AXPD" \
    --timings \
    --timings-json "$OUT_DIR/timings_checkout.json"
fi

if [ "$DO_REBUILD" = "1" ]; then
  echo ""
  echo "-- B) pathdb-build --rebuild (slow path; rebuild + apply ops + indexes)"
  "$AXIOGRAPH" db accept pathdb-build \
    --dir "$PLANE_DIR" \
    --snapshot "$SNAPSHOT" \
    --out "$REBUILD_AXPD" \
    --rebuild \
    --timings \
    --timings-json "$OUT_DIR/timings_rebuild.json"
fi

if [ "$FLAMEGRAPH" = "1" ]; then
  echo ""
  echo "-- C) Optional flamegraph (rebuild hot path)"
  echo "note: requires: cargo flamegraph"
  echo "note: on macOS you may need extra permissions for sampling."
  set +e
  cargo flamegraph --help >/dev/null 2>&1
  if [ $? -ne 0 ]; then
    echo "warn: cargo flamegraph not available; install with:"
    echo "  cargo install flamegraph"
  else
    (
      cd "$ROOT_DIR/rust" && \
        cargo flamegraph -p axiograph-cli --bin axiograph --release -- \
          db accept pathdb-build \
          --dir "$PLANE_DIR" \
          --snapshot "$SNAPSHOT" \
          --out "$OUT_DIR/flamegraph_rebuild.axpd" \
          --rebuild
    )
    echo "ok: flamegraph output written to rust/flamegraph.svg (default cargo-flamegraph path)"
  fi
  set -e
fi

echo ""
echo "Done."
echo "Outputs:"
if [ "$DO_CHECKOUT" = "1" ]; then
  echo "  $CHECKOUT_AXPD"
  echo "  $OUT_DIR/timings_checkout.json"
fi
if [ "$DO_REBUILD" = "1" ]; then
  echo "  $REBUILD_AXPD"
  echo "  $OUT_DIR/timings_rebuild.json"
fi

