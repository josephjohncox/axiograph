#!/bin/bash
set -euo pipefail

# Ontology engineering demo: semantic discovery + structural discovery.
#
# This demonstrates two complementary, untrusted LLM assists:
#
#   A) Semantic discovery (evidence plane):
#      `discover augment-proposals --llm-ollama` suggests `schema_hint` updates
#      to route generic proposals into canonical domains.
#
#   B) Structural discovery (candidate `.axi` draft):
#      `discover draft-module --llm-ollama` suggests extra subtypes and simple
#      relation constraints (symmetric/transitive).
#
# Run:
#   ./scripts/ontology_engineering_ollama_discovery_demo.sh
#
# Requirements:
# - If `LLM_BACKEND=ollama`: `ollama` installed + running (`ollama serve`), and the model available.
# - If `LLM_BACKEND=openai`: `OPENAI_API_KEY` set.
# - If `LLM_BACKEND=anthropic`: `ANTHROPIC_API_KEY` set.
#
# Notes:
# - LLM outputs are untrusted. Everything stays reviewable and quarantined
#   (`proposals.json` or candidate `.axi`) until you explicitly promote it.
# - Lean/lake is optional by default. Set REQUIRE_LEAN=1 to make certificate
#   verification mandatory.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/ontology_engineering_ollama_discovery_demo"
mkdir -p "$OUT_DIR"

LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-${MODEL:-nemotron-3-nano}}"
export OLLAMA_HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"

echo "== Axiograph ontology engineering demo =="
echo "root:  $ROOT_DIR"
echo "out:   $OUT_DIR"
echo "llm_backend: $LLM_BACKEND"
echo "llm_model:   $LLM_MODEL"
if [ "$LLM_BACKEND" = "ollama" ]; then
  echo "ollama_host: $OLLAMA_HOST"
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

verify_lean_cert_if_available() {
  local axi="$1"
  local cert="$2"
  if command -v lake >/dev/null 2>&1; then
    (cd "$ROOT_DIR" && make verify-lean-cert AXI="$axi" CERT="$cert")
  elif [ "${REQUIRE_LEAN:-0}" = "1" ]; then
    echo "error: Lean/lake not found and REQUIRE_LEAN=1" >&2
    exit 1
  else
    echo "skip: Lean/lake not found; set REQUIRE_LEAN=1 to make verification mandatory"
  fi
}

echo ""
echo "-- A) semantic discovery (augment proposals with LLM schema_hint routing)"
SRC_DIR="$OUT_DIR/semantic_src"
mkdir -p "$SRC_DIR"

cat > "$SRC_DIR/economic_flows.md" <<'MD'
# Economic Flows Notes

Customer pays Invoice; invoices have line items; costs and revenues roll up into accounts.
Transactions move money between accounts. A payment settles an invoice.
MD

cat > "$SRC_DIR/machining.md" <<'MD'
# Machinist Learning Notes

Feeds and speeds depend on tool material, workpiece material, and operation type.
Tool wear impacts surface finish; recommend cutting speed ranges with confidence.
MD

cat > "$SRC_DIR/schema_evolution.md" <<'MD'
# Schema Evolution Notes

We evolve schemas via migrations. A delta changes an instance along a functor.
Normalization and reconciliation should be certificate-checked against the denotation.
MD

"$AXIOGRAPH" ingest dir "$SRC_DIR" \
  --out-dir "$OUT_DIR/semantic_ingest" \
  --chunks "$OUT_DIR/semantic_chunks.json" \
  --facts "$OUT_DIR/semantic_facts.json" \
  --proposals "$OUT_DIR/semantic_proposals.json" \
  --domain generic

echo ""
echo "-- run LLM-assisted augment-proposals (semantic routing)"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/semantic_proposals.json" \
  --out "$OUT_DIR/semantic_proposals.aug.json" \
  --trace "$OUT_DIR/semantic_proposals.aug.trace.json" \
  --chunks "$OUT_DIR/semantic_chunks.json" \
  "${DISCOVER_LLM_FLAGS[@]}" \
  --overwrite-schema-hints

echo ""
echo "-- promote augmented proposals into candidate domain .axi modules"
"$AXIOGRAPH" discover promote-proposals \
  "$OUT_DIR/semantic_proposals.aug.json" \
  --out-dir "$OUT_DIR/candidates" \
  --min-confidence 0.4 \
  --domains all

echo ""
echo "-- B) structural discovery (draft a module + LLM structure suggestions)"
"$AXIOGRAPH" discover draft-module \
  "$ROOT_DIR/examples/schema_discovery/proto_api_proposals.json" \
  --out "$OUT_DIR/ProtoApi.llm_draft.axi" \
  --module ProtoApi_LLM_Proposals \
  --schema ProtoApi \
  --instance ProtoApiInstance \
  --infer-constraints \
  "${DISCOVER_LLM_FLAGS[@]}"

echo ""
echo "-- validate drafted module parses + typechecks (AST-level)"
"$AXIOGRAPH" check validate "$OUT_DIR/ProtoApi.llm_draft.axi"

echo ""
echo "-- visualize a small neighborhood"
"$AXIOGRAPH" tools viz "$OUT_DIR/ProtoApi.llm_draft.axi" \
  --out "$OUT_DIR/proto_api_llm_draft_service.json" \
  --format json \
  --plane data \
  --focus-name UserService \
  --hops 2 \
  --max-nodes 180

echo ""
echo "-- C) promotion gate (candidate -> accepted) + snapshot outputs"

ACCEPTED_DIR="$OUT_DIR/accepted"
mkdir -p "$ACCEPTED_DIR"

CANDIDATE_AXI="$OUT_DIR/ProtoApi.llm_draft.axi"
ACCEPTED_AXI="$ACCEPTED_DIR/ProtoApi.accepted.axi"
TYPECHECK_CERT="$ACCEPTED_DIR/ProtoApi.accepted.typecheck_cert.json"

echo ""
echo "-- gate 1/2 (Rust): validate candidate module"
"$AXIOGRAPH" check validate "$CANDIDATE_AXI"

echo ""
echo "-- gate 2/2 (certificate): emit typecheck certificate (axi_well_typed_v1)"
"$AXIOGRAPH" cert typecheck "$CANDIDATE_AXI" --out "$TYPECHECK_CERT"

echo ""
echo "-- gate 2/2 (Lean): verify typecheck certificate (optional, requires Lean/lake)"
verify_lean_cert_if_available "$CANDIDATE_AXI" "$TYPECHECK_CERT"

echo ""
echo "-- promote: accept the candidate module (copy into accepted plane)"
cp "$CANDIDATE_AXI" "$ACCEPTED_AXI"
echo "accepted: $ACCEPTED_AXI"

echo ""
echo "-- build a PathDB snapshot (.axpd) from accepted canonical .axi"
ACCEPTED_AXPD="$ACCEPTED_DIR/ProtoApi.accepted.axpd"
"$AXIOGRAPH" db pathdb materialize-axi "$ACCEPTED_AXI" --out "$ACCEPTED_AXPD"

echo ""
echo "-- visualize meta-plane and data-plane (accepted snapshot)"
"$AXIOGRAPH" tools viz "$ACCEPTED_AXPD" \
  --out "$ACCEPTED_DIR/proto_api_meta.json" \
  --format json \
  --plane meta \
  --focus-name ProtoApi \
  --hops 3 \
  --max-nodes 340

"$AXIOGRAPH" tools viz "$ACCEPTED_AXPD" \
  --out "$ACCEPTED_DIR/proto_api_user_service.json" \
  --format json \
  --plane data \
  --focus-name UserService \
  --hops 2 \
  --max-nodes 220

echo ""
echo "-- (optional) emit a query certificate anchored to accepted canonical .axi"
QUERY_CERT="$ACCEPTED_DIR/proto_api_query_cert.json"
"$AXIOGRAPH" cert query "$ACCEPTED_AXI" \
  'select ?rpc where UserService -proto_service_has_rpc-> ?rpc limit 10' \
  --out "$QUERY_CERT"

echo ""
echo "-- (optional) Lean: verify the query certificate"
verify_lean_cert_if_available "$ACCEPTED_AXI" "$QUERY_CERT"

echo ""
echo "Done."
echo "Semantic discovery outputs:"
echo "  $OUT_DIR/semantic_proposals.json"
echo "  $OUT_DIR/semantic_proposals.aug.json"
echo "  $OUT_DIR/candidates/"
echo "Structural discovery outputs:"
echo "  $OUT_DIR/ProtoApi.llm_draft.axi"
echo "  $OUT_DIR/proto_api_llm_draft_service.json"
echo "Accepted (gated) outputs:"
echo "  $ACCEPTED_AXI"
echo "  $ACCEPTED_AXPD"
echo "  $TYPECHECK_CERT"
echo "  $QUERY_CERT"
echo "  $ACCEPTED_DIR/proto_api_meta.json"
echo "  $ACCEPTED_DIR/proto_api_user_service.json"
