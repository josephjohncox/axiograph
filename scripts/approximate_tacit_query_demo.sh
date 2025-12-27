#!/usr/bin/env bash
set -euo pipefail

# Approximate + tacit knowledge demo:
# - generate a small `proto_api` synthetic PathDB snapshot (includes low-confidence edges)
# - run AxQL queries that demonstrate:
#     - confidence-threshold filtering (`min_confidence`)
#     - approximate attribute querying (`fuzzy`, `fts`)
# - render a viz HTML you can explore (confidence slider in the UI)
#
# Run from repo root:
#   ./scripts/approximate_tacit_query_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/approximate_tacit_query_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== Approximate + tacit knowledge demo =="
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
echo "-- A) Generate a small proto_api snapshot (.axpd)"
AXPD="$OUT_DIR/proto_api.axpd"
"$AXIOGRAPH" tools perf scenario \
  --scenario proto_api \
  --scale 2 \
  --index-depth 3 \
  --seed 1 \
  --path-queries 0 \
  --axql-queries 0 \
  --out-axpd "$AXPD" \
  --out-json "$OUT_DIR/perf.json"

echo ""
echo "-- B) Run queries (approximate + confidence-threshold filtering)"
"$AXIOGRAPH" repl --axpd "$AXPD" --quiet --continue-on-error \
  --cmd 'q select ?svc where ?svc is ProtoService limit 10' \
  --cmd 'q select ?rpc where name("acme.svc0.v1.Service0") -proto_service_has_rpc-> ?rpc limit 10' \
  --cmd 'q select ?rpc where name("doc_proto_api_0") -mentions_rpc-> ?rpc limit 10' \
  --cmd 'q select ?rpc where name("doc_proto_api_0") -mentions_rpc-> ?rpc min_confidence 0.90 limit 10' \
  --cmd 'q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -observed_next-> ?next limit 10' \
  --cmd 'q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -observed_next-> ?next min_confidence 0.80 limit 10' \
  --cmd 'q select ?svc where fuzzy(?svc, "name", "acme.svc0.v1.ServiceO", 1) limit 5' \
  --cmd 'q select ?c where ?c is DocChunk, fts(?c, "text", "GetWidget") limit 5' \
  >"$OUT_DIR/repl_output.txt"

echo ""
echo "-- C) Viz output (HTML explorer)"
"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_service0.html" \
  --format html \
  --plane both \
  --focus-name "acme.svc0.v1.Service0" \
  --hops 2 \
  --max-nodes 520 \
  --typed-overlay

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_doc0.html" \
  --format html \
  --plane data \
  --focus-name "doc_proto_api_0" \
  --hops 2 \
  --max-nodes 520

echo ""
echo "Done."
echo "Outputs:"
echo "  $AXPD"
echo "  $OUT_DIR/repl_output.txt"
echo "  $OUT_DIR/viz_service0.html"
echo "  $OUT_DIR/viz_doc0.html"

