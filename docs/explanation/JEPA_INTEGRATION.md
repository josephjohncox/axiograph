# JEPA and Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

## What is a JEPA (quick refresher)

A Joint-Embedding Predictive Architecture (JEPA) learns representations by
predicting the *latent embedding* of a target block from a context block,
rather than reconstructing raw pixels/tokens. The Image-based JEPA (I-JEPA)
uses a context block from an image to predict the representations of multiple
masked target blocks, and emphasizes that the masking strategy (large target
blocks + informative context) is crucial for learning semantic features.

In the broader JEPA framing, the model learns an encoder for percepts and a
predictor that forecasts the representation of the next percept from the
current one (optionally conditioned on an action), serving as a predictive
world model trained via self-supervision.

JEPA-style training avoids pixel-level reconstruction and can emphasize more
stable, "slow" features. However, JEPA methods can struggle when distractor
noise is *fixed* across time, highlighting a limitation to watch for in
practice.

## Why JEPA fits Axiograph's architecture

Axiograph already separates:
- **Accepted plane** (canonical `.axi` meaning plane),
- **Evidence plane** (proposals/chunks), and
- **Derived PathDB** (`.axpd`) optimized for query.

JEPA is a natural *untrusted* learner for the evidence plane:
- It can predict missing or future structure without committing to symbolic
  correctness.
- Its outputs can be turned into **candidate facts/relations** in
  `proposals.json` (evidence plane), which remain reviewable and replaceable.
- High-value inferences still require **certificates** checked by Lean.

This is aligned with Axiograph's "untrusted engine, trusted checker" design:
JEPA can propose; certificates validate.

## Axiograph as grounded + provable substrate

JEPA needs stable, structured context/target pairs. Axiograph provides:
- **Grounded state**: accepted-plane anchors (snapshot ids) + evidence overlays.
- **Typed structure**: schema-scoped facts and relations, making targets
  explicit and auditable.
- **Modal context**: world/context scopes are first-class, not hidden filters.
- **Dependent typing**: schema/theory constraints are explicit and checkable.
- **Provable checks**: Lean certificates for constraints, rewrite rules, and
  canonical semantics.

This enables a loop where JEPA learns from grounded snapshots and emits
predictions back into the evidence plane, while promotion into the accepted
plane remains certificate-checked.

## Mapping JEPA concepts onto Axiograph

### Context block
Axiograph contexts can be built from:
- A PathDB subgraph around a query anchor (entities + relations + confidence),
- Context/world metadata (provenance, source, time), and
- Nearby DocChunks (text evidence).

### Target block
Targets can be:
- Masked fact nodes (reified n-ary tuples),
- Masked relations (edge types + endpoints),
- Future snapshot deltas (time-evolution), or
- Missing attributes in schema-scoped instances.

### Predictor output
Instead of raw facts, the JEPA predictor outputs **embeddings**:
- Use nearest-neighbor to propose likely relation/attribute values.
- Store predicted candidates as evidence-plane proposals with confidence.
- Optionally store embeddings as snapshot-scoped sidecars for fast retrieval.

## Practical training setups (Axiograph-specific)

1) **Masked fact prediction**
- Sample a subgraph context.
- Mask a set of fact nodes or relation edges.
- Predict embeddings of the masked items.

2) **Cross-context prediction**
- Use one context/world as input (ObservedSensors).
- Predict embeddings in another context/world (Simulation or Literature).
- Helps discover alignment gaps and reconciliation candidates.

3) **Snapshot delta prediction**
- Context = snapshot N.
- Target = snapshot N+1 delta (added/changed facts).
- Useful for forecasting or "what changed" priors.

## Where it plugs in (current codebase)

- **Evidence plane:** `proposals.json` can store JEPA-generated candidates.
- **DocChunks:** existing chunk overlays enable embedding-grounded JEPA inputs.
- **Optional embeddings:** `axiograph db accept pathdb-embed` already stores
  snapshot-scoped embeddings for retrieval.
- **LLM sync / discovery:** JEPA can act as a fast, non-LLM candidate generator
  upstream of reconciliation and promotion.

## Current CLI / server hooks

- **Training export:** `axiograph discover jepa-export ...` (from full `.axi` modules).
- **World model proposals:** `axiograph ingest world-model ...` (emits `proposals.json`
  with provenance) or the built-in LLM plugin `axiograph ingest world-model-plugin-llm`.
- **REPL:** `wm` subcommand (configure backend, emit proposals, optional WAL commit).
- **DB server:** `POST /world_model/propose` (evidence plane; optional WAL commit).

## Integration with knowledge discovery + tooling

- **Knowledge discovery loop:** JEPA outputs are just another evidence stream
  (like web/RDF/LLM ingestion) and feed the same validate -> reconcile -> promote
  pipeline.
- **Dependent types + theory:** training/export uses full `.axi` modules, so
  schema/theory constraints and rewrite rules are part of the model context.
- **Modal scoping:** contexts/worlds are explicit fields, not hidden filters.
- **Verification:** promotion-time checks and certificates are the gate for
  high-value inferences.
- **LLM integration:** LLMs can request JEPA proposals or combine them with
  tool-loop suggestions; both are untrusted and must pass guardrails.

## Guardrails and limitations

- JEPA outputs are **not** certified; treat them as untrusted evidence.
- The slow-feature bias can miss "static noise" confounders; use
  careful masking and context selection for stability.
- Always map predictions into the Axiograph evidence plane first, then
  reconcile/promote with quality checks and certificates.
- Treat JEPA outputs as *hypotheses* that must pass guardrail costs and
  constraint checks before promotion.

## Self-supervised loop (iterative)

1) Export grounded training pairs from snapshot anchors.
2) Train JEPA to predict masked targets from context.
3) Emit top-k predictions into the evidence plane (proposals).
4) Reconcile/promote with constraints + certificates.
5) Repeat on the new accepted snapshot.

## JEPA plan (architectural, Axiograph-specific)

**1) Data/anchor layer**
- Define a canonical training export from *full* `.axi` modules (schema + theory
  + instance) plus context/world metadata, not just a PathDB export.
- Use accepted-plane anchors (snapshot ids) for reproducibility; PathDB exports
  are derived and optional convenience views.
- Include negative samples (distractors) to reduce trivial shortcuts.

**2) Model layer**
- Graph encoder (GNN/Transformer) for subgraph context.
- Text encoder for DocChunks (can reuse existing embedding pipeline).
- Predictor head trained to match target embeddings (JEPA loss).
- Targets: masked fact nodes, masked relations, or snapshot deltas.

**3) Evidence-plane integration**
- Convert top-k predictions into `Relation` proposals (evidence plane).
- Store prediction embeddings as a snapshot-scoped sidecar for retrieval.
- Attach confidence + provenance metadata to each proposal.

**4) Guardrails/evaluation layer**
- Evaluate precision/recall on held-out facts + reconciliation acceptance rate.
- Track constraint violations (axi_constraints_ok_v1) and rejection causes.
- Measure drift across contexts/snapshots (when JEPA predicts cross-context).

**5) Runtime surfaces**
- CLI entrypoint to generate JEPA proposals for a snapshot.
- Optional server endpoint to request JEPA-assisted candidates.
- Keep all outputs in the evidence plane; do not bypass certificates.

## Suggested next doc links

- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md`
- `docs/explanation/ARCHITECTURE.md`
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md`
- `docs/reference/CERTIFICATES.md`

## References (external)

- I-JEPA: "Self-Supervised Learning from Images with a Joint-Embedding Predictive Architecture" (arXiv:2301.08243)
- JEPA (slow features): "Joint Embedding Predictive Architectures for Learning the Low-Dimensional Structure of Data" (arXiv:2206.00496)
- "Self-Supervised Learning and World Models" lecture notes (Columbia University, 2023)
