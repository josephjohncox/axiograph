#!/bin/bash
set -euo pipefail

# Modalities demo: epistemic + deontic + explicit tacit evidence (no LLM required).
#
# This runs the same core steps as `examples/repl_scripts/modalities_axi_demo.repl`,
# but writes outputs to a stable directory under `build/` and generates HTML viz.
#
# Run:
#   ./scripts/modalities_axi_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/modalities_axi_demo"
mkdir -p "$OUT_DIR"

echo "== Modalities demo =="
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
echo "-- A) import canonical .axi and run a few queries (REPL non-interactive)"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi examples/modal/Modalities.axi" \
  --cmd "schema Modal" \
  --cmd "constraints Modal" \
  --cmd "validate_axi" \
  --cmd "q select ?p where name(\"Alice\") -Knows-> ?p limit 10" \
  --cmd "q select ?obl where name(\"W0\") -Obligatory-> ?obl limit 10" \
  --cmd "export_axi $OUT_DIR/modalities_axi_export_v1.axi" \
  --cmd "save $OUT_DIR/modalities_axi.axpd"

echo ""
echo "-- B) viz (meta + data)"
"$AXIOGRAPH" tools viz "$OUT_DIR/modalities_axi.axpd" \
  --out "$OUT_DIR/modalities_meta.html" \
  --format html \
  --plane meta \
  --focus-name Modal \
  --hops 3 \
  --max-nodes 420

"$AXIOGRAPH" tools viz "$OUT_DIR/modalities_axi.axpd" \
  --out "$OUT_DIR/modalities_data.html" \
  --format html \
  --plane data \
  --hops 2 \
  --max-nodes 420

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/modalities_axi_export_v1.axi"
echo "  $OUT_DIR/modalities_axi.axpd"
echo "  $OUT_DIR/modalities_meta.html"
echo "  $OUT_DIR/modalities_data.html"
