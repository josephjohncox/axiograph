# Session Research and Todos — 2026-04-23

This document is a working summary of the current session state across:

- completed subagent research,
- completed implementation work,
- active background tasks,
- and the next implementation tranches.

It is intended as a continuity artifact so future turns do not lose the reasoning and implementation ordering already established.

## 1. Status at a glance

### Completed in this session

- **Tranche 1 path certification** was implemented and verified historically;
  its public relation-id CLI/server/MCP surfaces are now retired in favor of
  canonical `.axi`-anchored typed query witnesses.
- **Tranche 2a read-only bounded-context reporting** was implemented and verified.
- **Tranche 2c JSON `BehaviorCaseV1` reporting** was implemented and verified.
- **Tranche 2b proof-native runtime support summaries** were implemented and verified.
- **First runtime schema-category / instance-functor IR slice** was implemented and verified.
- The repo’s current **DDD / fDDD / bounded-context / ontology-engineering** overlaps were researched and synthesized.
- The repo’s current **query/spec convergence seams** were researched and synthesized.
- The earlier research backlog around **path primitives**, **support/evidence**, **industrial harness**, **DDD**, and **applied semantics** has been consolidated into a single implementation ordering.

### Still in progress

- Continue the **unified query + spec language** design lane with implementation-grounded constraints.
- Continue the **industrial harness parity/relocation** tranche.
- Extend the DDD/context wrapper layer beyond `BehaviorCaseV1` into context
  bridges, outcome matching, aggregate/invariant boundaries, and promotion-gated
  case receipts.

### Active background work at the time of writing

- None. The final DDD/context wrapper framing, tranche ranking, and the read-only path-cert review all completed after the initial draft of this document and are incorporated below.

## 2. What was implemented

### Retired path certification tranche

The first thin core primitive selected by the architecture work was **path /
route certification**, but the public relation-id path certificate surfaces were
later retired under the greenfield cleanup policy.

Retired behavior:

- `axiograph cert path ...`
- `POST /cert/reachability`
- MCP/tool-loop `path_certify`
- the CLI-local `path_cert.rs` and `path_cert_tools.rs` helper surface

Replacement direction:

- use typed query witnesses over canonical `.axi` anchors for user/server/agent
  certification surfaces;
- keep low-level `reachability_v3` only as a canonical certificate family for
  Lean/Rust fixtures and narrow path-witness work;
- keep route/transport preview tools as planning surfaces, not raw relation-id
  certificate APIs.

### Verification completed before retirement

- Focused path-cert test slices passed before the surface was removed.
- The current verification burden moved to typed query witness, route preview,
  transport preview, and canonical certificate tests.
  - a tiny canonical module was rendered to graph JSON,
  - concrete `start` / `relation_id` values were discovered from that output,
  - `axiograph cert path` emitted a `reachability_v3` certificate,
  - and the emitted payload had the expected proof shape and anchor digest.
- `make verify-semantics` passed.

Manual CLI QA result:

- `kind=reachability_v3`
- `proof_type=step`
- `anchor=fnv1a64:d2ebd953ac74464f`

### Read-only bounded-context report tranche

The first DDD/fDDD/context wrapper-layer tranche that actually landed in code is a **read-only bounded-context report** layer.

Implemented files:

- `rust/crates/axiograph-cli/src/context_report.rs`
- `rust/crates/axiograph-cli/src/semantic_tools.rs`
- `rust/crates/axiograph-cli/src/db_server.rs`
- `rust/crates/axiograph-cli/src/mcp.rs`
- `rust/crates/axiograph-cli/src/llm.rs`
- `rust/crates/axiograph-cli/src/main.rs`

Implemented behavior:

- Added wrapper-layer types centered on:
  - `DomainContextId`
  - `BoundedContextV1`
  - `ContextReportRequestV1`
  - `ContextReportV1`
- Added a shared builder that composes existing:
  - business-rule applicability,
  - semantic coverage,
  - competency-question coverage/trust,
  - optional evolution preview,
  - and next-action / residual-unknown summaries.
- Added a new shared semantic tool:
  - `semantic_context_report`
- Added a new HTTP endpoint:
  - `POST /semantic/context-report`
- Added a new CLI surface:
  - `axiograph discover context-report <input.axpd|input.axi> --request <context.json> [--out <report.json>]`
- Added MCP and tool-loop exposure for the same report shape.

### Verification completed for bounded-context reporting

- LSP diagnostics on changed files showed no errors.
- Focused context-report tests passed.
- Full `cargo test -p axiograph-cli -- --nocapture` passed.
- Manual CLI QA passed using a tiny canonical module and a supplied bounded-context request.

Manual CLI QA result:

- `version=context_report_v1`
- `context_id=domain:family_lookup`
- `trust_class=runtime_enforced`
- `tested_rules=1`
- `competency_total=1`

### JSON BehaviorCaseV1 tranche

The next DDD/BDD/coding-agent tranche now exists as a thin runtime wrapper over
the existing bounded-context, rule, CQ, coverage, and trust-contract surfaces.

Implemented files:

- `rust/crates/axiograph-cli/src/behavior_case.rs`
- `rust/crates/axiograph-cli/src/semantic_tools.rs`
- `rust/crates/axiograph-cli/src/db_server.rs`
- `rust/crates/axiograph-cli/src/mcp.rs`
- `rust/crates/axiograph-cli/src/llm.rs`
- `rust/crates/axiograph-cli/src/main.rs`

Implemented behavior:

- Added JSON-only `BehaviorCaseV1` plus `BehaviorCaseCheckRequestV1`.
- Added `CaseReceiptV1` as the runtime receipt for anchors, trust class,
  claim strength, matched rules/scopes/surfaces, CQ status, residual
  obligations, and next actions.
- Added Rust and TypeScript/Vitest test skeleton previews in the first tranche.
- Added a new shared semantic tool:
  - `semantic_behavior_case`
- Added a new HTTP endpoint:
  - `POST /semantic/behavior-case`
- Added a new CLI surface:
  - `axiograph discover behavior-case <input.axpd|input.axi> --request <behavior-case.json> [--out <report.json>]`
- Kept Gherkin/BDD syntax out of the runtime contract for now; it should become
  an import/export view over the same JSON payload later.

Verification completed:

- Focused behavior-case tests passed.
- Adjacent semantic-tool, MCP, tool-loop, and server capability manifest tests
  passed.

### SchemaCategoryIr / InstanceFunctorIr tranche

The first category/functor runtime layer now exists in
`axiograph_pathdb::kernel_ir`.

Implemented files:

- `rust/crates/axiograph-pathdb/src/kernel_ir.rs`
- `rust/crates/axiograph-pathdb/src/lib.rs`
- `docs/reference/KERNEL_IR.md`

Implemented behavior:

- Added `SchemaCategoryIr`:
  - object types and relation objects are category objects,
  - relation roles are projection arrows,
  - subtype declarations are inclusion arrows,
  - and binary graph edges remain derived traversal views.
- Added `InstanceFunctorIr`:
  - object types interpret to closed membership sets,
  - relation objects interpret to stable fact-id sets,
  - role projections interpret fact ids as mappings to typed role values,
  - subtype inclusions transport subtype members into supertype images by
    identity.

Verification completed:

- Focused pathdb kernel test for relation objects, role projections, subtype
  inclusions, and instance-functor transport passed.

### Proof-native runtime support summary tranche

The next core report-layer tranche that landed was the support primitive upgrade behind the existing `support_summary` wire field.

Implemented files:

- `rust/crates/axiograph-cli/src/evidence_support.rs`
- `rust/crates/axiograph-cli/src/db_server.rs`
- `rust/crates/axiograph-cli/tests/db_server_e2e.rs`
- `docs/reference/QUERY_LANG.md`
- `docs/howto/DB_SERVER.md`
- `docs/research/SESSION_RESEARCH_AND_TODOS_2026_04_23.md`

Implemented behavior:

- Kept the wire field name `support_summary` stable.
- Strengthened the contract behind that field so support is now proof-native over `query_result_v3` witness rows.
- Added additive metadata centered on:
  - `basis`
  - `coverage`
  - `support_kind`
  - `witness_rows`
- Kept `contexts` and `evidence` as attachment-layer enrichments instead of the support basis itself.
- Historical note, now expressed through `QueryCertificatePolicyV1`: accepted-anchor certifiable queries can return `support_summary` even when `certificate_policy` is `none`.
- Made internal support-only certificate construction best-effort when the client did not request a certificate, so support enrichment cannot regress normal query execution.

### Verification completed for support summaries

- Focused `evidence_support` tests passed.
- Focused support-related `/query`, MCP, and tool-loop tests passed.
- Full `cargo test -p axiograph-cli -- --nocapture` passed after one regression fix.
- Manual store-backed `/query` QA passed, showing:
  - `support_present=True`
  - `basis_kind=query_result_v3`
  - `basis_emitted=False`
  - `supported_facts=1`

## 3. Stable architecture conclusions

These conclusions were consistent across Oracle, explore, librarian, and Artistry passes.

### 3.1 Core vs extension boundary

Keep in the **core runtime** now:

- compiled schema/category IR,
- stable anchors and semantic ids,
- path / morphism / transport seams,
- prepared typed query handles,
- thin certified path / route witnesses,
- explicit support / trust / lineage contracts,
- run / inspect parity for artifacts.

Keep **out of core for now**:
- broad rewrite-engine expansion,
- Kan / codensity optimization work,
- linear/resource typing as a primary user vocabulary,
- UI/viz-first work,
- DigitalTwin harness orchestration,
- MCP-specific semantics as kernel-defining concepts,
- and broad DDD / bounded-context vocabulary unless it stabilizes as a real shared shared contract over multiple runtime surfaces.

### 3.2 Current ranked implementation order

1. **Path / route certification** — now implemented.
2. **Support primitive contract** — now implemented behind the stable `support_summary` field.
3. **Run / inspect parity + harness boundary cleanup** — now the next most immediate open implementation tranche.

## 4. DDD / fDDD / bounded-context synthesis

### 4.1 Main conclusion

The repo does **not** currently have first-class DDD vocabulary.

It **does** already have most of the operational machinery DDD would need under different names:

- competency questions as executable specs,
- query/context scoping,
- rule applicability reports,
- coverage reports,
- trust contracts,
- refinement handles,
- evolution previews,
- semantic history and anchors.

### 4.2 Strongest mapping found

The best fit is to treat Axiograph as a **behavior compiler**, not an ontology catalog.

The strongest DDD / BDD / ontology-engineering analogies are:

- **bounded contexts** as executable scoped meaning planes,
- **scenarios / executable specs** as CQ- and query-backed behavior cases,
- **context maps** as translation / ACL / relationship policies,
- **aggregates / invariants** as obligation boundaries,
- and **typed workflows** as programmer-usable surfaces rather than explicit category jargon.

### 4.3 Most useful proposed wrapper vocabulary

The strongest wrapper-layer proposal produced in-session was:

- `ContextCell`
- `BehaviorCase`
- `Obligation`
- `TransportMap`
- `OutcomeEquiv`
- `UsageReceipt`

Important constraint: these should be implemented as **thin wrapper-layer artifacts over existing query / CQ / report / evolution contracts**, not as a second architecture and not as a new kernel vocabulary.

### 4.4 Practical tranche shape implied by the research

Most likely DDD/fDDD wrapper-layer tranche after path-cert:

- add a `ContextCellV1` / `BehaviorCaseV1` layer that composes:
  - CQ evaluation,
  - business-rule applicability,
  - coverage reporting,
  - and next-action / obligation outputs.

This would make DDD/BDD/context work **operational** without requiring a broad kernel redesign.

### 4.5 Final wrapper-layer implementation mapping

The final repo-backed mapping from the later DDD implementation seam audit was:

- `ContextCell` / bounded-context wrapper → build over:
  - `PreparedQueryExplorationV1`
  - `TrustContractV1` / query trust
  - semantic claim summaries
  - persisted through `accepted_plane.rs` / `sem/validations`
- `BehaviorCase` → build over:
  - `CompetencyQuestionV1`
  - `CompetencyQuestionEvaluationV1`
  - `BusinessRuleApplicabilityReportV1`
- `ContextMap` / context bridge → build over:
  - `ProjectionContextMappingV1`
  - backend context transport summaries
  - world-model semantic input / context-bearing wrappers
- `UsageReceipt` / case receipt → build over:
  - certified/validated query answers
  - gate summaries
  - world-model run records

That audit was explicit that **no DDD/fDDD/context wrappers exist yet under those names**. The correct move is a thin composition layer over the existing contracts, not new storage, new executors, or a parallel semantic architecture.

### 4.6 Final wrapper vocabulary proposal

The final Artistry pass refined the earlier names into a more programmer-facing wrapper vocabulary:

- `ContextCellV1` — anchored bounded-context slice
- `BehaviorCaseV1` — executable domain behavior/use case
- `ContextBridgeV1` — friendlier public name for transport across contexts
- `OutcomeMatchV1` — friendlier public name for behavior/result equivalence
- `CaseReceiptV1` — friendlier public name for a usage/execution receipt

Key conclusion: **keep fDDD as a docs umbrella, not a code prefix**. The category/type machinery should remain underneath these wrappers as anchors, transport, equivalence, and enforcement primitives, not as user-facing jargon.

### 4.7 Final ranked DDD/context tranche

The final Oracle ranking for the next DDD/fDDD/context code tranche was:

1. **Best next tranche:** a read-only bounded-context wrapper layer
   - likely `DomainContextId` / `BoundedContextV1`
   - plus a composed `ContextReportV1`
   - built from existing rule scopes, implementation surfaces, CQ refs, coverage, trust, and next actions
2. Domain capability / use-case wrappers over `AgentTaskRefV1` + CQ/report seams
3. Aggregate / invariant wrappers over relation/theory scopes and evolution preview seams

Important constraint from that ranking:

- do **not** reuse `ContextId` for bounded contexts,
- keep the first DDD/context tranche **read-only**,
- and compose existing reports/sidecars rather than duplicating storage or logic.

This tranche is now implemented.

## 5. Applied category theory / denotational semantics synthesis

### 5.1 Internal repo conclusion

The repo already has a denotationally meaningful programmer-facing surface, but it is expressed as:

- anchors,
- ids,
- typed handles,
- trust contracts,
- pushdown plans,
- and semantic reports,

rather than overt category-theory terminology.

### 5.2 External research conclusion

The practical import from external research was:

- **paths, morphisms, transport, dependent typing** belong in the core semantic layer,
- **rewrite, Kan, codensity, linear/resource typing** are best treated as optimization/resource layers unless and until they become unavoidable shared contracts,
- and any developer-facing surface should map category concepts to ordinary programming constructs:
  - contexts as explicit environments/scopes,
  - morphisms as typed transforms,
  - equivalence as explicit same-behavior/same-value contracts,
  - effects/resources as scoped workflows,
  - backend semantics as typed operational surfaces.

## 6. Unified query + spec language synthesis

### 6.1 Internal convergence points

The repo already has most of the pieces of a future unified query + spec language family:

- `query_ir_v1` + AxQL for machine-facing typed query execution,
- typed olog authoring,
- CQ generation / evaluation / repair,
- proposal validation and CQ gating,
- typed refinement handles shared across query / olog / migration / reconciliation,
- semantic evolution preview primitives,
- canonical `.axi` parsing / formatting / import / export.

### 6.2 Best external precedents

Most useful precedents identified:

- **CQL** for query + constraints + migration,
- **Gherkin / Cucumber** for executable behavior/spec layers,
- **TypeQL** for typed schema-first query surfaces,
- **GraphQL** for self-describing/introspectable typed query UX.

Main conclusion:

- Axiograph should likely compile human-readable scenarios/spec fragments into existing typed IRs,
- not replace the current IR stack with step-definition indirection or a query-only surface.

## 7. Evidence / support synthesis

Main conclusion from the earlier audit:

- runtime “support” is still computed by traversing ordinary ontology facts/edges,
- `support_summary` is still a bolt-on runtime payload,
- examples and docs still model `Evidence`, `EvidenceSupports`, `JustificationPath`, and similar concepts as ordinary ontology nouns,
- and the next slice should introduce a stronger runtime `SupportSummaryV1` / `SupportWitnessV1` style contract grounded directly in certifiable query proofs.

This slice is now implemented in runtime/tooling form behind the existing `support_summary` wire field.

Runtime tranche outcome (implemented after this note was written):

- `support_summary` stays on the same wire field across `/query`, MCP `axql_run`, and tool-loop `axql_run`,
- its runtime basis is now proof-native over `query_result_v3` witness rows,
- `supported_facts[*].witness_rows` record the supporting row/disjunct references,
- and `contexts` / `evidence` are now framed explicitly as attachment-layer enrichments rather than the support basis itself.

## 8. Industrial harness synthesis

The earlier audit concluded:

- current run / inspect parity exists in MCP/tool-loop,
- CLI is still the outlier,
- and the smallest next harness slice is a shared read-only status/report/readback surface, likely built over `inspect_industrial_harness_run`, while avoiding a premature multi-run/list API.

There is still also an open architectural question about relocating harness-specific orchestration toward examples/binaries once the shared contracts are clean enough.

## 9. Research dump and external references

This section is a durable dump of the core research conclusions from this session, with the main external references grouped by topic.

### 9.1 Applied category / denotational fundamentals

Main conclusions:

- The strongest *core* imports for Axiograph remain:
  - **paths**,
  - **morphisms**,
  - **transport**,
  - **equivalence**,
  - and **dependent typing / indexed artifacts**.
- The strongest *non-core / optimization-layer* concepts remain:
  - **rewrite search strategy**,
  - **Kan / codensity**,
  - **linear/resource typing as user-facing vocabulary**,
  - and heavy categorical surface syntax.
- The practical translation for average programmers is:
  - contexts as explicit environments/scopes,
  - morphisms as typed transforms,
  - equivalence as explicit same-behavior/same-value contracts,
  - effects/resources as scoped workflows,
  - and backend semantics as typed operational surfaces.

External references that drove this conclusion:

- Lean / mathlib / transport / dependent rewrite
- Rocq generalized rewriting
- CQL / categorical data migration
- Catlab
- egglog
- Effect / fp-ts / Cats / Kleisli / Arrow / Eq / Equivalence / Layer / Scope style programming references

### 9.2 Bounded-context / DDD / executable-spec fundamentals

Main conclusions:

- The strongest analogies are **DDD/context mapping + executable specs**, not full formal category theory as a developer-facing layer.
- The most useful DDD/fDDD artifacts for Axiograph are:
  - bounded contexts,
  - context maps,
  - aggregates / consistency boundaries,
  - public DTO / command / event surfaces,
  - executable specs,
  - and functional workflow types.
- The repo should import these as **wrapper-layer artifacts over existing ontology/query/report contracts**, not as a new kernel.

Concrete mapping derived from the research:

- bounded context → explicit context wrapper with stable id + accepted anchor + owned rule scopes + owned CQ/use-case surface
- context map → read-only translation / ACL / bridge metadata between contexts
- aggregate → invariant/consistency boundary wrapper, not a new storage model
- executable spec / behavior case → query/CQ/report-backed scenario contract
- workflow type → typed input/output/error/result pipeline over existing IR and report surfaces

### 9.3 fDDD-specific conclusion

Main conclusions:

- Keep **fDDD** as a documentation umbrella, not a code prefix.
- The useful parts of functional DDD for Axiograph are:
  - smart constructors / constrained types,
  - immutable records / discriminated unions,
  - result-oriented workflow composition,
  - boundary DTO mappers,
  - and command/query/event surfaces.
- The repo already has most of the low-level typed seams needed for this; what is missing is a friendlier wrapper vocabulary and composed reports.

### 9.4 Unified query + spec language conclusions

Main conclusions:

- The repo already contains most of the ingredients of a future unified query + spec language family:
  - `query_ir_v1`,
  - AxQL,
  - typed authoring,
  - typed refinement,
  - CQ repair and evaluation,
  - proposal validation,
  - semantic evolution previews,
  - and canonical `.axi` formatting/import/export.
- The most useful external precedents are:
  - **CQL** for query + constraints + migration,
  - **Gherkin/Cucumber** for executable scenario/spec layers,
  - **TypeQL** for typed schema-first query UX,
  - **GraphQL** for self-describing/introspectable typed query UX.
- The best eventual direction is to compile human-readable spec/scenario surfaces into the existing typed IR family rather than replacing the IR stack with step-definition indirection or a query-only language.

### 9.5 Main external sources

#### Category / transport / typed semantics

- https://github.com/leanprover-community/mathlib4/blob/6643e97efe48371dd1fe90b936ab305f528a4384/Mathlib/CategoryTheory/Groupoid/FreeGroupoid.lean
- https://github.com/leanprover-community/mathlib4/blob/6643e97efe48371dd1fe90b936ab305f528a4384/Mathlib/Tactic/DepRewrite.lean
- https://github.com/leanprover-community/mathlib4/blob/6643e97efe48371dd1fe90b936ab305f528a4384/Mathlib/CategoryTheory/Monoidal/Transport.lean
- https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/README.md
- https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/QueryExpRawSimple.md
- https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/QueryExpFront.md
- https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/PragmaExpCheck.md
- https://categoricaldata.net/papers.html
- https://github.com/AlgebraicJulia/Catlab.jl
- https://github.com/egraphs-good/egglog
- https://agda.readthedocs.io/en/stable/getting-started/a-taste-of-agda.html
- https://github.com/ekmett/kan-extensions/blob/f455d0d6bec741be8243b4ce73cbe830b1eb4704/src/Control/Monad/Codensity.hs

#### Effects / functional programming / average-programmer-friendly semantics

- https://github.com/typelevel/cats/blob/be4a99d885dbac0d99ef64aa896103956ffd6272/core/src/main/scala/cats/arrow/Category.scala
- https://github.com/typelevel/cats/blob/be4a99d885dbac0d99ef64aa896103956ffd6272/core/src/main/scala/cats/arrow/Arrow.scala
- https://github.com/typelevel/cats/blob/be4a99d885dbac0d99ef64aa896103956ffd6272/core/src/main/scala/cats/data/Kleisli.scala
- https://github.com/gcanti/fp-ts/blob/c0a6472121c67a2b083e62fcff13e7d022e39d8f/src/ReaderTaskEither.ts
- https://effect.website/
- https://github.com/tweag/linear-base/blob/e7412cfdaefb322e7f46d71cd9b75b1df7fc87e2/README.md

#### Bounded contexts / DDD / fDDD / executable specs

- https://martinfowler.com/bliki/BoundedContext.html
- https://martinfowler.com/bliki/CQRS.html
- https://learn.microsoft.com/en-us/azure/architecture/microservices/model/domain-analysis
- https://learn.microsoft.com/en-us/azure/architecture/microservices/model/tactical-domain-driven-design
- https://learn.microsoft.com/en-us/dotnet/architecture/microservices/microservice-ddd-cqrs-patterns/domain-events-design-implementation
- https://contextmapper.org/docs/bounded-context/
- https://contextmapper.org/docs/context-map/
- https://github.com/ddd-crew/bounded-context-canvas/blob/cdbd86eb19f75f797424543b11fc0b18f72bbe36/README.md
- https://docs.cucumber.io/bdd/
- https://docs.cucumber.io/bdd/example-mapping/
- https://cucumber.io/docs/gherkin/
- https://github.com/cucumber/cucumber-ruby/blob/afe3b553685f084a108bbb1629e564c9a9dde2e5/features/docs/gherkin/background.feature#L1-L13
- https://github.com/dotnet/eShop/blob/9b4f9434f46fdc5c1a6e9e936af2868340cdbc48/src/Ordering.API/Application/Commands/CreateOrderCommand.cs#L3-L84
- https://github.com/dotnet/eShop/blob/9b4f9434f46fdc5c1a6e9e936af2868340cdbc48/src/Ordering.API/Application/IntegrationEvents/Events/OrderStartedIntegrationEvent.cs#L3-L12
- https://github.com/eventflow/EventFlow/blob/4c070a03846ef8ecda4166a12d1165e157d766da/README.md#L240-L339
- https://github.com/eventflow/EventFlow/blob/4c070a03846ef8ecda4166a12d1165e157d766da/Source/EventFlow.Examples.Shipping/Domain/Model/CargoModel/CargoAggregate.cs#L30-L55
- https://github.com/eventflow/EventFlow/blob/4c070a03846ef8ecda4166a12d1165e157d766da/Source/EventFlow.Examples.Shipping/Domain/Model/CargoModel/Commands/CargoBookCommand.cs#L30-L49
- https://github.com/swlaschin/DomainModelingMadeFunctional/blob/8153616b1dc0d5a0bb9e965cbe14a46b0dd4f3cf/src/OrderTaking/PlaceOrder.PublicTypes.fs#L7-L123
- https://github.com/swlaschin/DomainModelingMadeFunctional/blob/8153616b1dc0d5a0bb9e965cbe14a46b0dd4f3cf/src/OrderTaking/PlaceOrder.Dto.fs#L8-L364
- https://github.com/swlaschin/DomainModelingMadeFunctional/blob/8153616b1dc0d5a0bb9e965cbe14a46b0dd4f3cf/src/OrderTaking/PlaceOrder.Api.fs#L4-L130
- https://github.com/swlaschin/DomainModelingMadeFunctional/blob/8153616b1dc0d5a0bb9e965cbe14a46b0dd4f3cf/src/OrderTaking/PlaceOrder.Implementation.fs#L18-L120
- https://github.com/tonyx/Sharpino/blob/df4c90897ff1f71597a16026f89cc09525745327/README.md#L18-L45

#### Ontology engineering / competency questions / design patterns

- https://github.com/OpenEnergyPlatform/ontology/wiki/writing-competency-questions
- https://ceur-ws.org/Vol-4176/caos-9.pdf
- http://ontologydesignpatterns.org/index.php?title=Main_Page
- https://github.com/odpa/patterns-repository

## 10. Outstanding work

### Immediate next implementation candidates

1. **Support primitive tranche**
   - implemented in runtime/tooling form,
   - further follow-on work would now be example/doc leakage cleanup rather than the core support contract itself.

2. **DDD/fDDD/context wrapper follow-on tranche**
   - the read-only bounded-context report layer is now implemented,
   - the next likely adjacent wrapper artifacts are:
     - `BehaviorCaseV1`
     - `ContextBridgeV1`
     - `OutcomeMatchV1`
     - `CaseReceiptV1`
   - all should still be built on top of existing CQ / semantic-claim / evolution-preview / trust surfaces.

3. **Industrial harness parity tranche**
   - status/report/readback in CLI,
   - likely thin projection over existing inspect bundle.

4. **Query + spec language design-to-implementation bridge**
   - continue with additive convergence over existing IR/refinement/evolution/CQ seams,
   - avoid a ground-up language redesign.

### Documentation work still pending

- curate and publish the external references/readings gathered in this session into longer-lived docs beyond this session artifact,
- add a durable DDD/fDDD/context crosswalk into repo docs,
- keep the retired path-cert primitive documented only as historical context,
- document the new bounded-context report primitive and where it sits in the larger DDD/fDDD/context roadmap,
- and produce a durable note about unified query + spec language constraints and non-goals.

## 11. Current todo state

Completed by restart and push into clean context and reference docs:

- focused DDD / bounded-context / denotational-semantics research,
- synthesis into a core-vs-extension plan,
- concrete plan for path-cert tranche,
- implementation and verification of tranche 1 path certification,
- implementation and verification of the first read-only bounded-context report tranche,
- implementation and verification of the runtime support primitive tranche,
- session research dump with grouped external sources appended to this document.

Pending / in progress:

- unified query + spec language research/design lane,
- next wrapper-layer tranche after bounded-context reporting,
- industrial harness parity/relocation tranche,
- docs/readings updates beyond this session artifact.

## 12. Retired path-cert tranche follow-up

The read-only review of the old path-cert tranche identified CLI contract drift,
empty-path behavior splits, verifier configuration drift, and anchor/module
selection fragility. The greenfield cleanup resolved those by deleting the
public relation-id path-cert surfaces rather than documenting another parallel
contract. The remaining work is to keep typed query witnesses, route previews,
transport previews, and runtime-theory reports as the user-facing paths.

These are not blockers for the tranche itself — the code passed focused tests, full crate tests, manual CLI QA, and `make verify-semantics` — but they are the concrete cleanup items the review surfaced.

## 13. Session index

### High-value sessions directly reread for this document

- `ses_248d14647ffej2SH7rEQx24ts1` — path primitive seam audit
- `ses_248d14644ffe080zq65r1kFdnY` — evidence/support seam audit
- `ses_248d14641ffcpa0YF6l9yv5pWB` — industrial harness seam audit
- `ses_2448e7b83ffeVjjGFKrnp9tzCZ` — core-vs-extension oracle ranking
- `ses_2448fd537ffelGl4Z8kxNU0YW6` — bounded context / BDD / ontology research
- `ses_2448fd536ffeG74YNHdioMLX2m` — operational behavior-compiler / `ContextCell` framing
- `ses_2447da51fffezvXOV2NMt4mqp3` — TDD-first path-cert implementation plan
- `ses_2446b1debffe1Q1CUXaAzU0yBl` — path-cert implementation session
- `ses_2446b1decffeHwicbLV28b2r2y` — query+spec convergence seam audit
- `ses_2446b1de7ffe1e0ep3cgfbKOYy` — external query/spec precedents research
- `ses_2440eebd2ffeHqy05YZiJQHRk5` — DDD implementation seam mapping
- `ses_2440ee3e3ffebOY1aOIUt6w22z` — DDD/fDDD practical artifact research
- `ses_2440ee3e2ffeUYqbcmQcO2dJey` — DDD/context tranche ranking
- `ses_2440ee3deffePqbGxINFxbS7Rh` — DDD wrapper vocabulary framing
- `ses_244085f9cffeqVzbDCeQC85rsU` — Oracle review of path-cert tranche

### Earlier subagent inventory inherited from prior compaction context

This session also inherits the earlier subagent inventory captured in the compacted context, including:

- path primitive work,
- evidence parity work,
- industrial harness work,
- semantic VCS hardening,
- homotopy/user-facing cleanup,
- MCP pattern research,
- industrial ontology research,
- and multiple plan/review sessions.

Those prior session ids should still be treated as part of the active working context even where they were not all reread individually in this pass.

## 14. Practical next move

The next clean execution order remains:

1. implement the **support primitive** tranche,
2. implement the **industrial harness parity/readback** tranche,
3. then either:
   - finish **industrial harness parity**, or
   - extend the DDD wrapper layer with **BehaviorCaseV1**,
4. continue the **query + spec language** convergence work from the existing IR family,
5. then revisit richer context bridges / outcome matches / receipts.

The main discipline from this session is unchanged:

- keep the **core smaller and harder**,
- keep **DDD/context** as thin wrappers over the real typed/query/report/evolution seams,
- and keep **advanced theory vocabulary** out of average-programmer-facing surfaces unless it has already stabilized as an operational contract.
