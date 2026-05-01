# Kernel IR

**Diataxis:** Reference  
**Audience:** contributors

This document specifies the target canonical ontology IR for Axiograph.

Accepted `.axi` remains the reviewable source of truth. The kernel IR is the
compiled semantic form that:

- Lean semantics will target,
- Rust typed execution will consume,
- PathDB will lower from,
- and RDF / property-graph / migration layers will project from.

Embeddings, vector indexes, ANN scores, retrieved chunks, and embedding-derived
relationship suggestions are not kernel IR. They are evidence/index sidecars
that may point at compiled IR ids and may emit typed proposals for review. See
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.

Current implemented slice (2026-04):

- `axiograph_pathdb::kernel_ir` currently provides:
  - `CompiledSchemaIr`
  - `SchemaCategoryIr`
  - `InstanceFunctorIr`
  - `RelationSemanticsIr`
  - `RoleIr`
  - deterministic semantic ids for compiled schema objects
    (`ObjectTypeId`, `RelationId`, `RoleId`)
  - `RoleKind::{Data, Context, Temporal}`
  - `CarrierSpecIr`
  - `WitnessViewIr`
- `SchemaCategoryIr` is now the first runtime category-shaped view over a
  compiled schema:
  - object types and relation objects are category objects,
  - relation roles lower to projection arrows,
  - subtype declarations lower to inclusion arrows,
  - and binary graph edges remain derived traversal views rather than kernel
    arrows.
- `InstanceFunctorIr` is now the first runtime interpretation of an instance as
  a functor out of that schema category:
  - object types map to closed membership sets,
  - relation objects map to stable fact-id sets,
  - role projections map fact ids to typed role values,
  - and subtype inclusions transport subtype members into supertype images by
    identity.
- the first runtime `TheoryIr` slice now exists in the compiled IR:
  - `ConstraintIr`
  - `PathEquationIr`
  - `OpaqueEquationIr`
  - `RewriteRuleIr`
  - `TheoryTouchedRoleIr`
  - `TheoryObligationRefIr`
  - `TheoryObligationKindIr`
  - `TheorySubjectRefIr`
  - `TheorySubjectKindIr`
  - deterministic theory-object ids
    (`TheoryId`, `ConstraintId`, `EquationId`, `RewriteRuleId`)
  - deterministic runtime-addressable obligation refs via
    `TheoryIr::obligation_refs()`
  - explicit obligation→subject and subject→obligation cross-links via
    `TheoryIr::subject_refs_for_obligation(...)` and
    `TheoryIr::obligation_refs_for_subject(...)`
  - path equations and rewrite rules retain touched relation roles, so carrier,
    context, temporal, and data roles remain addressable as typed subjects
    instead of disappearing behind relation-level references
  - `TheoryObligationGraphV1`, a deterministic runtime graph of theory nodes,
    obligation nodes, subject nodes, and support/touches edges for shared query
    refinement, CQ repair, migration authoring, and reconciliation tooling
  - `axiograph discover theory-graph <module.axi>`, which emits those graphs as
    JSON for agent/tool-loop inspection
  - `semantic_theory_graph`, the matching semantic tool-loop surface for agents
- runtime theory checking now sits on top of compiled `TheoryIr`:
  - `RuntimeTheoryCheckReportV1`
  - `RuntimeTheoryJudgmentV1`
  - `RuntimeTheoryClosureTierV1::{FiniteFragment, EvidenceWeighted, GlobalIndexed}`
  - `CompletenessClaimV1`
  - `OntologyClosureClaimV1`
  - `RuntimeTheoryAdmissibilityDiagnosticV1`
  - `RuntimeTheoryAssumptionDiagnosticV1`
  - `axiograph check theory <module.axi> --json`
  - `axiograph discover theory-check <module.axi>`
  - `semantic_theory_check`
  This checker makes scoped runtime claims about well-typedness,
  admissibility, closure, and completeness under declared world/evidence/ref
  assumptions. It is still outside the Lean trusted checker. See
  `docs/reference/RUNTIME_THEORY_CHECKER.md`.
- Lean now has a narrow operational semantic VCS scaffold in
  `lean/Axiograph/SemanticVCS.lean`, documented in
  `docs/reference/LEAN_THEORY_EVALUATION.md`. It formalizes finite
  `SemanticSlice` join/meet preservation, conservative merge materialization,
  and rebase transport preservation predicates. It is theorem-support outside
  the shipped verifier boundary, not a complete ontology-lattice proof.
- theory transport is now runtime-addressable:
  - `TheoryTransportPlanIr`
  - `TheoryTransportItemIr`
  - `TheoryTransportStatusIr::{Preserved, Transported, MissingObjectImage, MissingArrowImage, OpaqueOrOutOfFragment}`
  - `build_theory_transport_plan_ir(...)`
  - `TheoryIr::theory_transport_plan(...)`
  These plans classify each compiled theory obligation under a
  `SchemaMorphismV1` before migration/rebase tooling turns it into resolver
  handles. Transport items carry the same typed subject links as the source
  obligation, including touched roles from equations and rewrites. They are
  operational typed transport plans, not Lean certificates.
- semantic slice manifests built by `axiograph sem slice build` now enrich
  semantic-commit refs with compiled `KernelModuleIr` refs from accepted
  canonical modules:
  - schema/category object refs,
  - relation-object refs,
  - role projection arrow refs,
  - subtype inclusion arrow refs,
  - runtime-addressable theory obligations,
  - and instance-functor refs.
  This makes merge/rebase slice planning operate over the same category/theory
  handles as migration and authoring instead of only over commit summaries.
- `compile_theory_ir(...)` is now a checked projector rather than a blind
  format step:
  - structured constraints validate relation/field/param references,
  - constraints now retain compiled relation ids and compiled role ids for
    referenced fields/params,
  - parseable path equations validate against compiled carrier semantics,
  - parseable path equations now retain compiled relation ids and touched role
    refs,
  - rewrite rules validate declared vars, referenced relations, and endpoint
    typing against the same compiled schema slice and retain touched role refs.
- `.axi` import and meta-plane schema semantics now consult this compiled slice
  for carrier inference and witness-view selection instead of repeating
  endpoint heuristics locally.
- `axiograph_cli::semantic_claim` now projects the indexed schema/theory surface
  into a first runtime business-rule catalog:
  `RuntimeRuleCatalogV1 -> RuntimeRuleV1 { rule_id, scope, class, runtime_support, trust_class }`.
  This is a deterministic runtime report layer for agents/review/query tooling,
  not yet the full canonical `TheoryIr`.
- `axiograph_cli::backend_pushdown` now consumes `CompiledSchemaIr` plus
  backend/projection capability profiles and emits typed pushdown plans for the
  current first-class backends:
  - `TypeDbPushdownPlanV1`
  - `TerminusDbPushdownPlanV1`
  These plans are explicit about preserved tuple/role/context structure, native
  read-only query dialect, preserved lower-tier interfaces
  (native query / RDF dataset / SHACL validation where applicable), lifting
  contracts back into the higher typed/meta layer, carrier-edge convenience
  projections, and the trust caveats where backend-native querying becomes a
  reduced lens over Axiograph semantics.
- `BackendPushdownPlanV1::operational_surface()` now exposes the bounded
  agent-facing operational contract over those plans:
  - relation transport summary,
  - context transport strategy and preserved axis bindings,
  - residual obligations that still require Axiograph-side anchor/context
    rechecks or reindexing,
  - and the explicit reconciliation boundary where semantic refs, CQ gates, and
    persisted reconciliation previews remain authoritative.
- `axiograph_cli::query_ir::PreparedQueryMetadataV1` and
  `PreparedQueryExplorationV1` are the current typed compiled-query surfaces
  for reports/editors/agents:
  - stable prepared-query, input-IR, and elaborated-IR ids,
  - inferred types,
  - certifiability/trust metadata and explicit non-claims,
  - optional `KernelRefV1` citations when metadata is built from canonical
    compiled `KernelModuleIr`,
  - typed holes,
  - refinement candidates,
  - semantic claims/coverage,
  - and trust gaps.
- `axiograph_cli::typed_refinement` is the current shared apply/refine envelope
  over those compiled-IR-facing surfaces:
  - one runtime handle/candidate currency,
  - query-local, typed-olog, migration-preview, reconciliation-review, and
    CQ-repair domains inside that envelope,
  - machine-applicable refinement ids,
  - and explicit separation between runtime repair transport and trusted kernel
    proof objects.
- `axiograph_cli::evolution_preview` already exposes the matching compiled-IR
  and reconciliation preview objects for review workflows:
  - `build_compiled_ir_exploration_evolution_preview_v1(...)`
  - `build_migration_evolution_preview_v1(...)`
  - `build_reconciliation_evolution_preview_v1(...)`
- CQ evaluation now surfaces `PreparedQueryMetadataV1` plus shared runtime
  refinement candidates per competency question, rather than only raw query
  strings or trust/coverage strings.
- The full `KernelModuleIr` / shared `InstanceIr` / richer canonical
  `ConstraintIr` enum / broader equation language / certifiable theory proof
  export remain future work.

## Design Rules

1. The canonical semantic form is **relation-as-object + projection arrows**.
2. Binary edges are a **derived traversal view**, not the kernel.
3. `@context` and `@temporal` must survive lowering explicitly.
4. Stable identifiers belong to the IR, not only to storage projections.
5. One kernel IR should feed:
   - Lean-aligned semantics,
   - PathDB lowering,
   - RDF/OWL/SHACL adapters,
   - property-graph projection,
   - and `Δ/Σ/Π` migration machinery.
6. Engineering-facing reports must cite IR-level semantic ids rather than only
   prose labels or storage-local names.

## Why This IR Is The Hinge

The IR matters because “becoming a dependently typed ontology engine” is not
primarily about adding more category-theory terminology. It is about making one
compiled semantic object do real operational work.

In this repo, the IR is the hinge only if all of the following become true:

- authoring deltas and olog edits are expressed over stable IR ids;
- prepared queries elaborate against `SchemaCoreIr` / `TheoryIr` rather than
  raw runtime strings alone;
- migration previews name schema morphisms and transport obligations over the
  same objects and arrows;
- certificates cite the same module / schema / relation / role identifiers that
  the rest of the toolchain uses;
- and semantic diff / merge operate over these objects instead of storage-local
  artifacts.

That is the operational point of the IR: one semantic spine for authoring,
query, migration, certification, and review.

### Engineering-facing consequences

If the IR is genuinely the semantic spine, then these runtime outputs must cite
the same ids too:

- business-rule applicability reports should name theory/rule/object/role ids;
- semantic coverage reports should map code/tests/docs/interop artifacts onto
  those ids or report them as unmapped;
- SHACL/RDF/olog flows should resolve to the same ids when alignment is known;
- and coding-agent reports should point at the same objects when they explain
  why a claim is strong, weak, missing, or drifted.

## Current Operational Surfaces

The repo is no longer at the stage where compiled IR is only a design note. The
current operational seam already has three concrete surfaces:

- typed query/exploration:
  `PreparedQueryMetadataV1`, `PreparedQueryExplorationV1`, and
  `build_compiled_ir_exploration_evolution_preview_v1(...)`
- typed transport / projection review:
  `BackendPushdownPlanV1` plus `BackendPushdownOperationalSurfaceV1`
- typed reconciliation review:
  `ReconciliationPreviewReportV1` plus
  `build_reconciliation_evolution_preview_v1(...)`

Those surfaces are still first slices, not the finished kernel. But they are
already the correct default direction for agents and tooling:

- explore the ontology through compiled/query IR rather than raw token
  heuristics,
- cite prepared query handles and elaborated query ids in CQ/refinement reports
  rather than treating raw query text as the report identity,
- inspect backend pushdown as typed transport plus residual obligations rather
  than as opaque adapter behavior,
- and inspect merge/reconciliation through persisted preview objects rather than
  implicit backend history.

The shared refinement protocol is the operational bridge between those slices.
It is deliberately not a theorem-prover kernel object. It is a compiled-IR
runtime service that preserves:

- stable handle ids,
- typed holes and admissible next moves,
- surface-local repair operations for queries, typed olog authoring,
  migration/reconciliation review, and CQ repair,
- and trust/coverage deltas that can be carried forward into review, migration,
  and reconciliation workflows.

### KernelSurfaceV1 and KernelRefV1

`KernelSurfaceV1` is the intended shared runtime index/report surface over the
compiled kernel slice. It should gather refs from:

- `SchemaCategoryIr` objects, relation objects, projection arrows, subtype
  inclusions, anchors, and carrier/witness-view metadata;
- `TheoryIr` constraints, path equations, opaque equations, rewrite rules,
  theory subjects, touched roles, and obligations;
- `InstanceFunctorIr` object memberships, relation fact sets, projection images,
  subtype transports, and stable fact refs; and
- accepted module anchors plus trust, coverage, and non-claim summaries.

`KernelRefV1` is the corresponding typed reference currency. Reports should use
it when they need to point at a schema object, relation role, category arrow,
theory obligation, theory subject, instance-functor image, stable fact, accepted
anchor, or derived review handle.

This surface is intentionally operational. It is not a Lean proof object, not a
new semantic authority, and not a substitute for accepted `.axi` plus compiled
IR. Its job is to make the same typed refs available to query preparation, CQ
evaluation, migration and transport planning, semantic diff/reconciliation,
backend projection plans, authoring previews, and agent-facing repair reports.
When a report cites `KernelRefV1`, it is saying "this runtime result is indexed
against this compiled semantic handle," not "this claim is certified by Lean."
Strict semantic reports should validate their refs against `KernelSurfaceV1`
before they can be used for promotion, certification, merge materialization, or
strict coverage. User-facing names and labels are ergonomics; compiled
`KernelRefV1` handles are the runtime authority.

The inspection command is:

```bash
axiograph discover kernel-surface path/to/module.axi --out kernel_surface.json
```

The read-only MCP/tool-loop surface is `semantic_kernel_surface` with
`{"axi_text": "..."}`. It returns the same `KernelSurfaceV1` report family and
the same non-claims.

## Top-Level Shape

```rust
pub struct KernelModuleIr {
    pub module_digest: AxiDigest,
    pub schemas: Vec<SchemaCoreIr>,
    pub theories: Vec<TheoryIr>,
    pub instances: Vec<InstanceIr>,
}
```

`KernelModuleIr` is deterministic with respect to canonical accepted `.axi`
bytes. The same accepted module must lower to the same IR.

## SchemaCoreIR

`SchemaCoreIr` is the ontology meaning layer.

```rust
pub struct SchemaCoreIr {
    pub schema_id: SchemaId,
    pub objects: Vec<ObjectTypeDef>,
    pub subtype_inclusions: Vec<SubtypeInclusionDef>,
    pub relations: Vec<RelationObjectDef>,
    pub explicit_arrows: Vec<ArrowDef>,
    pub context_axes: Vec<ContextAxisDef>,
}
```

### Object types

```rust
pub struct ObjectTypeDef {
    pub object_id: ObjectTypeId,
    pub display_name: String,
}
```

### Subtyping

Subtyping is a first-class semantic arrow, not just closure metadata.

```rust
pub struct SubtypeInclusionDef {
    pub arrow_id: ArrowId,
    pub sub: ObjectTypeId,
    pub sup: ObjectTypeId,
}
```

### Relation objects

Relations are canonicalized as objects with ordered roles and projection arrows.

```rust
pub struct RelationObjectDef {
    pub relation_id: RelationId,
    pub tuple_object: ObjectTypeId,
    pub roles: Vec<RoleDef>,
    pub carrier_spec: Option<CarrierSpec>,
}
```

Each relation has a tuple/carrier object even when it will later admit a binary
projection.

### Roles

```rust
pub struct RoleDef {
    pub role_id: RoleId,
    pub name: String,
    pub order: u16,
    pub target: ObjectTypeId,
    pub projection_arrow: ArrowId,
    pub kind: RoleKind,
}

pub enum RoleKind {
    Data,
    Context(ContextAxisId),
    Temporal(ContextAxisId),
    Parameter,
    Evidence,
}
```

Role kinds are required so we do not erase ontology semantics into ad hoc field
names like `ctx` or `time`.

### Explicit arrows

Some schemas may have explicit arrows that are not merely role projections.

```rust
pub struct ArrowDef {
    pub arrow_id: ArrowId,
    pub source: ObjectTypeId,
    pub target: ObjectTypeId,
    pub name: String,
}
```

### Context axes

```rust
pub struct ContextAxisDef {
    pub axis_id: ContextAxisId,
    pub name: String,
    pub axis_kind: ContextAxisKind,
}

pub enum ContextAxisKind {
    World,
    Time,
    Other,
}
```

## CarrierSpec

`CarrierSpec` governs when a relation-object may induce a binary traversal view.

```rust
pub struct CarrierSpec {
    pub source_role: RoleId,
    pub target_role: RoleId,
    pub fiber_roles: Vec<RoleId>,
}
```

Interpretation:

- `source_role` and `target_role` define the binary carrier pair.
- `fiber_roles` are fixed parameters/fibers under which traversal is interpreted.
- roles not in the carrier pair or fiber set remain part of the canonical tuple
  object and are never silently erased.

If a relation has no `CarrierSpec`, it has no direct binary traversal meaning.

## TheoryIR

Theories must stop being a bag of partially structured text.

They are also where business-rule usefulness becomes concrete: constraints,
path equations, and rewrite rules are the stable theory objects that runtime
checker, coverage, and agent-facing reports should cite.

```rust
pub struct TheoryIr {
    pub theory_id: TheoryId,
    pub schema_id: SchemaId,
    pub constraints: Vec<ConstraintIr>,
    pub path_equations: Vec<PathEquationIr>,
    pub opaque_equations: Vec<OpaqueEquationIr>,
    pub rewrite_rules: Vec<RewriteRuleIr>,
}
```

### Path equations

These are semantic equations the typed path/rewrite kernel can interpret.

```rust
pub struct PathEquationIr {
    pub equation_id: EquationId,
    pub lhs: PathExprIr,
    pub rhs: PathExprIr,
}
```

### Opaque equations

These are reviewed semantic statements that are intentionally outside the
current certifiable kernel.

```rust
pub struct OpaqueEquationIr {
    pub equation_id: EquationId,
    pub text: String,
}
```

### Rewrite rules

```rust
pub struct RewriteRuleIr {
    pub rule_id: RewriteRuleId,
    pub lhs: PathExprIr,
    pub rhs: PathExprIr,
    pub source: RewriteRuleSource,
}

pub enum RewriteRuleSource {
    Builtin,
    AcceptedAxi,
}
```

### Runtime rule projection (implemented now)

Before the repo has a fully shared `TheoryIr`, the CLI/runtime already needs a
typed rule surface that coding agents and business-rule review flows can use
without parsing prose.

Today that slice is derived from `MetaPlaneIndex`, not from a completed kernel
module IR:

```rust
pub struct RuntimeRuleCatalogV1 {
    pub version: String,
    pub rules: Vec<RuntimeRuleV1>,
    pub notes: Vec<String>,
}

pub struct RuntimeRuleV1 {
    pub rule_id: String,
    pub class: RuntimeRuleClassV1,
    pub scope: RuntimeRuleScopeV1,
    pub runtime_support: RuntimeRuleSupportV1,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub certification: RuntimeRuleCertificationV1,
    pub summary: String,
}
```

Operational intent:

- `rule_id` is deterministic for the current indexed schema/theory surface, so
  agent-facing reports can point at a stable runtime rule object rather than a
  raw prose string.
- `scope.scope_id` is deterministic at relation or theory scope today:
  `schema/<schema>/relation/<relation>` or `schema/<schema>/theory/<theory>`.
- `class` distinguishes concrete rule families already present in the
  meta-plane index: `functional`, `key`, `at_most`, `typing`,
  `rewrite_rule`, `named_block_constraint`, and the current review-only
  residual cases.
- `runtime_support` and `trust_class` separate rules that are actually used by
  the runtime today (`quality_gate`, `query_planning`, `rewrite_helper`) from
  rules that are only advisory metadata or review-only declarations.

This layer is deliberately modest:

- it is a runtime projection over the live indexed ontology surface;
- it is useful for agent tooling, semantic summaries, and business-rule review;
- but it is not yet the canonical `TheoryIr`, and it does not by itself expand
  the trusted Lean-checked kernel.

## InstanceIR

`InstanceIr` is the tuple/fact interpretation of data.

```rust
pub struct InstanceIr {
    pub instance_id: InstanceId,
    pub schema_id: SchemaId,
    pub object_members: Vec<ObjectMembership>,
    pub relation_facts: Vec<RelationFactIr>,
}
```

```rust
pub struct RelationFactIr {
    pub fact_id: StableFactId,
    pub relation_id: RelationId,
    pub role_values: Vec<RoleValueIr>,
}

pub struct RoleValueIr {
    pub role_id: RoleId,
    pub value: StableValueRef,
}
```

Interpretation:

- each relation fact is a tuple object,
- each role projection is total for that tuple,
- and the stable fact identifier is part of the canonical semantic layer.

## TraversalView

`TraversalView` is explicitly derived from the kernel IR. It is not part of the
meaning plane.

```rust
pub struct TraversalView {
    pub binary_generators: Vec<TraversalGenerator>,
}

pub struct TraversalGenerator {
    pub relation_id: RelationId,
    pub source_role: RoleId,
    pub target_role: RoleId,
    pub fiber_roles: Vec<RoleId>,
}
```

This is what query planning, RPQ elaboration, and binary-edge convenience APIs
should use instead of re-deriving endpoints heuristically from field names.

## Lowering Rules From `.axi`

### Objects and subtypes

- `object T` lowers to `ObjectTypeDef`.
- `T <: U` lowers to `SubtypeInclusionDef`.
- the compiled IR should retain both supertypes and admissible subtypes so
  typed tooling can answer:
  - which refinements inhabit a given supertype,
  - which player types are valid at a role interface,
  - and which subtype-sensitive repairs or exploration moves are legal.

### Role interfaces

TypeDB is a useful reference point here, but Axiograph should import the idea
rather than the surface syntax:

- every role projection is a first-class typed interface,
- the role's declared target type is its interface supertype,
- admissible players are that type plus all compiled subtypes,
- and query/authoring/pushdown surfaces should expose the scoped role name
  explicitly, e.g. `Approval:approver`.

That means the compiled IR should make role interfaces directly available for:

- typed olog binding suggestions,
- subtype-aware query elaboration,
- backend pushdown plans,
- and semantic evolution previews that explain when a role is being pushed down
  to or pulled up from a subtype.

### Relations

- every declared relation lowers to one `RelationObjectDef`,
- every field lowers to a `RoleDef`,
- role order is preserved,
- `@context` and `@temporal` become `RoleKind::Context(..)` /
  `RoleKind::Temporal(..)` rather than disappearing into a naming convention.

### Binary relations

A binary relation may receive a `CarrierSpec` if and only if:

- the carrier pair is explicit, or
- the lowering rule is unambiguous under the schema's declared role structure.

Default heuristic target:

- if there are exactly two non-axis data roles, use them as the carrier pair.
- otherwise require explicit carrier metadata and do not guess.

## Stable IDs

All kernel objects must have deterministic ids derived from canonical module
content, not storage-local integers.

Required ids include:

- `SchemaId`
- `ObjectTypeId`
- `RelationId`
- `RoleId`
- `ArrowId`
- `ContextAxisId`
- `TheoryId`
- `ConstraintId`
- `EquationId`
- `RewriteRuleId`
- `StableFactId`

These ids are the semantic handles that Rust typed APIs, certificates, and VCS
history should prefer.

## Backends

### PathDB lowering

PathDB lowers from `InstanceIr`:

- object members become entities,
- relation facts become fact/tuple nodes,
- role projections become field edges,
- context/world axes become explicit scoping edges and indexes,
- binary edges become optional convenience projections from `TraversalView`.

### RDF lowering

RDF is a boundary projection:

- relation facts become reified resources or edge objects,
- role projections become predicates,
- named graphs map to explicit context/world axes.

### Property graph lowering

Property graph export is also a projection:

- object members project to nodes,
- relation facts project to relationship entities,
- direct LPG edges are only emitted when the relation has an unambiguous carrier
  pair and no semantically significant extra payload is lost.

### Migration

`Δ/Σ/Π` should operate over `SchemaCoreIr`, not over PathDB's derived binary-edge
view.

### Migration As Transport And Reindexing

Migration is not merely “copy data through an adapter”. In the semantic kernel
story it is:

- typed transport along a named schema/theory map, and
- explicit reindexing of semantic ids so source and target artifacts remain
  comparable across anchors.

Transport answers which object/arrow/relation/path structure is preserved,
reinterpreted, or made residual. Reindexing answers which source ids and target
ids are "the same enough" for CQ diff, semantic coverage, migration preview,
and later certification to speak precisely.

That is why migration preview should cite:

- the schema morphism or transport basis,
- the reindexing links or preserved ids,
- transported path equations / rewrite obligations,
- and residual obligations for anything not yet carried soundly.

### Reconciliation As Typed Evolution

Reconciliation is not file merge. It is typed evolution over competing semantic
deltas rooted at a common anchor:

- enumerate conflicts over schema/theory/instance/context artifacts,
- record explicit operator or policy decisions,
- attach CQ/trust/coverage consequences,
- and keep unresolved obligations first-class.

The runtime slice does not yet claim a full categorical merge calculus. The
current requirement is narrower and still useful: reconciliation objects and
previews must carry stable artifact ids, conflict sets, decisions, and
residuals in the same semantic language used by authoring, migration, and
querying.

## Exploration Surfaces

Ologs are the most legible human-facing frontend, but they are not the only
exploration mode.

The same compiled IR should also drive:

- query refinement and typed-hole filling,
- migration authoring and transport preview,
- reconciliation review,
- rule/CQ browsing,
- backend projection review,
- and implementation-surface mapping.

The operative principle is one semantic spine: every exploration surface should
talk about the same object ids, relation-object ids, role ids, rule ids, and
anchors.

## Olog Surface

Ologs are a frontend to the same IR, not a separate semantic subsystem.

The authoring surface should lower:

- boxes to object types,
- aspects to arrows or relation-object patterns,
- commutative diagrams to `PathEquationIr`,
- n-ary relationships to relation objects with roles.

## Remaining Convergence Work

The remaining convergence work for this spec is:

1. Add a shared kernel IR crate or module pair:
   - `schema_category_ir.rs`
   - `instance_ir.rs`
2. Preserve `@context` / `@temporal` in lowering.
3. Replace endpoint heuristics with `TraversalView`.
4. Rebase migration/category scaffolding on `SchemaCoreIr`.
5. Keep PathDB storage layout stable while changing the semantic lowering path.
6. Keep extending prepared-query and migration-preview forms so reports cite
   IR-level ids rather than raw surface names alone.
7. Make certificate payloads and semantic diffs name the same stable IR objects
   used by authoring and query tooling.
8. Make business-rule applicability, semantic-coverage, and agent-facing
   reports cite the same stable IR ids rather than ad hoc labels.
