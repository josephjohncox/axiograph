# Objective-Driven Research Adapters

**Diataxis:** Explanation  
**Audience:** contributors

## Boundary

Status note: this is an advanced/research interpretation. Axiograph's shipped
runtime surface is a predictive proposal adapter plus bounded proposal rollout.
The core does **not** claim native JEPA, MPC, world-model, control-system, or
autonomous-agent semantics. Those terms may describe external adapters or
experiments that emit evidence-plane proposals through the documented adapter
interfaces.

LeCun's "objective-driven AI" framework proposes a modular architecture where
an agent uses a learned world model and a cost module to plan actions that
minimize a total objective. The world model predicts future states from imagined
action sequences; a cost module evaluates predicted trajectories; and planning
is performed via model-predictive control (MPC) with receding horizon.
The cost module combines an immutable intrinsic cost with a trainable critic
that predicts future intrinsic cost.

Axiograph already separates trusted meaning (accepted plane) from untrusted
inference (evidence plane) and has explicit guardrails (quality checks,
constraints, certificates). That makes it a useful host for objective-driven
research adapters, but not the owner of their dynamics: guardrail costs and
task costs can guide proposal search while all outputs stay auditable evidence
until typed review accepts them.

## External-adapter mapping

LeCun's architecture (perception, learned predictive model, actor, cost module,
memory) can be mapped onto Axiograph's adapter boundary:

- **Perception** -> canonical `.axi` plus optional accepted/evidence lineage
  metadata supplied to an adapter
- **Learned predictive model** -> JEPA/H-JEPA-style or other predictive model
  owned by the adapter, not by the Axiograph kernel
- **Actor/planner** -> external proposal search or optimization that chooses
  candidate proposal sequences
- **Cost module** -> guardrail costs and task costs supplied as adapter context
- **Short-term memory** -> adapter-local cache or bounded rollout report state

If a learned model is multi-step/recurrent, that is adapter behavior. Axiograph
records bounded proposal search/evaluation reports; it does not provide a
native controller, actuator, or receding-horizon execution semantics.

## Guardrail costs vs task costs

**Guardrail costs (immutable):**

- Derived from certified constraints (e.g., key/functional violations,
  rewrite consistency, schema typing), and from safety policies.
- In Axiograph, we map these to the **immutable intrinsic cost** term in
  LeCun's architecture (interpretation: intrinsic cost = hard guardrail
  objective).
- Applied to *every predicted state* in a rollout; the total cost is a sum over
  time steps (trajectory-level guardrail enforcement).
- These costs must be explainable and, when required, certificate-checked.

**Task costs (configurable):**

- Encode goal-directed behavior for a specific objective (e.g., "maximize recall
  of relevant facts", "minimize reconciliation conflicts", "avoid low-confidence
  merges", "achieve target coverage for schema X").
- Can be swapped or reweighted by a "configurator" (task policy) without
  modifying guardrail terms (LeCun's configurator configures modules for the
  task at hand).
- Competency-question coverage is a natural task cost: penalize states where
  required typed CQs return too few answers or remain unresolved after lowering
  (drives finite, declared-scope completeness).

## Learned predictive model (multi-step, recurrent)

A research adapter may predict a *sequence* of future states from a sequence of
proposed actions; this is explicitly framed as recursive prediction over a
trajectory with cost summed across time. In Axiograph core, those predictions
are still proposal candidates, not accepted semantic state transitions.

In an Axiograph research-adapter interpretation:

- **State**: (accepted `.axi` snapshot id, schema/theory + instance, evidence
  overlays, context filters)
- **Action**: reconciliation choice, promotion decision, ingest/merge decision,
  schema evolution step, or query-driven expansion step
- **Transition**: predicted snapshot delta (facts added/removed/relinked) +
  provenance

State should be grounded in *full* `.axi` modules (schema + theory + instance),
including modal context scopes and dependent-type constraints. Derived PathDB
rows and `.axpd` images are query acceleration, not a training or semantic
interchange surface.

A JEPA/H-JEPA-style adapter fits here: it predicts future *representations* of
snapshots rather than raw facts, then a decoder/nearest-neighbor step turns
those predictions into candidate facts in the evidence plane.

## Self-supervised predictive training

The predictive adapter can be trained with self-supervised objectives using
snapshot anchors as ground truth: mask facts/relations, predict embeddings, and
roll forward over multiple steps. Outputs are still untrusted until they pass
guardrail checks and certificate validation.

## MPC/control interpretation

Axiograph's current public surfaces are:

- `axiograph discover training-export`
- `axiograph ingest predictive-proposal`
- `axiograph ingest predictive-proposals-llm`
- REPL `proposal`
- `POST /evidence/proposals/predict`
- `POST /planning/proposal-rollout`

Those surfaces can host adapter-generated proposal rollouts. Do not describe
them as MPC or control unless a deployment supplies explicit dynamics,
receding-horizon execution, control objectives, and an external actuator
contract beyond Axiograph's proposal workflow.

Research loop:

```
canonical .axi -> training export -> external adapter training
        ^                               |
        |                               v
promote <--- reconcile <--- propose <--- adapter prediction
   ^                                       |
   |                                       v
   +----- bounded proposal rollout reports <---- guardrail + task costs
```

## Hierarchical planning (skills + options)

Objective-driven systems usually need *hierarchical* planning to handle long
horizons. Instead of planning over primitive actions only, the planner composes
**skills/options** (macro-actions) that operate over multiple steps.

In Axiograph terms:

- **Primitive actions**: promote/reject/merge/rewrite/annotate, single-step.
- **Skills/options**: multi-step routines like "normalize a theory module",
  "reconcile schema X with snapshot Y", or "bootstrap a candidate ontology".

The planner can operate at multiple levels:

- High-level planner chooses a sequence of skills (coarse horizon).
- Low-level planner executes or refines each skill (fine horizon).

Costs propagate across levels:

- Guardrail costs apply at all levels (skills must respect constraints).
- Task costs can be defined per-skill or per-trajectory segment.

This gives two advantages:

1) Search becomes tractable (smaller branching factor at the high level).
2) Plans are more interpretable (skills map to auditable workflows).

## Implementation sketch for research adapters

### 1) State + action interfaces

- Define a `SemanticState` abstraction: accepted snapshot id + full `.axi` module
  view + context + evidence overlays.
- Define `Action` primitives: merge, split, promote, reject, rewrite, annotate.
- Define `Transition` outputs: predicted deltas + confidence.

### 2) Predictive proposal adapter service

- Pluggable predictive proposal adapter interface (optionally backed by a
  JEPA-style latent predictor).
- Rollout API: `rollout(state, actions, horizon) -> {proposal trajectories}`
- Optional uncertainty sampling: multiple trajectories from latent variables.

### 3) Cost module API

- `GuardrailCost`: immutable cost terms (hard constraints, policy rules).
- `TaskCost`: configurable cost terms (objective-specific scoring).
- `TotalCost = sum_t (GuardrailCost(s_t) + TaskCost(s_t))`.

### 4) Bounded proposal rollout engine

- Bounded search/evaluation over proposal candidates.
- Do not call this MPC unless a deployment supplies real dynamics,
  receding-horizon execution, and control objectives.

### 5) Evidence-plane integration

- Convert plan outputs into `proposals.json` entries with provenance.
- Use reconciliation + quality checks + certificates for acceptance.

## How this aligns with Axiograph's trust boundary

- Predictive proposal adapter outputs remain **untrusted** and land in the
  evidence plane.
- Guardrail costs can be certified and enforced at promotion time.
- High-value steps can emit certificates: "this plan step preserves constraint
  X" or "this transition respects rewrite rules".
- Hierarchical modeling is explicit: schema/world/context layers become
  separate model scopes, and each scope emits proposals that are typed and
  constrained by the `.axi` theory for that scope.
- Ingest flow stays coherent: predictive proposals are just another evidence
  stream (`proposals.json`) that enters the same validate -> reconcile -> promote
  pipeline as doc/web/RDF ingestion.

### LLM and tool-loop integration

LLMs remain an untrusted boundary, just like predictive proposal adapters:

- LLMs can request predictive proposals via the server endpoint, then review or
  refine them using the same guardrail reports and constraint checks.
- LLM-generated proposals and adapter-generated proposals both land in the
  evidence plane and are reconciled/promoted under identical rules.
- A research planner can treat LLM suggestions as candidate actions while still
  scoring them via guardrail + task costs.

## Related docs

- `docs/explanation/JEPA_INTEGRATION.md`
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md`
- `docs/reference/CERTIFICATES.md`

## References (external)

- Yann LeCun, "A Path Toward Autonomous Machine Intelligence" (OpenReview, 2022)
