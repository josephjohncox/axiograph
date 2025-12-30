#!/bin/bash
# ============================================================================
# Axiograph End-to-End Demo (Rust + Lean)
# ============================================================================
#
# This script demonstrates the current v6 workflow:
# - Canonical `.axi` modules (schema/theory/instance) are the human-facing source plane.
# - Rust imports modules into PathDB and runs queries (fast, untrusted).
# - Rust can emit certificates; Lean checks them (trusted semantics/checker).
#
# Run from repo root:
#   make demo
#
# Or run directly:
#   ./examples/run_demo.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RUN_DIR="$PROJECT_ROOT/build/demo_run"
mkdir -p "$RUN_DIR/build"

echo "== Axiograph end-to-end demo (Rust + Lean) =="
echo "root: $PROJECT_ROOT"
echo "run:  $RUN_DIR"

echo ""
echo "-- build binaries (via Makefile)"
cd "$PROJECT_ROOT"
make binaries

AXIOGRAPH="$PROJECT_ROOT/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$PROJECT_ROOT/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $PROJECT_ROOT/bin/axiograph or $PROJECT_ROOT/bin/axiograph-cli"
  exit 2
fi

echo ""
echo "-- validate canonical `.axi` input (Rust parser)"
"$AXIOGRAPH" validate "$PROJECT_ROOT/examples/economics/EconomicFlows.axi" >/dev/null
echo "ok: examples/economics/EconomicFlows.axi"

echo ""
echo "-- REPL demo: canonical module import (meta-plane + FactIndex + keys)"
cd "$RUN_DIR"
"$AXIOGRAPH" repl --script "$PROJECT_ROOT/examples/repl_scripts/supply_chain_hott_axi_demo.repl" --quiet

echo ""
echo "-- REPL demo: schema discovery module import (extensional constraints)"
"$AXIOGRAPH" repl --script "$PROJECT_ROOT/examples/repl_scripts/sql_schema_discovery_axi_demo.repl" --quiet

echo ""
echo "-- emit a query certificate from the snapshot export"
"$AXIOGRAPH" query-cert "$RUN_DIR/build/supply_chain_hott_export_v1.axi" \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out "$RUN_DIR/build/supply_chain_hott_query_cert_v1.json" >/dev/null
echo "wrote: build/supply_chain_hott_query_cert_v1.json"

echo ""
echo "-- verify in Lean (if lake is installed)"
if command -v lake >/dev/null 2>&1; then
  cd "$PROJECT_ROOT/lean"
  lake env lean --run Axiograph/VerifyMain.lean \
    "$RUN_DIR/build/supply_chain_hott_export_v1.axi" \
    "$RUN_DIR/build/supply_chain_hott_query_cert_v1.json" >/dev/null
  echo "ok: Lean verified query certificate"
else
  echo "skip: lake not found (install via elan)"
fi

echo ""
echo "Done."
echo "Next:"
echo "  - Docs: docs/README.md (and docs/explanation/BOOK.md)"
echo "  - More REPL scripts: examples/repl_scripts/"
