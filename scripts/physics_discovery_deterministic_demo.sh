#!/bin/bash
set -euo pipefail

# Physics discovery demo (deterministic, no LLM): ingest docs → proposals → augmentation → draft `.axi`
# → accepted-plane promotion (snapshot id) → PathDB rebuild → viz.
#
# This is the “safe default” ontology-engineering loop:
# - everything starts in the evidence plane (`proposals.json`)
# - canonical `.axi` drafting is explicit and reviewable
# - acceptance is append-only (accepted-plane log + snapshot ids)
# - derived PathDB snapshots are rebuilt from accepted snapshots
#
# Run:
#   ./scripts/physics_discovery_deterministic_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/physics_discovery_deterministic_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph physics discovery demo (deterministic) =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

SRC_DIR="$OUT_DIR/src"
mkdir -p "$SRC_DIR"

cat > "$SRC_DIR/physics_notes.md" <<'MD'
# Physics Notes (for engineering)

Newton's second law: Force equals mass times acceleration (F = m * a).
Kinetic energy: KE = 1/2 m v^2.

Dimensional analysis:
- Force has dimensions Mass * Length / Time^2.
- Energy has dimensions Mass * Length^2 / Time^2.

Machining tie-in (tacit heuristics):
- Cutting force tends to increase with depth of cut and feed.
- At high cutting speeds, a lot of heat goes into the chip.
MD

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
echo "-- A) ingest → proposals.json (evidence plane)"
"$AXIOGRAPH" ingest dir "$SRC_DIR" \
  --out-dir "$OUT_DIR/ingest" \
  --chunks "$OUT_DIR/chunks.json" \
  --facts "$OUT_DIR/facts.json" \
  --proposals "$OUT_DIR/proposals.json" \
  --domain physics

echo ""
echo "-- B) deterministic augmentation (no LLM)"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/proposals.json" \
  --out "$OUT_DIR/proposals.aug.json" \
  --trace "$OUT_DIR/proposals.aug.trace.json"

echo ""
echo "-- C) draft a candidate canonical .axi module (schema discovery)"
DRAFT_AXI="$OUT_DIR/PhysicsDiscovered.draft.axi"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/proposals.aug.json" \
  --out "$DRAFT_AXI" \
  --module PhysicsDiscovered \
  --schema PhysicsDiscovered \
  --instance PhysicsDiscoveredInstance \
  --infer-constraints

echo ""
echo "-- D) gate (Rust): validate candidate module parses/typechecks"
"$AXIOGRAPH" check validate "$DRAFT_AXI"

echo ""
echo "-- E) promote into accepted plane (append-only) and rebuild PathDB snapshot"
ACCEPTED_PLANE_DIR="$OUT_DIR/accepted_plane"
snapshot_id="$("$AXIOGRAPH" db accept promote "$DRAFT_AXI" \
  --dir "$ACCEPTED_PLANE_DIR" \
  --message "reviewed: deterministic physics discovery demo")"
echo "accepted snapshot: $snapshot_id"

ACCEPTED_AXPD="$OUT_DIR/PhysicsDiscovered.accepted.axpd"
"$AXIOGRAPH" db accept build-pathdb \
  --dir "$ACCEPTED_PLANE_DIR" \
  --snapshot "$snapshot_id" \
  --out "$ACCEPTED_AXPD"

echo ""
echo "-- F) export reversible snapshot .axi + viz"
SNAPSHOT_EXPORT_AXI="$OUT_DIR/PhysicsDiscovered.snapshot_export_v1.axi"
"$AXIOGRAPH" db pathdb export-axi "$ACCEPTED_AXPD" --out "$SNAPSHOT_EXPORT_AXI"

"$AXIOGRAPH" tools viz "$ACCEPTED_AXPD" \
  --out "$OUT_DIR/PhysicsDiscovered.both.html" \
  --format html \
  --plane both \
  --focus-name PhysicsDiscovered \
  --hops 3 \
  --max-nodes 520

echo ""
echo "Done."
echo "Key outputs:"
echo "  $OUT_DIR/proposals.json"
echo "  $DRAFT_AXI"
echo "  $ACCEPTED_PLANE_DIR/HEAD"
echo "  $ACCEPTED_AXPD"
echo "  $SNAPSHOT_EXPORT_AXI"
echo "  $OUT_DIR/PhysicsDiscovered.both.html"
