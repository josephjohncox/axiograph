#!/usr/bin/env bash
set -euo pipefail

# Ontology engineering demo (offline, all source types):
#   docs + SQL + JSON + RDF/SHACL + Confluence HTML + conversation + proto + repo index
# → merge proposals/chunks → draft `.axi` → gate → promote → PathDB snapshot + WAL chunks overlay
# → viz + network + quality reports.
#
# Run from repo root:
#   ./scripts/ontology_engineering_all_sources_offline_demo.sh
#
# Notes:
# - This script is deterministic and does not require any network/LLM access.
# - Proto ingestion uses a checked-in descriptor-set JSON (no `buf` required).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/ontology_engineering_all_sources_offline_demo"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== Axiograph ontology engineering demo (all sources, offline) =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph or $ROOT_DIR/bin/axiograph-cli"
  exit 2
fi

INPUTS_DIR="$OUT_DIR/inputs"
MIXED_DIR="$INPUTS_DIR/mixed_dir"
REPO_DIR="$INPUTS_DIR/repo_fixture"
mkdir -p "$MIXED_DIR" "$REPO_DIR/src"
mkdir -p "$OUT_DIR/ingest_conversation" "$OUT_DIR/ingest_proto" "$OUT_DIR/ingest_repo"

echo ""
echo "-- Write mixed-source inputs (docs/sql/json/rdf/html)"
cat > "$MIXED_DIR/notes.md" <<'MD'
# Demo Notes (mixed sources)

Physics:
- Newton's second law: F = m * a.
- Kinetic energy: KE = 1/2 m v^2.

Economics (structure, not truth):
- CashFlow(from, to, amount, currency, time).

Ontology engineering:
- "well-typed" ≠ "true": we only prove derivability from inputs.
MD

cat > "$MIXED_DIR/schema.sql" <<'SQL'
CREATE TABLE Accounts (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

CREATE TABLE Transfers (
  id INTEGER PRIMARY KEY,
  from_account_id INTEGER NOT NULL,
  to_account_id INTEGER NOT NULL,
  amount_cents INTEGER NOT NULL,
  FOREIGN KEY (from_account_id) REFERENCES Accounts(id),
  FOREIGN KEY (to_account_id) REFERENCES Accounts(id)
);
SQL

cat > "$MIXED_DIR/sample.json" <<'JSON'
{
  "service": {
    "name": "acme.payments.v1.PaymentService",
    "rpc": "CapturePayment",
    "http": { "method": "POST", "path": "/v1/payments/{payment_id}:capture" }
  }
}
JSON

cat > "$MIXED_DIR/demo.ttl" <<'TTL'
@prefix ex: <http://example.com/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:Alice a ex:Person .
ex:Bob a ex:Person .
ex:Alice ex:knows ex:Bob .

ex:PersonShape a sh:NodeShape ;
  sh:targetClass ex:Person ;
  sh:property [
    sh:path ex:knows ;
    sh:class ex:Person
  ] .
TTL

cat > "$MIXED_DIR/confluence_export.html" <<'HTML'
<!doctype html>
<html>
  <head><title>DEMO: Design Notes</title></head>
  <body>
    <h1>Design Notes</h1>
    <p>We treat canonical .axi as the meaning plane and emit certificates from Rust.</p>
    <p>We keep unknown vs false explicit; contexts/worlds scope assertions.</p>
  </body>
</html>
HTML

echo ""
echo "-- Write conversation transcript (Slack-ish)"
CONV_TXT="$INPUTS_DIR/conversation.txt"
cat > "$CONV_TXT" <<'TXT'
Alice: Let's ingest the proto API and extract workflows.
Bob: We should keep proposals separate from accepted axi.
Alice: Also run quality checks before promotion.
TXT

echo ""
echo "-- Write tiny repo fixture (repo index)"
cat > "$REPO_DIR/README.md" <<'MD'
# Repo Fixture

This is a tiny repo used by `ingest repo index` in demos.
MD

cat > "$REPO_DIR/src/lib.rs" <<'RS'
pub struct PaymentId(pub String);

pub fn capture_payment(id: PaymentId) -> bool {
    let _ = id;
    true
}
RS

# ---------------------------------------------------------------------------
# A) Ingest each source type into Evidence/Proposals artifacts
# ---------------------------------------------------------------------------

echo ""
echo "-- A1) ingest dir (mixed sources)"
"$AXIOGRAPH" ingest dir "$MIXED_DIR" \
  --out-dir "$OUT_DIR/ingest_mixed" \
  --confluence-space DEMO \
  --domain mixed

echo ""
echo "-- A2) ingest conversation"
"$AXIOGRAPH" ingest conversation "$CONV_TXT" \
  --out "$OUT_DIR/ingest_conversation/proposals.json" \
  --chunks "$OUT_DIR/ingest_conversation/chunks.json" \
  --facts "$OUT_DIR/ingest_conversation/facts.json" \
  --format slack

echo ""
echo "-- A3) ingest proto (offline via checked-in descriptor-set JSON)"
PROTO_ROOT="$ROOT_DIR/examples/proto/large_api"
PROTO_DESCRIPTOR="$ROOT_DIR/examples/proto/large_api/descriptor.json"
"$AXIOGRAPH" ingest proto ingest "$PROTO_ROOT" \
  --descriptor "$PROTO_DESCRIPTOR" \
  --out "$OUT_DIR/ingest_proto/proposals.json" \
  --chunks "$OUT_DIR/ingest_proto/chunks.json" \
  --schema-hint proto_api

echo ""
echo "-- A4) ingest repo index"
"$AXIOGRAPH" ingest repo index "$REPO_DIR" \
  --out "$OUT_DIR/ingest_repo/proposals.json" \
  --chunks "$OUT_DIR/ingest_repo/chunks.json" \
  --edges "$OUT_DIR/ingest_repo/edges.json" \
  --max-files 500

# ---------------------------------------------------------------------------
# B) Merge proposals + chunks (single discovery pipeline input)
# ---------------------------------------------------------------------------

echo ""
echo "-- B) merge proposals + chunks"
"$AXIOGRAPH" ingest merge \
  --proposals "$OUT_DIR/ingest_mixed/proposals.json" \
  --proposals "$OUT_DIR/ingest_conversation/proposals.json" \
  --proposals "$OUT_DIR/ingest_proto/proposals.json" \
  --proposals "$OUT_DIR/ingest_repo/proposals.json" \
  --chunks "$OUT_DIR/ingest_mixed/chunks.json" \
  --chunks "$OUT_DIR/ingest_conversation/chunks.json" \
  --chunks "$OUT_DIR/ingest_proto/chunks.json" \
  --chunks "$OUT_DIR/ingest_repo/chunks.json" \
  --out "$OUT_DIR/proposals.all.json" \
  --chunks-out "$OUT_DIR/chunks.all.json" \
  --schema-hint all_sources_demo

# ---------------------------------------------------------------------------
# C) Discovery → draft canonical `.axi` (untrusted)
# ---------------------------------------------------------------------------

echo ""
echo "-- C1) augment-proposals (heuristics only; no LLM)"
"$AXIOGRAPH" discover augment-proposals \
  "$OUT_DIR/proposals.all.json" \
  --out "$OUT_DIR/proposals.all.aug.json" \
  --trace "$OUT_DIR/proposals.all.aug.trace.json"

echo ""
echo "-- C2) draft module (candidate .axi for review)"
DRAFT_AXI="$OUT_DIR/AllSourcesDiscovered.proposals.axi"
"$AXIOGRAPH" discover draft-module \
  "$OUT_DIR/proposals.all.aug.json" \
  --out "$DRAFT_AXI" \
  --module AllSourcesDiscovered \
  --schema AllSources \
  --instance AllSourcesInstance \
  --infer-constraints

# ---------------------------------------------------------------------------
# D) Gates + promotion + snapshots
# ---------------------------------------------------------------------------

echo ""
echo "-- D1) gate (Rust): validate drafted module parses/typechecks"
"$AXIOGRAPH" check validate "$DRAFT_AXI"

echo ""
echo "-- D2) tooling reports (network + quality; untrusted helpers)"
"$AXIOGRAPH" tools analyze network "$DRAFT_AXI" --plane both --skip-facts --format json --out "$OUT_DIR/network_draft_axi.json"
"$AXIOGRAPH" check quality "$DRAFT_AXI" --plane both --profile fast --format json --no-fail --out "$OUT_DIR/quality_draft_axi.json"

echo ""
echo "-- D3) promote into accepted plane (append-only)"
ACCEPTED_DIR="$OUT_DIR/accepted_plane"
SNAPSHOT_ID="$("$AXIOGRAPH" db accept promote "$DRAFT_AXI" --dir "$ACCEPTED_DIR" --message "demo: all sources offline" --quality off)"
echo "accepted snapshot: $SNAPSHOT_ID"

echo ""
echo "-- D4) rebuild PathDB from accepted snapshot"
ACCEPTED_AXPD="$OUT_DIR/AllSources.accepted.axpd"
"$AXIOGRAPH" db accept build-pathdb --dir "$ACCEPTED_DIR" --snapshot "$SNAPSHOT_ID" --out "$ACCEPTED_AXPD"

echo ""
echo "-- D5) commit merged evidence into PathDB WAL (chunks + proposals)"
WAL_SNAPSHOT_ID="$("$AXIOGRAPH" db accept pathdb-commit --dir "$ACCEPTED_DIR" --accepted-snapshot "$SNAPSHOT_ID" --chunks "$OUT_DIR/chunks.all.json" --proposals "$OUT_DIR/proposals.all.aug.json" --message "demo: attach evidence overlay (chunks + proposals)")"
echo "pathdb WAL snapshot: $WAL_SNAPSHOT_ID"

echo ""
echo "-- D6) build/check out WAL snapshot (.axpd)"
ACCEPTED_WITH_CHUNKS_AXPD="$OUT_DIR/AllSources.accepted.with_chunks.axpd"
"$AXIOGRAPH" db accept pathdb-build --dir "$ACCEPTED_DIR" --snapshot "$WAL_SNAPSHOT_ID" --out "$ACCEPTED_WITH_CHUNKS_AXPD"

echo ""
echo "-- D7) export reversible PathDB snapshot .axi (for anchoring certificates)"
EXPORT_AXI="$OUT_DIR/AllSources.snapshot_export_v1.axi"
"$AXIOGRAPH" db pathdb export-axi "$ACCEPTED_WITH_CHUNKS_AXPD" --out "$EXPORT_AXI"

# ---------------------------------------------------------------------------
# E) Viz + analysis over the final snapshot
# ---------------------------------------------------------------------------

echo ""
echo "-- E1) visualize with typed overlay (meta-plane as type layer)"
"$AXIOGRAPH" tools viz "$ACCEPTED_WITH_CHUNKS_AXPD" \
  --out "$OUT_DIR/all_sources_both_typed.html" \
  --format html \
  --plane both \
  --typed-overlay \
  --hops 3 \
  --max-nodes 800

echo ""
echo "-- E2) network + quality over the .axpd snapshot"
"$AXIOGRAPH" tools analyze network "$ACCEPTED_WITH_CHUNKS_AXPD" --plane both --skip-facts --communities --format json --out "$OUT_DIR/network_axpd.json"
"$AXIOGRAPH" check quality "$ACCEPTED_WITH_CHUNKS_AXPD" --plane both --profile fast --format json --no-fail --out "$OUT_DIR/quality_axpd.json"

echo ""
echo "Done."
echo "Key outputs:"
echo "  merged proposals:     $OUT_DIR/proposals.all.json"
echo "  candidate axi:        $DRAFT_AXI"
echo "  accepted plane dir:   $ACCEPTED_DIR"
echo "  accepted axpd:        $ACCEPTED_AXPD"
echo "  wal axpd:             $ACCEPTED_WITH_CHUNKS_AXPD"
echo "  snapshot export axi:  $EXPORT_AXI"
echo "  viz (typed overlay):  $OUT_DIR/all_sources_both_typed.html"
echo "  network report:       $OUT_DIR/network_axpd.json"
echo "  quality report:       $OUT_DIR/quality_axpd.json"
