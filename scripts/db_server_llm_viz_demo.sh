#!/usr/bin/env bash
set -euo pipefail

# LLM-in-viz demo:
# - seed an accepted-plane snapshot from a canonical `.axi`
# - start `axiograph db serve` with LLM enabled (Ollama by default)
# - show the `/viz` URL (LLM panel appears in the sidebar)
# - call `/llm/agent` once and save the response
#
# Run from repo root:
#   ./scripts/db_server_llm_viz_demo.sh
#
# Optional:
# - Offline + deterministic:
#     LLM_BACKEND=mock ./scripts/db_server_llm_viz_demo.sh
# - Local models via Ollama (requires `ollama serve`):
#     LLM_BACKEND=ollama LLM_MODEL=nemotron-3-nano ./scripts/db_server_llm_viz_demo.sh
# - Networked (OpenAI):
#     LLM_BACKEND=openai LLM_MODEL=gpt-4o-mini OPENAI_API_KEY=... ./scripts/db_server_llm_viz_demo.sh
# - Networked (Anthropic):
#     LLM_BACKEND=anthropic LLM_MODEL=claude-3-5-sonnet-20241022 ANTHROPIC_API_KEY=... ./scripts/db_server_llm_viz_demo.sh
#
# Notes:
# - This script always has deterministic token-hash retrieval (built into PathDB).
# - Optionally, it can also commit snapshot-scoped *model embeddings* into the WAL
#   for hybrid retrieval (`semantic_search`), via `axiograph db accept pathdb-embed`.
#   Configure with:
#     - `EMBED_ENABLED=0` to disable
#     - `EMBED_BACKEND=ollama|openai` (defaults: ollama when `LLM_BACKEND=ollama`, openai when `LLM_BACKEND=openai`)
#     - `EMBED_MODEL=...` (defaults: `nomic-embed-text` for ollama, `text-embedding-3-small` for openai)
# - By default, if `KEEP_RUNNING=1`, this script skips the sample LLM HTTP call
#   (so the server stays up even if the model is slow / being pulled).
#   Override with: `RUN_SAMPLES=1`.
# - Control the demo HTTP client timeout with: `LLM_HTTP_TIMEOUT_SECS=240`.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/db_server_llm_viz_demo"
PLANE_DIR="$OUT_DIR/plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== PathDB server + LLM viz demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
make binaries

CERT_VERIFY="${CERT_VERIFY:-0}"
if [ "$CERT_VERIFY" = "1" ]; then
  echo ""
  echo "-- Build Lean verifier (optional; enables certify+verify in /viz)"
  set +e
  make lean-exe >"$OUT_DIR/lean_exe.log" 2>&1
  if [ $? -ne 0 ]; then
    echo "warn: failed to build Lean verifier (see $OUT_DIR/lean_exe.log); continuing without server-side verification"
  else
    echo "ok: installed verifier to $ROOT_DIR/bin/axiograph_verify"
  fi
  set -e
fi

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

INPUT_AXI="$ROOT_DIR/examples/ontology/OntologyRewrites.axi"
INCLUDE_DOCCHUNKS="${INCLUDE_DOCCHUNKS:-1}"

# LLM defaults (can be overridden by env vars).
LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-nemotron-3-nano}"

echo ""
echo "-- A) Seed accepted plane with OntologyRewrites"
ACCEPTED_SNAPSHOT_ID="$("$AXIOGRAPH" db accept promote "$INPUT_AXI" \
  --dir "$PLANE_DIR" \
  --message "demo: seed OntologyRewrites")"
echo "accepted snapshot: $ACCEPTED_SNAPSHOT_ID"

if [ "$INCLUDE_DOCCHUNKS" = "1" ]; then
  echo ""
  echo "-- A2) Add a tiny DocChunk overlay (PathDB WAL) for RAG-style grounding"
  CHUNKS_JSON="$OUT_DIR/demo_chunks.json"
  cat >"$CHUNKS_JSON" <<'EOF'
[
  {
    "chunk_id": "doc_ontology_rewrites_0",
    "document_id": "OntologyRewrites_notes.md",
    "page": null,
    "span_id": "para_0",
    "text": "Facts in OntologyRewrites.axi: People (Alice, Bob, Carol, Eve), Org (Acme, Globex). Example relations include Parent(parent, child) and Grandparent(grandparent, grandchild), plus employment/management links in the org part. Use AxQL to query and traverse; use contexts when present. This is a small demo knowledge graph; it has no external ground truth beyond the module inputs.",
    "bbox": null,
    "metadata": {"kind":"demo_note","topic":"facts"}
  },
  {
    "chunk_id": "doc_ontology_rewrites_1",
    "document_id": "OntologyRewrites_notes.md",
    "page": null,
    "span_id": "para_1",
    "text": "To explore: ask questions like 'who is Bob's parent', 'what is connected to Alice', or 'list the Manager chain'. The LLM tool-loop should ground answers by calling db_summary / describe_entity / axql_run, and it can cite these DocChunks as untrusted evidence pointers.",
    "bbox": null,
    "metadata": {"kind":"demo_note","topic":"howto"}
  }
]
EOF

  "$AXIOGRAPH" db accept pathdb-commit \
    --dir "$PLANE_DIR" \
    --accepted-snapshot "$ACCEPTED_SNAPSHOT_ID" \
    --chunks "$CHUNKS_JSON" \
    --message "demo: add doc chunks overlay for LLM grounding" >/dev/null
fi

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

EMBED_TARGET="${EMBED_TARGET:-}"
if [ -z "$EMBED_TARGET" ]; then
  if [ "$INCLUDE_DOCCHUNKS" = "1" ]; then
    EMBED_TARGET="docchunks"
  else
    EMBED_TARGET="entities"
  fi
fi

if [ "$EMBED_ENABLED" = "1" ] && [ -n "${EMBED_BACKEND:-}" ]; then
  echo ""
  echo "-- A3) Compute snapshot-scoped embeddings (PathDB WAL) (backend=$EMBED_BACKEND model=$EMBED_MODEL target=$EMBED_TARGET)"
  if [ "$EMBED_BACKEND" = "ollama" ]; then
    echo "note: make sure the embedding model is available: ollama pull $EMBED_MODEL"
  fi
  set +e
  "$AXIOGRAPH" db accept pathdb-embed \
      --dir "$PLANE_DIR" \
      --snapshot head \
      --target "$EMBED_TARGET" \
      --embed-backend "$EMBED_BACKEND" \
      --embed-model "$EMBED_MODEL" \
      --message "demo: snapshot-scoped embeddings ($EMBED_TARGET)" \
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
echo "-- B) Start server (ephemeral port) with LLM enabled"
READY="$OUT_DIR/ready.json"
ADMIN_TOKEN="${ADMIN_TOKEN:-demo-admin-token}"
LLM_HTTP_TIMEOUT_SECS="${LLM_HTTP_TIMEOUT_SECS:-240}"
RUN_SAMPLES="${RUN_SAMPLES:-}"
if [ -z "$RUN_SAMPLES" ]; then
  if [ "${KEEP_RUNNING:-0}" = "1" ]; then
    RUN_SAMPLES=0
  else
    RUN_SAMPLES=1
  fi
fi

LLM_FLAGS=()
if [ "$LLM_BACKEND" = "ollama" ]; then
  if [ -z "$LLM_MODEL" ]; then
    echo "error: set LLM_MODEL when LLM_BACKEND=ollama (e.g. nemotron-3-nano)"
    exit 2
  fi
  echo "note: make sure the model is available: ollama pull $LLM_MODEL"
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
  --layer "$([ "$INCLUDE_DOCCHUNKS" = "1" ] && echo pathdb || echo accepted)" \
  --snapshot head \
  --listen 127.0.0.1:0 \
  --admin-token "$ADMIN_TOKEN" \
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
echo "admin token (paste into the Add/LLM tabs to commit overlays): $ADMIN_TOKEN"
echo ""
echo "Open this in a browser (LLM panel is in the sidebar):"
echo "  http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420"

echo ""
echo "-- C) Optional: call LLM endpoints once (RUN_SAMPLES=$RUN_SAMPLES)"
if [ "$RUN_SAMPLES" = "1" ]; then
  set +e
  python3 - "$ADDR" "$LLM_HTTP_TIMEOUT_SECS" <<'PY' >"$OUT_DIR/llm_to_query_response.json"
import json, sys, urllib.request
addr = sys.argv[1]
timeout = int(sys.argv[2])
payload = { "question": "find Person named Alice" }
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{addr}/llm/to_query",
  data=data,
  headers={"Content-Type": "application/json"},
  method="POST",
)
resp = urllib.request.urlopen(req, timeout=timeout)
print(resp.read().decode("utf-8"))
PY
  if [ $? -ne 0 ]; then
    echo "warn: /llm/to_query sample failed (see $OUT_DIR/server.log)"
  fi

  # The tool-loop endpoint can take longer (multiple LLM calls). Keep it optional.
  if [ "${RUN_AGENT_SAMPLE:-0}" = "1" ]; then
    python3 - "$ADDR" "$LLM_HTTP_TIMEOUT_SECS" <<'PY' >"$OUT_DIR/llm_agent_response.json"
import json, sys, urllib.request
addr = sys.argv[1]
timeout = int(sys.argv[2])
payload = {
  "question": "find the grandparents of Alice",
  "max_steps": 6,
  "max_rows": 25,
}
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
      echo "warn: /llm/agent sample failed (set RUN_AGENT_SAMPLE=0 to skip)"
    fi
  fi
  set -e
else
  echo "skip: RUN_SAMPLES=0"
fi

echo ""
echo "-- D) Capture viz artifacts (HTML + JSON)"
if command -v curl >/dev/null 2>&1; then
  curl -sS "http://$ADDR/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
    >"$OUT_DIR/viz_alice.html"
  curl -sS "http://$ADDR/viz.json?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420" \
    >"$OUT_DIR/viz_alice.json"
  curl -sS "http://$ADDR/status" \
    >"$OUT_DIR/status.json"
else
  echo "skip: curl not found; not capturing viz artifacts"
fi

echo ""
echo "Wrote:"
echo "  $OUT_DIR/server.log"
echo "  $OUT_DIR/llm_agent_response.json"
echo "  $OUT_DIR/viz_alice.html"
echo "  $OUT_DIR/viz_alice.json"
echo "  $OUT_DIR/status.json"
echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo "Note: this script stops the server when it exits."
  echo "Tip: keep it running by setting KEEP_RUNNING=1."
fi
