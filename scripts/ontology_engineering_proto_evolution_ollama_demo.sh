#!/bin/bash
set -euo pipefail

# Ontology engineering demo (Proto, over time): multi-service proto APIs → proposals → LLM augmentation → drafted `.axi` → promotion gate → PathDB + viz.
#
# This is intentionally a "story" demo:
#   - We start from the checked-in multi-service example `examples/proto/large_api/`
#     (payments/users/catalog + custom annotations + doc-comment chunks).
#   - We evolve the proto surface over several ticks (add orders, then fulfillment).
#   - Each tick runs the same ontology-engineering loop:
#       1) `ingest proto ingest` (Buf descriptor-set JSON → proposals.json + chunks.json)
#       2) `discover augment-proposals` (LLM suggests schema hints and additional grounded relations)
#       3) `discover draft-module` (LLM suggests extra subtyping + constraints)
#       4) promotion gate: validate + typecheck certificate + Lean check
#       5) build `.axpd` snapshot + reversible snapshot export `.axi` + viz pages
#
# Run:
#   ./scripts/ontology_engineering_proto_evolution_ollama_demo.sh
#
# Requirements:
# - `buf` installed (used by `axiograph ingest proto ingest`)
# - If `LLM_BACKEND=ollama`: `ollama` installed + running (`ollama serve`), and the model available.
# - If `LLM_BACKEND=openai`: `OPENAI_API_KEY` set.
# - If `LLM_BACKEND=anthropic`: `ANTHROPIC_API_KEY` set.
# - Lean/lake optional (promotion gate will run if available)
#
# Optional:
# - `AXIOGRAPH_LLM_TIMEOUT_SECS=600` to allow longer-running model calls (0 disables).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/ontology_engineering_proto_evolution_ollama_demo"
PROTO_ROOT="$OUT_DIR/proto_workspace"

LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-${MODEL:-nemotron-3-nano}}"
export OLLAMA_HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"

echo "== Axiograph ontology engineering (Proto evolution) demo =="
echo "root:  $ROOT_DIR"
echo "out:   $OUT_DIR"
echo "llm_backend: $LLM_BACKEND"
echo "llm_model:   $LLM_MODEL"
if [ "$LLM_BACKEND" = "ollama" ]; then
  echo "ollama_host: $OLLAMA_HOST"
fi

if ! command -v buf >/dev/null 2>&1; then
  echo "error: buf not found. Install it from https://buf.build and retry." >&2
  exit 1
fi

DISCOVER_LLM_FLAGS=()
if [ "$LLM_BACKEND" = "ollama" ]; then
  if ! command -v ollama >/dev/null 2>&1; then
    echo "error: ollama not found. Install it from https://ollama.com and retry." >&2
    exit 1
  fi
  if ! ollama list >/dev/null 2>&1; then
    echo "error: Ollama server not reachable. Start it with: ollama serve" >&2
    exit 1
  fi
  if ! ollama show "$LLM_MODEL" >/dev/null 2>&1; then
    echo "-- pulling model: $LLM_MODEL"
    ollama pull "$LLM_MODEL"
  fi
  DISCOVER_LLM_FLAGS+=(--llm-ollama --llm-ollama-host "$OLLAMA_HOST" --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "openai" ]; then
  : "${OPENAI_API_KEY:?error: set OPENAI_API_KEY when LLM_BACKEND=openai}"
  DISCOVER_LLM_FLAGS+=(--llm-openai --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "anthropic" ]; then
  : "${ANTHROPIC_API_KEY:?error: set ANTHROPIC_API_KEY when LLM_BACKEND=anthropic}"
  DISCOVER_LLM_FLAGS+=(--llm-anthropic --llm-model "$LLM_MODEL")
else
  echo "warn: unknown LLM_BACKEND=$LLM_BACKEND; running without LLM"
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo ""
echo "-- seed proto workspace from examples/proto/large_api"
cp -R "$ROOT_DIR/examples/proto/large_api" "$PROTO_ROOT"

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

run_tick() {
  local tick="$1"
  local schema="$2"
  local instance="$3"
  local focus_service="$4"
  local focus_doc="$5"

  local tick_dir="$OUT_DIR/tick${tick}"
  local accepted_dir="$tick_dir/accepted"
  mkdir -p "$tick_dir" "$accepted_dir"

  local proposals="$tick_dir/proposals.json"
  local chunks="$tick_dir/chunks.json"
  local descriptor="$tick_dir/descriptor.json"
  local aug="$tick_dir/proposals.aug.json"
  local aug_trace="$tick_dir/proposals.aug.trace.json"
  local candidate_axi="$tick_dir/ProtoApi.tick${tick}.llm_draft.axi"
  local typecheck_cert="$accepted_dir/ProtoApi.tick${tick}.typecheck_cert.json"
  local accepted_axi="$accepted_dir/ProtoApi.tick${tick}.accepted.axi"
  local accepted_axpd="$accepted_dir/ProtoApi.tick${tick}.accepted.axpd"
  local accepted_axpd_with_chunks="$accepted_dir/ProtoApi.tick${tick}.accepted.with_chunks.axpd"
  local snapshot_export_axi="$accepted_dir/ProtoApi.tick${tick}.snapshot_export_v1.axi"

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "-- tick $tick: ingest proto workspace -> proposals/chunks"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  "$AXIOGRAPH" ingest proto ingest "$PROTO_ROOT" \
    --out "$proposals" \
    --chunks "$chunks" \
    --descriptor-out "$descriptor" \
    --schema-hint proto_api

  echo ""
  echo "-- tick $tick: LLM augment-proposals (schema hints + grounded additions)"
  "$AXIOGRAPH" discover augment-proposals \
    "$proposals" \
    --out "$aug" \
    --trace "$aug_trace" \
    --chunks "$chunks" \
    "${DISCOVER_LLM_FLAGS[@]}" \
    --llm-add-proposals \
    --overwrite-schema-hints

  echo ""
  echo "-- tick $tick: structural discovery (draft candidate module + LLM structure suggestions)"
  "$AXIOGRAPH" discover draft-module \
    "$aug" \
    --out "$candidate_axi" \
    --module "ProtoApi_Tick${tick}_LLM" \
    --schema "$schema" \
    --instance "$instance" \
    --infer-constraints \
    "${DISCOVER_LLM_FLAGS[@]}"

  echo ""
  echo "-- tick $tick: validate drafted module parses + typechecks (AST-level)"
  "$AXIOGRAPH" check validate "$candidate_axi"

  echo ""
  echo "-- tick $tick: promotion gate (certificate) emit typecheck certificate (axi_well_typed_v1)"
  "$AXIOGRAPH" cert typecheck "$candidate_axi" --out "$typecheck_cert"

  echo ""
  echo "-- tick $tick: promotion gate (Lean) verify typecheck certificate (optional)"
  (cd "$ROOT_DIR" && make verify-lean-cert AXI="$candidate_axi" CERT="$typecheck_cert")

  echo ""
  echo "-- tick $tick: promote (copy candidate -> accepted plane)"
  cp "$candidate_axi" "$accepted_axi"

  echo ""
  echo "-- tick $tick: build PathDB snapshot (.axpd) from accepted canonical .axi"
  "$AXIOGRAPH" db pathdb import-axi "$accepted_axi" --out "$accepted_axpd"

  echo ""
  echo "-- tick $tick: import doc chunks into the snapshot (extension layer)"
  "$AXIOGRAPH" db pathdb import-chunks "$accepted_axpd" \
    --chunks "$chunks" \
    --out "$accepted_axpd_with_chunks"

  echo ""
  echo "-- tick $tick: export reversible PathDB snapshot (.axi) for certificate anchoring"
  "$AXIOGRAPH" db pathdb export-axi "$accepted_axpd" --out "$snapshot_export_axi"

  echo ""
  echo "-- tick $tick: viz (meta-plane)"
  "$AXIOGRAPH" tools viz "$accepted_axpd" \
    --out "$accepted_dir/proto_api_meta.html" \
    --format html \
    --plane meta \
    --focus-name "$schema" \
    --hops 3 \
    --max-nodes 520

  echo ""
  echo "-- tick $tick: viz (data-plane, focus service + docs)"
  "$AXIOGRAPH" tools viz "$accepted_axpd_with_chunks" \
    --out "$accepted_dir/proto_api_${focus_service}.html" \
    --format html \
    --plane data \
    --focus-name "$focus_service" \
    --hops 3 \
    --max-nodes 720

  echo ""
  echo "-- tick $tick: viz (doc plane, focus proto file)"
  "$AXIOGRAPH" tools viz "$accepted_axpd_with_chunks" \
    --out "$accepted_dir/proto_api_${focus_doc}.html" \
    --format html \
    --plane data \
    --focus-name "$focus_doc" \
    --hops 2 \
    --max-nodes 520

  echo ""
  echo "-- tick $tick: sample semantic queries (non-interactive REPL)"
  "$AXIOGRAPH" repl --quiet --continue-on-error \
    --cmd "load $accepted_axpd" \
    --cmd "q select ?svc where ?svc is ProtoService limit 50" \
    --cmd "q select ?rpc where name(\"$focus_service\") -proto_service_has_rpc-> ?rpc limit 50" \
    --cmd "q select ?rpc ?scope where ?rpc is ProtoRpc, ?rpc -proto_rpc_auth_scope-> ?scope limit 100" \
    --cmd "q select ?rpc where ?rpc is ProtoRpc, ?rpc -proto_rpc_idempotent-> false limit 100" \
    --cmd "q select ?rpc ?tag where ?rpc is ProtoRpc, ?rpc -proto_rpc_has_tag-> ?tag limit 100" \
    --cmd "q select ?f where ?f is ProtoField, ?f -proto_field_pii-> true limit 100" \
    --cmd "q select ?f ?ex where ?f is ProtoField, ?f -proto_field_example-> ?ex limit 100"

  echo ""
  echo "-- tick $tick: doc search + grounding (extension layer, non-certified)"
  "$AXIOGRAPH" repl --quiet --continue-on-error \
    --cmd "load $accepted_axpd_with_chunks" \
    --cmd "q select ?c where name(\"$focus_doc\") -document_has_chunk-> ?c limit 30" \
    --cmd "q select ?c where name(\"$focus_service\") -has_doc_chunk-> ?c limit 30" \
    --cmd "q select ?c where ?c is DocChunk, fts(?c, \"text\", \"idempotent\") limit 10" \
    --cmd "q select ?c where ?c is DocChunk, fts(?c, \"text\", \"capture payment\") limit 10" \
    --cmd "q select ?c where ?c is DocChunk, fts(?c, \"search_text\", \"$focus_service\") limit 10"

  echo ""
  echo "-- tick $tick: optional certified query anchored to snapshot export"
  local query_cert="$accepted_dir/proto_api_query_cert_v1.json"
  "$AXIOGRAPH" cert query "$snapshot_export_axi" \
    'select ?rpc where name("acme_payments_v1_PaymentService") -proto_service_has_rpc-> ?rpc limit 10' \
    --out "$query_cert"
  (cd "$ROOT_DIR" && make verify-lean-cert AXI="$snapshot_export_axi" CERT="$query_cert")

  echo ""
  echo "-- tick $tick outputs:"
  echo "  accepted axi:         $accepted_axi"
  echo "  accepted axpd:        $accepted_axpd"
  echo "  accepted axpd+chunks: $accepted_axpd_with_chunks"
  echo "  snapshot export axi:  $snapshot_export_axi"
  echo "  typecheck cert:       $typecheck_cert"
  echo "  meta viz:             $accepted_dir/proto_api_meta.html"
  echo "  service viz:          $accepted_dir/proto_api_${focus_service}.html"
  echo "  doc viz:              $accepted_dir/proto_api_${focus_doc}.html"
}

# ---------------------------------------------------------------------------
# Tick 0: baseline (payments/users/catalog from examples/proto/large_api)
# ---------------------------------------------------------------------------

run_tick 0 ProtoApiTick0 ProtoApiTick0Instance acme_payments_v1_PaymentService acme_payments_v1_payments_proto

# ---------------------------------------------------------------------------
# Tick 1: add Orders API + link payments <-> orders
# ---------------------------------------------------------------------------

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "-- tick 1 edit: add Orders API + evolve Payments API (order_id)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

mkdir -p "$PROTO_ROOT/acme/orders/v1"

cat > "$PROTO_ROOT/acme/orders/v1/orders.proto" <<'PROTO'
syntax = "proto3";

package acme.orders.v1;

import "acme/annotations/v1/annotations.proto";
import "acme/catalog/v1/catalog.proto";
import "acme/payments/v1/payments.proto";

// Orders API.
//
// Typical interaction flow (documented):
// - SearchProducts (catalog.v1) to find candidates
// - CreateOrder (orders.v1) to reserve intent
// - CreatePayment + CapturePayment (payments.v1) to finalize purchase
// - GetOrder to check status

message OrderItem {
  string product_id = 1 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "prd_123"];
  int32 quantity = 2 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "2"];
  // (Demo-only) embed product details to make cross-package links explicit.
  acme.catalog.v1.Product product = 3;
}

enum OrderStatus {
  ORDER_STATUS_UNSPECIFIED = 0;
  ORDER_STATUS_CREATED = 1;
  ORDER_STATUS_PAID = 2;
  ORDER_STATUS_CANCELLED = 3;
}

message Order {
  string order_id = 1 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "ord_123"];
  string user_id = 2 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "usr_123"];
  repeated OrderItem items = 3;
  OrderStatus status = 4;
  // Optional linkage: if the order was paid via our payments API.
  string payment_id = 5 [(acme.annotations.v1.field).example = "pay_123"];
  // (Demo-only) reuse the payments Money type to make cross-package typing explicit.
  acme.payments.v1.Money estimated_total = 6;
}

message CreateOrderRequest {
  string user_id = 1 [(acme.annotations.v1.field).required = true];
  repeated OrderItem items = 2;
}

message CreateOrderResponse {
  Order order = 1;
}

message GetOrderRequest {
  string order_id = 1 [(acme.annotations.v1.field).required = true];
}

message GetOrderResponse {
  Order order = 1;
}

service OrderService {
  // Create an order intent.
  rpc CreateOrder(CreateOrderRequest) returns (CreateOrderResponse) {
    option (acme.annotations.v1.http) = { post: "/v1/orders" body: "*" };
    option (acme.annotations.v1.semantics) = {
      idempotent: true
      auth_scope: "orders.write"
      stability: "beta"
      tags: "orders"
      tags: "create"
    };
  }

  // Get an order by id.
  rpc GetOrder(GetOrderRequest) returns (GetOrderResponse) {
    option (acme.annotations.v1.http) = { get: "/v1/orders/{order_id}" };
    option (acme.annotations.v1.semantics) = {
      idempotent: true
      auth_scope: "orders.read"
      stability: "beta"
      tags: "orders"
      tags: "get"
    };
  }
}
PROTO

# Add `order_id` as a linkage point on payments.
perl -0777 -i -pe 's/message Payment \\{\n  string payment_id = 1/\\0\\n  string order_id = 5 [(acme.annotations.v1.field).example = \"ord_123\"];\\n/;' \
  "$PROTO_ROOT/acme/payments/v1/payments.proto"

run_tick 1 ProtoApiTick1 ProtoApiTick1Instance acme_orders_v1_OrderService acme_orders_v1_orders_proto

echo ""
echo "-- tick0 vs tick1: diff accepted canonical modules"
diff -u "$OUT_DIR/tick0/accepted/ProtoApi.tick0.accepted.axi" "$OUT_DIR/tick1/accepted/ProtoApi.tick1.accepted.axi" || true

# ---------------------------------------------------------------------------
# Tick 2: add Fulfillment API (shipments) and an ordering-to-fulfillment workflow hint.
# ---------------------------------------------------------------------------

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "-- tick 2 edit: add Fulfillment API (shipments) + doc workflow hints"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

mkdir -p "$PROTO_ROOT/acme/fulfillment/v1"

cat > "$PROTO_ROOT/acme/fulfillment/v1/fulfillment.proto" <<'PROTO'
syntax = "proto3";

package acme.fulfillment.v1;

import "acme/annotations/v1/annotations.proto";
import "acme/orders/v1/orders.proto";

// Fulfillment API.
//
// Typical interaction flow (documented):
// - CreateOrder (orders.v1)
// - CreatePayment + CapturePayment (payments.v1)
// - CreateShipment (fulfillment.v1)
// - GetShipment to track progress

enum ShipmentStatus {
  SHIPMENT_STATUS_UNSPECIFIED = 0;
  SHIPMENT_STATUS_CREATED = 1;
  SHIPMENT_STATUS_IN_TRANSIT = 2;
  SHIPMENT_STATUS_DELIVERED = 3;
}

message Shipment {
  string shipment_id = 1 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "shp_123"];
  string order_id = 2 [(acme.annotations.v1.field).required = true, (acme.annotations.v1.field).example = "ord_123"];
  ShipmentStatus status = 3;
  // (Demo-only) embed the order to make cross-package links explicit.
  acme.orders.v1.Order order = 4;
}

message CreateShipmentRequest {
  string order_id = 1 [(acme.annotations.v1.field).required = true];
}

message CreateShipmentResponse {
  Shipment shipment = 1;
}

message GetShipmentRequest {
  string shipment_id = 1 [(acme.annotations.v1.field).required = true];
}

message GetShipmentResponse {
  Shipment shipment = 1;
}

service FulfillmentService {
  // Create a shipment for an order.
  rpc CreateShipment(CreateShipmentRequest) returns (CreateShipmentResponse) {
    option (acme.annotations.v1.http) = { post: "/v1/shipments" body: "*" };
    option (acme.annotations.v1.semantics) = {
      idempotent: false
      auth_scope: "fulfillment.write"
      stability: "beta"
      tags: "fulfillment"
      tags: "create"
    };
  }

  // Get shipment by id.
  rpc GetShipment(GetShipmentRequest) returns (GetShipmentResponse) {
    option (acme.annotations.v1.http) = { get: "/v1/shipments/{shipment_id}" };
    option (acme.annotations.v1.semantics) = {
      idempotent: true
      auth_scope: "fulfillment.read"
      stability: "beta"
      tags: "fulfillment"
      tags: "get"
    };
  }
}
PROTO

run_tick 2 ProtoApiTick2 ProtoApiTick2Instance acme_fulfillment_v1_FulfillmentService acme_fulfillment_v1_fulfillment_proto

echo ""
echo "-- tick1 vs tick2: diff accepted canonical modules"
diff -u "$OUT_DIR/tick1/accepted/ProtoApi.tick1.accepted.axi" "$OUT_DIR/tick2/accepted/ProtoApi.tick2.accepted.axi" || true

echo ""
echo "Done."
echo "Open the service viz pages for each tick:"
echo "  $OUT_DIR/tick0/accepted/proto_api_acme_payments_v1_PaymentService.html"
echo "  $OUT_DIR/tick1/accepted/proto_api_acme_orders_v1_OrderService.html"
echo "  $OUT_DIR/tick2/accepted/proto_api_acme_fulfillment_v1_FulfillmentService.html"
echo ""
echo "Open the doc viz pages for each tick:"
echo "  $OUT_DIR/tick0/accepted/proto_api_acme_payments_v1_payments_proto.html"
echo "  $OUT_DIR/tick1/accepted/proto_api_acme_orders_v1_orders_proto.html"
echo "  $OUT_DIR/tick2/accepted/proto_api_acme_fulfillment_v1_fulfillment_proto.html"
