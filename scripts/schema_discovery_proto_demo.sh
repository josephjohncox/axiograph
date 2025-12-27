#!/bin/bash
set -euo pipefail

# Schema discovery demo: proto/gRPC-ish proposals.json → draft `.axi` module → import/query/viz.
#
# This uses a tiny checked-in toy proposals file so the resulting `.axi` stays small
# and reviewable. For a large-scale example, see `docs/howto/INGEST_PROTO.md` and use
# `axiograph ingest proto ingest ...` to generate a bigger `proposals.json`.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/schema_discovery_proto_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph schema discovery (proto) demo =="
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
echo "-- draft a candidate .axi module (schema discovery)"
"$AXIOGRAPH" discover draft-module \
  "$ROOT_DIR/examples/schema_discovery/proto_api_proposals.json" \
  --out "$OUT_DIR/ProtoApi.proposals.axi" \
  --module ProtoApi_Proposals \
  --schema ProtoApi \
  --instance ProtoApiInstance \
  --infer-constraints

echo ""
echo "-- validate drafted module parses + typechecks (AST-level)"
"$AXIOGRAPH" check validate "$OUT_DIR/ProtoApi.proposals.axi"

echo ""
echo "-- visualize the imported schema (meta-plane) and a small neighborhood"
"$AXIOGRAPH" tools viz "$OUT_DIR/ProtoApi.proposals.axi" \
  --out "$OUT_DIR/proto_schema_meta.dot" \
  --format dot \
  --plane meta \
  --focus-name ProtoApi \
  --hops 3 \
  --max-nodes 260

"$AXIOGRAPH" tools viz "$OUT_DIR/ProtoApi.proposals.axi" \
  --out "$OUT_DIR/user_service.html" \
  --format html \
  --plane data \
  --focus-name UserService \
  --hops 2 \
  --max-nodes 140

echo ""
echo "-- import + query in a non-interactive REPL session"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi $OUT_DIR/ProtoApi.proposals.axi" \
  --cmd "schema ProtoApi" \
  --cmd "constraints ProtoApi" \
  --cmd "validate_axi" \
  --cmd "q select ?rpc where UserService -proto_service_has_rpc-> ?rpc limit 10" \
  --cmd "q select ?ep where GetUser -proto_rpc_http_endpoint-> ?ep limit 10"

echo ""
echo "Done."
echo "Draft module: $OUT_DIR/ProtoApi.proposals.axi"
