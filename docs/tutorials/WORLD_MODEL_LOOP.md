# World Model Loop (JEPA + Guardrails + Promotion)

**Diataxis:** Tutorial  
**Audience:** users and contributors

This tutorial shows how to:
1) export JEPA training pairs from a full `.axi` module,  
2) run a world model (real model preferred),  
3) emit proposals with guardrails, and  
4) validate/commit/promote.

We'll use the small `examples/Family.axi` dataset.

---

## 0) Build the binaries

```bash
make binaries
```

---

## 1) Export JEPA training pairs (generic masked-tuple)

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

---

## 2) Explicit relation masks (endpoint-focused)

Sometimes you want to always mask a specific field (e.g., `parent`).
You can post-process the export (or implement this directly in your plugin).

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

## 3) Run a real world model (LLM-backed; no Python)

By default, the demos use the **built-in** world model plugin
(`axiograph ingest world-model-plugin-llm`). It supports:

- **OpenAI** (default when `WORLD_MODEL_BACKEND` is unset),
- **Anthropic**, or
- **Ollama** (local).

Select the backend with environment variables before running the demo.

```bash
# OpenAI (default)
export WORLD_MODEL_BACKEND=openai
export OPENAI_API_KEY=...
export WORLD_MODEL_MODEL=gpt-4o-mini

bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals.json \
  --world-model-llm \
  --world-model-model "$WORLD_MODEL_MODEL"
```

The output is **evidence-plane** `proposals.json`, with provenance metadata
describing the world model and guardrail costs.

---

## 4) Run a deterministic ONNX world model (offline)

Use this for offline, deterministic runs (no network, no LLM calls).

```bash
export WORLD_MODEL_BACKEND=onnx
export WORLD_MODEL_MODEL_PATH=models/world_model_small.onnx
./scripts/setup_onnx_runtime.sh
source .venv-onnx/bin/activate
./scripts/build_world_model_onnx.py --out "$WORLD_MODEL_MODEL_PATH"

bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals_onnx.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_onnx.py \
  --world-model-model onnx_v1
```

---

## 5) Run a transformer-style world model (stub)

The transformer stub is a skeleton that shows how to wire a PyTorch model.
Swap in your own checkpoint or training loop.

```bash
bin/axiograph ingest world-model \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals_transformer.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_transformer_stub.py \
  --world-model-model transformer_v1
```

---

## 6) Python plugin (optional; legacy)

If you still want a Python-backed LLM proposer, use:
`scripts/axiograph_world_model_plugin_real.py`. The built-in plugin is now the
default for demos.

---

## 7) Validate proposals (guardrails + constraints)

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

## 8) Use the REPL / server loop

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
  -d '{"goals":["predict missing parent links"],"max_new_proposals":50}' | jq .
```

---

## 9) MPC plan -> draft .axi -> promote

Use the MPC plan endpoint to generate multi-step proposals, then draft and
promote a canonical module.

Plan (REPL example):

```text
axiograph> wm plan build/wm_plan.json --steps 2 --rollouts 2 --goal "predict missing parent links" --axi examples/Family.axi --cq "has_parent=select ?p where ?p is Person limit 1"
```

Merge plan proposals into one `proposals.json`:

```bash
python - <<'PY'
import json, time
report = json.load(open("build/wm_plan.json"))
proposals = []
for step in report.get("steps", []):
    proposals.extend(step["proposals"]["proposals"])
out = {
    "version": 1,
    "generated_at": str(int(time.time())),
    "source": {"source_type": "world_model_plan", "locator": report.get("trace_id", "wm_plan")},
    "schema_hint": None,
    "proposals": proposals,
}
json.dump(out, open("build/wm_plan_proposals.json", "w"), indent=2)
print("wrote build/wm_plan_proposals.json")
PY
```

Draft a canonical module:

```bash
bin/axiograph discover draft-module \
  --proposals build/wm_plan_proposals.json \
  --out build/wm_plan_draft.axi \
  --module FamilyWM \
  --schema Fam \
  --instance WMPlan \
  --infer-constraints
```

## 10) Promotion (accepted plane)

Once proposals pass guardrails and review, promote into the accepted plane.
See `docs/howto/ACCEPTED_PLANE.md` for the full workflow.

---

## 11) Physics-scale demo (larger corpus)

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

# If you see `... is Physics.Entity` in CQs, skip it by default:
#   (Entity is a synthetic fallback in some exports)
# bin/axiograph discover competency-questions build/physics_base.axpd --out build/physics_cq.json
# To include it explicitly:
#   --include-entity
```

Translate natural-language CQs to AxQL (LLM backend required):

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
  --export build/family_jepa.json \
  --out build/family_proposals.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --guardrail-weight quality_error=20 \
  --task-cost latency=3.2:0.5:ms \
  --horizon-steps 4
```

- Add MPC rollouts: `axiograph tools perf world-model ...`
- Use server MPC with auto-commit: `POST /world_model/plan` with `auto_commit=true`.
- Use guardrail weights + task costs to shape the objective.
- Add domain-specific theory constraints and see how they influence costs.
