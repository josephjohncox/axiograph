#!/usr/bin/env bash
set -euo pipefail

# RDF/OWL/SHACL ingestion demo (public datasets + local fixtures).
#
# This script is deterministic for the local fixture, and optionally uses
# datasets downloaded by `scripts/fetch_public_rdfowl_datasets.sh`.
#
# Run from repo root:
#   ./scripts/rdfowl_public_datasets_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/rdfowl_public_datasets_demo"
mkdir -p "$OUT_DIR"

echo "== RDF/OWL/SHACL ingestion demo =="
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
echo "-- A) Ingest local SHACL fixture (no network)"
FIXTURE_DIR="$ROOT_DIR/examples/rdfowl/w3c_shacl_minimal"
FIXTURE_OUT="$OUT_DIR/w3c_shacl_minimal"
mkdir -p "$FIXTURE_OUT"

"$AXIOGRAPH" ingest dir "$FIXTURE_DIR" \
  --out-dir "$FIXTURE_OUT" \
  --domain rdfowl

echo ""
echo "-- B) Draft a candidate axi_v1 module from the proposals (schema discovery)"
FIXTURE_PROPOSALS="$FIXTURE_OUT/proposals.json"
FIXTURE_AXI="$FIXTURE_OUT/Discovered_w3c_shacl_minimal.axi"
"$AXIOGRAPH" discover draft-module "$FIXTURE_PROPOSALS" --out "$FIXTURE_AXI" \
  --module "Discovered_W3C_SHACL_Minimal" \
  --schema "Discovered_W3C_SHACL_Minimal" \
  --infer-constraints

echo ""
echo "-- C) Import drafted module into PathDB (.axpd) and run tooling"
FIXTURE_AXPD="$FIXTURE_OUT/Discovered_w3c_shacl_minimal.axpd"
"$AXIOGRAPH" db pathdb import-axi "$FIXTURE_AXI" --out "$FIXTURE_AXPD"

"$AXIOGRAPH" tools analyze network "$FIXTURE_AXPD" --plane both --format json --out "$FIXTURE_OUT/network.json"
"$AXIOGRAPH" check quality "$FIXTURE_AXPD" --plane both --profile fast --format json --no-fail --out "$FIXTURE_OUT/quality.json"

echo ""
echo "-- C2) Viz: meta-plane + typed data-plane overlay"
"$AXIOGRAPH" tools viz "$FIXTURE_AXPD" \
  --out "$FIXTURE_OUT/viz_meta.html" \
  --format html \
  --plane meta \
  --focus-name "Discovered_W3C_SHACL_Minimal" \
  --hops 3 \
  --max-nodes 520

"$AXIOGRAPH" tools viz "$FIXTURE_AXPD" \
  --out "$FIXTURE_OUT/viz_data_typed.html" \
  --format html \
  --plane data \
  --typed-overlay \
  --focus-name Alice \
  --hops 2 \
  --max-nodes 520

echo ""
echo "-- D) Optional: ingest W3C data-shapes repo (if downloaded)"
W3C_DIR="$ROOT_DIR/build/datasets/w3c_data_shapes"
if [ -d "$W3C_DIR" ]; then
  echo "found: $W3C_DIR"
  # Keep this small-ish: ingest only a narrow slice of the test suite.
  # (The full repo is large; you can point `ingest dir` at the whole thing if desired.)
  W3C_SLICE="$W3C_DIR/tests/core/node"
  if [ -d "$W3C_SLICE" ]; then
    W3C_OUT="$OUT_DIR/w3c_data_shapes_core_node"
    mkdir -p "$W3C_OUT"
    "$AXIOGRAPH" ingest dir "$W3C_SLICE" --out-dir "$W3C_OUT" --domain rdfowl
    echo "ingested: $W3C_SLICE -> $W3C_OUT"
  else
    echo "skip: expected slice not found: $W3C_SLICE"
  fi
else
  echo "skip: $W3C_DIR (run ./scripts/fetch_public_rdfowl_datasets.sh)"
fi

echo ""
echo "Done."
echo "Outputs:"
echo "  $FIXTURE_OUT/proposals.json"
echo "  $FIXTURE_AXI"
echo "  $FIXTURE_AXPD"
echo "  $FIXTURE_OUT/network.json"
echo "  $FIXTURE_OUT/quality.json"
echo "  $FIXTURE_OUT/viz_meta.html"
echo "  $FIXTURE_OUT/viz_data_typed.html"
