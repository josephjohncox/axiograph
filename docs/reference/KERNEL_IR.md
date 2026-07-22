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

## Implemented Canonical Compiler Contract

The authoritative compiler is `axiograph_kernel::CanonicalCompiler` in
`rust/crates/axiograph-kernel/src/package.rs`. It accepts a
`KernelCompilationRequest` containing:

- exact source bytes wrapped by `CanonicalModuleSource`;
- the root module name;
- the complete import closure;
- an explicit `RepositoryIdV2`; and
- an immutable accepted `SnapshotIdV2`.

The compiler rejects missing, duplicate, and conflicting modules. It computes
module and package identities from length-framed exact bytes, so comments,
whitespace, import order, and declaration order remain identity-significant.
It does not normalize or reserialize source before hashing. The canonical
package path uses full SHA-256 identities and canonical JSON manifests; it does not
use bincode or FNV.

A successful compilation returns one immutable `CompiledKernelSnapshot` with:

1. `KernelSnapshotIr`: package/import-closure identity plus schema, theory, and
   instance IR under the accepted snapshot handle;
2. `SchemaPresentationIr`: objects, relation objects, ordered role-projection
   generators, explicit aspects/functions, subtype inclusions, endpoint-indexed
   paths, forward category equations, contextual congruence witnesses,
   relation-span free-groupoid equations, and derived category-formation
   evidence;
3. `TypedTheoryIr`: constraints, equations, rewrites, and explicit non-claims
   addressed to the presentation; and
4. `InstanceModelIr`: finite carriers, relation facts, total generator
   interpretations, checked equations, one dependent witness per ordered fact
   role, context/world witnesses, and an explicit checked lifecycle state.

`CompiledKernelSnapshot::payload_fingerprints()` emits one strictly sorted
`KernelPayloadFingerprintV2` for every addressable `KernelRefV2`. The digest is
domain-separated by payload kind and covers the complete serialized compiled
payload at that typed, revision-scoped address. Parent containers and their
addressable children are both committed: for example, a relation fingerprint
covers its ordered role list, while each role also has its own fingerprint.
The same applies to schemas/objects, theories/constraints/equations/rewrites,
and instances/facts.

Two different digests are intentionally not conflated:

- `KernelSnapshotIr::ir_digest()` commits the compiler's canonical digest
  payload and is used by in-process IR/projection reports;
- `AcceptedBuildManifest.kernel_ir_digest` is the immutable AxiStore object id
  of the serialized `KernelSnapshotIr` bytes.

`AxpdBuildSpec::from_accepted_kernel` recomputes the second digest with the
`KernelIr` object-kind framing before materialization. Comparing the manifest
object id directly with the inner `ir_digest()` is wrong and previously made a
real compiled snapshot impossible to materialize.

AxiStore's merge checker uses this index instead of set inclusion over refs.
Two refs that look structurally related are not a keep unless both the typed ref
and payload fingerprint match exactly. Changed payloads require an explicit
drop+introduce or typed transport decision. AxiStore recompiles candidate
closures from stored exact `.axi` bytes and reproduces the fingerprint index
before materialization. These hashes establish finite payload identity, not
semantic equivalence between arbitrary categorical presentations or a Lean
proof of transport.

Relation objects are first-class schema objects. Every declared role becomes a
projection generator, including relation-valued roles. Binary edges are not
kernel primitives. Explicit aspects/functions and subtype inclusions use the
same generator representation. Subtype closure is checked for cycles and
coherence before IR construction.

### Executable finite theory payloads

`SchemaPresentationIr` is the sole compiled category presentation. It records
the exact object order, typed object and relation-object identities, role
projections in declared order, explicit generators, endpoint-indexed paths, and
accepted parallel-path equations. Its `category_formation` field contains only
derived evidence: one identity path per object, checked lifecycle state,
explicit non-claims, and a deterministic finite reachability saturation
certificate. `CategoryFormationIr::verify` checks the presentation and replays
every saturation tree built from identity, generator, and typed composition
nodes. It requires the entry set to equal the exact finite transitive closure and
rejects wrong endpoints, missing seeds, duplicates, extra pairs, bad indexes,
unknown algorithms, equation/congruence drift, and lifecycle/evidence drift.
Presentations beyond 64 objects or 4,096 arrows remain compilable but carry a
blocking `saturation_bound` residual instead of a false completeness claim.

Forward `SchemaPathIr` uses an endpoint-checked flat generator sequence: the
empty sequence is identity, concatenation is composition, and associativity and
unit normalization are structural. `PathCongruenceCertificateIr` replays a
presented equation inside a prefix/suffix context. `FormalGroupoidPathIr` adds
signed generators; normalization cancels adjacent inverse pairs. A formal
inverse is runtime-executable only when the generator is explicitly reversible.
Canonical `step(from, Relation, to)` equations compile to two role projections
(an inverse source projection followed by a target projection) in
`formal_groupoid_equations`. Rust checks their endpoints but does not claim that
formation proves rewrite termination, confluence, or model satisfaction.

### Exact-byte Rust/Lean category boundary

The trusted wire family is `category_kernel_v3`. Rust projects one
`SchemaPresentationIr` to deterministic object and arrow indexes without
changing declaration order or inventing a second presentation. The payload
contains:

- object-type and relation-object names in canonical presentation order;
- role-projection, subtype, aspect, and function arrows with typed endpoints;
- one explicit empty identity path per object;
- forward parallel-path equations;
- one contextual congruence replay witness per equation; and
- complete bounded generator-reachability explanations.

`VerifyMain` loads the anchored `.axi` bytes, Lean parses those exact bytes,
`compileAxiSchemaPresentation` forms the same relation-as-object presentation,
and `categoryKernelPresentationV3` exports Lean's deterministic name/index
view. Verification requires literal equality with the Rust payload before it
replays congruence and saturation. A changed object order, role order, arrow
kind, endpoint, identity, equation side, congruence offset, lifecycle, or
reachability explanation rejects.

Run `make verify-lean-e2e-category-kernel-v3` for the regulated-shipment
positive path plus formation, congruence, and saturation tamper rejections. The
broader finite-theory regression suite remains `make verify-lean-theory`.

Every `InstanceModelIr` reifies each tuple field as a
`RoleIndexedWitnessIr`, including the role id, declared order, target type, role
kind, and typed value. Context and world roles also appear as
`ScopeWitnessIr`. The implemented transport fragment is identity transport,
and issuing it revalidates the complete instance plus exact scope-witness
membership. Non-identity transport without a declared rule returns a typed
`UnsupportedTransport` error. Typed path holes bind schema, endpoints, and
the typed-hole residual; they stay in `Residual` until a caller selects and
rechecks a matching candidate.

`CompiledKernelSnapshot::finite_theory_gate_receipt` is the shared untrusted
runtime gate seam. Authoring validation stores an `Authoring` receipt,
prepared-query metadata stores a `Query` receipt, and AxiStore canonical
candidate recompilation requires a passing `Merge` receipt before payload
comparison or protected-main materialization. The sole trusted category wire
format is anchored `category_kernel_v3`: it carries the compiler's finite
name/index projection, exact identities, parallel equations, contextual
congruence witnesses, and bounded saturation explanations for independent Lean
reconstruction and replay.

Finite instance compilation rejects duplicate assignments and fact ids,
missing or repeated role projections, relation-object references to undeclared
fact labels, out-of-codomain values, partial or non-functional generator maps,
non-injective subtype inclusions, violated equations, failed finite
refinements, and unsupported constraint claims. Call
`validate_instance_model_ir` after transporting or loading a finite model to
repeat the executable checks. This Rust validation is a decision procedure,
not a Lean proof.

PathDB's `RuntimeModuleIndex`, `RuntimeSchemaIndex`, and
`RuntimeSemanticIndex` are derived execution/query indexes. They are not an
alternate meaning plane, cannot mint accepted handles, and must be derived only
after `CanonicalCompiler` accepts the exact source bytes. An in-process
`RuntimeModuleIndex` retains the immutable `CompiledKernelSnapshot` that
licensed its derivation. Runtime-index serialization deliberately drops that
handle; deserialization produces citations, not reconstructed authority.
Callers must pass the retained snapshot or canonical-backed runtime index and
must not reconstruct canonical IR from PathDB indexes or raw AST fragments.
PathDB materializes explicit object-endpoint aspect/function interpretations as
labeled execution edges so canonical composable arrows remain queryable. It
rejects relation-object generator endpoints rather than fabricating fact
objects; those interpretations remain available in `InstanceModelIr` until a
typed derived adapter is implemented. Projection coverage counts only
`TheoryEquationIr::schema_equation` values as `path_equations`; relation-span
formal-groupoid equations are not double-counted as forward category equations.

CLI path validation resolves named local imports deterministically, rejects
missing or ambiguous module names, hashes exact module bytes in computed
closure order, and compiles the package before reporting it valid. REPL imports
retain every exact module source and immutable package snapshot; PathDB receives
only a package-shaped derived adapter after canonical compilation succeeds.

Rust and Lean share parser and formation-checker corpus cases under
`fixtures/canonical/`, including the compiler corpus under
`fixtures/canonical/w02/` and the category-presentation corpus under
`fixtures/canonical/category_kernel/`. The latter requires equal accept/reject
outcomes for category formation and, for accepted cases, sends the Rust-emitted
`category_kernel_v3` payload through Lean's exact-byte reconstruction. This is
not a claim that Lean checks the complete finite `InstanceModelIr` or every
runtime theory predicate. The trusted boundary remains the import closure of
`lean/Axiograph/VerifyMain.lean`.

The primary concrete fixture is
`examples/regulated_shipment/RegulatedShipment.axi`. Its compiled snapshot
contains nine relation objects, explicit ordered role projections, one indexed
relation-fact role, one finite reviewer refinement, three explicit functions,
one forward generator-factorization equation, one relation-span equation, a
rewrite, and finite instances. The baseline/candidate authoring report uses
exact payload fingerprints for a finite evolution diff; AxiStore uses the same
compiled candidate in reviewed merge accounting and SQLite materialization.

Explicit current limitations:

- cardinality is interpreted only for finite relation projections in the
  compiled instance;
- arbitrary named refinement predicates are rejected except for the supported
  finite fragment; role-level `key(...)` refinements also reject because no
  value-level witness semantics is implemented—use a checked theory
  `constraint key Relation(...)` instead;
- `InstanceModelIr` makes no topological sheaf, closure, completeness,
  ontology-closure, dependent-product, or arbitrary Π-type claim;
- PathDB's explicit-generator execution adapter currently supports object-type
  endpoints only and rejects relation-object endpoints; and
- derived PathDB indexes still exist for runtime tooling, but are outside the
  canonical compiler's authority.

## Derived Runtime-Index Inventory

The inventory below records non-authoritative runtime/report surfaces. It does
not define a second category presentation:

- `axiograph_pathdb::kernel_ir` provides `RuntimeSchemaIndex`, `InstanceIr`,
  `RelationSemanticsIr`, `RoleIr`, `CarrierSpecIr`, and `WitnessViewIr` for
  execution and query planning after canonical compilation.
- `RuntimeSemanticIndex` contains only sorted read-only
  `RuntimeIrRef::Canonical` entries copied from `KernelSnapshotIr::refs`, with
  their canonical labels. It does not synthesize runtime category objects,
  arrows, object images, or arrow images.
- `RuntimeModuleIndex::canonical_snapshot()` retains the accepted
  `CompiledKernelSnapshot` only for in-process derived indexes. Serialization
  preserves citations and deliberately drops that authority.
- The removed `SchemaCategoryIr` and `InstanceFunctorIr` types must not be
  reintroduced. They duplicated `SchemaPresentationIr` objects/generators and
  misleadingly presented a partial closed-world instance projection as another
  categorical semantics. Runtime code that needs category meaning must cite the
  canonical presentation; runtime instance rows remain execution data.
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
  This checker now makes scoped runtime claims only about typed admissibility,
  review status, blockers, and residuals. It does not derive obligations,
  saturate rewrites, prove termination, reach a theory fixpoint, or claim
  completeness/ontology closure. It remains outside the Lean trusted checker.
  See `docs/reference/RUNTIME_THEORY_CHECKER.md`.
- Lean now has `Axiograph.Theory.Finite`, a narrow finite
  category/dependent/groupoid semantics module:
  - relations are objects with checked role-projection arrows,
  - paths are endpoint-indexed,
  - free-groupoid laws are proved by mathlib denotation,
  - finite interpretations carry dependent role/refinement/context/transport
    witnesses,
  - typed path holes preserve expected endpoints, and
  - finite generator reachability is saturated with replayable explanation
    certificates.
  `Certificate.Format` imports this module, and `VerifyMain` dispatches the
  anchored `category_kernel_v3` family through it. That checked family covers
  exact presentation reconstruction, identities, typed composition, parallel
  equations, contextual congruence replay, and bounded generator reachability;
  broader interpretation, refinement, and transport definitions remain theorem
  support.
  Neither slice makes a general rewrite, ontology-closure, univalence, HIT, or
  topos claim.
- Lean now has a narrow operational semantic VCS conformance slice in
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
- semantic slice builders can enrich supplied semantic-commit refs with
  canonical `KernelRefV2` citations retained by `RuntimeModuleIndex`:
  - schema object, relation object, role, and generator refs;
  - theory, constraint, equation, and rewrite refs; and
  - instance and stable fact refs.
  This makes merge/rebase planning cite the same canonical identities as
  authoring and migration without defining instance-functor or runtime-category
  handles.
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
  This is a deterministic runtime review/query surface derived from canonical
  theory data. It cites canonical theory objects without redefining semantic
  authority or expanding the trusted kernel.
- `axiograph-projections` consumes only the immutable
  `CompiledKernelSnapshot`; it does not compile a second schema IR. It emits
  `ProjectionManifestV1` for PathDB, TypeDB, TerminusDB, RDF/OWL, and portable
  property graphs. Every manifest carries the repository/snapshot/IR anchor,
  closed backend capability declarations, `KernelRefV2`-anchored records,
  finite coverage, a semantic-loss report, one read-only native artifact, and
  Axiograph-only mutation authority.
- `check_readback_v1` compares `(record_id, payload_fingerprint)` pairs for one
  manifest and emits `ReadbackReportV1`. An exact match proves only equality of
  the declared finite transport records. The enclosed
  `ExternalEvidenceEnvelopeV1` is statically `evidence_only`, cannot change
  accepted state, and requires typed proposal review and promotion. It does not
  establish categorical equivalence, completeness, ontology closure, a
  constraint theorem, or a Lean certificate.
- `axiograph_cli::query_ir::PreparedQueryMetadataV1` and
  `PreparedQueryExplorationV1` are the current typed compiled-query surfaces
  for reports/editors/agents:
  - stable prepared-query, input-IR, and elaborated-IR ids,
  - inferred types,
  - certifiability/trust metadata and explicit non-claims,
  - optional derived `RuntimeIrRef` citations when metadata is built under a
    canonical compiled-snapshot anchor,
  - an optional canonical `Query` finite-theory gate receipt when query
    preparation has the accepted `CompiledKernelSnapshot`,
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
- The anchored `category_kernel_v3` family already exports and checks the
  supported finite `SchemaPresentationIr` category slice in Lean. Certifiable
  export of the complete `KernelSnapshotIr`, including all
  `InstanceModelIr` semantics and every theory predicate, remains future work.

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
  `ProjectionManifestV1`, `SemanticLossReportV1`, and `ReadbackReportV1` from
  `axiograph-projections`
- untrusted reconciliation analysis:
  `UntrustedReconciliationPreviewV2` plus
  `build_reconciliation_evolution_preview_v1(...)`; accepted merges instead
  use AxiStore `SemReconciliationV2` reviewed candidates and compiled payload
  fingerprints

Those surfaces are still first slices, not the finished kernel. But they are
already the correct default direction for agents and tooling:

- explore the ontology through compiled/query IR rather than raw token
  heuristics,
- cite prepared query handles and elaborated query ids in CQ/refinement reports
  rather than treating raw query text as the report identity,
- inspect backend projections as capability-declared finite transport plus
  `KernelRefV2`-anchored semantic losses rather than as opaque adapter behavior,
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

### Derived Runtime Indexes And Citations

`RuntimeModuleIndex`, `RuntimeSchemaIndex`, and `RuntimeSemanticIndex` are
non-authoritative PathDB/query projections produced only after exact-byte
canonical compilation succeeds. The semantic surface uses
`RuntimeIrRef::Canonical { citation }`, whose payload is the exact typed
`KernelRefV2` plus its canonical label. Runtime-theory diagnostic reports may
use the other `RuntimeIrRef` variants internally, but those variants are not
entries in `RuntimeSemanticIndex` and do not define category objects or arrows.

A canonical runtime citation means only that a report points to an address in
the accepted compiled IR. It is not an accepted snapshot handle, a Lean proof,
or a substitute for `CompiledKernelSnapshot`. Strict reports validate citation
membership, but promotion and certification must also bind the immutable
compiled-snapshot anchor.

The inspection command remains:

```bash
axiograph discover kernel-surface path/to/module.axi --out kernel_surface.json
```

The command name is a report/API label. Its output is a
`RuntimeSemanticIndex`, not a second kernel compiler. The read-only tool-loop
surface `semantic_kernel_surface` exposes the same derived report and explicit
non-claims.

## Top-Level Shape

```rust
pub struct KernelSnapshotIr { /* private vectors; accessor-only */ }
pub struct SchemaPresentationIr { /* relation objects + generators + theory */ }
pub struct InstanceModelIr { /* finite carriers + total functions + facts */ }
pub struct CompiledKernelSnapshot { /* immutable IR + manifest + handle */ }
```

`CompiledKernelSnapshot` is deterministic with respect to the complete ordered
import closure, exact source bytes, repository id, and accepted snapshot id.
Semantic merge must compile the base/left/right candidate packages through the
same facade. A stale manifest, conflicting duplicate module, missing import,
wrong module header, unsupported predicate, or source/AST mismatch rejects
before accepted state moves.

Addresses alone are insufficient for reviewed merged candidates because an id
can remain in the same namespace while a typed payload changes. Candidate gates
must compare canonical compiled payloads under their snapshot handles and
require typed drop/introduce decisions; address inclusion is not semantic
preservation.

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

This layer is deliberately modest: it is a runtime projection over the live
indexed ontology surface, useful for agent tooling, semantic summaries, and
business-rule review, but it does not by itself expand the trusted Lean-checked
kernel.

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
- capability-declared backend projection manifests,
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

`axiograph_dsl::identity_v2` defines validated, domain-specific V2 wire types.
Malformed values, uppercase or short hashes, unknown versions, and cross-kind
deserialization fail. The canonical preimage contains:

1. the 12-byte ASCII magic `AXIOGRAPH-ID`;
2. version `2` as unsigned 16-bit big-endian;
3. the lowercase ASCII domain and its unsigned 16-bit big-endian length;
4. the unsigned 32-bit big-endian field count; and
5. each ordered field with an unsigned 64-bit big-endian byte length.

SHA-256 hashes that preimage. No code normalizes newlines or Unicode. Module
identity uses the exact accepted UTF-8 bytes as its only framed field.

`compile_kernel_identity_index_v2(...)` is the implemented additive index. It
rechecks AST/source equality and module validation, then derives typed ids for:

- schemas and object types;
- relations and roles in declared order;
- theories, constraints, equations, and rewrite rules;
- instances; and
- facts with module, schema, instance, relation, ordered role ids, and exact
  role values in the preimage.

Every module-local derivation includes `ModuleDigestV2` and its typed parent id.
`KernelRefV2` also carries the module digest explicitly. Equal labels in two
modules therefore remain distinct and cannot be deduplicated as one semantic
object. Fact insertion rejects a repeated id with a different typed payload.

The old alternate compiler/IR names are not public Rust surfaces. PathDB keeps
only explicitly derived runtime indexes and citations. Their storage-local ids
do not satisfy the canonical package identity contract; certificates and
Semantic VCS must bind the immutable compiled-snapshot handle before claiming
canonical identity.

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
4. Rebase migration/category lowering on `SchemaCoreIr`.
5. Keep any explicitly supported `.axpd` byte-format contract isolated from
   semantic lowering changes; storage stability is opt-in, not semantic
   authority.
6. Keep extending prepared-query and migration-preview forms so reports cite
   IR-level ids rather than raw surface names alone.
7. Make certificate payloads and semantic diffs name the same stable IR objects
   used by authoring and query tooling.
8. Make business-rule applicability, semantic-coverage, and agent-facing
   reports cite the same stable IR ids rather than ad hoc labels.
