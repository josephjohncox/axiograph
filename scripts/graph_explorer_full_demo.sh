#!/usr/bin/env bash
set -euo pipefail

# Graph Explorer “full stack” demo:
# - accepted plane snapshots (canonical `.axi`)
# - PathDB WAL overlays (evidence plane: chunks + proposals)
# - contexts/world scoping (`@context Context` → `axi_fact_in_context`)
# - snapshot time travel in the browser explorer (`/viz`)
# - optional reasoning help via the LLM tool-loop panel
#
# Run from repo root:
#   ./scripts/graph_explorer_full_demo.sh
#
# Optional:
# - Local models via Ollama (requires `ollama serve`):
#     LLM_BACKEND=ollama LLM_MODEL=gemma3 KEEP_RUNNING=1 ./scripts/graph_explorer_full_demo.sh
# - Networked (OpenAI):
#     LLM_BACKEND=openai LLM_MODEL=gpt-4o-mini OPENAI_API_KEY=... KEEP_RUNNING=1 ./scripts/graph_explorer_full_demo.sh
# - Networked (Anthropic):
#     LLM_BACKEND=anthropic LLM_MODEL=claude-3-5-sonnet-20241022 ANTHROPIC_API_KEY=... KEEP_RUNNING=1 ./scripts/graph_explorer_full_demo.sh
#
# Notes:
# - Deterministic token-hash retrieval is always available (built into PathDB).
# - Optionally, you can commit snapshot-scoped *model embeddings* into the WAL
#   for hybrid retrieval (`semantic_search`), via `axiograph db accept pathdb-embed`.
#   Configure with:
#     - `EMBED_ENABLED=0` to disable
#     - `EMBED_BACKEND=ollama|openai` (defaults: ollama when `LLM_BACKEND=ollama`, openai when `LLM_BACKEND=openai`)
#     - `EMBED_MODEL=...` (defaults: `nomic-embed-text` for ollama, `text-embedding-3-small` for openai)
# - By default, this starts the server on an ephemeral port and prints a `/viz` URL.
# - The explorer UI includes:
#     - plane toggles (accepted/evidence/data),
#     - context scoping filter,
#     - snapshot selector (time travel),
#     - and an LLM panel (server-only).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/graph_explorer_full_demo"
PLANE_DIR="$OUT_DIR/plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== Graph Explorer full demo =="
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

# LLM defaults (can be overridden by env vars).
LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-nemotron-3-nano}"

echo ""
echo "-- A) Create 3 ticks of a context-scoped module (Family)"
TICK0="$OUT_DIR/Family_tick0.axi"
TICK1="$OUT_DIR/Family_tick1.axi"
TICK2="$OUT_DIR/Family_tick2.axi"

cp "$ROOT_DIR/examples/Family.axi" "$TICK0"

python3 - "$TICK0" <<'PY'
import sys

path = sys.argv[1]
text = open(path, "r", encoding="utf-8").read()

# Add explicit Morphism + Homotopy artifacts into the canonical `.axi` so the
# explorer can show:
# - 1-morphisms (as reified tuple facts) and
# - 2-morphisms / higher paths (explicit “same endpoints, different derivation”)
#
# This stays faithful to our canonical `.axi` meaning-plane story: these are
# *declared* and *auditable* objects, not hidden DB semantics.

schema_insert_after = "  relation Sibling(a: Person, b: Person)\n"
if schema_insert_after not in text:
  raise SystemExit("error: could not find Sibling relation decl to insert HoTT artifacts")

schema_block = """\

  -- ==========================================================================
  -- HoTT / higher-path artifacts (demo)
  -- ==========================================================================
  -- We model:
  -- - 1-morphisms as explicit tuple facts (`Morphism`)
  -- - 2-morphisms as explicit “path equality” witnesses (`Homotopy`)
  --
  -- These are *not* ground truth. They are derivations/witnesses that can later
  -- be certificate-checked (Rust emits; Lean verifies).

  object PathDerivation

  -- A morphism is a directed relationship with an explicit derivation label.
  relation Morphism(from: Person, to: Person, derivation: PathDerivation) @context Context @temporal Time

  -- A homotopy witnesses that two derivations between the same endpoints are equivalent.
  relation Homotopy(from: Person, to: Person, lhs: PathDerivation, rhs: PathDerivation) @context Context @temporal Time
"""

text = text.replace(schema_insert_after, schema_insert_after + schema_block)

instance_insert_after = """\
  Sibling = {
    (a=Carol, b=Dan),
    (a=Dan, b=Carol)
  }
"""
if instance_insert_after not in text:
  raise SystemExit("error: could not find Sibling assignment to insert Morphism/Homotopy facts")

instance_block = """\

  -- --------------------------------------------------------------------------
  -- Explicit morphisms + homotopies (CensusData, 2020)
  -- --------------------------------------------------------------------------
  -- Dan reaches Bob in two different ways:
  --   (1) DirectParent: Parent(child=Dan, parent=Bob)
  --   (2) ViaSiblingThenParent: Sibling(Dan, Carol) then Parent(Carol, Bob)
  --
  -- We record both derivations as Morphism facts, and the higher-path “these are
  -- the same relationship” as a Homotopy witness.

  PathDerivation = {DirectParent, ViaSiblingThenParent}

  Morphism = {
    (from=Dan, to=Bob, derivation=DirectParent, ctx=CensusData, time=T2020),
    (from=Dan, to=Bob, derivation=ViaSiblingThenParent, ctx=CensusData, time=T2020)
  }

  Homotopy = {
    (from=Dan, to=Bob, lhs=DirectParent, rhs=ViaSiblingThenParent, ctx=CensusData, time=T2020)
  }
"""

text = text.replace(instance_insert_after, instance_insert_after + instance_block)

open(path, "w", encoding="utf-8").write(text)
PY

python3 - "$TICK0" "$TICK1" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace(
  "Person = {Alice, Bob, Carol, Dan, Eve, Frank}",
  "Person = {Alice, Bob, Carol, Dan, Eve, Frank, Zoe}",
)
text = text.replace(
  "(child=Eve, parent=Frank, ctx=FamilyTree, time=T2023)\n  }",
  "(child=Eve, parent=Frank, ctx=FamilyTree, time=T2023),\n    (child=Zoe, parent=Eve, ctx=FamilyTree, time=T2023)\n  }",
)
open(dst, "w", encoding="utf-8").write(text)
PY

python3 - "$TICK1" "$TICK2" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace(
  "(a=Dan, b=Carol)\n  }",
  "(a=Dan, b=Carol),\n    (a=Eve, b=Zoe),\n    (a=Zoe, b=Eve)\n  }",
)
open(dst, "w", encoding="utf-8").write(text)
PY

echo ""
echo "-- B) Promote ticks into the accepted plane (append-only)"
SNAP0="$("$AXIOGRAPH" db accept promote "$TICK0" --dir "$PLANE_DIR" --message "demo: Family tick0")"
SNAP1="$("$AXIOGRAPH" db accept promote "$TICK1" --dir "$PLANE_DIR" --message "demo: Family tick1 (add Zoe)")"
SNAP2="$("$AXIOGRAPH" db accept promote "$TICK2" --dir "$PLANE_DIR" --message "demo: Family tick2 (add Sibling Eve↔Zoe)")"
echo "accepted snapshots:"
echo "  tick0: $SNAP0"
echo "  tick1: $SNAP1"
echo "  tick2: $SNAP2"

echo ""
echo "-- C) Create a tiny evidence-plane overlay (chunks + proposals)"
CHUNKS="$OUT_DIR/chunks.json"
cat >"$CHUNKS" <<'EOF'
[
  {
    "chunk_id": "chunk_family_0",
    "document_id": "demo_family_notes.md",
    "page": null,
    "span_id": "para_0",
    "text": "CensusData (2020): Carol has parents Alice and Bob. Dan has parents Alice and Bob.",
    "bbox": null,
    "metadata": {"kind":"note","context":"CensusData","time":"T2020"}
  },
  {
    "chunk_id": "chunk_family_1",
    "document_id": "demo_family_notes.md",
    "page": null,
    "span_id": "para_1",
    "text": "FamilyTree (2023): Eve has parents Carol and Frank. Zoe has parent Eve.",
    "bbox": null,
    "metadata": {"kind":"note","context":"FamilyTree","time":"T2023"}
  },
  {
    "chunk_id": "chunk_family_2",
    "document_id": "demo_family_notes.md",
    "page": null,
    "span_id": "para_2",
    "text": "HoTT artifacts (CensusData, 2020): Dan reaches Bob in two different ways: DirectParent (Parent(child=Dan,parent=Bob)) and ViaSiblingThenParent (Sibling(Dan,Carol) then Parent(Carol,Bob)). These are recorded as Morphism(from=Dan,to=Bob,derivation=...) facts and a Homotopy(from=Dan,to=Bob,lhs=...,rhs=...) witness.",
    "bbox": null,
    "metadata": {"kind":"note","context":"CensusData","time":"T2020","topic":"morphism_homotopy"}
  }
]
EOF

PROPOSALS="$OUT_DIR/proposals.json"
cat >"$PROPOSALS" <<'EOF'
{
  "version": 1,
  "generated_at": "demo",
  "source": { "source_type": "demo", "locator": "graph_explorer_full_demo" },
  "schema_hint": "family_demo",
  "proposals": [
    {
      "kind": "Entity",
      "proposal_id": "demo:person:Alice",
      "confidence": 0.95,
      "evidence": [{"chunk_id":"chunk_family_0","locator":"demo_family_notes.md","span_id":"para_0"}],
      "public_rationale": "Mentioned in CensusData paragraph 0.",
      "metadata": {"demo":"true"},
      "entity_id": "demo:person:Alice",
      "entity_type": "Person",
      "name": "Alice",
      "attributes": {"source":"CensusData"},
      "description": null
    },
    {
      "kind": "Entity",
      "proposal_id": "demo:person:Bob",
      "confidence": 0.95,
      "evidence": [{"chunk_id":"chunk_family_0","locator":"demo_family_notes.md","span_id":"para_0"}],
      "public_rationale": "Mentioned in CensusData paragraph 0.",
      "metadata": {"demo":"true"},
      "entity_id": "demo:person:Bob",
      "entity_type": "Person",
      "name": "Bob",
      "attributes": {"source":"CensusData"},
      "description": null
    },
    {
      "kind": "Entity",
      "proposal_id": "demo:person:Carol",
      "confidence": 0.95,
      "evidence": [{"chunk_id":"chunk_family_0","locator":"demo_family_notes.md","span_id":"para_0"}],
      "public_rationale": "Mentioned in CensusData paragraph 0.",
      "metadata": {"demo":"true"},
      "entity_id": "demo:person:Carol",
      "entity_type": "Person",
      "name": "Carol",
      "attributes": {"source":"CensusData"},
      "description": null
    },
    {
      "kind": "Relation",
      "proposal_id": "demo:rel:Carol_parent_Alice",
      "confidence": 0.80,
      "evidence": [{"chunk_id":"chunk_family_0","locator":"demo_family_notes.md","span_id":"para_0"}],
      "public_rationale": "CensusData paragraph 0 asserts Carol’s parents.",
      "metadata": {"demo":"true"},
      "relation_id": "demo:rel:Carol_parent_Alice",
      "rel_type": "Parent",
      "source": "demo:person:Carol",
      "target": "demo:person:Alice",
      "attributes": {"context":"CensusData","time":"T2020"}
    },
    {
      "kind": "Relation",
      "proposal_id": "demo:rel:Carol_parent_Bob",
      "confidence": 0.80,
      "evidence": [{"chunk_id":"chunk_family_0","locator":"demo_family_notes.md","span_id":"para_0"}],
      "public_rationale": "CensusData paragraph 0 asserts Carol’s parents.",
      "metadata": {"demo":"true"},
      "relation_id": "demo:rel:Carol_parent_Bob",
      "rel_type": "Parent",
      "source": "demo:person:Carol",
      "target": "demo:person:Bob",
      "attributes": {"context":"CensusData","time":"T2020"}
    }
  ]
}
EOF

echo ""
echo "-- D) Commit overlays into the PathDB WAL (one per accepted tick)"
# The WAL chain is global; committing with a new accepted snapshot id rebuilds
# the base and replays earlier ops, so evidence stays available across ticks.
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$SNAP0" --chunks "$CHUNKS" --message "demo: tick0 chunks overlay" >/dev/null
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$SNAP0" --proposals "$PROPOSALS" --message "demo: tick0 proposals overlay" >/dev/null
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$SNAP1" --chunks "$CHUNKS" --message "demo: tick1 (replay overlays on new base)" >/dev/null
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$SNAP2" --chunks "$CHUNKS" --message "demo: tick2 (replay overlays on new base)" >/dev/null

EMBED_ENABLED="${EMBED_ENABLED:-1}"
EMBED_BACKEND="${EMBED_BACKEND:-}"
if [ -z "$EMBED_BACKEND" ]; then
  if [ "$LLM_BACKEND" = "ollama" ] || [ "$LLM_BACKEND" = "openai" ]; then
    EMBED_BACKEND="$LLM_BACKEND"
  fi
fi

EMBED_MODEL="${EMBED_MODEL:-}"
if [ -z "$EMBED_MODEL" ]; then
  if [ "$EMBED_BACKEND" = "ollama" ]; then
    EMBED_MODEL="${EMBED_OLLAMA_MODEL:-nomic-embed-text}"
  elif [ "$EMBED_BACKEND" = "openai" ]; then
    EMBED_MODEL="${EMBED_OPENAI_MODEL:-text-embedding-3-small}"
  fi
fi

if [ "$EMBED_ENABLED" = "1" ] && [ -n "${EMBED_BACKEND:-}" ]; then
  echo ""
  echo "-- D2) Compute snapshot-scoped DocChunk embeddings (PathDB WAL) (backend=$EMBED_BACKEND model=$EMBED_MODEL)"
  if [ "$EMBED_BACKEND" = "ollama" ]; then
    echo "note: make sure the embedding model is available: ollama pull $EMBED_MODEL"
  fi
  set +e
  "$AXIOGRAPH" db accept pathdb-embed \
      --dir "$PLANE_DIR" \
      --snapshot head \
      --target docchunks \
      --embed-backend "$EMBED_BACKEND" \
      --embed-model "$EMBED_MODEL" \
      --message "demo: snapshot-scoped embeddings (docchunks)" \
      >"$OUT_DIR/embed_snapshot_id.txt" 2>"$OUT_DIR/embed.log"
  if [ $? -ne 0 ]; then
    echo "warn: embedding step failed; continuing without stored embeddings (see $OUT_DIR/embed.log)"
  else
    EMBED_SNAPSHOT_ID="$(cat "$OUT_DIR/embed_snapshot_id.txt" 2>/dev/null || true)"
    if [ -n "$EMBED_SNAPSHOT_ID" ]; then
      echo "ok: embeddings committed (pathdb snapshot=$EMBED_SNAPSHOT_ID)"
    fi
  fi
  set -e
fi

echo ""
echo "-- E) Start axiograph db serve (store-backed, pathdb layer)"
READY="$OUT_DIR/ready.json"
LLM_HTTP_TIMEOUT_SECS="${LLM_HTTP_TIMEOUT_SECS:-240}"
ADMIN_TOKEN="${ADMIN_TOKEN:-demo-admin-token}"

LLM_FLAGS=()
if [ "$LLM_BACKEND" = "ollama" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=ollama (e.g. gemma3)"
    exit 2
  fi
  echo "note: requires: ollama serve  (and: ollama pull $LLM_MODEL)"
  LLM_FLAGS+=(--llm-ollama --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "openai" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=openai (example: gpt-4o-mini)"
    exit 2
  fi
  LLM_FLAGS+=(--llm-openai --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "anthropic" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=anthropic (example: claude-3-5-sonnet-20241022)"
    exit 2
  fi
  LLM_FLAGS+=(--llm-anthropic --llm-model "$LLM_MODEL")
else
  LLM_FLAGS+=(--llm-mock)
fi

"$AXIOGRAPH" db serve \
  --dir "$PLANE_DIR" \
  --layer pathdb \
  --snapshot head \
  --role master \
  --admin-token "$ADMIN_TOKEN" \
  --listen 127.0.0.1:0 \
  --ready-file "$READY" \
  "${LLM_FLAGS[@]}" \
  >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

python3 - "$READY" <<'PY' >"$OUT_DIR/addr.txt"
import json, time, sys
path = sys.argv[1]
deadline = time.time() + 30
while time.time() < deadline:
  try:
    with open(path) as f:
      j = json.load(f)
    if "addr" in j:
      print(j["addr"])
      sys.exit(0)
  except Exception:
    time.sleep(0.05)
print("error: server did not write ready file", file=sys.stderr)
sys.exit(2)
PY

ADDR="$(cat "$OUT_DIR/addr.txt")"

echo "server: http://$ADDR"
echo "admin token (paste into the Add tab to commit overlays): $ADMIN_TOKEN"
echo ""
echo "Open this in a browser:"
echo "  http://$ADDR/viz?focus_name=CensusData&plane=both&typed_overlay=true&hops=3&max_nodes=650"
echo ""
echo "Try in the explorer:"
echo "  - Toggle planes: accepted/evidence"
echo "  - Change context filter: CensusData vs FamilyTree"
echo "  - Use the snapshot dropdown to time-travel ticks"
echo "  - Toggle node kinds: morphism/homotopy/fact/meta"
echo "  - Shift-click 2 nodes to highlight a path"
echo "  - In the Query tab (AxQL):"
echo "      * select ?m ?from ?to where ?m = Morphism(from=?from, to=?to) in CensusData limit 10"
echo "      * select ?h ?lhs ?rhs where ?h = Homotopy(from=Dan, to=Bob, lhs=?lhs, rhs=?rhs) in CensusData limit 10"
echo "  - Ask (LLM panel):"
echo "      * what parents does Carol have in CensusData?"
echo "      * what changed between snapshots?"
echo "      * show the morphisms from Dan to Bob (and their derivations)"
echo "      * explain the homotopy witness between Dan → Bob derivations"
echo "      * add Jamison who is a son of Bob  (then: Add tab → commit)"
echo ""

# Optional: call the LLM agent once (useful for CI scripts; skip for KEEP_RUNNING=1).
RUN_SAMPLES="${RUN_SAMPLES:-}"
if [ -z "$RUN_SAMPLES" ]; then
  if [ "${KEEP_RUNNING:-0}" = "1" ]; then
    RUN_SAMPLES=0
  else
    RUN_SAMPLES=1
  fi
fi
if [ "$RUN_SAMPLES" = "1" ]; then
  set +e
  python3 - "$ADDR" "$LLM_HTTP_TIMEOUT_SECS" <<'PY' >"$OUT_DIR/llm_agent_response.json"
import json, sys, urllib.request
addr = sys.argv[1]
timeout = int(sys.argv[2])
payload = {"question": "find Person named Alice", "max_steps": 6, "max_rows": 25}
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{addr}/llm/agent",
  data=data,
  headers={"Content-Type": "application/json"},
  method="POST",
)
resp = urllib.request.urlopen(req, timeout=timeout)
print(resp.read().decode("utf-8"))
PY
  if [ $? -ne 0 ]; then
    echo "warn: /llm/agent sample failed (see $OUT_DIR/server.log)"
  fi
  set -e
fi

echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo "Note: this script stops the server when it exits."
  echo "Tip: keep it running by setting KEEP_RUNNING=1."
fi
