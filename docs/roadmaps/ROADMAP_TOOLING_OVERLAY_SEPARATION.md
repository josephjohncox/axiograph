# Tooling Overlay Separation Roadmap

This roadmap corrects a usability mistake in the first software-authoring
examples: the canonical `.axi` module should not carry DDD/fDDD, BDD, coverage,
codegen, test, API, or implementation-surface vocabulary just so the tools can
operate. Those are tooling overlays over the ontology. They are not usually the
domain representation itself.

## Principle

Keep three layers separate:

1. **Representation layer**
   - Canonical `.axi` modules describe domain meaning: objects, relations,
     constraints, equations, rewrites, contexts, worlds, and accepted facts.
   - The representation layer should not be polluted with tool-process
     categories such as `BoundedContext`, `Aggregate`, `Command`,
     `ImplementationSurface`, `CodeModule`, `TestSpec`, `Language`,
     `BehaviorCase`, or `CoverageStatus` unless those are genuinely part of the
     domain being modeled.
2. **Typed tooling overlay layer**
   - DDD/fDDD maps, behavior cases, competency questions, implementation
     surfaces, code refs, coverage policies, codegen requests, and agent plans
     live in typed JSON/IR manifests.
   - These manifests reference stable ids from the compiled `.axi`
     schema/category/theory/instance IR.
   - They can be validated against the ontology without becoming ontology
     terms.
3. **Execution/report layer**
   - Runtime theory checks, behavior-case reports, semantic coverage reports,
     codegen previews, continuous software coverage gates, merge/rebase plans,
     and resolver handles are derived artifacts.
   - They are reviewable, diffable, and anchor-aware, but they are not accepted
     domain facts unless explicitly promoted through a separate ontology change.

The general rule is:

> Put business/domain meaning in `.axi`; put software-engineering workflows in
> typed tools that use `.axi`.

## Why This Matters

Embedding DDD/fDDD and coverage concepts directly in domain `.axi` files has
bad product behavior:

- It makes authors model Axiograph's tools instead of their business.
- It creates redundant ontology structure because behavior cases and coverage
  reports already know about code refs, tests, CQs, and generated skeletons.
- It makes the meaning plane less reusable across different engineering
  methods. A domain ontology should still work if a team uses fDDD, event
  storming, BPMN, scenario tests, property tests, or a custom agent workflow.
- It blurs the trust boundary. A code coverage edge is a tool claim under a
  repo/ref/test run, not an accepted domain fact by default.
- It makes semantic VCS harder: ontology diffs should not be noisy because the
  CI/codegen tooling changed.

## Target Architecture

### Canonical `.axi`

The order-fulfillment example should eventually have a pure domain module such
as `OrderFulfillmentDomain.axi` containing only domain semantics:

- orders, payments, shipments, reservations, approvals, fulfillment states,
  contexts, time, and evidence-bearing events when they are business facts;
- business constraints such as "a shipment requires a paid or reserved order";
- path equations and rewrites that are genuinely domain-theoretic;
- competency-question targets as queryable domain patterns, not necessarily CQ
  objects.

It should not need implementation/test/tooling objects.

### Tooling Manifests

Add or refine typed overlays such as:

- `BehaviorCaseV1`
  - executable scenario/CQ/spec over ontology refs;
  - owns generated test skeleton requests;
  - owns expected codegen languages.
- `FdddContextMapV1`
  - maps a domain slice to a bounded-context/aggregate/command vocabulary for
    a given engineering method;
  - references domain object/relation/theory ids rather than declaring them as
    ontology objects.
- `ImplementationSurfaceManifestV1`
  - maps endpoints, jobs, UI surfaces, reports, PLC logic, services, configs,
    and migrations to ontology refs;
  - carries code refs, repo refs, runtime language, owner, and test refs.
- `CoveragePolicyV1`
  - declares what counts as acceptable software coverage for a behavior case or
    semantic VCS ref.
- `CodegenPlanV1`
  - requests Go/Python/Rust/TypeScript/C/C++/PLC/etc. skeletons from a behavior
    case and implementation-surface manifest.

These overlays are typed, validated, and anchor-aware, but not part of the
domain `.axi` representation.

### Continuous Coverage Tool

The continuous software coverage checker should consume:

- canonical `.axi` module or accepted semantic ref;
- compiled IR digest;
- behavior-case manifest;
- optional DDD/fDDD context-map overlay;
- implementation-surface manifest;
- coverage policy;
- runtime theory-check report;
- generated/actual code refs and test results.

It should emit:

- semantic coverage report;
- CQ status;
- runtime-theory closure summary;
- code/test/materialization status;
- drift report;
- codegen plan/report;
- typed resolver handles for missing obligations.

The current `axiograph-software-authoring` crate is a first slice, but
it currently consumes a report where too much of the software mapping came from
`.axi`. The refactor should move that mapping into overlay manifests.

## Refactor Plan

1. **Split the example ontology**
   - Create `examples/software_authoring/OrderFulfillmentDomain.axi`.
   - Keep only domain objects, relations, constraints, and equations.
   - Remove DDD/tooling concepts from the domain `.axi`: bounded contexts,
     aggregates-as-DDD, commands-as-DDD, implementation surfaces, code modules,
     languages, API endpoints, test specs, CQ objects, and coverage relations.
   - Keep events only when they are domain facts, not when they exist solely to
     model fDDD commands/events.
2. **Move DDD/fDDD mapping to typed overlays**
   - Use `order_fulfillment_tooling_overlay.json` as the first bundled
     `ToolingOverlayBundleV1`.
   - Map bounded contexts, aggregates, functions, processes, business rules,
     and context maps to compiled ontology refs.
   - Validate every referenced ontology id against the compiled IR.
3. **Move implementation mapping to typed overlays**
   - Keep implementation surfaces inside the same typed overlay bundle unless
     a later scale boundary justifies splitting.
   - Move endpoint/job/UI/code/test/language refs out of `.axi`.
   - Add explicit repo/ref/test-run fields so the continuous checker can
     distinguish target files from verified implementation coverage.
4. **Move coverage policy to typed overlays**
   - Keep coverage policy inside the same typed overlay bundle unless a later
     scale boundary justifies splitting.
   - Declare required CQs, required generated languages, required code refs,
     required runtime theory status, and strict/fail-open behavior.
5. **Update behavior-case tooling**
   - Make `discover behavior-case` accept overlay manifests.
   - Behavior cases should bind to ontology refs plus overlay refs, not expect
     coverage/tool concepts to be ontology facts.
   - Reports should still include `SemanticSliceSelectorV1`, but selectors
     should cite domain IR ids and overlay ids separately.
6. **Update continuous-check tooling**
   - Make `axiograph-software-authoring continuous-check` consume the
     new manifests directly.
   - Fail closed only when the selected `CoveragePolicyV1` requires it.
   - Emit resolver handles for missing overlays, not candidate domain facts.
7. **Update examples and tests**
   - Rewrite software-authoring examples around pure domain `.axi` plus typed
     overlay manifests.
   - Keep an explicit transitional test that proves behavior/codegen/coverage
     tooling can operate without DDD/coverage concepts embedded in `.axi`.
8. **Preserve AXI self-validation as a separate later track**
   - If Axiograph wants to validate its own DSL, report schemas, coverage
     concepts, behavior-case concepts, and semantic VCS concepts using `.axi`,
     create a separate `AxiographMeta.axi` or metamodel package.
   - Do not use business-domain examples to host that self-validation
     vocabulary.

## Acceptance Criteria

- The primary software-authoring `.axi` example has no DDD/tooling-only object
  types such as `ImplementationSurface`, `CodeModule`, `TestSpec`, `Language`,
  or `APIEndpoint`.
- DDD/fDDD concepts are represented in typed overlay manifests that reference
  stable compiled IR ids.
- Behavior-case, coverage, and codegen reports still work over the pure domain
  ontology.
- Continuous software coverage can run from manifests plus accepted ontology
  anchors.
- Semantic VCS diffs separate domain changes from tool/coverage/codegen changes.
- Docs explain that AXI self-validation is a separate metamodel use case, not
  the default modeling pattern for user ontologies.

## Later Todo: AXI Self-Validation

Add a dedicated metamodel package for Axiograph validating Axiograph:

- `AxiographMeta.axi` models modules, schemas, theories, reports, behavior
  cases, coverage policies, semantic VCS objects, and tool contracts.
- It is used to validate Axiograph's own report/tool schemas and examples.
- It is not imported into ordinary business ontologies unless a user is
  explicitly modeling Axiograph itself.
