# World Model Plugin Protocol (`axiograph_world_model_v1`)

**Diataxis:** Reference  
**Audience:** tool/plugin authors

This protocol lets an **untrusted** world model propose evidence-plane facts
(`proposals.json`) from grounded Axiograph context.

The plugin reads a JSON request from stdin and writes a JSON response to stdout.

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
        "kind": "relation",
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

---

## Example plugins

Real model (API-backed; OpenAI/Anthropic/Ollama):  
`scripts/axiograph_world_model_plugin_real.py`

Environment variables:

```bash
export WORLD_MODEL_BACKEND=openai|anthropic|ollama
export WORLD_MODEL_MODEL=...
export OPENAI_API_KEY=...
export ANTHROPIC_API_KEY=...
export OLLAMA_HOST=http://127.0.0.1:11434
```

Transformer stub (skeleton for PyTorch):  
`scripts/axiograph_world_model_plugin_transformer_stub.py`

Baseline (no ML, deterministic):  
`scripts/axiograph_world_model_plugin_baseline.py`

---

## Integration points

- CLI: `axiograph discover world-model-propose` / `axiograph ingest world-model`
- REPL: `wm` subcommand
- Server: `POST /world_model/propose`

All outputs remain **evidence-plane** until validated and promoted.
