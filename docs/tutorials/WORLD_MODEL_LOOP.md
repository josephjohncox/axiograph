# World Model Loop (Canonical `.axi` + Optional JEPA + Guardrails + Promotion)

**Diataxis:** Tutorial  
**Audience:** users and contributors

This tutorial shows how to:
1) run a world model directly from a canonical `.axi` module,
2) optionally attach JEPA/training export metadata,
3) emit proposals with guardrails, and  
4) validate/commit/promote.

We'll use the small `examples/Family.axi` dataset.

---

## 0) Build the binaries

```bash
make binaries
```

---

## 1) Run directly from canonical `.axi`

The world-model request contract is grounded in the canonical `.axi` module.
If you point `axiograph ingest world-model` at a canonical module, Axiograph
passes that text and digest as the primary semantic input and can derive a
JEPA/training export sidecar automatically when the backend benefits from it.
When the flow starts from a live snapshot instead of a file, Axiograph first
exports the selected canonical module and carries typed lineage anchors
(`axi_digest_v1`, `pathdb_snapshot_id`, `accepted_snapshot_id`) alongside that
same module text.

```bash
# OpenAI (default)
export WORLD_MODEL_BACKEND=openai
export OPENAI_API_KEY=...
export WORLD_MODEL_MODEL=gpt-4o-mini

bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals.json \
  --world-model-llm \
  --world-model-model "$WORLD_MODEL_MODEL"
```

The output is **evidence-plane** `proposals.json`, with provenance metadata
describing the world model and guardrail costs.

---

## 2) Optional JEPA training export (generic masked-tuple)

Generic mask strategy: choose a fixed number of fields per tuple.

```bash
bin/axiograph discover jepa-export examples/Family.axi \
  --out build/family_jepa.json \
  --mask-fields 1
```

This export includes **full schema + theory + instance**, plus a list of masked
targets. It is anchored to `axi_digest_v1`.

Grounding note:
- Use full `.axi` modules as training input (schema + theory + instance + contexts + rewrite rules).
- PathDB `.axpd` exports are derived for query performance, not canonical training truth.
- This JEPA export is optional derived metadata. It should help a model, not
  replace the canonical `.axi` request input.

---

## 3) Explicit relation masks (endpoint-focused)

Sometimes you want to always mask a specific field (e.g., `parent`).
You can post-process the export, or implement the same masking policy inside a
model runner. The JSON shape is illustrative; it is not a separate adapter
protocol.

Example JSON snippet (explicit mask list):

```json
{
  "schema": "Fam",
  "instance": "TinyFamily",
  "relation": "Parent",
  "fields": [["child","Carol"],["parent","Alice"],["ctx","CensusData"],["time","T2020"]],
  "mask_fields": ["parent"]
}
```

This is the **explicit mask** strategy; the generic approach is just
`--mask-fields N`.

---

## 4) Run a real world model with an explicit training sidecar (optional)

By default, the demos use the **built-in** world model runner
(`axiograph ingest world-model-plugin-llm`). It supports:

- **OpenAI** (default when `WORLD_MODEL_BACKEND` is unset),
- **Anthropic**, or
- **Ollama** (local).

Select the backend with environment variables before running the demo. Axiograph
derives any optional JEPA/training sidecar inline from the canonical `.axi`
input; `axiograph discover jepa-export` remains available when you want to
inspect or persist that derived metadata separately.

```bash
# OpenAI (default)
export WORLD_MODEL_BACKEND=openai
export OPENAI_API_KEY=...
export WORLD_MODEL_MODEL=gpt-4o-mini

bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals.json \
  --world-model-llm \
  --world-model-model "$WORLD_MODEL_MODEL"
```

---

## 5) Run a deterministic ONNX world model (offline)

Use this for offline, deterministic runs (no network, no LLM calls).

```bash
export WORLD_MODEL_BACKEND=onnx
export WORLD_MODEL_MODEL_PATH=models/world_model_small.onnx
./scripts/setup_onnx_runtime.sh
source .venv-onnx/bin/activate
./scripts/build_world_model_onnx.py --out "$WORLD_MODEL_MODEL_PATH"

bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals_onnx.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_onnx.py \
  --world-model-model onnx_v1
```

---

## 6) Run a transformer-style world model (stub)

The transformer stub is a skeleton that shows how to wire a PyTorch model.
Swap in your own checkpoint or training loop.

```bash
bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals_transformer.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_transformer_stub.py \
  --world-model-model transformer_v1
```

---

## 7) Command adapters for offline/debug models

The built-in LLM world-model runner is the default demo path. Command adapters
are examples for research, offline model experiments, or integration debugging,
not the core world-model surface. If you specifically need a Python-backed
proposer, use `scripts/axiograph_world_model_plugin_real.py`.

---

## 8) Validate proposals (guardrails + constraints)

```bash
bin/axiograph check quality examples/Family.axi --profile fast --plane both
```

Preview validation (proposal overlay):

```bash
bin/axiograph db accept pathdb-commit \
  --dir build/accepted_plane \
  --accepted-snapshot head \
  --proposals build/family_proposals.json \
  --message "world model: family proposals"
```

---

## 9) Use the REPL / server loop

REPL:

```text
axiograph> wm use llm
axiograph> wm propose build/wm_proposals.json --goal "predict missing parent links"
```

Note: `wm use llm` reads `WORLD_MODEL_BACKEND` + `WORLD_MODEL_MODEL` (or provider
model env vars) from the environment.

Optional ONNX plugin (offline):

```text
axiograph> wm use command scripts/axiograph_world_model_plugin_onnx.py
axiograph> wm propose build/wm_proposals_onnx.json --goal "draft candidate relations"
```

Server:

```bash
curl -sS -X POST http://127.0.0.1:7878/world_model/propose \
  -H 'Content-Type: application/json' \
  -d '{"goals":["predict missing parent links"],"max_new_proposals":50}'
```

Pipe curl output to `jq` or another JSON viewer only when you want pretty
printing.

---

## 10) MPC plan -> draft .axi -> promote

Use the MPC plan endpoint to generate multi-step proposals, then draft and
promote a canonical module.

Plan and commit the merged proposal overlay directly (REPL example):

```text
axiograph> wm plan build/wm_plan.json --steps 2 --rollouts 2 --goal "predict missing parent links" --axi examples/Family.axi --cq "has_parent=select ?p where ?p is Person limit 1" --commit-dir build/wm_plan_commits --message "world-model parent-link plan"
```

Or draft a canonical module from a single proposal report:

```bash
bin/axiograph discover draft-module \
  --proposals build/wm_proposals_onnx.json \
  --out build/wm_plan_draft.axi \
  --module FamilyWM \
  --schema Fam \
  --instance WMPlan \
  --infer-constraints
```

## 11) Promotion (accepted plane)

Once proposals pass guardrails and review, promote into the accepted plane.
See `docs/howto/CANONICAL_SEMANTIC_SPINE.md` and
`docs/howto/SNAPSHOT_STORE.md` for accepted-plane promotion and derived query
snapshots.

---

## 12) Physics-scale demo (larger corpus)

The physics examples include differential geometry, mechanics, QFT, and algebra.
This flow uses:
- `examples/physics/PhysicsOntology.axi`
- `examples/physics/PhysicsMeasurements.axi`

End-to-end script (accepted plane → CQs → MPC plan → promote → viz):

```bash
# OpenAI (default)
export WORLD_MODEL_BACKEND=openai
export OPENAI_API_KEY=...
export WORLD_MODEL_MODEL=gpt-4o-mini
./scripts/world_model_mpc_physics_flow_demo.sh
```

REPL-only script:

```bash
# Anthropic
export WORLD_MODEL_BACKEND=anthropic
export ANTHROPIC_API_KEY=...
export WORLD_MODEL_MODEL=claude-3-5-sonnet-20240620
./scripts/world_model_mpc_physics_repl_demo.sh
```

Server + viz demo (stepwise auto-commit):

```bash
# Ollama (local)
export WORLD_MODEL_BACKEND=ollama
export OLLAMA_HOST=http://127.0.0.1:11434
export WORLD_MODEL_MODEL=llama3.1
./scripts/world_model_mpc_physics_server_demo.sh
```

Offline deterministic ONNX:

```bash
export WORLD_MODEL_BACKEND=onnx
export WORLD_MODEL_MODEL_PATH=models/world_model_small.onnx
./scripts/setup_onnx_runtime.sh
source .venv-onnx/bin/activate
./scripts/build_world_model_onnx.py --out "$WORLD_MODEL_MODEL_PATH"
./scripts/world_model_mpc_physics_flow_demo.sh
```

Generate schema-driven competency questions:

```bash
bin/axiograph discover competency-questions \
  build/physics_base.axpd \
  --out build/physics_cq.json \
  --max-questions 120

# If broad top-level types dominate generated CQs, narrow the run with schema,
# relation, or object filters instead of treating an execution artifact as
# domain meaning.
```

For reviewed, human-authored CQs, prefer `.cq` files such as
`examples/competency_questions/physics.cq`. They load through the same typed
`CompetencyQuestionV1` path without asking users to write JSON or AxQL by hand.
Simple `expect: ...` records lower to executable typed queries; less structured
questions remain explicit unresolved authoring obligations until refined.

Translate natural-language CQ prompts to executable CQs when you want an LLM to
help with the lowering (LLM backend required):

```bash
bin/axiograph discover competency-questions \
  build/physics_base.axpd \
  --from-nl examples/competency_questions/physics_cq_nl.txt \
  --llm-openai --llm-model gpt-4o-mini \
  --out build/physics_cq_structured.json
```

## Next steps

- Try guardrail weight overrides and task costs:

```bash
bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --out build/family_proposals.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --guardrail-weight quality_error=20 \
  --task-cost latency=3.2:0.5:ms \
  --horizon-steps 4
```

- Add MPC rollouts: `axiograph tools perf world-model ...`
- Use server MPC with auto-commit: `POST /world_model/plan` with
  `auto_commit=true`.
- Use guardrail weights + task costs to shape the objective.
- Add domain-specific theory constraints and see how they influence costs.
