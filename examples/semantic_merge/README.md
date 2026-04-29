# Semantic Merge: Plant Operations

This example demonstrates semantic VCS merge/rebase as typed ontology work,
not text merge. It models a small chemical-plant setting where two review
branches evolve from the same accepted base:

- `PlantOperationsCore.axi`: accepted operational spine.
- `PlantProcurementCertification.axi`: procurement, receiving, certificate,
  and release-document obligations.
- `PlantSimulationSafety.axi`: simulation, probabilistic-risk, optimizer, and
  safety-envelope obligations.

The teaching goal is realistic co-evolution: procurement and simulation teams
change different bounded-context slices, Axiograph builds a semantic merge plan,
and Lean checks the reduced runtime plan against the finite merge/rebase theory.

## Flow At A Glance

1. Validate all plant `.axi` modules as canonical semantic inputs.
2. Run the finite runtime-theory check for the accepted base module.
3. Promote the base into an isolated accepted-plane directory.
4. Create review refs for procurement certification and simulation safety.
5. Dry-run semantic merge and rebase plans without mutating accepted meaning.
6. Check the reduced merge/rebase payloads with the Lean semantic VCS checker.
7. Emit conformance coverage for the finite surface this example claims.

## What This Exercises

- Canonical `.axi` validation for each ontology module.
- Runtime theory checking over accepted modules.
- Semantic refs and review branches in the accepted plane.
- Dry-run semantic merge with a `SemanticMergePlanV1`.
- Lean-readable merge/rebase payloads checked by
  `axiograph_semantic_vcs_check`.
- A clean merge fixture and clean rebase fixture that pass Lean checking.
- A conflicting merge fixture with a required resolver step.
- A blocked rebase fixture and generated blocked rebase plan that demonstrate
  fail-closed residual obligations.
- A transport-only rebase fixture that has no explicit blockers but still fails
  because a required transport is opaque/out-of-fragment.
- A `semantic_vcs_conformance_coverage_v1` report that records every claimed
  pass/reject case exercised by the flow.

## Run

```bash
./examples/semantic_merge/run_merge_flow.sh
```

Or run the focused conformance target:

```bash
make verify-lean-semantic-vcs
```

The script writes reports under `build/examples/semantic_merge/`, including:

- `base_theory_check.json`: runtime-theory report for the accepted base.
- `status_base.json`, `status_procurement.json`, and
  `status_simulation.json`: accepted-plane/ref construction checkpoints.
- `merge_plan.json`: full Rust runtime merge report.
- `merge_plan_lean.json`: reduced Rust-generated Lean checker payload.
- `rebase_plan_lean.json`: generated blocked rebase checker payload.
- `semantic_vcs_conformance_coverage.json`: machine-readable coverage of the
  finite semantic VCS cases claimed by this example.

## Coverage Claim

For this example, complete coverage means complete for the finite conformance
surface recorded by `semantic_vcs_conformance_coverage_v1`, not complete for all
semantic VCS behavior. The positive cases are the static clean merge fixture,
the static clean rebase fixture, and the Rust-generated clean merge payload. The
negative cases are the static conflicting merge fixture, static blocked rebase
fixture, static failed-transport-without-blocker fixture, and the Rust-generated
rebase payload with unresolved transports.

The current flow does not claim a Rust-generated clean rebase for the plant
scenario. It also does not cover every operator-facing `SemanticMergePlanV1` or
`SemanticRebasePlanV1` field, every resolver policy, competency-question
preservation, trust-regression preservation, or inclusion in the shipped
`VerifyMain` certificate boundary.

## Theory Boundary

The Lean checker here is not a general ontology theorem prover. It verifies
that Rust-emitted merge/rebase payloads satisfy the finite operational
materialization predicates encoded in `lean/Axiograph/SemanticVCS.lean`:

- no blockers,
- no required unresolved resolver steps,
- no residual obligations,
- required transport items are either preserved or transported.

It does not claim complete ontology closure, full HoTT/univalence, or globally
optimal merge. Those remain explicit non-claims until represented in the
certified fragment.
