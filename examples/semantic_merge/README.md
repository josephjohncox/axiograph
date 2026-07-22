# Semantic Merge: Plant Operations

This example demonstrates finite semantic merge/rebase contracts as typed
ontology work, not text merge. It models a small chemical-plant setting where
two candidate slices evolve from the same accepted base:

- `PlantOperationsCore.axi`: accepted operational spine.
- `PlantProcurementCertification.axi`: procurement, receiving, certificate,
  and release-document obligations.
- `PlantSimulationSafety.axi`: simulation, probabilistic-risk, optimizer, and
  safety-envelope obligations.

The teaching goal is realistic co-evolution: procurement and simulation teams
change different bounded-context slices, Rust tests build semantic plans, and
Lean checks strict conformance fixtures against the finite merge/rebase theory.

## Flow At A Glance

1. Validate the finite Rust merge/rebase model through focused unit tests.
2. Build the Lean semantic VCS checker.
3. Accept the clean merge and clean rebase fixtures.
4. Reject conflicting, blocked, dropped-ref, cross-lineage, and missing-target
   fixtures.

The former filesystem accepted-plane runner and broad `axiograph sem` CLI were
removed. Dry-run `SemanticMergePlanV1` values remain untrusted analysis.
Durable accepted merge state uses `SemReconciliationV2`, reviewed typed
candidates, compiled payload fingerprints, and typed keep/drop/introduce/
transport decisions in AxiStore. Protected main moves only through
authenticated exact-two-parent `SemCommitV2` materialization.

## What This Exercises

- Canonical `.axi` validation for each ontology module.
- Runtime theory checking over accepted modules.
- Typed semantic refs, slices, and `SemanticMergePlanV1` contracts.
- Lean-readable merge/rebase payloads checked by
  `axiograph_semantic_vcs_check`.
- A clean merge case and clean rebase case that pass Lean checking.
- A conflicting merge case with a required resolver step.
- A blocked rebase case that demonstrates fail-closed residual obligations.
- A transport-only rebase case that has no explicit blockers but still fails
  because a required transport is opaque/out-of-fragment.

## Run the focused conformance target

```bash
make verify-lean-semantic-vcs
```

The Make target runs Rust semantic-plan tests, accepts the two clean static
fixtures, and requires rejection of every adversarial fixture. It does not
create mutable semantic refs or a second persistence layout.

## Coverage Claim

For this example, complete coverage means complete for the finite conformance
fixture set, not complete for all semantic VCS behavior. The positive cases are
the static clean merge and clean rebase plans. The negative cases cover
conflicts, blocked and unacknowledged transport, cross-lineage results, dropped
refs, and missing rebase targets.

The current flow does not cover every operator-facing `SemanticMergePlanV1` or
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

AxiStore additionally checks finite payload-union accounting by recompiling
all reviewed candidates from exact `.axi` bytes. That runtime check is outside
`VerifyMain` and does not convert this Lean conformance model into a general
categorical pushout theorem.

Neither layer claims complete ontology closure, general dependent transport,
full HoTT/univalence, arbitrary higher paths, or globally optimal merge. Those
remain explicit non-claims until represented in the certified fragment.
