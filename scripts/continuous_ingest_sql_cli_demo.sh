#!/bin/bash
set -euo pipefail

# Continuous ingest demo (SQL): incremental DDL changes → proposals → drafted `.axi` → PathDB → viz.
#
# Run:
#   ./scripts/continuous_ingest_sql_cli_demo.sh
#
# This is intentionally CLI-only (no interactive REPL).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/continuous_ingest_sql_cli_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph continuous ingest (SQL) demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

SQL="$OUT_DIR/schema.sql"

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
echo "-- tick 0: initial SQL"
cat > "$SQL" <<'SQL'
CREATE TABLE Users (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL
);

CREATE TABLE Orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL,
  amount_cents INTEGER NOT NULL,
  FOREIGN KEY (user_id) REFERENCES Users(id)
);
SQL

"$AXIOGRAPH" ingest sql "$SQL" --out "$OUT_DIR/sql_tick0.proposals.json"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/sql_tick0.proposals.json" \
  --out "$OUT_DIR/SqlTick0.proposals.axi" \
  --module SqlTick0_Proposals \
  --schema SqlTick0 \
  --instance SqlTick0Instance \
  --infer-constraints
"$AXIOGRAPH" check validate "$OUT_DIR/SqlTick0.proposals.axi"

"$AXIOGRAPH" db pathdb import-axi "$OUT_DIR/SqlTick0.proposals.axi" \
  --out "$OUT_DIR/sql_tick0.axpd"

"$AXIOGRAPH" tools viz "$OUT_DIR/sql_tick0.axpd" \
  --out "$OUT_DIR/sql_tick0_schema.html" \
  --format html \
  --plane both \
  --focus-name SqlTick0 \
  --hops 3 \
  --max-nodes 260

echo ""
echo "-- tick 1: evolve SQL (add column + new table)"
cat > "$SQL" <<'SQL'
CREATE TABLE Users (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL,
  display_name TEXT
);

CREATE TABLE Orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL,
  amount_cents INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  FOREIGN KEY (user_id) REFERENCES Users(id)
);

CREATE TABLE Refunds (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL,
  reason TEXT,
  FOREIGN KEY (order_id) REFERENCES Orders(id)
);
SQL

"$AXIOGRAPH" ingest sql "$SQL" --out "$OUT_DIR/sql_tick1.proposals.json"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/sql_tick1.proposals.json" \
  --out "$OUT_DIR/SqlTick1.proposals.axi" \
  --module SqlTick1_Proposals \
  --schema SqlTick1 \
  --instance SqlTick1Instance \
  --infer-constraints
"$AXIOGRAPH" check validate "$OUT_DIR/SqlTick1.proposals.axi"

"$AXIOGRAPH" db pathdb import-axi "$OUT_DIR/SqlTick1.proposals.axi" \
  --out "$OUT_DIR/sql_tick1.axpd"

"$AXIOGRAPH" tools viz "$OUT_DIR/sql_tick1.axpd" \
  --out "$OUT_DIR/sql_tick1_schema.html" \
  --format html \
  --plane both \
  --focus-name SqlTick1 \
  --hops 3 \
  --max-nodes 320

echo ""
echo "-- diff drafted modules (tick0 vs tick1)"
diff -u "$OUT_DIR/SqlTick0.proposals.axi" "$OUT_DIR/SqlTick1.proposals.axi" || true

echo ""
echo "Done."
echo "Open:"
echo "  $OUT_DIR/sql_tick0_schema.html"
echo "  $OUT_DIR/sql_tick1_schema.html"
