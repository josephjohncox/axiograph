#!/usr/bin/env bash
set -euo pipefail

# RDF named graphs → contexts demo (offline).
#
# This script demonstrates how Semantic Web "named graphs" map cleanly onto
# Axiograph's first-class *contexts/worlds*:
#
# - TriG named graphs become `Context` entities.
# - Each statement is scoped to a context (so “missing is unknown”, not false).
# - `discover draft-module` preserves relation scoping (`@context Context` + `ctx=...`).
# - You can then query the *same* predicate in different contexts using `ctx use ...`.
#
# Run from repo root:
#   ./scripts/rdf_named_graph_context_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/rdf_named_graph_context_demo"
FIXTURE_DIR="$ROOT_DIR/examples/rdfowl/named_graphs_minimal"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== RDF named graphs → contexts demo =="
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
echo "-- A) Ingest TriG fixture → proposals.json"
"$AXIOGRAPH" ingest dir "$FIXTURE_DIR" --out-dir "$OUT_DIR" --domain rdfowl

PROPOSALS="$OUT_DIR/proposals.json"
DRAFT_AXI="$OUT_DIR/Discovered_RdfNamedGraphs.axi"
AXPD="$OUT_DIR/Discovered_RdfNamedGraphs.axpd"

echo ""
echo "-- B) Draft a candidate axi_v1 module (schema discovery + context preservation)"
"$AXIOGRAPH" discover draft-module "$PROPOSALS" --out "$DRAFT_AXI" \
  --module "Discovered_RdfNamedGraphs" \
  --schema "Discovered_RdfNamedGraphs" \
  --infer-constraints

echo ""
echo "-- C) Import drafted module into PathDB (.axpd)"
"$AXIOGRAPH" db pathdb import-axi "$DRAFT_AXI" --out "$AXPD"

echo ""
echo "-- D) Query per-context (REPL non-interactive)"
"$AXIOGRAPH" repl --axpd "$AXPD" --quiet --continue-on-error \
  --cmd 'ctx list' \
  --cmd 'ctx use g_plan' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  --cmd 'ctx use g_observed' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  --cmd 'ctx clear' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  >"$OUT_DIR/repl_output.txt"

echo ""
echo "-- E) Viz (typed overlay)"
"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_a.html" \
  --format html \
  --plane both \
  --typed-overlay \
  --focus-name a \
  --hops 2 \
  --max-nodes 320

echo ""
echo "Done."
echo "Outputs:"
echo "  $PROPOSALS"
echo "  $DRAFT_AXI"
echo "  $AXPD"
echo "  $OUT_DIR/repl_output.txt"
echo "  $OUT_DIR/viz_a.html"

