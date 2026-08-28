#!/bin/bash
# ============================================================================
# Axiograph Breadth-First Repo Tour (Rust + Lean)
# ============================================================================
#
# This script demonstrates a breadth-first tour across current typed surfaces:
# - Canonical `.axi` modules (schema/theory/instance) are the human-facing source plane.
# - Rust elaborates modules into typed runtime reports, CQ coverage, and
#   verifier-readable certificates.
# - Lean checks the strongest supported certificate fragment.
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

echo "== Axiograph breadth-first repo tour (Rust + Lean) =="
echo "root: $PROJECT_ROOT"
echo "run:  $RUN_DIR"

echo ""
echo "-- build binaries (via Makefile)"
cd "$PROJECT_ROOT"
make binaries

AXIOGRAPH="$PROJECT_ROOT/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
	echo "error: expected executable at $PROJECT_ROOT/bin/axiograph"
	exit 2
fi

echo ""
echo '-- validate canonical .axi input (Rust parser/type gate)'
"$AXIOGRAPH" check validate "$PROJECT_ROOT/examples/economics/EconomicFlows.axi" >/dev/null
echo "ok: examples/economics/EconomicFlows.axi"

echo ""
echo '-- inspect compiled theory graph from canonical .axi'
"$AXIOGRAPH" discover theory-graph "$PROJECT_ROOT/examples/ontology/OntologyRewrites.axi" \
	--out "$RUN_DIR/build/ontology_rewrites_theory_graph.json" >/dev/null
echo "wrote: build/ontology_rewrites_theory_graph.json"

echo ""
echo "-- REPL flow: canonical module import (meta-plane + FactIndex + keys)"
cd "$RUN_DIR"
"$AXIOGRAPH" repl --script "$PROJECT_ROOT/examples/repl_scripts/supply_chain_hott_axi_demo.repl" --quiet

echo ""
echo "-- run question-first competency questions from .cq"
"$AXIOGRAPH" discover competency-questions "$PROJECT_ROOT/examples/manufacturing/SupplyChainHoTT.axi" \
	--from-cq "$PROJECT_ROOT/examples/competency_questions/supply_chain.cq" \
	--no-schema \
	--out "$RUN_DIR/build/supply_chain_competency_questions.json" >/dev/null
echo "wrote: build/supply_chain_competency_questions.json"

echo ""
echo "-- emit a canonical .axi constraints certificate for Lean"
"$AXIOGRAPH" cert constraints "$PROJECT_ROOT/examples/ontology/OntologyRewrites.axi" \
	--out "$RUN_DIR/build/ontology_rewrites_constraints_cert.json" >/dev/null
echo "wrote: build/ontology_rewrites_constraints_cert.json"

echo ""
echo "-- behavior case: BDD/DDD wrapper over typed ontology surfaces"
"$AXIOGRAPH" discover behavior-case "$PROJECT_ROOT/examples/industrial/RegulatedProductionLine.axi" \
	--request "$PROJECT_ROOT/examples/behavior_cases/regulated_ship_release.json" \
	--cq-file "$PROJECT_ROOT/examples/behavior_cases/regulated_ship_release.cq" \
	--overlay "$PROJECT_ROOT/examples/behavior_cases/regulated_ship_release_overlay.json" \
	--out "$RUN_DIR/build/regulated_ship_release_behavior_case_report.json" >/dev/null
echo "wrote: build/regulated_ship_release_behavior_case_report.json"

echo ""
echo '-- regulated production line example crate over reviewable .axi'
cargo run --manifest-path "$PROJECT_ROOT/rust/Cargo.toml" \
	-p axiograph-example-industrial \
	--bin axiograph-industrial-example \
	-- run-regulated-seed \
	--axi "$PROJECT_ROOT/examples/industrial/RegulatedProductionLine.axi" \
	--cache-root "$RUN_DIR/build/industrial" \
	--run-id demo-industrial-001 \
	--created-at-unix-secs 1713810000 \
	--json >/dev/null
echo "wrote: build/industrial/runs/demo-industrial-001"

echo ""
echo "-- verify in Lean (if lake is installed)"
if command -v lake >/dev/null 2>&1; then
	cd "$PROJECT_ROOT/lean"
	lake env lean --run Axiograph/VerifyMain.lean \
		"$PROJECT_ROOT/examples/ontology/OntologyRewrites.axi" \
		"$RUN_DIR/build/ontology_rewrites_constraints_cert.json" >/dev/null
	echo "ok: Lean verified constraints certificate"
else
	echo "skip: lake not found (install via elan)"
fi

echo ""
echo "Done."
echo "Next:"
echo "  - Docs: docs/README.md (and docs/explanation/BOOK.md)"
echo "  - Example catalog: examples/README.md and examples/catalog.json"
echo "  - More REPL scripts: examples/repl_scripts/"
