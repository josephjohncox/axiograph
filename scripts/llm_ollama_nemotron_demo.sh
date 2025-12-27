#!/bin/bash
set -euo pipefail

# LLM demo (Ollama): nemotron-3-nano runs a structured tool loop (`llm agent`).
#
# Run:
#   ./scripts/llm_ollama_nemotron_demo.sh
#
# Requirements:
# - `ollama` installed and running (`ollama serve` or the Ollama app)
# - model available (the script will try to `ollama pull` if missing)
#
# Notes:
# - The LLM is untrusted: it proposes tool calls and structured queries.
# - Rust executes tools against a snapshot; the model answers grounded in tool outputs.
# - If your Ollama version rejects the request `format` field, Axiograph retries without it.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/llm_ollama_nemotron_demo"
mkdir -p "$OUT_DIR"

MODEL="${MODEL:-nemotron-3-nano}"
export OLLAMA_HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"
SCALE="${SCALE:-2}"
INDEX_DEPTH="${INDEX_DEPTH:-3}"
SEED="${SEED:-1}"

echo "== Axiograph LLM demo (Ollama) =="
echo "root:  $ROOT_DIR"
echo "out:   $OUT_DIR"
echo "model: $MODEL"
echo "ollama_host: $OLLAMA_HOST"

if ! command -v ollama >/dev/null 2>&1; then
  echo "error: ollama not found. Install it from https://ollama.com and retry." >&2
  exit 1
fi

if ! ollama list >/dev/null 2>&1; then
  echo "error: Ollama server not reachable. Start it with: ollama serve" >&2
  exit 1
fi

if ! ollama show "$MODEL" >/dev/null 2>&1; then
  echo "-- pulling model: $MODEL"
  ollama pull "$MODEL"
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

echo ""
echo "-- non-interactive REPL session (LLM tool loop)"
"$AXIOGRAPH" repl --continue-on-error \
  --cmd "gen proto_api $SCALE $INDEX_DEPTH $SEED" \
  --cmd "llm use ollama $MODEL" \
  --cmd "llm status" \
  --cmd "llm agent list ProtoService" \
  --cmd "llm agent what RPCs does acme.svc0.v1.Service0 have?" \
  --cmd "llm agent what is the HTTP endpoint for acme.svc0.v1.Service0.GetWidget?" \
  --cmd "llm agent what does doc_proto_api_0 mention?" \
  --cmd "llm agent what is the suggested next step after acme.svc0.v1.Service0.CreateWidget?" \
  --cmd "viz $OUT_DIR/proto_api_service0.html format html plane data focus_name acme.svc0.v1.Service0 hops 2 max_nodes 320" \
  --cmd "viz $OUT_DIR/proto_api_doc0.html format html plane data focus_name doc_proto_api_0 hops 2 max_nodes 360" \
  --cmd "export_axi $OUT_DIR/proto_api_llm_export_v1.axi"

echo ""
echo "Done."
echo "Open:"
echo "  $OUT_DIR/proto_api_service0.html"
echo "  $OUT_DIR/proto_api_doc0.html"
