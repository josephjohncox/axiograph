#!/usr/bin/env bash
set -euo pipefail

# Fetch publicly available RDF/OWL/SHACL datasets for realistic ingestion/perf experiments.
#
# This script writes into:
#   build/datasets/
#
# Notes:
# - These datasets are NOT part of the trusted kernel; they are used to exercise
#   boundary adapters (RDF/OWL ingestion, future SHACL validation, etc).
# - Network access is required.
#
# Run from repo root:
#   ./scripts/fetch_public_rdfowl_datasets.sh

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/datasets"
mkdir -p "$OUT_DIR"

echo "== Fetch public RDF/OWL/SHACL datasets =="
echo "out: $OUT_DIR"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing required command: $1" >&2
    exit 2
  fi
}

need_cmd git

if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  echo "error: need curl or wget" >&2
  exit 2
fi

echo ""
echo "-- A) W3C Data Shapes WG repo (SHACL test suite + examples)"
W3C_DIR="$OUT_DIR/w3c_data_shapes"
if [ -d "$W3C_DIR/.git" ]; then
  echo "updating: $W3C_DIR"
  git -C "$W3C_DIR" pull --ff-only
else
  echo "cloning: https://github.com/w3c/data-shapes -> $W3C_DIR"
  git clone --depth 1 https://github.com/w3c/data-shapes "$W3C_DIR"
fi

echo ""
echo "-- B) LUBM (Lehigh University Benchmark) ontology + generator"
# LUBM homepage: http://swat.cse.lehigh.edu/projects/lubm/
LUBM_DIR="$OUT_DIR/lubm"
mkdir -p "$LUBM_DIR"

LUBM_OWL="$LUBM_DIR/univ-bench.owl"
if [ ! -f "$LUBM_OWL" ]; then
  echo "downloading: univ-bench.owl"
  $FETCH "http://swat.cse.lehigh.edu/onto/univ-bench.owl" > "$LUBM_OWL"
else
  echo "exists: $LUBM_OWL"
fi

LUBM_GEN_ZIP="$LUBM_DIR/uba1.7.zip"
if [ ! -f "$LUBM_GEN_ZIP" ]; then
  echo "downloading: uba1.7.zip (data generator)"
  $FETCH "http://swat.cse.lehigh.edu/projects/lubm/uba1.7.zip" > "$LUBM_GEN_ZIP"
else
  echo "exists: $LUBM_GEN_ZIP"
fi

echo ""
echo "Done."
echo "Datasets:"
echo "  $W3C_DIR"
echo "  $LUBM_DIR"

