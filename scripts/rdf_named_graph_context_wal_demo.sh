#!/usr/bin/env bash
set -euo pipefail

# RDF named graphs → contexts → PathDB WAL (offline).
#
# This demo shows **cross-domain data preservation**:
#
# - `.axi` is the canonical meaning plane (accepted snapshot).
# - `proposals.json` is the evidence plane (untrusted, preserved in the PathDB WAL).
# - PathDB (`.axpd`) is derived: checkout is reproducible from:
#     (accepted snapshot id, wal ops)
#
# Run from repo root:
#   ./scripts/rdf_named_graph_context_wal_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/rdf_named_graph_context_wal_demo"
FIXTURE_DIR="$ROOT_DIR/examples/rdfowl/named_graphs_minimal"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== RDF named graphs → contexts → PathDB WAL demo =="
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

ACCEPTED_DIR="$OUT_DIR/accepted_plane"

echo ""
echo "-- A) Create a tiny accepted-plane base module (canonical meaning plane)"
BASE_AXI="$OUT_DIR/WalBase.axi"
cat >"$BASE_AXI" <<'EOF'
module WalBase

schema rdfowl:
  # Minimal schema so the evidence-plane RDF ingest can be queried with AxQL.
  # (The full RDF/OWL semantics live in the evidence plane; `.axi` stays the
  # canonical meaning plane.)
  object Context
  object RdfResource
  object Person

  # In this fixture, resources are typed as `Person` via `rdf:type`, but relation
  # endpoints are declared at the `RdfResource` supertype.
  subtype Person < RdfResource
  relation knows(from: RdfResource, to: RdfResource)

instance WalBaseInst of rdfowl:
  Context = {}
  RdfResource = {}
EOF

SNAPSHOT_ID="$("$AXIOGRAPH" db accept promote --dir "$ACCEPTED_DIR" "$BASE_AXI" --message "demo: base snapshot")"
echo "accepted snapshot: $SNAPSHOT_ID"

echo ""
echo "-- B) Ingest TriG fixture → proposals.json (evidence plane)"
"$AXIOGRAPH" ingest dir "$FIXTURE_DIR" --out-dir "$OUT_DIR" --domain rdfowl
PROPOSALS="$OUT_DIR/proposals.json"
CHUNKS="$OUT_DIR/chunks.json"

echo ""
echo "-- C) Commit (chunks + proposals) into the PathDB WAL"
WAL_SNAPSHOT_ID="$("$AXIOGRAPH" db accept pathdb-commit --dir "$ACCEPTED_DIR" --accepted-snapshot "$SNAPSHOT_ID" --chunks "$CHUNKS" --proposals "$PROPOSALS" --message "demo: preserve evidence overlay (chunks + proposals)")"
echo "pathdb wal snapshot: $WAL_SNAPSHOT_ID"

echo ""
echo "-- D) Checkout a derived PathDB snapshot (.axpd)"
AXPD="$OUT_DIR/evidence_plane.axpd"
"$AXIOGRAPH" db accept pathdb-build --dir "$ACCEPTED_DIR" --snapshot "$WAL_SNAPSHOT_ID" --out "$AXPD"

echo ""
echo "-- E) Query per-context (REPL non-interactive)"
"$AXIOGRAPH" repl --axpd "$AXPD" --quiet --continue-on-error \
  --cmd 'ctx list' \
  --cmd 'ctx use g_plan' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  --cmd 'ctx use g_observed' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  --cmd 'ctx clear' \
  --cmd 'q --elaborate select ?to where ?f = knows(from=a, to=?to) limit 10' \
  --cmd 'q select ?x where attr(?x, "iri", "http://example.org/a") limit 3' \
  --cmd 'q select ?r where ?r is ProposalRun limit 3' \
  >"$OUT_DIR/repl_output.txt"

echo ""
echo "-- F) Viz (typed overlay)"
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
echo "  $ACCEPTED_DIR"
echo "  $CHUNKS"
echo "  $PROPOSALS"
echo "  $AXPD"
echo "  $OUT_DIR/repl_output.txt"
echo "  $OUT_DIR/viz_a.html"
