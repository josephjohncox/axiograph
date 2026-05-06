# Predictive Proposal Adapter Payload (`axiograph_predictive_proposal_v1`)

**Diataxis:** Reference
**Audience:** tool/plugin authors

This typed payload lets an **untrusted** predictive proposal adapter propose
evidence-plane facts (`proposals.json`) from grounded Axiograph context.

This is the core contract. It is not a claim that the adapter is an autonomous
execution system, native world model, MPC planner, JEPA runtime, or control
system. Advanced research adapters may use those techniques internally, but the
Axiograph boundary remains "predictive proposal in, typed evidence out,
reviewed before promotion."

Command plugins read one JSON request from stdin and write one JSON response to
stdout. HTTP and host-managed tool surfaces can carry the same typed payload, but
their framing and lifecycle are not Axiograph-specific JSON-RPC. Keep generic
protocol infrastructure in maintained crates and services (`rmcp`,
`lsp-server`/`lsp-types`, `hyper`/`http-body-util`, and `reqwest`-backed
clients); keep this contract focused on the semantic request/response shape.

Axiograph ships a built-in LLM-backed runner so API-backed runs do not need
Python adapter scripts. Command plugins remain useful for deterministic offline
demos, research adapters, and integration debugging.

Note: the **LLM prompt** is only used by the built-in LLM adapter. Custom ONNX,
latent-prediction, or hierarchical reasoning adapters receive the raw request
and can interpret it however they choose.

---

## Payload version

```
"protocol": "axiograph_predictive_proposal_v1"
```

---

## Request schema (simplified)

```json
{
  "protocol": "axiograph_predictive_proposal_v1",
  "trace_id": "pp::1730000000",
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
          "export": { "version": "axi_training_export_v1", "...": "..." }
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
    "notes": ["rollout=bounded_proposal_rollout"]
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
- `semantic_input.layers[kind=training_export]` embeds a masked-tuple training
  export when the caller has one. It is optional derived metadata, not the
  primary contract.
- If a training export layer is present, it should be derived from the same
  canonical `.axi` bytes and therefore carry the same `axi_digest_v1` and
  module name.
- `semantic_input.layers[kind=guardrail]` is **optional** and provides cost context.
- Snapshot/store paths and `export_path`-style file references are intentionally not
  first-class request fields in this protocol.
- `options.task_costs` and `options.horizon_steps` enable bounded proposal
  rollout contexts. This is not an autonomous-execution claim; it only scopes
  proposal generation and evaluation.
- `input.axi_module_text` should be a full canonical `.axi` module (schema +
  theory + instance + contexts + rewrite rules), not a PathDB export.

---

## Response schema (simplified)

```json
{
  "protocol": "axiograph_predictive_proposal_v1",
  "trace_id": "pp::1730000000",
  "generated_at_unix_secs": 1730000001,
  "proposals": {
    "version": 1,
    "generated_at": "1730000001",
    "source": {"source_type": "predictive_proposal_adapter", "locator": "pp::1730000000"},
    "schema_hint": null,
    "proposals": [
      {
        "kind": "Relation",
        "proposal_id": "rel::Parent::Alice::Bob",
        "confidence": 0.62,
        "evidence": [],
        "public_rationale": "predicted by proposal adapter",
        "metadata": {"model": "baseline"},
        "schema_hint": "Fam",
        "relation_id": "rel::Parent::Alice::Bob",
        "rel_type": "Parent",
        "source": "Alice",
        "target": "Bob",
        "attributes": {
          "axi_source_field": "child",
          "axi_target_field": "parent",
          "ctx": "FamilyTree",
          "time": "T2023"
        }
      }
    ]
  },
  "notes": ["ok: baseline predictive proposer"],
  "error": null
}
```

If the adapter fails, set `"error"` to a human-readable message.

Note: `ProposalV1.kind` must use the enum variants `Entity` or `Relation`
(`entity`/`relation` will not deserialize).

---

## Example command adapters

The contract is the predictive proposal adapter payload above. Any adapter can
be implemented by an LLM, ONNX model, simulator, optimizer, or hand-written
baseline, but those implementation details stay outside the ontology kernel.

Built-in LLM adapter (no Python adapter, uses OpenAI/Anthropic/Ollama):

```bash
bin/axiograph ingest predictive-proposals-llm --backend openai --model gpt-4o-mini
```

Environment variables (LLM):

```bash
export PREDICTIVE_PROPOSAL_BACKEND=openai
export PREDICTIVE_PROPOSAL_MODEL=gpt-4o-mini
export OPENAI_API_KEY=...
```

Note: the built-in adapter requires a model name; use `PREDICTIVE_PROPOSAL_MODEL` or the
provider-specific env vars (`OPENAI_MODEL`, `ANTHROPIC_MODEL`, `OLLAMA_MODEL`).

Deterministic ONNX model (learned, no randomness):
`scripts/axiograph_predictive_proposal_plugin_onnx.py`

Environment variables (ONNX):

```bash
export PREDICTIVE_PROPOSAL_MODEL_PATH=models/predictive_proposal_small.onnx
```

Transformer stub (skeleton for PyTorch):
`scripts/axiograph_predictive_proposal_plugin_transformer_stub.py`

Baseline (no ML, deterministic):
`scripts/axiograph_predictive_proposal_plugin_baseline.py`

API-backed model (LLM-based; optional, untrusted). Set `PREDICTIVE_PROPOSAL_BACKEND`
explicitly when you want an API-backed run.
`scripts/axiograph_predictive_proposal_plugin_llm.py`

HTTP backend (any language/runtime). The HTTP server owns transport details; the
predictive proposal adapter owns only this typed payload:

```bash
axiograph ingest predictive-proposal examples/Family.axi \
  --out build/family_proposals.json \
  --proposal-adapter-http http://127.0.0.1:9999/evidence/proposals/predict
```

---

## Integration points

- CLI: `axiograph ingest predictive-proposal`
- CLI (built-in adapter): `axiograph ingest predictive-proposals-llm`
- CLI: `--proposal-adapter-llm`, `--proposal-adapter-plugin`,
  `--proposal-adapter-http`
- REPL: `proposal` subcommand (`proposal use llm` / `proposal use http <url>` / `proposal use command ...`).
- Server: `POST /evidence/proposals/predict`,
  `POST /planning/proposal-rollout`

Only the entrypoints above are documented public surfaces for predictive
proposal adapters and bounded proposal rollout examples.

When these entrypoints start from a live PathDB snapshot, they first export the
selected canonical module and attach typed lineage anchors
(`axi_digest_v1`, `pathdb_snapshot_id`, `accepted_snapshot_id`) before invoking
the predictive proposal adapter. Reversible `PathDBExportV1` snapshots are not
part of the adapter request contract.

All outputs remain **evidence-plane** until validated and promoted.
If a predictive proposal adapter uses embeddings or vector retrieval internally,
those retrieval results remain sidecar evidence. They may support proposals, but
they do not become canonical relationships without typed validation, CQ/trust
review, and semantic VCS promotion. See
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.
