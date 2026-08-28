# Bounded Proposal Rollout

**Diataxis:** Tutorial
**Audience:** users and contributors

This tutorial shows the current proposal-generation path:

1. run a predictive proposal adapter from a canonical `.axi` module,
2. optionally attach masked-tuple training metadata,
3. run bounded proposal rollouts with guardrail costs, and
4. review, validate, commit, and promote.

The core workflow makes no autonomous-execution claim. The adapter may be an
LLM, deterministic command adapter, HTTP service, or research model; Axiograph
treats every output as evidence-plane proposals until typed review accepts it.

We'll use the small `examples/Family.axi` dataset.

---

## 0) Build the binaries

```bash
make binaries
```

---

## 1) Run directly from canonical `.axi`

The predictive proposal request is grounded in a canonical `.axi` module. When
the caller must supply the exact canonical module bytes from the accepted
AxiStore closure. Typed lineage (`revision_digest_v2`, optional
`materialization_id`, and `accepted_snapshot_id`) accompanies those same bytes;
it is never reconstructed from PathDB rows.

```bash
bash scripts/predictive_proposal_demo.sh
```

The script defaults to the deterministic baseline adapter when configured by
the local runtime. Set `PREDICTIVE_PROPOSAL_BACKEND=openai`, `anthropic`, or `ollama`
when you want an API/local-model run. These provider names configure adapter
execution only; they do not change the ontology kernel or promote model output
out of the evidence plane.

The output is evidence-plane `proposals.json`, with provenance metadata
describing the predictive proposal adapter and guardrail costs.

---

## 2) Optional masked-tuple training export

Generate a derived training view when you want to inspect or persist the masked
targets passed to an adapter:

```bash
bin/axiograph discover training-export examples/Family.axi \
  --out build/family_training_export.json \
  --mask-fields 1
```

This export includes full schema, theory, instance, and typed masked targets. It
is anchored to the exact-byte `revision_digest_v2`.

Grounding rules:

- Use full `.axi` modules as training input: schema, theory, instance, contexts,
  and rewrite rules.
- PathDB `.axpd` images are derived query substrates, not canonical training
  truth.
- Training export metadata is optional derived context. It can help an adapter,
  but it does not replace the canonical `.axi` request input.

---

## 3) Run an API/local predictive proposal adapter

For API/local-model runs, use the built-in LLM-backed adapter. It supports
OpenAI, Anthropic, and Ollama.

```bash
export PREDICTIVE_PROPOSAL_BACKEND=openai
export OPENAI_API_KEY=...
export PREDICTIVE_PROPOSAL_MODEL=gpt-4o-mini

bin/axiograph ingest predictive-proposal examples/Family.axi \
  --out build/family_proposals.json \
  --proposal-adapter-llm \
  --proposal-adapter-model "$PREDICTIVE_PROPOSAL_MODEL"
```

The adapter input remains exact canonical `.axi` bytes plus typed optional
layers. Derived SQLite materializations are not semantic request contracts.

---

## 4) Run a command adapter

Command adapters are for offline experiments, integration debugging, or
optional research adapters. They read `axiograph_predictive_proposal_v1` JSON from
stdin and write the same protocol response to stdout.

```bash
bin/axiograph ingest predictive-proposal examples/Family.axi \
  --out build/family_proposals_baseline.json \
  --proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py \
  --proposal-adapter-plugin-arg=--strategy \
  --proposal-adapter-plugin-arg=oracle \
  --proposal-adapter-model baseline_oracle
```

---

## 5) Validate proposals

Run guardrails and constraints against the accepted module and proposal overlay:

```bash
bin/axiograph check quality examples/Family.axi --profile fast --plane both
```

Keep `build/family_proposals.json` in the evidence plane. Review and validate
its typed deltas against exact accepted `.axi`; only a reviewed canonical
change may be promoted. Do not commit proposal rows directly into PathDB.

---

## 6) Use REPL and server surfaces

REPL:

```text
axiograph> proposal use llm
axiograph> proposal propose build/predictive_proposals.json --goal "predict missing parent links"
```

Note: `proposal use llm` reads `PREDICTIVE_PROPOSAL_BACKEND` and
`PREDICTIVE_PROPOSAL_MODEL` (or provider model environment variables). The REPL
`proposal` command is the current interactive surface for predictive proposal
adapters.

Server proposal endpoint:

```bash
curl -sS -X POST http://127.0.0.1:7878/evidence/proposals/predict \
  -H 'Content-Type: application/json' \
  -d '{"goals":["predict missing parent links"],"max_new_proposals":50}'
```

Pipe curl output to `jq` or another JSON viewer only when you want pretty
printing.

---

## 7) Bounded rollout -> draft `.axi` -> promote

Use the bounded proposal rollout endpoint or REPL command to generate multi-step
proposal reports. These are bounded search/evaluation passes over proposal
candidates, not autonomous execution loops.

REPL example:

```text
axiograph> proposal plan build/bounded_proposal_rollout_plan.json --steps 2 --rollouts 2 --goal "predict missing parent links" --axi examples/Family.axi --cq-file examples/competency_questions/family_parent.cq --commit-dir build/bounded_proposal_rollout_commits --message "bounded proposal rollout: parent links"
```

Server example:

```bash
curl -sS -X POST http://127.0.0.1:7878/planning/proposal-rollout \
  -H 'Content-Type: application/json' \
  -d '{"horizon_steps":3,"rollouts":2,"max_new_proposals":50,"goals":["fill missing parent links"]}'
```

Draft a canonical module from a proposal report:

```bash
bin/axiograph discover draft-module \
  --proposals build/predictive_proposals.json \
  --out build/bounded_proposal_rollout_draft.axi \
  --module FamilyProposalDraft \
  --schema Fam \
  --instance ProposalDraft \
  --infer-constraints
```

Once proposals pass guardrails and review, construct and promote an AxiStore
`PromotionPlan`. See `docs/howto/CANONICAL_SEMANTIC_SPINE.md` and
`docs/howto/SNAPSHOT_STORE.md` for accepted-state promotion and derived SQLite
materializations.

---

## 8) Physics-scale demo

The physics examples include differential geometry, mechanics, QFT, and
algebra. This flow uses:

- `examples/physics/PhysicsOntology.axi`
- `examples/physics/PhysicsMeasurements.axi`

REPL script:

```bash
export PREDICTIVE_PROPOSAL_BACKEND=anthropic
export ANTHROPIC_API_KEY=...
export PREDICTIVE_PROPOSAL_MODEL=claude-3-5-sonnet-20240620
./scripts/physics_bounded_proposal_rollout_repl_demo.sh
```

Generate schema-driven competency questions from exact canonical `.axi`:

```bash
bin/axiograph discover competency-questions \
  examples/physics/PhysicsOntology.axi \
  --out build/physics_cq.json \
  --max-questions 120
```

For reviewed, human-authored CQs, prefer `.cq` files such as
`examples/competency_questions/physics.cq`. They load through the same typed
`CompetencyQuestionV1` path without asking users to write JSON or AxQL by hand.

---

## Next steps

Try guardrail weight overrides and task costs:

```bash
bin/axiograph ingest predictive-proposal examples/Family.axi \
  --out build/family_proposals.json \
  --proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py \
  --guardrail-weight quality_error=20 \
  --task-cost latency=3.2:0.5:ms \
  --horizon-steps 4
```

Add bounded rollout profiling:

```bash
bin/axiograph tools perf proposal-rollout \
  --input examples/Family.axi \
  --proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py \
  --horizon-steps 3 \
  --rollouts 2 \
  --out-json build/predictive_proposal_perf.json
```

Use server planning with auto-commit through `POST /planning/proposal-rollout`
with `auto_commit=true`.
