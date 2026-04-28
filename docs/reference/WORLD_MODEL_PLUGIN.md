# World Model Integration Payload (`axiograph_world_model_v1`)

**Diataxis:** Reference  
**Audience:** tool/plugin authors

This typed payload lets an **untrusted** world model propose evidence-plane facts
(`proposals.json`) from grounded Axiograph context.

Command plugins read one JSON request from stdin and write one JSON response to
stdout. HTTP and host-managed tool surfaces can carry the same typed payload, but
their framing and lifecycle are not Axiograph-specific JSON-RPC. Keep generic
protocol infrastructure in maintained crates and services (`rmcp`,
`lsp-server`/`lsp-types`, `hyper`/`http-body-util`, and `reqwest`-backed
clients); keep this contract focused on the semantic request/response shape.

Axiograph ships a built-in LLM-backed runner so normal demos do not need Python
adapter scripts. Command plugins remain useful for offline models, research
prototypes, and integration debugging.

Note: the **LLM prompt** is only used by the built-in LLM plugin. Custom ONNX or
hierarchical reasoning models receive the raw request and can interpret it
however they choose.

---

## Payload version

```
"protocol": "axiograph_world_model_v1"
```

---

## Request schema (simplified)

```json
{
  "protocol": "axiograph_world_model_v1",
  "trace_id": "wm::1730000000",
  "generated_at_unix_secs": 1730000000,
  "input": {
    "axi_digest_v1": "fnv1a64:...",
    "axi_module_text": "module ...",
    "semantic_input": {
      "kind": "canonical_axi_semantics_v1",
      "module_name": "Family",
      "pathdb_snapshot_id": "pathdb:...",
      "accepted_snapshot_id": "accepted:...",
      "layers": [
        {
          "kind": "training_export",
          "export": { "version": "axi_jepa_export_v1", "...": "..." }
        },
        {
          "kind": "guardrail",
          "report": { "version": "guardrail_costs_v1", "...": "..." }
        }
      ]
    },
    "notes": ["source=db_server", "..."]
  },
  "options": {
    "max_new_proposals": 50,
    "seed": 1,
    "goals": ["predict missing parent links"],
    "objectives": [{"name": "goal", "description": "...", "weight": 1.0}],
    "task_costs": [{"name": "latency", "value": 1.2, "weight": 0.5, "unit": "ms"}],
    "horizon_steps": 4,
    "notes": ["planner=mpc"]
  }
}
```

Notes:
- `input.axi_module_text` is the primary semantic input. Plugins should be able to
  reason from canonical `.axi` alone.
- `input.axi_digest_v1` is the stable anchor for that canonical module and should
  match the digest of `input.axi_module_text`.
- `input.semantic_input` is typed derived metadata about that canonical input:
  module selection, snapshot lineage ids, and optional semantic layers.
- `semantic_input.layers[kind=training_export]` embeds a JEPA/training export when
  the caller has one. It is optional derived metadata, not the primary contract.
- If a training export layer is present, it should be derived from the same
  canonical `.axi` bytes and therefore carry the same `axi_digest_v1` and
  module name.
- `semantic_input.layers[kind=guardrail]` is **optional** and provides cost context.
- Snapshot/store paths and `export_path`-style file references are intentionally not
  first-class request fields in this protocol.
- `options.task_costs` and `options.horizon_steps` enable MPC/planning contexts.
- `input.axi_module_text` should be a full canonical `.axi` module (schema +
  theory + instance + contexts + rewrite rules), not a PathDB export.

---

## Response schema (simplified)

```json
{
  "protocol": "axiograph_world_model_v1",
  "trace_id": "wm::1730000000",
  "generated_at_unix_secs": 1730000001,
  "proposals": {
    "version": 1,
    "generated_at": "1730000001",
    "source": {"source_type": "world_model", "locator": "wm::1730000000"},
    "schema_hint": null,
    "proposals": [
      {
        "kind": "Relation",
        "proposal_id": "rel::Parent::Alice::Bob",
        "confidence": 0.62,
        "evidence": [],
        "public_rationale": "predicted by world model",
        "metadata": {"model": "baseline"},
        "schema_hint": "Fam",
        "relation_id": "rel::Parent::Alice::Bob",
        "rel_type": "Parent",
        "source": "Alice",
        "target": "Bob",
        "attributes": {"ctx": "FamilyTree", "time": "T2023"}
      }
    ]
  },
  "notes": ["ok: baseline predictor"],
  "error": null
}
```

If the plugin fails, set `"error"` to a human-readable message.

Note: `ProposalV1.kind` must use the enum variants `Entity` or `Relation`
(`entity`/`relation` will not deserialize).

---

## Example plugins

Built-in LLM runner (no Python adapter, uses OpenAI/Anthropic/Ollama):

```bash
bin/axiograph ingest world-model-plugin-llm --backend openai --model gpt-4o-mini
```

Environment variables (LLM):

```bash
export WORLD_MODEL_BACKEND=openai
export WORLD_MODEL_MODEL=gpt-4o-mini
export OPENAI_API_KEY=...
```

Note: the built-in plugin requires a model name; use `WORLD_MODEL_MODEL` or the
provider-specific env vars (`OPENAI_MODEL`, `ANTHROPIC_MODEL`, `OLLAMA_MODEL`).

Deterministic ONNX model (learned, no randomness):  
`scripts/axiograph_world_model_plugin_onnx.py`

Environment variables (ONNX):

```bash
export WORLD_MODEL_MODEL_PATH=models/world_model_small.onnx
```

Transformer stub (skeleton for PyTorch):  
`scripts/axiograph_world_model_plugin_transformer_stub.py`

Baseline (no ML, deterministic):  
`scripts/axiograph_world_model_plugin_baseline.py`

API-backed model (LLM-based; optional, untrusted).  
If `WORLD_MODEL_BACKEND` is unset, it defaults to **OpenAI** when `OPENAI_API_KEY` is available.  
`scripts/axiograph_world_model_plugin_real.py`

HTTP backend (any language/runtime). The HTTP server owns transport details; the
world-model backend owns only this typed payload:

```bash
axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals.json \
  --world-model-http http://127.0.0.1:9999/world_model
```

---

## Integration points

- CLI: `axiograph ingest world-model`
- CLI (built-in plugin): `axiograph ingest world-model-plugin-llm`
- CLI: `--world-model-llm` and `--world-model-http` (server + propose)
- REPL: `wm` subcommand (`wm use llm` / `wm use http <url>` / `wm use command ...`)
- Server: `POST /world_model/propose`, `POST /world_model/plan`

Only the entrypoints above are documented. Any temporary migration aliases in
development builds are unsupported and should not appear in examples or client
configuration.

When these entrypoints start from a live PathDB snapshot, they first export the
selected canonical module and attach typed lineage anchors
(`axi_digest_v1`, `pathdb_snapshot_id`, `accepted_snapshot_id`) before invoking
the world model. Reversible `PathDBExportV1` snapshots are not part of the
world-model request contract.

All outputs remain **evidence-plane** until validated and promoted.
If a world-model backend uses embeddings or vector retrieval internally, those
retrieval results remain sidecar evidence. They may support proposals, but they
do not become canonical relationships without typed validation, CQ/trust review,
and semantic VCS promotion. See `docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.
