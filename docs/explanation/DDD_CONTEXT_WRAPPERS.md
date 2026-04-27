# DDD / fDDD Context Wrappers

**Diataxis:** Explanation  
**Audience:** contributors

This note explains how Domain-Driven Design, functional DDD, bounded contexts,
and executable behavior fit into Axiograph **without** creating a second
architecture.

## Summary

Axiograph already has most of the machinery DDD needs, but it appears under
ontology/query/report names rather than DDD names.

The intended direction is therefore:

- keep the **kernel** centered on canonical `.axi`, compiled IR, anchors,
  queries, transport, trust, and certificates,
- keep **DDD/fDDD** as a **wrapper layer** over those contracts,
- and expose vocabulary that average programmers can use without requiring
  explicit category-theory jargon.

## Why wrappers instead of a new kernel

The repo already has:

- typed query preparation and execution,
- trust contracts,
- rule applicability reports,
- coverage reports,
- competency-question evaluation,
- refinement handles,
- semantic evolution previews,
- and semantic history.

Those are the real operational seams. Rebuilding them under a second DDD-native
runtime would only duplicate logic and split the source of truth.

## Implemented wrapper tranche

The implemented wrapper-layer slice now includes read-only bounded-context and
behavior-case reporting plus context-map selectors for merge/rebase planning.
It is built around:

- `DomainContextId`
- `BoundedContextV1`
- `ContextReportRequestV1`
- `ContextReportV1`
- `BehaviorCaseV1`
- `CaseReceiptV1`
- `ContextMapV1`

That tranche composes existing:

- business-rule applicability,
- semantic coverage,
- competency-question trust/coverage,
- optional evolution-preview sidecars,
- `SemanticSliceSelectorV1` for merge/rebase slices,
- runtime theory-check closure summaries for bounded-context invariants,
  behavior-case CQ/spec obligations, and context-map transport assumptions,
- and semantic tool-loop surfaces including `semantic_context_map`.

It does not introduce new persistence or new kernel semantics.

The wrapper should use `RuntimeTheoryCheckReportV1` when it needs stronger
language than "rule found". In fDDD terms, aggregate invariants and behavior
cases become theory obligations; context maps become schema/theory morphisms
with transport reports; and a bounded-context report can say which obligations
are checked, review-only, residual, or blocking under the declared world and
evidence assumptions.

## Recommended public wrapper vocabulary

These are the most useful public-facing wrapper concepts identified in the
research and implementation work.

### 1. `BoundedContextV1`

Represents a read-only domain area with:

- a wrapper-layer context id,
- a label/summary,
- owned rule scopes,
- mapped implementation surfaces,
- coverage edges,
- and competency questions.

This is the programmer-facing entrypoint for “where does this behavior belong?”

Current operational seam:

- `ContextReportV1.semantic_slice_selector` turns the bounded context into a
  typed semantic slice selector.
- That selector can feed semantic merge/rebase planning instead of requiring
  separate DDD-specific merge logic.

### 2. `BehaviorCaseV1`

Represents an executable domain behavior or use case layered over:

- `QueryIrV1`,
- competency questions,
- trust targets,
- and implementation surfaces.

Practical reading:

- BDD scenario,
- CQ-backed executable spec,
- typed use case.

Important boundary: DDD/fDDD wrappers are tooling overlays over the ontology,
not the default ontology representation itself. A business `.axi` module should
normally model domain meaning; bounded contexts, aggregates, commands,
implementation surfaces, code refs, coverage policies, and generated test
plans should live in typed manifests that reference compiled IR ids. Modeling
those tool concepts in `.axi` is reserved for a separate Axiograph metamodel or
self-validation track.

Current operational seam:

- `BehaviorCaseReportV1.semantic_slice_selector` includes the case id, context
  scopes, implementation surfaces, and competency questions.
- This lets a behavior case become a typed merge/rebase slice for review
  branches, implementation work, and CQ-gated ontology evolution.
- See `examples/software_authoring/` for the concrete authoring loop: a pure
  domain `.axi` module, a typed tooling overlay, weak definition/coverage
  queries, a JSON behavior case, runtime theory closure checks, continuous
  software coverage gates, and multi-language test skeleton previews for Go,
  Python, Rust, and TypeScript.
- The `axiograph-software-authoring` crate provides the current
  usability bar: a CI-style check should read the behavior-case report, verify
  required generated language surfaces, report code-ref materialization gaps,
  detect CQ/rule drift, and surface unresolved semantic obligations before a
  team treats a generated skeleton as accepted implementation.

### 3. `ContextMapV1`

Represents a DDD/fDDD context map as a typed bridge between two bounded-context
semantic slices.

The object carries:

- source and target context ids,
- source and target `SemanticSliceSelectorV1`,
- a DDD relationship kind such as `shared_kernel`,
  `anti_corruption_layer`, or `published_language`,
- typed overlap refs,
- merge policy hints,
- residual obligations,
- and next actions.

This is the friendlier public wrapper over transport/migration/reconciliation
seams. It describes how one bounded context translates into another without
making raw categorical language the primary API.

### 4. `OutcomeMatchV1`

Recommended later wrapper tranche.

This is the programmer-facing form of behavioral/result equivalence. It should
say whether two outcomes are strong/weak/unknown/conflicted matches, while the
underlying semantics may still be transport/equivalence/proof driven.

### 5. `CaseReceiptV1`

Recommended later wrapper tranche.

This is the read-only execution/audit receipt for a behavior case. It should
compose anchors, trust, matched rules, residual obligations, and next actions.

## What stays underneath the wrappers

The wrapper layer should be built on top of existing contracts such as:

- `QueryIrV1`
- `PreparedQueryV1`
- `TrustContractV1`
- `BusinessRuleApplicabilityReportV1`
- `CoverageReportV1`
- `AgentEngineeringReportV1`
- `EvolutionPreviewV1`
- `RuntimeRefinementCandidateV1`
- semantic refs/commits/validations in semantic VCS
- `SemanticSliceSelectorV1`
- `SemanticMergePlanV1`

## Non-goals

- Do not turn `ContextId` into bounded-context identity.
- Do not add new kernel-level DDD terms first.
- Do not make aggregates or CQRS the primary organizing principle of the repo.
- Do not expose Kan/codensity/linear-typing vocabulary as user-facing app code.
- Do not replace query/spec/refinement/evolution seams with step-definition indirection.

## External references

- Martin Fowler — Bounded Context: https://martinfowler.com/bliki/BoundedContext.html
- Microsoft Learn — domain analysis: https://learn.microsoft.com/en-us/azure/architecture/microservices/model/domain-analysis
- Microsoft Learn — tactical DDD: https://learn.microsoft.com/en-us/azure/architecture/microservices/model/tactical-domain-driven-design
- Context Mapper docs — bounded contexts: https://contextmapper.org/docs/bounded-context/
- Context Mapper docs — context maps: https://contextmapper.org/docs/context-map/
- Bounded Context Canvas: https://github.com/ddd-crew/bounded-context-canvas/blob/cdbd86eb19f75f797424543b11fc0b18f72bbe36/README.md
- Domain Modeling Made Functional: https://github.com/swlaschin/DomainModelingMadeFunctional
- EventFlow: https://github.com/eventflow/EventFlow
