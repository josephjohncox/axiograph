# World Model Plugin Protocol (`axiograph_world_model_v1`)

**Diataxis:** Reference  
**Audience:** tool/plugin authors

This protocol lets an **untrusted** world model propose evidence-plane facts
(`proposals.json`) from grounded Axiograph context.

The plugin reads a JSON request from stdin and writes a JSON response to stdout.
You can implement this protocol in any language (or behind HTTP), and Axiograph
also ships a built-in LLM-backed plugin to avoid Python in core flows.

Note: the **LLM prompt** is only used by the built-in LLM plugin. Custom ONNX or
hierarchical reasoning models receive the raw request and can interpret it
however they choose.

---

## Protocol string

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
    "export": { "version": "axi_jepa_export_v1", "...": "..." },
    "export_path": "/path/to/export.json",
    "snapshot": {
      "kind": "axpd|store",
      "path": "/path/to/axpd_or_store",
      "snapshot_id": "fnv1a64:...",
      "accepted_snapshot_id": "fnv1a64:..."
    },
    "guardrail": { "version": "guardrail_costs_v1", "...": "..." },
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
- `input.export` embeds a full JEPA training export (schema/theory/instance).
- `input.export_path` is a file path when the export is large.
- `input.guardrail` is **optional** and provides cost context.
- `options.task_costs` and `options.horizon_steps` enable MPC/planning contexts.
- `input.axi_module_text` should be a full `.axi` module (schema + theory + instance + contexts + rewrite rules), not just a PathDB export.

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

Built-in LLM plugin (no Python, uses OpenAI/Anthropic/Ollama):

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

HTTP backend (any language/runtime):

```bash
axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals.json \
  --world-model-http http://127.0.0.1:9999/world_model
```

---

## Integration points

- CLI: `axiograph ingest world-model` (legacy: `axiograph discover world-model-propose`)
- CLI (built-in plugin): `axiograph ingest world-model-plugin-llm`
- CLI: `--world-model-llm` and `--world-model-http` (server + propose)
- REPL: `wm` subcommand (`wm use llm` / `wm use http <url>` / `wm use command ...`)
- Server: `POST /world_model/propose`, `POST /world_model/plan`

All outputs remain **evidence-plane** until validated and promoted.
