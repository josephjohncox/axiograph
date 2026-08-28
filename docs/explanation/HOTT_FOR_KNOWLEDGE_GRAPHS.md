# HoTT And Groupoid Semantics For Knowledge Graphs

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph uses HoTT and groupoid ideas as a disciplined way to reason about
paths, equivalence, transport, and semantic evolution. The current system does
not claim full HoTT or univalence. It implements finite runtime checks in Rust
and selected certificate fragments in Lean.

## Why This Matters

Knowledge graphs become hard to evolve when relationships are treated as flat
edges. Axiograph needs richer structure:

- typed paths for query and composition,
- path equations for semantic equivalence,
- rewrites for normalization,
- transports for schema evolution and rebase,
- higher-path intuition for reconciling competing derivations,
- explicit residual obligations when the runtime fragment cannot decide.

## Current Operational Model

The public spine is:

```text
canonical .axi
  -> KernelSnapshotIr / SchemaPresentationIr / TypedTheoryIr / InstanceModelIr
  -> canonical RuntimeIrRef citations + typed runtime reports
  -> optional Lean certificate for the supported fragment
```

Runtime path checking must prove enough to be operationally useful:

- every path step resolves to a compiled arrow or role projection,
- adjacent endpoints compose,
- context/world/time roles are preserved or explicitly transported,
- path equations mention parallel endpoints,
- rewrites preserve admissible variables and endpoints,
- unsupported higher-order cases become typed residual obligations.

## Groupoid Intuition

Some semantic paths are reversible. Examples:

- a schema refactor with an explicit inverse transport,
- a reversible material-flow accounting transformation,
- a normalized path witness and its inverse,
- a semantic VCS rebase that can be replayed against a declared source slice.

The runtime does not assume all paths are invertible. Invertibility is a typed
claim that must be represented by a checked witness or left as a residual.

## Transport

Transport is the key bridge between HoTT intuition and ontology engineering.
If a schema/category state changes, obligations must be transported:

- facts transport along object and relation images,
- path equations transport along arrow images,
- business rules transport along bounded-context morphisms,
- behavior cases transport along CQ and implementation-surface refs,
- failed transport becomes a resolver handle or residual obligation.

This is the semantic foundation of merge/rebase: rebase is not text movement;
it is typed transport of obligations across an accepted target state.

## Higher Paths And Reconciliation

When two branches produce different derivations for the same semantic target,
the system should not hide that conflict. It should expose:

- the competing path witnesses,
- the equations or rewrites each branch used,
- the affected theory obligations,
- CQ/trust/coverage impact,
- resolver steps that can accept, reject, weaken, or transport the claim.

This is higher-path intuition turned into operational review machinery.

## Lean Boundary

Lean is used for selected finite fragments:

- path normalization/equivalence,
- rewrite derivation replay,
- reduced semantic VCS merge/rebase predicates,
- query-answer soundness where the prepared-query fragment is supported.

The Lean checker verifies the encoded finite payload. It does not certify all
Rust behavior, all graph backends, or global ontology closure.

## Non-Claims

Axiograph does not currently claim:

- univalence as an executable kernel principle,
- full higher inductive types,
- arbitrary higher-category reasoning,
- complete merge lattices for all ontologies,
- global closure under all imported worlds and evidence.

Those ideas remain design inspiration until a specific fragment is encoded,
tested, and wired through the trusted boundary.
