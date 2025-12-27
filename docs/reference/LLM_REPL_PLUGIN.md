# Axiograph LLM Plugin Protocol (`axiograph_llm_plugin_v2` / `axiograph_llm_plugin_v3`)

**Diataxis:** Reference  
**Audience:** contributors

The Axiograph REPL (and some CLI discovery workflows) support an optional
**LLM-assisted** layer.

There are two related protocols:

- `axiograph_llm_plugin_v2`: translate questions → structured query (`query_ir_v1` preferred; AxQL fallback) and (optionally) summarize results.
- `axiograph_llm_plugin_v3`: a **tool-loop step** protocol for agentic workflows (LLM calls tools; Rust executes; LLM answers).

1. an LLM proposes a **structured** query (AxQL)
2. Rust executes the proposed query against the loaded snapshot
3. (optional) the LLM summarizes results into a natural-language answer

This document specifies the **plugin protocol** used by the REPL so we can use:

- a local lightweight model runner (Ollama, llama.cpp, llamafile, …), or
- a remote LLM API (later), without changing the REPL itself.

The LLM is **untrusted**: it produces *candidate queries*. Axiograph is the
source of truth for execution (and later: certificate production for Lean).

The same plugin protocol is also used by evidence-plane discovery augmentation:

```bash
cd rust
cargo run -p axiograph-cli -- discover augment-proposals build/proposals.json \
  --out build/proposals.aug.json \
  --llm-plugin python3 --llm-plugin-arg scripts/axiograph_llm_plugin_mock.py
```

## Built-in Ollama backend (no plugin)

If you have Ollama installed, you can use local models directly:

```text
axiograph> llm use ollama nemotron-3-nano
axiograph> llm ask find Node named b
```

End-to-end demo script (Ollama + `nemotron-3-nano` by default):

```bash
./scripts/llm_ollama_nemotron_demo.sh
```

The REPL calls Ollama's native HTTP API at `OLLAMA_HOST` (default:
  `http://127.0.0.1:11434`).

## REPL commands

```text
llm status
llm use mock
llm use ollama [model]
llm use command <exe> [args...]
llm model <model_name>
llm ask <question...>
llm answer <question...>
llm agent <question...>
```

See `docs/tutorials/REPL.md` for a walkthrough.

## Transport

Plugins are external commands:

- **stdin**: a single JSON request
- **stdout**: a single JSON response
- **stderr**: may be used for debug logs (shown only on plugin failure)

The REPL runs plugins without a shell (no `sh -c`), so argv splitting is safe.

## Request schema (v2)

Top-level:

```json
{
  "protocol": "axiograph_llm_plugin_v2",
  "model": "optional-model-name",
  "task": { ... }
}
```

### Task: `to_query`

The plugin should translate a user question into a **structured query**.

Preferred output: `query_ir_v1` (typed JSON that compiles into AxQL).
Fallback output: `axql` (string).

```json
{
  "kind": "to_query",
  "question": "find nodes named b",
  "schema": {
    "types": ["Node", "..."],
    "relations": ["rel_0", "..."],
    "schemas": ["MySchema", "..."],
    "relation_signatures": ["MySchema.Rel(a: Node, b: Node)  (source_field=a, target_field=b)", "..."],
    "relation_constraints": ["MySchema.Rel: symmetric; key(a, b)", "..."],
    "contexts": ["Observed", "Policy", "..."],
    "times": ["T2023", "..."]
  }
}
```

Preferred response (typed IR):

```json
{
  "query_ir_v1": {
    "version": 1,
    "select": ["?x"],
    "where": [
      { "kind": "type", "term": "?x", "type": "Node" },
      { "kind": "attr_eq", "term": "?x", "key": "name", "value": "b" }
    ],
    "limit": 20
  }
}
```

Fallback response (AxQL text):

```json
{
  "axql": "select ?x where ?x is Node, ?x.name = \"b\" limit 20"
}
```

### Task: `answer`

The plugin should produce a natural-language answer grounded in results.

```json
{
  "kind": "answer",
  "question": "how do I reach c from a?",
  "query": { "kind": "axql", "axql": "select ?y where ..." },
  "results": {
    "vars": ["?x", "?y"],
    "rows": [
      {
        "?x": { "id": 1, "entity_type": "Node", "name": "a" },
        "?y": { "id": 2, "entity_type": "Node", "name": "b" }
      }
    ],
    "truncated": false
  }
}
```

### Task: `augment_proposals` (evidence-plane discovery)

The plugin can optionally help *augment* `proposals.json` with:

- additional entity/relation proposals (still untrusted), and/or
- per-proposal `schema_hint` routing hints (used by the promotion stage).

Request shape:

```json
{
  "kind": "augment_proposals",
  "proposals": { "version": 1, "generated_at": "...", "source": { "...": "..." }, "proposals": [ ... ] },
  "evidence_chunks": { "chunk_id": "chunk text (truncated)", "...": "..." },
  "max_new_proposals": 2000
}
```

Notes:

- `evidence_chunks` is optional; when present, it includes only a bounded subset
  of chunk texts referenced by evidence pointers (truncated for safety).
- The plugin should treat the input as *approximate* evidence, not truth.

## Response schema

Top-level (fields are optional; shape depends on the task):

```json
{
  "query_ir_v1": { "version": 1, "select": ["?x"], "where": [ ... ], "limit": 20 },
  "axql": "select ?x where ...",
  "answer": "…",
  "added_proposals": [ ... ],
  "schema_hint_updates": [ { "proposal_id": "...", "schema_hint": "machinist_learning" } ],
  "notes": ["..."],
  "error": "…"
}
```

If `error` is present, the REPL treats the request as failed.

## Tool loop (v3)

The REPL’s `llm agent ...` command uses a tool loop:

1. the model proposes a tool call (e.g. `axql_run`, `fts_chunks`, `viz_render`)
2. Rust executes the tool against the loaded snapshot
3. repeat until the model returns `final_answer`

This avoids brittle “LLM emits AxQL text” workflows.

### Task: `tool_loop_step`

```json
{
  "protocol": "axiograph_llm_plugin_v3",
  "model": "optional-model-name",
  "task": {
    "kind": "tool_loop_step",
    "question": "what RPCs does ... have?",
    "schema": {
      "types": ["ProtoService", "..."],
      "relations": ["proto_service_has_rpc", "..."],
      "schemas": ["ProtoSchema", "..."],
      "relation_signatures": ["ProtoSchema.ProtoServiceHasRpc(svc: ProtoService, rpc: ProtoRpc)  (source_field=svc, target_field=rpc)", "..."],
      "relation_constraints": ["ProtoSchema.ProtoServiceHasRpc: key(svc, rpc)", "..."],
      "contexts": ["Observed", "..."],
      "times": ["T2023", "..."]
    },
    "tools": [
      { "name": "axql_run", "description": "...", "args_schema": { "...": "..." } }
    ],
    "transcript": [
      { "tool": "fts_chunks", "args": { "query": "..." }, "result": { "hits": [ ... ] } }
    ]
  }
}
```

Note: for large snapshots, Axiograph may compact `schema` and/or truncate large `transcript` results
to keep the request size bounded. Plugins should treat `schema` as hints and be robust to missing
fields (they can always request more detail via additional tool calls).

Note: Axiograph may also include **backend-prefetched** retrieval steps in the `transcript` (e.g.
`db_summary` + `semantic_search`) so tool-loop mode behaves like a RAG pipeline by default.

### Response: tool call

```json
{
  "tool_call": { "name": "axql_run", "args": { "query_ir_v1": { "version": 1, "where": [ ... ] }, "limit": 25 } }
}
```

### Response: final answer

```json
{
  "final_answer": {
    "answer": "…",
    "citations": ["DocChunk#128", "doc_proto_api_0"],
    "queries": ["select ..."],
    "notes": ["..."]
  }
}
```

## Reference implementation

This repo includes a deterministic “mock LLM” plugin:

- `scripts/axiograph_llm_plugin_mock.py`

To use it:

```text
axiograph> llm use command python3 scripts/axiograph_llm_plugin_mock.py
axiograph> llm ask find Node named b
```

It also implements `augment_proposals` for `axiograph discover augment-proposals`.

## Model download + selection

Model selection is passed through via `llm model <name>`. Model downloads are
intentionally delegated to the plugin (or your model runner):

- Ollama: `ollama pull <model>`
- llama.cpp: download a `.gguf` and point your plugin at the file path
