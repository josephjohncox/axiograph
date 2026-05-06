# Runtime Typechecker Audit (2026-04)

**Diataxis:** Reference  
**Audience:** contributors

This note records the current gap between the runtime typechecker/usefulness
story in the docs and the Rust surfaces that are actually available today.

It is intentionally scoped to three questions:

- what is already useful today,
- what is still missing for higher-order/dependent-style ontology engineering,
- and what is still missing for typed exploration and coding-agent workflows.

## Confirmed useful slice today

The runtime already has a real first slice for typed ontology work:

- `rust/crates/axiograph-cli/src/query_ir.rs`
  - `QueryIrV1::prepare_with_meta`
  - `PreparedQueryV1`
  - `PreparedQueryV1::exploration_view`
- `rust/crates/axiograph-cli/src/axql.rs`
  - `AxqlElaborationReport`
  - `AxqlTypedHoleV1`
  - `AxqlExplorationSuggestionV1`
- `rust/crates/axiograph-cli/src/db_server.rs`
  - `POST /query`
  - `POST /discover/check-olog`
  - `POST /semantic/coverage`
  - `POST /semantic/agent-report`
- `rust/crates/axiograph-cli/src/semantic_claim.rs`
  - `RuntimeRuleCatalogV1`
  - `CoverageReportV1`
  - `AgentEngineeringReportV1`

That means Axiograph already supports:

- typed query preparation over `query_ir_v1`,
- inferred variable types,
- typed holes for a narrow class of schema ambiguities,
- trust contracts with explicit non-claims,
- typed olog fragment checking,
- and agent-facing semantic coverage/report payloads over the current
  runtime-visible rule surface.

This is enough for useful runtime-assisted ontology exploration.

It is not yet enough to claim a runtime typechecker that is broadly useful for
higher-order/dependent ontology engineering.

## Main gaps

### 1. The runtime module typechecker is still too thin on theory, even though it is no longer schema-only

`rust/crates/axiograph-pathdb/src/axi_module_typecheck.rs` currently checks:

- schema existence,
- object assignments,
- relation tuple field sets,
- tuple field type compatibility,
- and a first runtime slice of theory objects such as structured constraints and
  path/rewrite declarations.

But `typecheck_axi_v1_module` still does not typecheck theory contents in a
deep enough way for the higher-order/dependent ontology story.

Missing consequences:

- no strong runtime check for the denotational meaning of path equations as
  theory objects,
- no strong runtime check for rewrite-rule admissibility against the intended
  path/rewrite semantics,
- no runtime check for theory-level higher-order/dependent obligations,
- and no typed runtime witness that a reviewed module is theory-sound even in a
  limited fragment.

This is the biggest blocker to the claim that the runtime typechecker is useful
for ontology engineering beyond schema-aware querying.

### 2. The live kernel IR is still too schema-heavy and too stringly

`rust/crates/axiograph-pathdb/src/kernel_ir.rs` currently exposes:

- `CompiledSchemaIr`,
- `RelationSemanticsIr`,
- `RoleIr`,
- `CarrierSpecIr`,
- `WitnessViewIr`,
- and a first theory-shaped slice (`ConstraintIr`, `PathEquationIr`,
  `RewriteRuleIr`, `TheoryIr`) around compiled schema semantics.

It still does not expose the fuller semantic spine described in
`docs/reference/KERNEL_IR.md`:

- `KernelModuleIr`,
- `TheoryIr`,
- `InstanceIr`,
- deterministic ids for relation/rule/object/role/equation handles,
- or first-class theory/path-equation/rewrite objects.

The live IR also still uses string names for most semantic references
(`name`, `target_type`, relation lookup by `&str`) rather than a stable typed id
surface.

Missing consequences:

- query elaboration cannot return stable semantic object handles,
- authoring/coverage/agent reports cannot round-trip through one shared IR id
  family,
- and higher-order/dependent exploration cannot cite theory-level objects as
  first-class runtime items.

### 3. Query exploration is real, but still first-order and display-oriented

`rust/crates/axiograph-cli/src/query_ir.rs` and
`rust/crates/axiograph-cli/src/axql.rs` provide a useful exploration shell, but
the live `query_ir_v1`/hole model is still narrow:

- `QueryIrV1` only covers first-order query atoms (`type`, `edge`, `attr_*`,
  `fact`, `shape`, `has_out`, `attrs`);
- relation names, type names, role names, and paths are still strings;
- `AxqlTypedHoleKindV1` only covers:
  - `AmbiguousFactRelationSchema`
  - `AmbiguousEdgeRelationSchema`
  - `EndpointTypingDeferred`
- `AxqlExplorationSuggestionV1` now includes
  `refinement_candidates: Vec<AxqlRefinementCandidateV1>` where each candidate
  carries:
  - a stable typed handle (`handle.id`, `handle.op`, `handle.scope`),
  - a human preview fragment,
  - and optional relation/schema/role metadata.
- `PreparedQueryV1` now exposes typed apply helpers over those handles and
  returns refined `query_ir_v1` plus trust/introspection deltas.

Missing consequences:

- no first-class holes for unresolved roles, context restrictions, relation
  objects, result-shape obligations, or theory obligations,
- the current apply protocol is still conservative:
  it only targets a single conjunctive query body and does not yet support
  disjunct-targeted refinement or higher-order theory-object refinement,
- and no typed higher-order surface for querying theory/rule/equation objects
  as first-class semantic terms.

This is why the docs should continue to present hole-driven exploration as an
implemented first slice with a significant next step, not as a completed typed
editor/runtime loop.

### 4. Typed authoring checks now expose a first typed-hole slice, but not yet a full repair/proof protocol

`rust/crates/axiograph-cli/src/typed_authoring.rs` already gives:

- `CheckedOlogFragmentV1`,
- `OlogTypedHoleV1`,
- repair-oriented diagnostics,
- typed holes for missing relation roles, role/type mismatches, projection
  holes, and path endpoint failures,
- and a typed change summary over relation/object/path primitives.

That is enough to make authoring materially closer to the query-side
hole/exploration model.

But the authoring surface still stops short of a full higher-order/theory-rich
repair protocol:

- `OlogPathEquationV1` uses string step ids,
- typed olog repairs now have reusable refinement handles and runtime apply
  helpers, including the live `/discover/check-olog` server path,
- and path equations are only checked for compositional endpoint shape, not as
  theory-valid equations in a runtime theory fragment.

Missing consequences:

- typed authoring now shares the same machine-applicable refinement-handle
  protocol used by queries, migration preview, reconciliation review, and CQ
  repair,
- editors/agents can apply basic olog repairs through the same runtime
  refinement API family,
- and authoring cannot yet expose expected repair/proof obligations as typed API
  data.

### 5. Coding-agent report surfaces are live on the DB server, but not yet a uniform tool-loop currency

`rust/crates/axiograph-cli/src/db_server.rs` exposes typed report endpoints:

- `/discover/check-olog`
- `/semantic/coverage`
- `/semantic/agent-report`

But the REPL/LLM tool registry in `rust/crates/axiograph-cli/src/llm.rs` does
not expose equivalent first-class tools for those report families. The tool-loop
surface currently centers on:

- `axql_elaborate`
- `axql_explore`
- `axql_run`
- proposal helpers
- proposal-adapter helpers

Missing consequences:

- coding agents do not consume the same typed engineering report family across
  REPL, DB server, and tool-loop workflows,
- semantic coverage and agent-report objects are harder to make part of the
  default agent loop,
- and “typed semantic service mode” remains truer on the DB/API surface than in
  the in-process tool-loop.

### 6. Agent/coverage/runtime-rule reports still use string ids rather than stable semantic handles

`rust/crates/axiograph-cli/src/semantic_claim.rs` is a useful reporting layer,
but it still uses stringly ids such as:

- `RuntimeRuleScopeV1 { scope_id: String, schema: String, relation: Option<String>, theory: Option<String> }`
- `RuntimeRuleV1 { rule_id: String, ... }`
- `CoverageEdgeV1 { surface_id: String, rule_id: String, ... }`
- `AgentEngineeringReportV1 { matched_scope_ids: Vec<String>, matched_rule_ids: Vec<String>, ... }`

This is good enough for reports.

It is not yet good enough for a runtime typechecker that should feed typed
repair, typed diff, typed migration, and typed agent actions over one semantic
object graph.

## What to build next

If the goal is to make the runtime typechecker materially more useful for
higher-order/dependent ontology engineering and coding-agent workflows, the next
pieces should be:

1. Extend the runtime module checker from schema/instance checks to a limited
   theory fragment:
   - path equations,
   - rewrite-rule admissibility,
   - theory object indexing,
   - explicit runtime non-claims where the fragment stops.
2. Promote the live kernel IR from `CompiledSchemaIr` to a shared module/theory
   IR with deterministic per-object ids.
3. Replace string suggestions in query/authoring exploration with
   machine-applicable refinement objects plus an apply/refine API.
4. Unify query holes and authoring holes into one anchor-scoped repair model.
5. Expose semantic coverage / agent engineering / typed olog checks as
   first-class tool-loop tools, not only DB server endpoints.
6. Move agent/coverage/rule reports from synthetic string ids to stable semantic
   refs derived from the compiled IR.

## Positioning guidance

Until those pieces exist, the honest runtime claim is:

> Axiograph has a useful runtime typechecker/exploration layer for schema-aware
> query elaboration, typed trust reporting, narrow typed olog checks, and
> agent-facing semantic summaries, but it does not yet provide a full runtime
> higher-order/dependent ontology engineering surface.
