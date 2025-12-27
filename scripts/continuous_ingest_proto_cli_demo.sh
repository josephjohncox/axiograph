#!/bin/bash
set -euo pipefail

# Continuous ingest demo (Proto): proto source changes → buf descriptor → proposals → drafted `.axi` → PathDB → viz.
#
# Run:
#   ./scripts/continuous_ingest_proto_cli_demo.sh
#
# This is intentionally CLI-only (no interactive REPL).
#
# Requirements:
# - `buf` must be installed (used via `axiograph ingest proto ingest`).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/continuous_ingest_proto_cli_demo"
PROTO_ROOT="$OUT_DIR/proto_module"
mkdir -p "$PROTO_ROOT/acme/toy/v1"

echo "== Axiograph continuous ingest (Proto) demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

cat > "$PROTO_ROOT/buf.yaml" <<'YAML'
version: v2
modules:
  - path: .
YAML

PROTO="$PROTO_ROOT/acme/toy/v1/toy.proto"

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
echo "-- tick 0: initial proto API"
cat > "$PROTO" <<'PROTO'
syntax = "proto3";

package acme_toy_v1;

message GetThingRequest {
  string thing_id = 1;
}

message GetThingResponse {
  string thing_id = 1;
  string display_name = 2;
}

service ThingService {
  rpc GetThing(GetThingRequest) returns (GetThingResponse);
}
PROTO

"$AXIOGRAPH" ingest proto ingest "$PROTO_ROOT" \
  --out "$OUT_DIR/proto_tick0.proposals.json" \
  --descriptor-out "$OUT_DIR/proto_tick0.descriptor.json" \
  --schema-hint proto_api

"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/proto_tick0.proposals.json" \
  --out "$OUT_DIR/ProtoTick0.proposals.axi" \
  --module ProtoTick0_Proposals \
  --schema ProtoTick0 \
  --instance ProtoTick0Instance \
  --infer-constraints
"$AXIOGRAPH" check validate "$OUT_DIR/ProtoTick0.proposals.axi"

"$AXIOGRAPH" db pathdb import-axi "$OUT_DIR/ProtoTick0.proposals.axi" \
  --out "$OUT_DIR/proto_tick0.axpd"

# Focus: the service node (name is sanitized; it's a single identifier in `.axi`).
"$AXIOGRAPH" tools viz "$OUT_DIR/proto_tick0.axpd" \
  --out "$OUT_DIR/proto_tick0_service.html" \
  --format html \
  --plane data \
  --focus-name acme_toy_v1_ThingService \
  --hops 2 \
  --max-nodes 260

echo ""
echo "-- tick 1: evolve proto API (add RPC + field)"
cat > "$PROTO" <<'PROTO'
syntax = "proto3";

package acme_toy_v1;

message GetThingRequest {
  string thing_id = 1;
}

message GetThingResponse {
  string thing_id = 1;
  string display_name = 2;
  string created_at = 3;
}

message ListThingsRequest {
  int32 page_size = 1;
  string page_token = 2;
}

message ListThingsResponse {
  repeated GetThingResponse things = 1;
  string next_page_token = 2;
}

service ThingService {
  rpc GetThing(GetThingRequest) returns (GetThingResponse);
  rpc ListThings(ListThingsRequest) returns (ListThingsResponse);
}
PROTO

"$AXIOGRAPH" ingest proto ingest "$PROTO_ROOT" \
  --out "$OUT_DIR/proto_tick1.proposals.json" \
  --descriptor-out "$OUT_DIR/proto_tick1.descriptor.json" \
  --schema-hint proto_api

"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/proto_tick1.proposals.json" \
  --out "$OUT_DIR/ProtoTick1.proposals.axi" \
  --module ProtoTick1_Proposals \
  --schema ProtoTick1 \
  --instance ProtoTick1Instance \
  --infer-constraints
"$AXIOGRAPH" check validate "$OUT_DIR/ProtoTick1.proposals.axi"

"$AXIOGRAPH" db pathdb import-axi "$OUT_DIR/ProtoTick1.proposals.axi" \
  --out "$OUT_DIR/proto_tick1.axpd"

"$AXIOGRAPH" tools viz "$OUT_DIR/proto_tick1.axpd" \
  --out "$OUT_DIR/proto_tick1_service.html" \
  --format html \
  --plane data \
  --focus-name acme_toy_v1_ThingService \
  --hops 2 \
  --max-nodes 320

echo ""
echo "-- diff drafted modules (tick0 vs tick1)"
diff -u "$OUT_DIR/ProtoTick0.proposals.axi" "$OUT_DIR/ProtoTick1.proposals.axi" || true

echo ""
echo "Done."
echo "Open:"
echo "  $OUT_DIR/proto_tick0_service.html"
echo "  $OUT_DIR/proto_tick1_service.html"
