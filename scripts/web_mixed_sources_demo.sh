#!/usr/bin/env bash
set -euo pipefail

# Mixed-source web ingest demo (no crawling).
#
# This is a small “scrape a few pages” smoke test. It’s useful for validating:
# - HTML → Markdown conversion
# - chunk extraction
# - proposals emission
#
# Run:
#   ./scripts/web_mixed_sources_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/web_mixed_sources_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph web ingest demo (mixed sources) =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
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
echo "-- Ingest a small set of URLs"
"$AXIOGRAPH" ingest web ingest \
  --out-dir "$OUT_DIR/ingest" \
  --url "https://en.wikipedia.org/wiki/General_relativity" \
  --url "https://www.rfc-editor.org/rfc/rfc9110" \
  --url "https://doc.rust-lang.org/book/" \
  --max-pages 3 \
  --delay-ms 400 \
  --respect-robots \
  --domain general

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/ingest/manifest.jsonl"
echo "  $OUT_DIR/ingest/chunks.json"
echo "  $OUT_DIR/ingest/proposals.json"
