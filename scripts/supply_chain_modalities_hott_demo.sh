#!/usr/bin/env bash
set -euo pipefail

# Modalities + dependent types demo (supply chain):
# - imports a canonical `.axi` module that uses contexts/worlds + 2-cells + proof terms
# - shows typed query elaboration + context scoping
# - generates HTML viz with typed overlay and confidence controls
#
# Run from repo root:
#   ./scripts/supply_chain_modalities_hott_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/supply_chain_modalities_hott_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== SupplyChain modalities + dependent types demo =="
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

AXPD="$OUT_DIR/supply_chain_modalities_hott.axpd"
EXPORT_AXI="$OUT_DIR/supply_chain_modalities_hott_export_v1.axi"

echo ""
echo "-- A) Import canonical .axi, explore, and save a snapshot"
"$AXIOGRAPH" repl --quiet --continue-on-error \
  --cmd 'import_axi examples/manufacturing/SupplyChainModalitiesHoTT.axi' \
  --cmd 'schema SupplyChainModal' \
  --cmd 'constraints SupplyChainModal' \
  --cmd 'validate_axi' \
  --cmd 'stats' \
  --cmd 'ctx list' \
  --cmd 'ctx use Plan' \
  --cmd 'q --elaborate select ?flow ?time where ?flow = Flow(from=RawMetal_A, to=RawMaterial_WH, material=Steel_Billet, time=?time) limit 10' \
  --cmd 'ctx use Observed' \
  --cmd 'q --elaborate select ?flow ?time where ?flow = Flow(from=RawMetal_A, to=RawMaterial_WH, material=Steel_Billet, time=?time) limit 10' \
  --cmd 'ctx use Plan' \
  --cmd 'q --elaborate select ?eq ?route2 ?proof where ?eq = RouteEquivalence(from=RawMetal_A, to=Machining_Plant, route1=Route_Via_SupplierA, route2=?route2, proof=?proof) limit 10' \
  --cmd 'ctx use Observed' \
  --cmd 'q --elaborate select ?eq where ?eq = RouteEquivalence(from=RawMetal_A, to=Machining_Plant, route1=Route_Via_SupplierA) limit 10' \
  --cmd 'q --elaborate select ?obl where name("Plan") -Obligatory-> ?obl limit 10' \
  --cmd 'ctx use Policy' \
  --cmd 'q --elaborate select ?ev ?p where ?f = EvidenceSupports(ev=?ev, prop=?p) limit 10' \
  --cmd 'ctx use Observed' \
  --cmd 'q --elaborate select ?ev ?p where ?f = EvidenceSupports(ev=?ev, prop=?p) limit 10' \
  --cmd 'q --elaborate select ?p2 where ?j = JustificationEquiv(path1=Justification_Policy, path2=?p2) limit 10' \
  --cmd 'add_entity DocChunk doc_policy_0 text="Policy: if supplier delay risk is high, switch to backup supplier RawMetal_B." document_id=policy0 span_id=s0' \
  --cmd 'add_entity DocChunk doc_erp_0 text="ERP: Supplier RawMetal_A lead time slipped to 14 days for Steel_Billet." document_id=erp0 span_id=s0' \
  --cmd 'add_edge EvidenceChunk PolicyDoc_0 doc_policy_0' \
  --cmd 'add_edge EvidenceChunk ERPEvent_0 doc_erp_0' \
  --cmd 'add_edge EvidenceSuggestsObligation ERPEvent_0 UseBackupSupplier_B confidence 0.45' \
  --cmd 'add_edge EvidenceSuggestsObligation PolicyDoc_0 UseBackupSupplier_B confidence 0.95' \
  --cmd 'q select ?obl where name("ERPEvent_0") -EvidenceSuggestsObligation-> ?obl limit 10' \
  --cmd 'q select ?obl where name("ERPEvent_0") -EvidenceSuggestsObligation-> ?obl min_confidence 0.80 limit 10' \
  --cmd 'q select ?c where ?c is DocChunk, fts(?c, text, backup) limit 10' \
  --cmd 'add_entity Homotopy demo_homotopy_0' \
  --cmd 'export_axi '"$EXPORT_AXI" \
  --cmd 'save '"$AXPD" \
  >"$OUT_DIR/repl_output.txt"

echo ""
echo "-- B) Viz output (HTML explorer + typed overlay)"
"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_rawmetal_a.html" \
  --format html \
  --plane both \
  --focus-name "RawMetal_A" \
  --hops 2 \
  --max-nodes 420 \
  --typed-overlay

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_erp_event_0.html" \
  --format html \
  --plane both \
  --focus-name "ERPEvent_0" \
  --hops 2 \
  --max-nodes 420 \
  --typed-overlay

echo ""
echo "Done."
echo "Outputs:"
echo "  $AXPD"
echo "  $EXPORT_AXI"
echo "  $OUT_DIR/repl_output.txt"
echo "  $OUT_DIR/viz_rawmetal_a.html"
echo "  $OUT_DIR/viz_erp_event_0.html"
