# Objective-Driven AI in Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

## Why this belongs in Axiograph

LeCun's "objective-driven AI" framework proposes a modular architecture where
an agent uses a learned world model and a cost module to plan actions that
minimize a total objective. The world model predicts future states from imagined
action sequences; a cost module evaluates predicted trajectories; and planning
is performed via model-predictive control (MPC) with receding horizon.
The cost module combines an immutable intrinsic cost with a trainable critic
that predicts future intrinsic cost.

Axiograph already separates trusted meaning (accepted plane) from untrusted
inference (evidence plane) and has explicit guardrails (quality checks,
constraints, certificates). That makes it a good host for objective-driven
planning: we can treat guardrail costs as immutable terms and task costs as
configurable terms while keeping all results auditable and certifiable.

## Architectural mapping

LeCun's architecture (perception, world model, actor, cost module, memory)
maps naturally to Axiograph:

- **Perception** -> PathDB snapshot state + context metadata (accepted + evidence)
- **World model** -> JEPA/H-JEPA style predictive model over snapshot deltas
- **Actor/Planner** -> action proposal + optimization over trajectories
- **Cost module** -> guardrail costs (immutable) + task costs (configurable)
- **Short-term memory** -> snapshot cache + rollout buffer

The world model is multi-step/recurrent: the same model is applied repeatedly to
predict a trajectory of states, and the total cost sums across time steps. This
is the MPC loop described in the position paper.

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
  key AxQL questions return too few answers (drives completeness).

## World model (multi-step, recurrent)

The world model predicts a *sequence* of future states from a sequence of
proposed actions; this is explicitly framed as recursive prediction over a
trajectory with cost summed across time.

In Axiograph terms:
- **State**: (accepted `.axi` snapshot id, schema/theory + instance, evidence
  overlays, context filters)
- **Action**: reconciliation choice, promotion decision, ingest/merge decision,
  schema evolution step, or query-driven expansion step
- **Transition**: predicted snapshot delta (facts added/removed/relinked) +
  provenance

State should be grounded in *full* `.axi` modules (schema + theory + instance),
including modal context scopes and dependent-type constraints. PathDB exports
remain derived views for query performance, not the canonical training target.

A JEPA/H-JEPA world model fits here: it predicts future *representations* of
snapshots rather than raw facts, then a decoder/nearest-neighbor step turns
those predictions into candidate facts in the evidence plane.

## Self-supervised world model training

The world model can be trained with self-supervised objectives using snapshot
anchors as ground truth: mask facts/relations, predict embeddings, and roll
forward over multiple steps. Outputs are still untrusted until they pass
guardrail checks and certificate validation.

## MPC loop (planning)

At each step (Mode-2 planning in the paper):
1) Build the current state representation.
2) Actor proposes an action sequence.
3) World model rolls out predicted states for the sequence.
4) Cost module evaluates the trajectory (sum of guardrail + task costs).
5) Actor updates the action sequence to reduce cost (gradient-based or search).
6) Execute the first action (receding horizon), then repeat.

This loop mirrors classical MPC, except the world model and cost are learned.
The paper notes that optimization can be gradient-based and can also use
dynamic programming or search when action spaces are discrete.
In Axiograph, planning can use discrete search (A*, MCTS) for symbolic actions
or gradient-based optimization when actions are continuous or differentiable.

MPC + SSL loop (how Axiograph closes the cycle):

```
snapshot anchor -> export pairs -> self-supervised train
        ^                               |
        |                               v
promote <--- reconcile <--- propose <--- world model (JEPA)
   ^                                       |
   |                                       v
   +----------- MPC planning <---- cost module (guardrail + task)
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

## Implementation sketch in Axiograph

### 1) State + action interfaces
- Define a `WorldState` abstraction: accepted snapshot id + full `.axi` module
  view + context + evidence overlays.
- Define `Action` primitives: merge, split, promote, reject, rewrite, annotate.
- Define `Transition` outputs: predicted deltas + confidence.

### 2) World model service
- Pluggable world model interface (JEPA-style latent predictor).
- Rollout API: `rollout(state, actions, horizon) -> {trajectories}`
- Optional uncertainty sampling: multiple trajectories from latent variables.

### 3) Cost module API
- `GuardrailCost`: immutable cost terms (hard constraints, policy rules).
- `TaskCost`: configurable cost terms (objective-specific scoring).
- `TotalCost = sum_t (GuardrailCost(s_t) + TaskCost(s_t))`.

### 4) Planner/MPC engine
- Receding horizon planning with discrete search or gradient-based updates.
- Allows mixed planning: use symbolic search for action proposals and continuous
  gradient refinement for embeddings.

### 5) Evidence-plane integration
- Convert plan outputs into `proposals.json` entries with provenance.
- Use reconciliation + quality checks + certificates for acceptance.

## How this aligns with Axiograph's trust boundary

- World model outputs remain **untrusted** and land in the evidence plane.
- Guardrail costs can be certified and enforced at promotion time.
- High-value steps can emit certificates: "this plan step preserves constraint
  X" or "this transition respects rewrite rules".
- Hierarchical modeling is explicit: schema/world/context layers become
  separate model scopes, and each scope emits proposals that are typed and
  constrained by the `.axi` theory for that scope.
- Ingest flow stays coherent: world-model proposals are just another evidence
  stream (`proposals.json`) that enters the same validate -> reconcile -> promote
  pipeline as doc/web/RDF ingestion.

### LLM and tool-loop integration

LLMs remain an untrusted boundary, just like world models:
- LLMs can request world-model proposals via the server endpoint, then review
  or refine them using the same guardrail reports and constraint checks.
- LLM-generated proposals and world-model proposals both land in the evidence
  plane and are reconciled/promoted under identical rules.
- The planner/MPC loop can treat LLM suggestions as candidate actions while
  still scoring them via guardrail + task costs.

## Related docs

- `docs/explanation/JEPA_INTEGRATION.md`
- `docs/explanation/KNOWLEDGE_GENERATION_AND_LEARNING.md`
- `docs/reference/CERTIFICATES.md`

## References (external)

- Yann LeCun, "A Path Toward Autonomous Machine Intelligence" (OpenReview, 2022)
