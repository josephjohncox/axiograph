# Self-Supervised Learning with Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

## Why Axiograph is a good SSL substrate

Axiograph provides a *grounded* and *provable* knowledge substrate:
- **Grounded snapshots**: accepted-plane anchors (snapshot ids) define stable
  states for training and evaluation.
- **Typed structure**: schema-scoped facts/relations allow explicit targets.
- **Provable checks**: Lean certificates let us measure constraint violations
  and rule-preservation on model outputs.

This makes Axiograph a natural backbone for self-supervised learning (SSL)
while keeping outputs auditable and reversible.

## Core SSL loop (iterative)

Iteration checklist (minimal loop):

```
export -> train -> propose -> reconcile -> promote -> retrain
```

1) **Export training pairs**
   - Sample (context, target) pairs from a snapshot anchor.
   - Prefer *full* `.axi` modules (schema + theory + instance) plus context
     metadata; PathDB exports are derived convenience views.
   - Context: subgraph + context metadata + DocChunks.
   - Target: masked facts/edges/attributes or snapshot deltas.

2) **Train model**
   - JEPA-style latent prediction or contrastive objectives in research
     adapters.
   - Multi-step rollouts for predictive proposal training/evaluation.

3) **Emit evidence**
   - Convert predictions into `proposals.json` (evidence plane).
   - Attach confidence + provenance metadata.

4) **Reconcile + certify**
   - Run quality checks + constraints.
   - Promote to accepted plane only when consistent and certified.

5) **Repeat**
   - New snapshot anchor becomes the next training corpus.

## Self-supervised objectives (Axiograph-friendly)

- **Masked fact prediction**
  - Mask a reified fact node and predict its embedding or fields.
- **Constraint-aware prediction**
  - Use schema/theory constraints as auxiliary losses or guardrail costs.
- **Masked relation prediction**
  - Mask relation type or endpoint, predict its embedding.
- **Attribute completion**
  - Mask attribute values in schema-scoped instances.
- **Temporal delta prediction**
  - Predict snapshot N+1 deltas from snapshot N.
- **Cross-context prediction**
  - Predict facts in another context/world (Observed vs Simulation).

## Research note: multi-step predictive models

Some research adapters may predict sequences of future representation states.
Training can use multi-step rollouts where a model is applied repeatedly and the
loss is accumulated across steps. In Axiograph's general ontology-engineering
demos, the core runtime surface is named bounded proposal rollout: an
evidence-plane proposal search/evaluation loop.

Do not treat that runtime surface as a native JEPA, world-model, MPC, or
control-system claim. Those semantics belong to external adapters or
deployments that supply explicit dynamics, objectives, constraints, and
receding-horizon execution outside the trusted Axiograph boundary.

## Guardrails and evaluation

- **Constraint checks**: use `axi_constraints_ok_v1` and typechecks as
  automatic guardrail metrics.
- **Rewrite consistency**: penalize predictions that violate rewrite rules.
- **Certificate fitness**: track how often predictions can be certified.

## Implementation hooks in the codebase

- **Snapshot anchors**: accepted-plane snapshot ids (stable training inputs) and
  canonical `.axi` digests for derived training views.
- **Training export**: `axiograph discover training-export`
  (canonical full `.axi` -> training pairs).
- **Predictive proposals**: `axiograph ingest predictive-proposal`
  (evidence-plane `proposals.json` with provenance) or the built-in LLM adapter
  `axiograph ingest predictive-proposals-llm`, plus REPL `proposal` and server
  `POST /evidence/proposals/predict`.
- **Bounded rollout**: REPL `proposal plan` and server
  `POST /planning/proposal-rollout` produce bounded proposal reports, not
  native MPC/control execution.
- **Evidence plane**: `proposals.json` ingestion + WAL overlays.
- **DocChunks**: existing chunk overlays for textual grounding.
- **Certificates**: Lean checker for promotion-time validation.

`PathDBExportV1` is not a training or semantic interchange surface. Keep it for
debug/live-byte/parser parity only.

## Related docs

- `docs/explanation/JEPA_INTEGRATION.md`
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md`
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md`
- `docs/reference/CERTIFICATES.md`
