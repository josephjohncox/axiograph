#!/usr/bin/env bash
set -euo pipefail

# Wikipedia crawl → proposals → draft module → PathDB.
#
# This is a large-ish scrape/crawl demo intended to stress the web ingestion path.
# It respects robots.txt by default and has conservative rate limits.
#
# Run:
#   ./scripts/web_wikipedia_crawl_demo.sh
#
# Tunables (env vars):
#   MAX_PAGES=200 MAX_DEPTH=2 DELAY_MS=500

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/web_wikipedia_crawl_demo"
mkdir -p "$OUT_DIR"

MAX_PAGES="${MAX_PAGES:-200}"
MAX_DEPTH="${MAX_DEPTH:-2}"
DELAY_MS="${DELAY_MS:-500}"

echo "== Axiograph web crawl demo (Wikipedia) =="
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
echo "-- A) Crawl + ingest"
"$AXIOGRAPH" ingest web ingest \
  --out-dir "$OUT_DIR/ingest" \
  --crawl \
  --seed "https://en.wikipedia.org/wiki/Physics" \
  --max-pages "$MAX_PAGES" \
  --max-depth "$MAX_DEPTH" \
  --delay-ms "$DELAY_MS" \
  --respect-robots \
  --same-host \
  --domain general

echo ""
echo "-- B) Draft a candidate axi_v1 module (untrusted) from proposals"
"$AXIOGRAPH" discover draft-module "$OUT_DIR/ingest/proposals.json" \
  --out "$OUT_DIR/wikipedia_discovered.axi" \
  --module "WikipediaDiscovered" \
  --schema "WikipediaDiscovered" \
  --infer-constraints

echo ""
echo "-- C) Import drafted module into PathDB (.axpd) and run tooling"
"$AXIOGRAPH" db pathdb import-axi "$OUT_DIR/wikipedia_discovered.axi" --out "$OUT_DIR/wikipedia_discovered.axpd"

"$AXIOGRAPH" tools analyze network "$OUT_DIR/wikipedia_discovered.axpd" --plane both --format json --out "$OUT_DIR/network.json"
"$AXIOGRAPH" check quality "$OUT_DIR/wikipedia_discovered.axpd" --plane both --profile fast --format json --no-fail --out "$OUT_DIR/quality.json"

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/ingest/proposals.json"
echo "  $OUT_DIR/wikipedia_discovered.axi"
echo "  $OUT_DIR/wikipedia_discovered.axpd"
echo "  $OUT_DIR/network.json"
echo "  $OUT_DIR/quality.json"
