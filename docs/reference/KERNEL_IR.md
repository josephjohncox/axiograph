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

Current implemented slice (2026-04):

- `axiograph_pathdb::kernel_ir` currently provides `CompiledSchemaIr`,
  `RelationSemanticsIr`, `RoleIr`, `RoleKind::{Data, Context, Temporal}`,
  `CarrierSpecIr`, and `WitnessViewIr`.
- `.axi` import and meta-plane schema semantics now consult this compiled slice
  for carrier inference and witness-view selection instead of repeating
  endpoint heuristics locally.
- The full `KernelModuleIr` / `SchemaCoreIr` / `TheoryIr` / `InstanceIr` /
  deterministic per-object ids remain future work.

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
- `TheoryId`
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

## Olog Surface

Ologs are a frontend to the same IR, not a separate semantic subsystem.

The authoring surface should lower:

- boxes to object types,
- aspects to arrows or relation-object patterns,
- commutative diagrams to `PathEquationIr`,
- n-ary relationships to relation objects with roles.

## First Implementation Slice

The first implementation cut for this spec should:

1. Add a shared kernel IR crate or module pair:
   - `schema_category_ir.rs`
   - `instance_ir.rs`
2. Preserve `@context` / `@temporal` in lowering.
3. Replace endpoint heuristics with `TraversalView`.
4. Rebase migration/category scaffolding on `SchemaCoreIr`.
5. Keep PathDB storage layout stable while changing the semantic lowering path.
