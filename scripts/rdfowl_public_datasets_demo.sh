#!/usr/bin/env bash
set -euo pipefail

# RDF/OWL/SHACL ingestion demo (public datasets + local boundary example).
#
# This script is deterministic for the local committed example, and optionally uses
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

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Ingest local SHACL boundary example (no network)"
SOURCE_DIR="$ROOT_DIR/examples/rdfowl/w3c_shacl_minimal"
SOURCE_OUT="$OUT_DIR/w3c_shacl_minimal"
mkdir -p "$SOURCE_OUT"

"$AXIOGRAPH" ingest dir "$SOURCE_DIR" \
  --out-dir "$SOURCE_OUT" \
  --domain rdfowl

echo ""
echo "-- B) Draft a candidate axi_v1 module from the proposals (schema discovery)"
SOURCE_PROPOSALS="$SOURCE_OUT/proposals.json"
SOURCE_AXI="$SOURCE_OUT/Discovered_w3c_shacl_minimal.axi"
"$AXIOGRAPH" discover draft-module "$SOURCE_PROPOSALS" --out "$SOURCE_AXI" \
  --module "Discovered_W3C_SHACL_Minimal" \
  --schema "Discovered_W3C_SHACL_Minimal" \
  --infer-constraints

echo ""
echo "-- C) Run tooling over process-local state derived from the candidate .axi"
"$AXIOGRAPH" tools analyze network "$SOURCE_AXI" --plane both --format json --out "$SOURCE_OUT/network.json"
"$AXIOGRAPH" check quality "$SOURCE_AXI" --plane both --profile fast --format json --no-fail --out "$SOURCE_OUT/quality.json"

echo ""
echo "-- C2) Viz: meta-plane + typed data-plane overlay"
"$AXIOGRAPH" tools viz "$SOURCE_AXI" \
  --out "$SOURCE_OUT/viz_meta.json" \
  --format json \
  --plane meta \
  --focus-name "Discovered_W3C_SHACL_Minimal" \
  --hops 3 \
  --max-nodes 520

"$AXIOGRAPH" tools viz "$SOURCE_AXI" \
  --out "$SOURCE_OUT/viz_data_typed.json" \
  --format json \
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
echo "  $SOURCE_OUT/proposals.json"
echo "  $SOURCE_AXI"
echo "  $SOURCE_OUT/network.json"
echo "  $SOURCE_OUT/quality.json"
echo "  $SOURCE_OUT/viz_meta.json"
echo "  $SOURCE_OUT/viz_data_typed.json"
