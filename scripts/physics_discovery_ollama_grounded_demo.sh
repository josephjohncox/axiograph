#!/bin/bash
set -euo pipefail

# Physics discovery demo: ingest docs → proposals → LLM grounded augmentation → draft `.axi` → gate → PathDB → viz.
#
# This showcases:
# - evidence-plane ingestion from small physics notes
# - LLM grounded expansion (`--llm-add-proposals`) that adds new untrusted entities/relations
# - candidate `.axi` drafting (schema discovery)
# - promotion gate: Rust validate + Lean `axi_well_typed_v1` (optional)
# - derived artifacts: `.axpd`, reversible snapshot export `.axi`, HTML viz
#
# Run:
#   ./scripts/physics_discovery_ollama_grounded_demo.sh
#
# Requirements:
# - If `LLM_BACKEND=ollama`: `ollama` installed + running (`ollama serve`), and the model available.
# - If `LLM_BACKEND=openai`: `OPENAI_API_KEY` set.
# - If `LLM_BACKEND=anthropic`: `ANTHROPIC_API_KEY` set.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/physics_discovery_ollama_demo"
mkdir -p "$OUT_DIR"

LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-${MODEL:-nemotron-3-nano}}"
export OLLAMA_HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"

echo "== Axiograph physics discovery demo =="
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

echo ""
echo "-- A) ingest physics notes → proposals.json (evidence plane)"
SRC_DIR="$OUT_DIR/src"
mkdir -p "$SRC_DIR"

cat > "$SRC_DIR/physics_notes.md" <<'MD'
# Physics Notes (for engineering)

Newton's second law: Force equals mass times acceleration (F = m * a).
Kinetic energy: KE = 1/2 m v^2.

Dimensional analysis:
- Force has dimensions Mass * Length / Time^2.
- Energy has dimensions Mass * Length^2 / Time^2.

Machining tie-in:
- Cutting force tends to increase with depth of cut and feed (heuristic).
- At high cutting speeds, a lot of heat goes into the chip (heuristic).
MD

"$AXIOGRAPH" ingest dir "$SRC_DIR" \
  --out-dir "$OUT_DIR/ingest" \
  --chunks "$OUT_DIR/chunks.json" \
  --facts "$OUT_DIR/facts.json" \
  --proposals "$OUT_DIR/proposals.json" \
  --domain physics

echo ""
echo "-- B) LLM grounded augmentation: add proposals + (optional) routing hints"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/proposals.json" \
  --out "$OUT_DIR/proposals.aug.json" \
  --trace "$OUT_DIR/proposals.aug.trace.json" \
  --chunks "$OUT_DIR/chunks.json" \
  "${DISCOVER_LLM_FLAGS[@]}" \
  --llm-add-proposals

echo ""
echo "-- C) draft a candidate .axi module from augmented proposals"
DRAFT_AXI="$OUT_DIR/PhysicsDiscovered.llm_draft.axi"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/proposals.aug.json" \
  --out "$DRAFT_AXI" \
  --module PhysicsDiscovered_Proposals \
  --schema PhysicsDiscovered \
  --instance PhysicsDiscoveredInstance \
  --infer-constraints \
  "${DISCOVER_LLM_FLAGS[@]}"

echo ""
echo "-- D) promotion gate (candidate -> accepted) + snapshot outputs"
ACCEPTED_DIR="$OUT_DIR/accepted"
mkdir -p "$ACCEPTED_DIR"

ACCEPTED_AXI="$ACCEPTED_DIR/PhysicsDiscovered.accepted.axi"
TYPECHECK_CERT="$ACCEPTED_DIR/PhysicsDiscovered.accepted.typecheck_cert.json"

echo ""
echo "-- gate 1/2 (Rust): validate candidate module"
"$AXIOGRAPH" check validate "$DRAFT_AXI"

echo ""
echo "-- gate 2/2 (certificate): emit typecheck certificate (axi_well_typed_v1)"
"$AXIOGRAPH" cert typecheck "$DRAFT_AXI" --out "$TYPECHECK_CERT"

echo ""
echo "-- gate 2/2 (Lean): verify typecheck certificate (optional, requires Lean/lake)"
(cd "$ROOT_DIR" && make verify-lean-cert AXI="$DRAFT_AXI" CERT="$TYPECHECK_CERT")

echo ""
echo "-- promote: accept the candidate module (copy into accepted plane)"
cp "$DRAFT_AXI" "$ACCEPTED_AXI"
echo "accepted: $ACCEPTED_AXI"

echo ""
echo "-- build a PathDB snapshot (.axpd) from accepted canonical .axi"
ACCEPTED_AXPD="$ACCEPTED_DIR/PhysicsDiscovered.accepted.axpd"
"$AXIOGRAPH" db pathdb import-axi "$ACCEPTED_AXI" --out "$ACCEPTED_AXPD"

echo ""
echo "-- export a reversible PathDB snapshot (.axi) for certificate anchoring"
SNAPSHOT_EXPORT_AXI="$ACCEPTED_DIR/PhysicsDiscovered.snapshot_export_v1.axi"
"$AXIOGRAPH" db pathdb export-axi "$ACCEPTED_AXPD" --out "$SNAPSHOT_EXPORT_AXI"

echo ""
echo "-- viz (meta + data planes) for the accepted snapshot"
"$AXIOGRAPH" tools viz "$ACCEPTED_AXPD" \
  --out "$ACCEPTED_DIR/physics_discovered_both.html" \
  --format html \
  --plane both \
  --focus-name PhysicsDiscovered \
  --hops 3 \
  --max-nodes 420

echo ""
echo "-- (optional) merge with the canonical PhysicsKnowledge module for side-by-side exploration"
MERGED_AXPD="$ACCEPTED_DIR/PhysicsKnowledge_plus_discovered.axpd"
MERGED_EXPORT_AXI="$ACCEPTED_DIR/PhysicsKnowledge_plus_discovered_export_v1.axi"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/machining/PhysicsKnowledge.axi" \
  --cmd "import_axi $ACCEPTED_AXI" \
  --cmd "save $MERGED_AXPD" \
  --cmd "export_axi $MERGED_EXPORT_AXI"

"$AXIOGRAPH" tools viz "$MERGED_AXPD" \
  --out "$ACCEPTED_DIR/physics_merged_both.html" \
  --format html \
  --plane both \
  --focus-name Physics \
  --hops 3 \
  --max-nodes 520

echo ""
echo "Done."
echo "Key outputs:"
echo "  $OUT_DIR/proposals.json"
echo "  $OUT_DIR/proposals.aug.json"
echo "  $DRAFT_AXI"
echo "  $ACCEPTED_AXI"
echo "  $ACCEPTED_AXPD"
echo "  $SNAPSHOT_EXPORT_AXI"
echo "  $TYPECHECK_CERT"
echo "  $ACCEPTED_DIR/physics_discovered_both.html"
echo "  $MERGED_AXPD"
echo "  $ACCEPTED_DIR/physics_merged_both.html"
