# Semantic Merge Lattice Roadmap

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap is the execution plan for typed semantic VCS merging over ontology
slices. It is intentionally runtime-first: Rust builds useful typed merge plans
now, while Lean certification remains a later narrow-fragment strengthening.

## Current Ground

The implementation already has the pieces to make semantic merge operational:

- `SchemaCategoryIr`: schemas as finite categories, with object types,
  relation objects, role projection arrows, and subtype inclusion arrows.
- `InstanceFunctorIr`: accepted instance data interpreted as a functor from the
  schema category into finite sets of members, fact ids, and role-value maps.
- `TheoryIr`: runtime-addressable constraints, equations, rewrite rules, and
  theory obligations.
- `EvolutionPreviewV1`: the shared mutation-review payload for authoring,
  migration, reconciliation, and promotion.
- `SemCommitV1` / `SemReconciliationV1`: persisted semantic VCS refs,
  commits, and reconciliation previews.
- `RuntimeRefinementHandleV1`: typed resolver handles shared by query,
  olog authoring, migration, reconciliation review, and CQ repair.

The next runtime work is to make these objects drive slice selection,
conservative auto-merge, rebase/transport planning, and MCP-visible resolver
flows.

Current implementation note: `axiograph sem slice build` resolves the selected
semantic ref to its accepted snapshot, reads the accepted canonical `.axi`
modules, compiles them to `KernelModuleIr`, and enriches persisted slice
manifests with concrete schema/category, theory, and instance-functor refs. Pure
tool-loop payloads remain report-level unless a caller supplies the compiled IR
or a stored manifest.

## Theoretical Contract

This design is informed by category theory, applied category theory, dependent
type theory, HoTT/groupoid semantics, and fDDD/DDD, but it must not overclaim.

- Category theory: accepted schemas present finite categories; relation tuples
  are relation objects; roles are projection arrows; subtypes are inclusion
  arrows; path equations and rewrites are theory structure over those arrows.
- Applied category theory: ologs, schema morphisms, functorial data migration,
  diagrams, and pushout/pullback-inspired reconciliation guide the runtime
  shapes. The runtime does not claim arbitrary categorical colimit
  construction.
- Dependent type theory: every semantics-bearing artifact should be indexed by
  schema, theory, context/world, lifecycle state, accepted anchor, and slice.
  Rust enforces these as runtime/type-state guardrails; Lean certifies later
  narrow fragments.
- HoTT/groupoids: paths, rewrites, equivalences, and transport inform resolver
  obligations. This does not make the runtime a full HoTT or univalence
  implementation.
- fDDD/DDD: bounded contexts are typed semantic slices, behavior cases are
  executable specifications, aggregates/invariants are theory obligations, and
  context maps are typed morphisms/functors between slices.

Current DDD/fDDD seam: `ContextReportV1` and `BehaviorCaseReportV1` now carry
`SemanticSliceSelectorV1`, and `ContextMapV1` produces source/target selectors,
typed overlap refs, merge-policy hints, and residual obligations. These wrapper
objects feed semantic merge/rebase planning; they do not form a second merge
kernel.

The merge lattice is operational and finite: it is a runtime partial order over
known persisted typed slices. It is not a claim that all ontologies form a
complete lattice.

Lean alignment note: `lean/Axiograph/SemanticVCS.lean` now formalizes this
finite operational reading with `SemanticSlice`, finite join/meet candidates,
conservative merge materialization predicates, rebase transport predicates, and
small preservation lemmas. It also exposes executable fail-closed
`checkMergePlanMaterialization` and `checkRebasePlanMaterialization` gates with
soundness lemmas. `lean/Axiograph/SemanticVCS/Json.lean` defines the first
strict JSON shape for future runtime exports. These modules are theorem support
outside the shipped verifier boundary until runtime `SemanticMergePlanV1` /
`SemanticRebasePlanV1` payloads are exported as Lean-checkable certificates. See
`docs/reference/LEAN_THEORY_EVALUATION.md`.

## Greenfield Policy

Do not preserve legacy merge, export, wrapper, or query compatibility harnesses
unless they protect:

- accepted snapshot anchors,
- the live trusted verifier boundary,
- production `.axpd` byte truth,
- or an explicit short-lived migration cutover.

When a clean typed runtime contract exists, remove the older parallel surface in
the same implementation slice.

## Runtime Object Program

### Typed slices

`SemanticSliceV1` / `SemanticSliceManifestV1` represent an anchor-scoped
restriction of accepted ontology meaning.

Each slice must carry:

- base semantic ref and commit id,
- accepted snapshot id,
- kernel IR digest,
- selector used to extract it,
- selected stable refs for schema/category objects, relation objects, roles,
  subtype inclusions, theory obligations, instances, contexts/worlds, CQs,
  behavior cases, implementation surfaces, evidence, and world-model runs,
- trust class,
- non-claims around completeness and certification.

Persistence target:

```text
sem/slices/<slice-id>.json
```

### Slice selectors

`SemanticSliceSelectorV1` should support extraction by:

- schema/category ids,
- relation objects,
- roles/projection arrows,
- subtype inclusions,
- theory obligations,
- context/world ids,
- competency questions,
- bounded contexts,
- behavior cases,
- implementation surfaces,
- world-model runs,
- explicit IR refs.

Selectors must resolve to stable IR refs where possible. Raw names are allowed
only as boundary input; persisted manifests must cite normalized ids.

### Merge lattice

`SemanticMergeLatticeV1` computes:

- inclusion edges,
- overlap edges,
- dependency edges,
- conflict edges,
- missing-support edges.

Each edge carries:

- source/target slice ids,
- shared typed refs,
- trust class,
- notes about whether the edge is runtime-checked, review-only,
  evidence-backed, or certifiable-fragment.

### Conservative merge compiler

`SemanticMergePlanV1` is the runtime compiler output for merge and rebase.

It may auto-join only:

- disjoint typed slices,
- identical/idempotent deltas,
- non-overlapping theory obligations,
- non-regressing CQ/trust/coverage deltas.

It must emit resolver steps for:

- overlapping subtype/type-family changes,
- role movement,
- relation-object reification changes,
- path-equation or rewrite conflicts,
- context/world leakage,
- incompatible facts,
- CQ regressions,
- implementation-surface drift.

It must also emit typed blockers, not only prose residuals. `blockers` classify
the reason a plan cannot materialize:

- unresolved semantic conflicts,
- unapplied resolver steps,
- quality gate failures,
- CQ gate failures,
- trust regressions,
- semantic coverage regressions,
- runtime theory blockers or residual obligations,
- preview-level non-ok results.

`can_materialize` is false whenever any blocker or resolver step remains. The
blocker list is the compact machine-readable summary that CLI, MCP, and agents
should inspect before attempting mutation.

Plan validation must fail closed even when a caller mutates or miscomputes the
boolean flag: residual obligations, conflicts, resolver handles, preview
non-ok state, runtime-theory blockers, or typed blockers make the plan
non-materializable.

Resolver steps reuse `RuntimeRefinementHandleV1`; no second resolver protocol is
allowed.

### Rebase as transport

Semantic rebase is typed transport of a source slice across a target
schema/category state.

The rebase plan must report:

- source slice,
- target/onto ref,
- transport basis,
- transported refs,
- failed transports,
- residual obligations,
- resolver handles.

The runtime object is `SemanticRebasePlanV1`. It separates successful
`transported_refs` from `failed_transports`, carries transport-basis notes, and
copies residual obligations into typed blockers so agents cannot accidentally
materialize an incomplete transport.

No accepted-plane mutation is allowed until resolver steps and CQ/trust gates
pass.

## CLI And MCP Surfaces

CLI targets:

- `axiograph sem merge --dry-run --source <ref> --target <ref> --json`
  returns a merge dry-run envelope including `SemanticMergePlanV1`.
- `axiograph sem merge --source <ref> --target <ref>` materializes only when
  the merge plan is materializable.
- `axiograph sem rebase --source <ref> --onto <ref> --slice <selector.json>`
  returns a rebase/transport plan.
- `axiograph sem slice build|show|diff` persists and inspects `sem/slices/`
  manifests directly.

MCP/tool-loop targets:

- `semantic_slice_build`
- `semantic_slice_show`
- `semantic_slice_diff`
- `semantic_merge_plan`
- `semantic_rebase_plan`
- `semantic_resolver_steps`

These tools are read-only and return structured plans/handles; they are meant
for coding agents, review agents, and ontology-engineering agents that need to
ask what can be merged, what must be resolved, and what code/test/ontology
actions come next.

## Runtime Tranches

1. Land contracts and pure analysis.
   - Add `SemanticSliceManifestV1`, `SemanticSliceSelectorV1`,
     `SemanticMergeLatticeV1`, `SemanticMergePlanV1`, and
     `SemanticResolverStepsReportV1`.
   - Build plans from existing `SemMergeDryRunResultV1`.
   - Persist first-class slice manifests under `sem/slices/`.
   - Expose `axiograph sem slice build|show|diff`.
   - Expose pure MCP/tool-loop tools over supplied ref/dry-run payloads.

2. Deepen selectors over kernel IR.
   - Resolve CLI-built slices against accepted `KernelModuleIr`.
   - Include explicit `SchemaCategoryIr`, `TheoryIr`, and `InstanceFunctorIr`
     refs instead of commit-summary refs only.
   - Add context/world, CQ, behavior-case, and implementation-surface selectors.

3. Enforce materialization gates.
   - Block merge/rebase materialization when plans contain required resolver
     steps, CQ regressions, trust regressions, or unresolved transport failures.
   - Report quality, CQ, trust, coverage, runtime-theory, residual-obligation,
     and conflict blockers explicitly in `SemanticMergePlanV1`.
   - Validate plans fail-closed from typed blockers and residual fields rather
     than trusting `can_materialize` alone.
   - Return explicit `SemanticRebasePlanV1` transport results with
     `transported_refs` and `failed_transports`.
   - Persist failed plans under `sem/validations/`.

4. Add resolver application loops.
   - Reuse `RuntimeRefinementHandleV1`.
   - Persist resolver state in reconciliation records.
   - Make resolved plans produce typed semantic commits.

5. Lean alignment.
   - Use `lean/Axiograph/SemanticVCS.lean` as the starting model for finite
     slice inclusion, join/meet preservation, materialization gates, and
     rebase transport predicates.
   - Certify only narrow fragments:
     path/rewrite equivalence, transported path equations, selected schema
     morphism transports, and query-row soundness under merged refs.
   - Keep runtime merge plans useful but explicitly non-proof objects.

## Test Plan

- Slice extraction by schema/category ids, relation objects, roles, theory
  obligations, context/world, CQ, behavior case, and implementation surface.
- Lattice behavior for inclusion, disjoint joins, overlaps, conflicts, missing
  dependencies, and trust-class propagation.
- Merge compiler behavior:
  disjoint slices auto-join; duplicate/idempotent deltas collapse; overlapping
  relation/role/theory/context refs emit resolver handles.
- Rebase behavior:
  compatible transport succeeds; failed transport emits residual obligations.
- VCS behavior:
  merge dry-run returns a `SemanticMergePlanV1`; materialization fails closed
  when resolver steps or typed blockers remain.
- MCP/tool-loop behavior:
  tools are listed and return anchors, IR refs, resolver handles, residual
  obligations, and next actions.
- Regression:
  `behavior_case`, `kernel_ir`, reconciliation, semantic tool, MCP, LLM
  tool-loop, and `sem merge` tests keep passing.

## Acceptance Commands

```bash
cargo fmt --manifest-path rust/Cargo.toml --all
cargo test --manifest-path rust/Cargo.toml -p axiograph-pathdb kernel_ir -- --nocapture
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli semantic_merge_lattice -- --nocapture
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli semantic_tool_specs_expose_all_semantic_report_tools -- --nocapture
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli sem_merge -- --nocapture
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli tool_definitions_include_shared_semantic_report_tools -- --nocapture
git diff --check
```
