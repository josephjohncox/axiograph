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
   - JEPA-style latent prediction or contrastive objectives.
   - Multi-step rollouts for world-model training.

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

## World model training (multi-step, recurrent)

The world model predicts *trajectories* of future states. Training uses
multi-step rollouts where the model is applied repeatedly and the loss is
accumulated across steps. This aligns with objective-driven planning and MPC.

## Guardrails and evaluation

- **Constraint checks**: use `axi_constraints_ok_v1` and typechecks as
  automatic guardrail metrics.
- **Rewrite consistency**: penalize predictions that violate rewrite rules.
- **Certificate fitness**: track how often predictions can be certified.

## Implementation hooks in the codebase

- **Snapshot anchors**: accepted-plane snapshot ids (stable training inputs) and
  optional `axiograph db pathdb export-axi` for derived convenience views.
- **Training export**: `axiograph discover jepa-export` (canonical full `.axi` -> training pairs).
- **World model proposals**: `axiograph ingest world-model` (evidence-plane `proposals.json`
  with provenance) or the built-in LLM plugin `axiograph ingest world-model-plugin-llm`,
  plus REPL `wm` and server `POST /world_model/propose`.
- **Evidence plane**: `proposals.json` ingestion + WAL overlays.
- **DocChunks**: existing chunk overlays for textual grounding.
- **Certificates**: Lean checker for promotion-time validation.

## Related docs

- `docs/explanation/JEPA_INTEGRATION.md`
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md`
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md`
- `docs/reference/CERTIFICATES.md`
