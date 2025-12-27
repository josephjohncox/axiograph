#!/bin/bash
set -euo pipefail

# Schema discovery demo: SQL DDL → proposals.json → draft `.axi` module → import/query in REPL.
#
# This demonstrates the “automated ontology engineering” loop:
# - structured sources land as untrusted `proposals.json`,
# - we draft a readable canonical `.axi` module (still untrusted; for review),
# - then import it to get a meta-plane so AxQL can use schema-directed planning.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/schema_discovery_sql_demo"
mkdir -p "$OUT_DIR"

echo "== Axiograph schema discovery (SQL) demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

SQL="$OUT_DIR/sample.sql"
cat > "$SQL" <<'SQL'
CREATE TABLE Users (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

CREATE TABLE Orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL,
  amount_cents INTEGER NOT NULL,
  FOREIGN KEY (user_id) REFERENCES Users(id)
);
SQL

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
echo "-- ingest SQL → proposals.json"
"$AXIOGRAPH" ingest sql "$SQL" --out "$OUT_DIR/sql_proposals.json"

echo ""
echo "-- (optional) augment proposals (heuristics only; LLM plugins can also be used here)"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/sql_proposals.json" \
  --out "$OUT_DIR/sql_proposals.aug.json" \
  --trace "$OUT_DIR/sql_proposals.aug.trace.json"

echo ""
echo "-- draft a candidate .axi module (schema discovery)"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/sql_proposals.aug.json" \
  --out "$OUT_DIR/SqlSchema.proposals.axi" \
  --module SqlSchema_Proposals \
  --schema SqlSchema \
  --instance SqlSchemaInstance \
  --infer-constraints

echo ""
echo "-- validate drafted module parses + typechecks (AST-level)"
"$AXIOGRAPH" check validate "$OUT_DIR/SqlSchema.proposals.axi"

echo ""
echo "-- import + query in a non-interactive REPL session"
"$AXIOGRAPH" repl --quiet \
  --cmd "import_axi $OUT_DIR/SqlSchema.proposals.axi" \
  --cmd "schema SqlSchema" \
  --cmd "constraints SqlSchema" \
  --cmd "validate_axi" \
  --cmd "q select ?c where Users -SqlHasColumn-> ?c limit 10" \
  --cmd "q select ?t where Orders -SqlForeignKey-> ?t limit 10" \
  --cmd "q select ?f where ?f = SqlHasColumn(from=Users, to=Users_id) limit 10"

echo ""
echo "-- visualize the imported schema (meta-plane) and a small neighborhood"
"$AXIOGRAPH" tools viz "$OUT_DIR/SqlSchema.proposals.axi" \
  --out "$OUT_DIR/sql_schema_meta.dot" \
  --format dot \
  --plane meta \
  --focus-name SqlSchema \
  --hops 3 \
  --max-nodes 240

"$AXIOGRAPH" tools viz "$OUT_DIR/SqlSchema.proposals.axi" \
  --out "$OUT_DIR/sql_schema_users.html" \
  --format html \
  --plane data \
  --focus-name Users \
  --hops 2 \
  --max-nodes 140

echo ""
echo "Done."
echo "Draft module: $OUT_DIR/SqlSchema.proposals.axi"
