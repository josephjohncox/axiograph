# Rust Lifecycle Types

**Diataxis:** Reference  
**Audience:** contributors

This document specifies the target Rust-side type/state model for Axiograph's
semantic workflows.

Lean remains the trusted checker. These Rust types are there to make the
semantic workflow explicit and difficult to misuse.

Current implemented slice (2026-04):

- `axiograph_pathdb::anchor` is live and re-exported from the crate root with
  `AxiDigest`, `AcceptedSnapshotId`, `PathdbSnapshotId`, `ProposalDigest`,
  `WorldModelRunId`, `SchemaId`, `TheoryId`, `ContextId`, `StableFactId`, and
  `AcceptedAxiAnchor`.
- `axiograph_pathdb::lifecycle` is live and re-exported from the crate root
  with `Parsed`, `Validated`, `Reviewed`, `Accepted`, and `Certified`.
- `axiograph_pathdb::axi_module_typecheck` implements
  `Module<Validated>` / `Module<Reviewed>` plus `ReviewStamp`, and
  `axiograph_pathdb::axi_module_import` accepts only `Module<S>` where
  `S: WellTypedModuleState`.
- the validated-module boundary now includes a first real theory slice, not
  only schema/instance checks:
  - structured constraints are checked against declared relation/field/param
    surfaces,
  - rewrite rules are checked against compiled carrier semantics and endpoint
    typing,
  - parseable path equations are checked against the same runtime semantics,
  - and opaque equations remain explicit rather than being silently treated as
    part of the certifiable fragment.
- `axiograph-cli::semantic_claim` now exposes first typed runtime report
  families for engineering usefulness:
  `BusinessRuleApplicabilityReportV1`, `ImplementationSurfaceRuleReportV1`,
  `CoverageReportV1`, `AgentTaskRefV1`, and `AgentEngineeringReportV1`.
- prepared query exploration now carries both human-readable typed holes and
  machine-applicable refinement candidates:
  `PreparedQueryExplorationV1`,
  `AxqlTypedHoleV1`,
  `AxqlRefinementCandidateV1`.
- The broader `ProposalSet` / `Snapshot` / `WorldState` / `Query` / `Answer` /
  `FactId` / `TypedFact` surface remains target design.

## Design Rules

1. Every important artifact carries a **lifecycle state**.
2. Every important artifact carries a **stable semantic anchor**.
3. Process-local DB identity and persistent semantic identity are distinct.
4. Raw `u32` ids and bare strings are adapter-layer concerns, not core API
   surface.

## Lifecycle States

Core typestate markers:

```rust
pub enum Parsed {}
pub enum Validated {}
pub enum Reviewed {}
pub enum Accepted {}
pub enum Certified {}
```

These are for artifact construction and trusted workflow transitions.

States such as `Superseded` and `Retracted` should be modeled as lifecycle
events in the semantic VCS, not as ordinary compile-time artifact states.

## Stable Anchor Types

The core API should expose stable newtypes for semantic identity.

```rust
#[serde(transparent)]
pub struct AxiDigest(String);

#[serde(transparent)]
pub struct AcceptedSnapshotId(String);

#[serde(transparent)]
pub struct PathdbSnapshotId(String);

#[serde(transparent)]
pub struct ProposalDigest(String);

#[serde(transparent)]
pub struct WorldModelRunId(String);

#[serde(transparent)]
pub struct SchemaId(String);

#[serde(transparent)]
pub struct TheoryId(String);

#[serde(transparent)]
pub struct ContextId(String);

#[serde(transparent)]
pub struct StableFactId(String);
```

## Two-Part Identity Model

Every core handle should distinguish:

- persistent semantic identity, and
- live in-memory / process-local identity.

```rust
pub struct LiveId<T> {
    pub branded: DbBranded<u32>,
    pub _marker: PhantomData<T>,
}

pub struct Anchored<A> {
    pub anchor: A,
}
```

Typical handle shape:

```rust
pub struct FactId<A> {
    pub stable: StableFactId,
    pub live: Option<LiveId<FactTag>>,
    pub anchor: A,
}
```

Interpretation:

- `stable` is the semantic id used across snapshots, certificates, and diffs.
- `live` is the process-local id valid only for a branded `PathDB` instance.
- `anchor` says which semantic world/snapshot this fact belongs to.

## Artifact Types

### Modules

```rust
pub struct Module<S> {
    pub digest: AxiDigest,
    pub ast: SchemaV1Module,
    pub _state: PhantomData<S>,
}
```

### Proposals

```rust
pub struct ProposalSet<S, A> {
    pub digest: ProposalDigest,
    pub base_anchor: A,
    pub _state: PhantomData<S>,
}
```

### Snapshots

```rust
pub struct Snapshot<S, A> {
    pub anchor: A,
    pub _state: PhantomData<S>,
}
```

### World state

```rust
pub struct WorldState<A> {
    pub meaning_anchor: A,
    pub pathdb_snapshot: Option<PathdbSnapshotId>,
    pub schema_ir: Arc<KernelModuleIr>,
    pub db: Arc<PathDB>,
}
```

### Queries and answers

```rust
pub struct Query<S, A> {
    pub anchor: A,
    pub ir: QueryIr,
    pub _state: PhantomData<S>,
}

pub struct Answer<S, A> {
    pub anchor: A,
    pub rows: Vec<Row<A>>,
    pub certificate: Option<CertificateV2>,
    pub _state: PhantomData<S>,
}

pub type CertifiedAnswer<A> = Answer<Certified, A>;
```

### Typed semantic values

```rust
pub struct TypedFact<S, R, A> {
    pub id: FactId<A>,
    pub schema: S,
    pub relation: R,
    pub fields: Vec<TypedField<A>>,
}
```

### Runtime checker report types

Typed runtime usefulness should extend to report objects, not only facts and
queries.

Target shapes:

```rust
pub struct RuleApplicabilityReport<A> {
    pub anchor: A,
    pub context: Option<ContextId>,
    pub matched_rules: Vec<RuleMatch>,
    pub trust: TrustContractV1,
    pub residual_obligations: Vec<ResidualObligation>,
}

pub struct SemanticCoverageReport<A> {
    pub anchor: A,
    pub covered_objects: Vec<SemanticObjectRef>,
    pub uncovered_objects: Vec<SemanticGap>,
    pub drift_findings: Vec<SemanticDriftFinding>,
    pub trust: TrustContractV1,
}

pub struct AgentSemanticReport<A> {
    pub anchor: A,
    pub task: AgentTaskRef,
    pub propositions: Vec<ScopedClaim<A>>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub next_actions: Vec<SuggestedAction>,
}
```

These report families matter because ontology/business-rule/coding-agent
usefulness is expressed through report objects as much as through typed domain
objects. If the report layer falls back to ad hoc `serde_json::Value`, the
semantic typing discipline will not survive to the user-facing workflows.

The same rule now applies to exploration tooling. Typed holes are useful for
humans, but agents/editors need machine-applicable next moves. So the runtime
surface should preserve:

- human-readable diagnostics,
- typed holes that explain what is missing or ambiguous,
- and explicit refinement candidates that can be applied as structured query or
  authoring patches.

## Transition Discipline

The target public transitions are:

```rust
parse_axi(text) -> Module<Parsed>
typecheck_axi(Module<Parsed>) -> Result<Module<Validated>>
review_module(Module<Validated>, ReviewStamp) -> Module<Reviewed>
promote(Module<Reviewed>) -> Result<Snapshot<Accepted, AcceptedSnapshotId>>

compile_query(WorldState<A>, Query<Parsed, Unbound>) -> Result<Query<Validated, A>>
execute_query(WorldState<A>, Query<Validated, A>) -> Result<Answer<Validated, A>>
verify_certificate(Answer<Validated, A>, LeanCheck) -> Result<CertifiedAnswer<A>>
```

No direct constructor should allow callers to skip these workflow edges.

## Public API Rules

Core crate APIs should prefer:

- `SchemaId` over `String schema_name`
- `RelationId` / typed relation handles over raw relation labels
- `FactId<A>` over raw `u32`
- `Query<S, A>` over untyped query blobs
- `CertifiedAnswer<A>` over “rows plus maybe some metadata”
- typed report objects over one-off JSON/prose payloads for business-rule,
  coverage, preview, and agent-facing semantics

Raw boundary APIs should move under:

- adapter modules,
- CLI/HTTP serialization layers,
- or clearly named `raw` surfaces.

## Module Layout

Target low-level modules in `axiograph-pathdb`:

- `src/anchor.rs`
- `src/lifecycle.rs`

Likely support modules:

- `src/typed_handles.rs`
- `src/world_state.rs`

## Relationship To Existing Types

The target model should absorb and generalize existing good patterns:

- `DbToken` / `DbBranded<T>` remain the live identity discipline.
- `AxiTypedEntity` / `AxiTypedFact` become anchor-aware typed handles.
- `CheckedDb` and typed builders should return typed handles instead of raw ids.
- `TypedAxiV1Module` has been superseded at the canonical `.axi` boundary by
  `axiograph_pathdb::axi_module_typecheck::Module<Validated>`; reviewed modules
  use `Module<Reviewed>`.

## Builder Discipline

Typed builders should produce typed values directly.

Target shape:

```rust
let fact: TypedFact<SchemaHandle<A>, RelationHandle<A>, A> =
    checked_db
        .fact_builder(relation_handle)
        .set_field(role_handle, value_ref)?
        .commit()?;
```

Not target shape:

```rust
let fact_id: u32 = checked_db.fact_builder(...).commit()?;
```

## Query Discipline

Public query execution should become anchor-aware and typed.

Target shape:

```rust
let q: Query<Validated, AcceptedSnapshotId> = compile_query(...)?;
let answer: Answer<Validated, AcceptedSnapshotId> = execute_query(...)?;
let certified: CertifiedAnswer<AcceptedSnapshotId> = verify_certificate(answer, ...)?;
```

Once a result is `CertifiedAnswer<AcceptedSnapshotId>`, downstream code should
not need to ask which accepted snapshot it refers to. The type already carries
that answer.

## First Implementation Slice

Current status of the first implementation cut:

1. Done: `anchor.rs` and `lifecycle.rs` are live.
2. Done for the main semantic workflow seams: accepted-plane, WAL, db-server
   snapshot state, and world-model lineage now use stable anchor newtypes
   internally, while HTTP/CLI JSON remains string-compatible at the boundary.
3. Done: the canonical `.axi` import boundary now uses
   `Module<Validated>` plus `Module<Reviewed>` instead of `TypedAxiV1Module`.
4. Pending: broaden typed builders so more public handles carry anchors/states.
5. Pending: wrap public query execution in `Query<S, A>` / `Answer<S, A>`.
