# Browser Explorer (Viz + Server + LLM)

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

This tutorial walks through using Axiograph’s self-contained HTML explorer:

- **viz UI** (`/viz`) for exploration,
- optional **LLM assistance** (tool-loop; untrusted convenience feature),
- and quick **graph debugging** features (filters, typed overlays, path highlight).

We’ll use the canonical example module: `examples/ontology/OntologyRewrites.axi`.

---

## 0) Build

```bash
make binaries
```

All commands below assume `bin/axiograph` exists (built by the Makefile).

---

## 1) Create a snapshot (`.axi` → `.axpd`)

```bash
bin/axiograph db pathdb import-axi examples/ontology/OntologyRewrites.axi \
  --out build/viz_tutorial.axpd
```

## 1b) Add DocChunks for LLM grounding (recommended)

If you want the LLM panel to have *document-like evidence* to cite, import a
`chunks.json` file as `DocChunk` nodes (extension layer).

```bash
cat > build/viz_tutorial_chunks.json <<'EOF'
[
  {
    "chunk_id": "doc_viz_tutorial_0",
    "document_id": "OntologyRewrites_notes.md",
    "page": null,
    "span_id": "para_0",
    "text": "OntologyRewrites.axi includes Parent(parent, child) and Grandparent(grandparent, grandchild). Example people: Alice, Bob, Carol, Eve. Ask: who is Bob's parent?",
    "bbox": null,
    "metadata": {"kind":"demo_note"}
  }
]
EOF

bin/axiograph db pathdb import-chunks build/viz_tutorial.axpd \
  --chunks build/viz_tutorial_chunks.json \
  --out build/viz_tutorial_with_chunks.axpd
```

---

## 2) Start the server (with LLM enabled)

### Option A: Offline (recommended first)

This uses a deterministic “mock LLM” (good for demos/tests).

```bash
AXIOGRAPH_LLM_MAX_STEPS=8 \
bin/axiograph db serve \
  --axpd build/viz_tutorial_with_chunks.axpd \
  --listen 127.0.0.1:7878 \
  --llm-mock
```

### Option B: Local models via Ollama

```bash
ollama serve
ollama pull gemma3

AXIOGRAPH_LLM_MAX_STEPS=10 AXIOGRAPH_LLM_TIMEOUT_SECS=240 \
bin/axiograph db serve \
  --axpd build/viz_tutorial_with_chunks.axpd \
  --listen 127.0.0.1:7878 \
  --llm-ollama \
  --llm-model gemma3
```

Notes:

- The LLM is **untrusted**: it proposes tool calls; Axiograph executes them.
- If your model never returns a `final_answer`, Axiograph auto-finalizes by summarizing the last query result.

---

## 3) Open the explorer

Open this URL:

```text
http://127.0.0.1:7878/viz?focus_name=Alice&plane=both&typed_overlay=true&hops=3&max_nodes=420
```

What you should see:

- a node list on the left,
- a small neighborhood graph (SVG) on the top-right,
- a node detail panel (attrs + edges) on the bottom-right,
- a filter panel (planes, context filter if present, confidence slider),
- and an **LLM panel** (only when served over HTTP).

---

## 4) Try the “typed overlay”

Click a fact node (e.g. a `Parent` or `Grandparent` tuple).

In the node attributes, look for overlay keys:

- `axi_overlay_relation_signature`
- `axi_overlay_constraints`

This is the “meta-plane as type layer” overlay.

---

## 5) Highlight a path (shift-click)

Shift-click two nodes (either in the list or on the graph).

The UI highlights a shortest path *within the currently filtered subgraph* and shows the steps in the detail panel.

---

## 6) Ask a question in the LLM panel

Try:

- `who is Bob's parent`

Expected (for `OntologyRewrites.axi`): **Alice**.

If the model answers via tool-loop queries, the UI will also highlight returned entities.

---

## 7) Optional: call LLM endpoints directly (curl)

```bash
curl -sS http://127.0.0.1:7878/status | jq .llm
```

```bash
curl -sS -X POST http://127.0.0.1:7878/llm/to_query \
  -H 'Content-Type: application/json' \
  -d '{"question":"who is Bob\'s parent"}' | jq .
```

```bash
curl -sS -X POST http://127.0.0.1:7878/llm/agent \
  -H 'Content-Type: application/json' \
  -d '{"question":"who is Bob\'s parent"}' | jq .
```

---

## 8) Scripted demo (optional)

Run:

```bash
./scripts/db_server_llm_viz_demo.sh
```

This seeds an accepted plane, starts a server, and prints a `/viz` URL you can open.

By default, if the script runs with `LLM_BACKEND=ollama`, it also attempts to
compute snapshot-scoped Ollama embeddings (stored in the PathDB WAL) using
`nomic-embed-text`.

You can override the embedding model:

```bash
ollama serve
ollama pull nomic-embed-text

EMBED_OLLAMA_MODEL=nomic-embed-text ./scripts/db_server_llm_viz_demo.sh
```

Or disable embeddings entirely:

```bash
EMBED_ENABLED=0 ./scripts/db_server_llm_viz_demo.sh
```

---

## 9) Full Explorer Demo (planes + contexts + snapshots)

If you want a single demo that exercises:

- **accepted vs evidence planes** (toggle in the sidebar),
- **context/world scoping** (dropdown filter),
- **snapshot time travel** (snapshot selector at the top),
- and **LLM-assisted “reasoning”** (tool-loop panel),

run:

```bash
./scripts/graph_explorer_full_demo.sh
```

Then open the printed `/viz` URL (it focuses on the shared `Context` node `CensusData`, which bridges accepted and evidence overlays).

---

## Troubleshooting

- If the LLM “keeps calling tools” and never finishes, increase steps:
  - `export AXIOGRAPH_LLM_MAX_STEPS=12`
- If Ollama calls time out:
  - `export AXIOGRAPH_LLM_TIMEOUT_SECS=0` (disables the internal timeout)
- If you just want deterministic behavior:
  - use `--llm-mock`
