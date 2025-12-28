#!/usr/bin/env bash
set -euo pipefail

# REPL discovery primitives demo (no network/LLM required).
#
# Shows:
# - `describe` (rich entity inspector)
# - `open chunk ...` (evidence text navigation, via scenario-generated DocChunks)
# - `diff ctx ...` (context/world fact diffs)
# - `q --explain ...` (type elaboration + plan)
# - `neigh ... --out ...` (REPL-driven viz export; HTML explorer with plane filters)
#
# Run from repo root:
#   ./scripts/repl_discovery_primitives_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/repl_discovery_primitives_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph REPL discovery primitives demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"
echo ""

echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
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
echo "-- Non-interactive REPL session"
"$AXIOGRAPH" repl --continue-on-error \
  --cmd "import_axi examples/Family.axi" \
  --cmd "ctx list" \
  --cmd "ctx use CensusData" \
  --cmd "q --explain select ?f ?child ?parent where ?f = Parent(child=?child, parent=?parent) limit 10" \
  --cmd "diff ctx CensusData FamilyTree rel Parent limit 10" \
  --cmd "describe Alice --out 8 --in 8 --attrs 24" \
  --cmd "neigh Alice --plane both --hops 3 --format html --out $OUT_DIR/family_alice.html --max_nodes 260" \
  --cmd "gen proto_api 2 3 1" \
  --cmd "describe acme.svc0.v1.Service0 --out 10 --in 10 --attrs 24" \
  --cmd "open chunk doc_proto_api_overview_0 --max_chars 800" \
  --cmd "open chunk doc_proto_service_0 --max_chars 800" \
  --cmd "open chunk doc_proto_rpc_0_1 --max_chars 800" \
  --cmd "neigh acme.svc0.v1.Service0 --plane data --hops 2 --format html --out $OUT_DIR/proto_api_service0.html --max_nodes 340" \
  --cmd "neigh doc_proto_api_0 --plane data --hops 2 --format html --out $OUT_DIR/proto_api_doc0.html --max_nodes 380"

echo ""
echo "Done."
echo "Open:"
echo "  $OUT_DIR/family_alice.html"
echo "  $OUT_DIR/proto_api_service0.html"
echo "  $OUT_DIR/proto_api_doc0.html"
