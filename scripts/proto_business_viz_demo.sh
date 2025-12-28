#!/usr/bin/env bash
set -euo pipefail

# Proto visualizer demo (business fleet):
# - generates a large synthetic proto/gRPC API surface with *dozens* of services
# - includes doc chunks for grounding (service/rpc docs + a checkout narrative)
# - renders multiple HTML explorer views (orders/payments/docs)
#
# Run from repo root:
#   ./scripts/proto_business_viz_demo.sh
#
# Tunables:
#   SCALE=48 INDEX_DEPTH=3 SEED=1 ./scripts/proto_business_viz_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

SCALE="${SCALE:-48}"
INDEX_DEPTH="${INDEX_DEPTH:-3}"
SEED="${SEED:-1}"

OUT_DIR="$ROOT_DIR/build/proto_business_viz_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

AXPD="$OUT_DIR/proto_api_business_scale${SCALE}_depth${INDEX_DEPTH}_seed${SEED}.axpd"

echo "== Proto visualizer demo (business fleet) =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"
echo "scale: SCALE=$SCALE INDEX_DEPTH=$INDEX_DEPTH SEED=$SEED"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Generate PathDB snapshot (.axpd) (scenario=proto_api_business)"
"$AXIOGRAPH" tools perf scenario \
  --scenario proto_api_business \
  --scale "$SCALE" \
  --index-depth "$INDEX_DEPTH" \
  --seed "$SEED" \
  --path-queries 0 \
  --axql-queries 0 \
  --out-axpd "$AXPD" \
  --out-json "$OUT_DIR/perf.json"

echo ""
echo "-- B) Run a few representative queries (AxQL + doc grounding)"
"$AXIOGRAPH" repl --axpd "$AXPD" --quiet --continue-on-error \
  --cmd 'q select ?svc where ?svc is ProtoService limit 30' \
  --cmd 'q select ?rpc where name("acme.orders.v1.OrderService") -proto_service_has_rpc-> ?rpc limit 50' \
  --cmd 'q select ?dst where name("acme.orders.v1.OrderService") -calls-> ?dst limit 30' \
  --cmd 'q select ?ep where name("acme.payments.v1.PaymentService.AuthorizePayment") -proto_rpc_http_endpoint-> ?ep limit 10' \
  --cmd 'q select ?rpc where name("doc_orders_api") -mentions_rpc-> ?rpc limit 10' \
  --cmd 'q select ?rpc where name("doc_orders_api") -mentions_http_endpoint/proto_http_endpoint_of_rpc-> ?rpc max_hops 3 limit 10' \
  --cmd 'q select ?c where ?c is DocChunk, fts(?c, "text", "Checkout") limit 5' \
  >"$OUT_DIR/repl_output.txt"

echo ""
echo "-- C) Viz output (HTML explorer)"
"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_orders_service.html" \
  --format html \
  --plane both \
  --focus-name "acme.orders.v1.OrderService" \
  --hops 2 \
  --max-nodes 900 \
  --typed-overlay

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_payments_service.html" \
  --format html \
  --plane both \
  --focus-name "acme.payments.v1.PaymentService" \
  --hops 2 \
  --max-nodes 900 \
  --typed-overlay

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_doc_orders.html" \
  --format html \
  --plane data \
  --focus-name "doc_orders_api" \
  --hops 2 \
  --max-nodes 900

"$AXIOGRAPH" tools viz "$AXPD" \
  --out "$OUT_DIR/viz_checkout_chunk.html" \
  --format html \
  --plane data \
  --focus-name "doc_proto_business_checkout_0" \
  --hops 2 \
  --max-nodes 900

echo ""
echo "Done."
echo "Outputs:"
echo "  $AXPD"
echo "  $OUT_DIR/repl_output.txt"
echo "  $OUT_DIR/viz_orders_service.html"
echo "  $OUT_DIR/viz_payments_service.html"
echo "  $OUT_DIR/viz_doc_orders.html"
echo "  $OUT_DIR/viz_checkout_chunk.html"
