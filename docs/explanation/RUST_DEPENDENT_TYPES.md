# Dependent-Type Techniques in Rust (Axiograph v6)

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph’s architecture is “**untrusted engine, trusted checker**”:

- **Rust** computes results (fast, optimized, allowed to be wrong).
- **Lean** checks a **certificate** that justifies the result (small, trusted).

Rust does not have full dependent types, but we can still encode many of the
useful *dependent-typing effects* using a combination of:

- typestate / phantom types,
- lifetime “branding”,
- newtypes that enforce invariants,
- const generics (for small indices like path length),
- and witness-carrying results that line up with Lean certificates.

This document is the concrete Rust-side design we converge on while tightening
the Lean-checked semantics/certificate boundary.

## Design goals

1. **Make illegal states unrepresentable (when practical).**
2. **Tie witnesses to the exact data they justify** (avoid “proof reuse” bugs).
3. **Make the trusted boundary obvious** in types and module layout.
4. **Align Rust witnesses with Lean certificates** (same structure, same invariants).
5. **Keep runtime fast** (zero-cost wrappers where possible).

## The three-phase pipeline (typestate)

Most Axiograph artifacts should follow a consistent pipeline:

1. **Parsed**: syntactically valid, not semantically validated.
2. **Validated**: passes local invariants (shape checks, bounds, well-formedness).
3. **Certified**: accompanied by a certificate and accepted by the Lean checker.

In Rust, encode this as typestate:

```rust
pub enum Parsed {}
pub enum Validated {}
pub enum Certified {}

pub struct ModuleAst<S> {
    state: std::marker::PhantomData<S>,
    /* fields */
}
```

Then expose constructors that move the state forward:

- `parse(text) -> ModuleAst<Parsed>`
- `validate(ast: ModuleAst<Parsed>) -> Result<ModuleAst<Validated>, ...>`
- `emit_certificate(ast: &ModuleAst<Validated>) -> CertificateV2`
- `check(certificate) -> Result<ModuleAst<Certified>, ...>` (or a `CertifiedResult<T>`)

This makes it hard to accidentally run “semantic operations” on merely-parsed data.

## Graph-anchored witnesses (lifetime branding)

The most common “dependent type” in Axiograph is:

> *a path/witness is only meaningful for a particular graph*.

In Rust, we can encode this by tying witness lifetimes to a `VerifiedGraph` borrow:

```rust
pub struct VerifiedGraph { /* nodes/edges */ }

pub struct Path<'g> {
    graph: std::marker::PhantomData<&'g VerifiedGraph>,
    /* steps */
}

pub struct ReachabilityProof<'g> {
    graph: std::marker::PhantomData<&'g VerifiedGraph>,
    /* witness chain */
}
```

This prevents accidentally using a proof from one graph to justify a result on a
different graph (a very common soundness pitfall).

If the graph is stored in an arena, use a “branding token”:

```rust
pub struct Brand<'g>(std::marker::PhantomData<&'g mut ()>);
```

and require `Brand<'g>` to construct graph-bound IDs/proofs.

### PathDB: process-local snapshot branding (`DbToken`)

For PathDB-backed features, we apply the same idea using a **process-local**
branding token:

- every `PathDB` instance carries a fresh `DbToken` (`PathDB::db_token()`),
- typed/witness-bearing wrappers store the token,
- and conversions back to raw IDs check that the token matches.

This prevents a common class of “wrong snapshot” bugs: accidentally applying a
typed value from DB A to DB B.

## Schema-scoped typing (PathDB + `.axi` meta-plane)

Another core “dependent type effect” in Axiograph is:

> *an entity/fact is only well-typed relative to a particular `.axi` schema*.

Because PathDB can contain multiple imported modules (and many schemas reuse
common type names like `Text`), **type names alone are not enough**: we need a
schema-qualified view.

In Rust, we represent this using *validated wrappers* constructed against the
meta-plane index:

- `axiograph_pathdb::axi_typed::AxiTypedEntity` — an entity + a witness that it
  belongs to schema `S` and inhabits type `T` (up to declared subtyping).
- `axiograph_pathdb::axi_typed::AxiTypedFact` — a fact node + a witness that all
  of its field edges exist and are well-typed under the relation declaration.

These wrappers are also **DB-branded**: extracting raw IDs requires a `&PathDB`
and performs a `DbToken` check:

- `typed_entity.entity_id(&db) -> Result<u32, DbTokenMismatch>`
- `typed_fact.fact_id(&db) -> Result<u32, DbTokenMismatch>`
- `typed_field.value_id(&db) -> Result<u32, DbTokenMismatch>`

Construction is explicit and fallible:

- `AxiTypingContext::from_db(&PathDB)` builds a cached meta-plane index.
- `schema.typed_entity(&db, entity_id, "TypeName") -> Result<AxiTypedEntity, ...>`
- `schema.typed_fact(&db, fact_id) -> Result<AxiTypedFact, ...>`

This is the Rust analogue of a Lean `Sigma` type: we return *data + a witness*
that downstream code can rely on without re-checking.

## Checked-by-construction: `CheckedDb` + typed builders

At some boundaries we want stronger guarantees than “you *can* typecheck”:
we want a clear typestate split between:

- **unchecked** snapshots (may contain ill-typed overlay data), and
- **checked** snapshots (basic `.axi` well-formedness holds, so downstream code
  can assume it without re-checking).

In Rust we represent this using a small “checked kernel” wrapper:

- `axiograph_pathdb::checked_db::CheckedDb` (read-only)
- `axiograph_pathdb::checked_db::CheckedDbMut` (write + typed builders)

These wrappers are intentionally **not** the trusted gate (Lean is). They are
runtime guardrails: they prevent accidental construction of nonsense and
produce better errors earlier.

### Example: construct a well-typed fact node

`CheckedDbMut::fact_builder(schema, relation)` returns a builder that:

- enforces that all declared fields are present,
- rejects unknown fields,
- checks value types with subtype closure, and
- derives `axi_fact_in_context` when a `ctx` field exists (runtime scoping).

This is “correct by construction” in the sense that once `commit()` succeeds,
the resulting fact node is well-typed w.r.t. the meta-plane.

## Explicit type algebra: `AxiType` + `TypingEnv`

To avoid smuggling typing semantics through ad-hoc strings, PathDB also exposes
a minimal explicit “type algebra”:

- `axiograph_pathdb::axi_type::AxiType` (object/fact/path types, schema-scoped)
- `axiograph_pathdb::axi_type::TypingEnv` (a `MetaPlaneIndex` plus helpers)

This is a foundation for richer Rust-side typing checks and typed execution
without duplicating Lean.

## Correspondence to Lean semantics (Topos view)

This repo uses Lean to pin down the *meaning* of core concepts, and uses Rust to
execute them efficiently. The runtime stays “type-directed” by indexing the
canonical `.axi` meta-plane and by using validated wrappers at key boundaries.

High-level correspondence:

- `.axi schema` ⇔ a category presentation (CQL-style).
  - Rust: meta-plane graph + `axi_semantics::MetaPlaneIndex`.
  - Lean (scaffold): `lean/Axiograph/Topos/Overview.lean`.
- `.axi instance` ⇔ a functor into finite sets.
  - Rust: PathDB entities + reified fact nodes; field edges act like projections.
  - Lean: the target category is `FintypeCat` (mathlib).
- theory constraints ⇔ typed predicates/subobjects.
  - Rust: imported as `SchemaIndex.constraints_by_relation` and exploited for planning/indexing.
  - Lean: checked in the trusted gate for certifiable subsets (`axi_constraints_ok_v1`).
- contexts/worlds ⇔ world-indexed knowledge (presheaf/sheaf intuition).
  - Rust: `ctx` tuple field (from `@context`) plus derived edge `axi_fact_in_context` + context indexes.
  - Lean: explanation/scaffold only for now; keep certificates snapshot-scoped and avoid closed-world assumptions.

The most important rule is: **runtime indexes are optimizations, not semantics**.
When we need trust, we emit a certificate and Lean checks it against the
canonical `.axi` anchor.

See:

- `docs/explanation/TOPOS_THEORY.md` (explanation-level semantics roadmap),
- `docs/reference/CERTIFICATES.md` (trusted boundary),
- `lean/Axiograph/VerifyMain.lean` (checker executable),
- `rust/crates/axiograph-pathdb/src/axi_semantics.rs` and `rust/crates/axiograph-pathdb/src/axi_typed.rs`
  (type-directed execution).

For convenience and safety, PathDB also exposes schema-scoped selection helpers:

- `PathDB::find_by_axi_type(schema_name, type_name)` intersects the type index
  with the `axi_schema` attribute plane, avoiding cross-schema conflation.

### Example: FactIndex + `AxiTypedFact` (data + witness)

This is a practical “dependent typing” pattern we use throughout Axiograph:

1) use a fast, untrusted index to *find candidates* (FactIndex),
2) then turn the selected IDs into *witness-carrying typed values* (schema-scoped checks).

```rust
use anyhow::Result;
use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::{
    axi_module_import::import_axi_schema_v1_module_into_pathdb,
    axi_typed::AxiTypingContext,
    PathDB,
};

fn demo() -> Result<()> {
    let text = r#"
module Demo
schema S:
  object Node
  relation Flow(from: Node, to: Node)
theory Keys on S:
  constraint key Flow(from, to)
instance I of S:
  Node = {a, b}
  Flow = { (from=a, to=b) }
"#;

    let m = parse_axi_v1(text)?;
    let mut db = PathDB::new();
    import_axi_schema_v1_module_into_pathdb(&mut db, &m)?;
    db.build_indexes();

    // Fast candidate selection (untrusted optimization).
    let flow_facts = db.fact_nodes_by_axi_schema_relation("S", "Flow");
    let Some(fact_id) = flow_facts.iter().next() else {
        anyhow::bail!("expected at least one Flow fact");
    };

    // Turn a raw `u32` into a witness-carrying typed fact (schema-scoped check).
    let typing = AxiTypingContext::from_db(&db)?;
    let s = typing.schema("S")?;
    let typed = s.typed_fact(&db, fact_id)?;

    // `typed.fields` is now a checked view of the tuple fields.
    for f in &typed.fields {
        let value_id = f.value_id(&db)?;
        println!("field {} -> entity {}", f.field, value_id);
    }

    Ok(())
}
```

The key idea: once you have an `AxiTypedFact`, downstream code can be written against
the typed wrapper instead of raw IDs, which is the Rust analogue of Lean returning a
`Sigma` (value + proof).

### Example: key lookup (`fact_nodes_by_axi_key`) + typed fact

When a relation has a declared key constraint (imported into the meta-plane), you
can do a near-index lookup for the corresponding fact node(s) and then validate it
as a schema-scoped typed tuple:

```rust
use anyhow::{anyhow, Result};
use axiograph_pathdb::{axi_typed::AxiTypingContext, PathDB, StrId};

fn entity_named_in_schema(
    db: &PathDB,
    schema_name: &str,
    name: &str,
) -> Result<u32> {
    let name_attr: StrId = db.interner.intern("name");
    let schema_attr: StrId = db.interner.intern("axi_schema");
    let schema_value: StrId = db.interner.intern(schema_name);
    let name_value: StrId = db.interner.intern(name);

    let in_schema = db.entities.entities_with_attr_value(schema_attr, schema_value);
    let named = db.entities.entities_with_attr_value(name_attr, name_value);
    let hits = &in_schema & &named;

    hits.iter()
        .next()
        .ok_or_else(|| anyhow!("no entity named `{name}` in schema `{schema_name}`"))
}

fn demo(db: &PathDB) -> Result<()> {
    // Example: `SupplyChain` declares `constraint key BOM(product, component)`.
    let schema = "SupplyChain";
    let relation = "BOM";

    // Resolve key values to snapshot-scoped entity ids.
    let widget = entity_named_in_schema(db, schema, "Widget")?;
    let body = entity_named_in_schema(db, schema, "Body")?;

    // Best-effort: returns `None` when the key index isn't available.
    let Some(hits) = db.fact_nodes_by_axi_key(
        schema,
        relation,
        &["product", "component"],
        &[widget, body],
    ) else {
        anyhow::bail!("no key index available for {schema}.{relation}");
    };

    // Typically unique.
    let Some(&fact_id) = hits.first() else {
        anyhow::bail!("no BOM(product=Widget, component=Body) fact found");
    };

    // Now "dependently typed": tuple fields are validated against the relation declaration.
    let typing = AxiTypingContext::from_db(db)?;
    let s = typing.schema(schema)?;
    let typed = s.typed_fact(db, fact_id)?;

    println!(
        "typed fact {} has {} fields",
        typed.fact_id(db)?,
        typed.fields.len()
    );
    Ok(())
}
```

## Typestate in practice: normalized paths

Path normalization is a semantics-critical invariant (free-groupoid word reduction).
We encode it as typestate:

- `axiograph_pathdb::UnnormalizedPathExprV2`
- `axiograph_pathdb::NormalizedPathExprV2`

and provide a typestate-safe optimizer entrypoint:

- `axiograph_pathdb::ProofProducingOptimizer::normalize_path_typed_v2`

This lets downstream code accept only normalized paths when required, without
re-running normalization checks everywhere.

## Newtypes for invariants (probability, indices, bounds)

Prefer *invariant-carrying newtypes* over raw primitives in internal APIs.

### Fixed-point probability (`VProb`)

Lean uses a fixed-point probability to avoid floating-point ambiguity in proofs.
Rust should mirror this exactly (same scaling, same rounding rules).

In Axiograph v6, this is implemented as:

- `axiograph_pathdb::VProb` (alias of `certificate::FixedPointProbability`)
- denominator: `Precision = 1_000_000`
- representation: a bounded numerator `u32 ∈ [0, Precision]`

Key operations:

- deterministic `from_f32_bits` conversion (Rust↔Lean agree on mapping)
- fixed-point multiplication (`mul`) with integer division rounding down
- `to_f32()` for display/logging only (not for proofs)

This aligns the Rust runtime with Lean’s semantics and keeps certificate checks stable.

### Bounded IDs

Instead of `usize` or `u32` everywhere:

```rust
pub struct NodeId(u32);
pub struct RelationId(u32);
```

and only construct these through a graph that enforces bounds. This is the
Rust analogue of dependent “index < n” types.

## Certificate-carrying results (witness + value)

Make the trusted boundary visible by bundling results with their justifications:

```rust
pub struct CertifiedResult<T> {
    pub value: T,
    pub certificate: CertificateV2,
}
```

Then keep two APIs:

- `fn compute_untrusted(...) -> CertifiedResult<T>` (engine output)
- `fn check_in_lean(cert: &CertificateV2) -> Result<(), CheckError>` (trusted gate)

Consumers that require correctness accept only:

- `Checked<CertifiedResult<T>>` or directly a `T` produced by a checker.

## Proof-mode generics (zero-overhead optional witnesses)

Many runtime operations have a “fast” mode and a “proof-producing” mode.
To avoid duplicating implementations, Axiograph uses a **compile-time** switch:

- `NoProof`: do not even evaluate witness-producing code paths
- `WithProof`: evaluate and return witness payloads

This pattern lives in:
- `rust/crates/axiograph-pathdb/src/proof_mode.rs`

Typical shape:

```rust
fn operation<M: ProofMode>(input: Input) -> Proved<M, Output, ProofPayload> {
    let output = compute_fast(&input);
    let proof = M::capture(|| compute_proof(input));
    Proved { value: output, proof }
}
```

This keeps the “no proof” path truly zero-overhead while making proof production
explicit in the type signature.

## Length-indexed paths (const generics)

For some invariants (especially where code generation or storage depends on size),
const generics are useful:

```rust
pub struct Path<const N: usize> { steps: [EdgeId; N] }
```

This is not a full dependent type, but it prevents a large class of “wrong length”
bugs and aligns naturally with Lean lemmas about path length.

In practice:

- use const generics for *small, fixed-size* things,
- use runtime `Vec` for large/variable paths,
- and use `VerifiedPath { steps: Vec<_>, len: NatWitness }` when a certificate
  must explicitly justify the length.

## How this connects to Verus

Some invariants are best checked with a Rust-side verifier (Verus) even before
the Lean certificate exists:

- bounds and index safety,
- algebraic invariants for probability composition,
- “witness chain is valid” in a simplified model.

The Verus crate lives at `rust/verus/`. Run it (optionally) with:

- `make verify-verus`

The long-term strategy is:

1. verify *core algebra/invariants* in Verus (fast feedback for Rust code),
2. verify *semantic correctness* via Lean certificates (trusted final gate),
3. keep the two aligned by sharing the same certificate types and fixed-point math.

## Next concrete steps

1. Replace floating probabilities in Verus models with the fixed-point `VProb`
   used by Lean certificates (single source of truth for probability semantics).
   - Runtime step done: `axiograph_pathdb::VerifiedProb` is now backed by fixed-point `VProb`.
2. Keep extending typestate wrappers:
   - already implemented: `axiograph_pathdb::axi_module_typecheck::TypedAxiV1Module`
   - already implemented: `axiograph_pathdb::typestate::{UnnormalizedPathExprV2, NormalizedPathExprV2}`
   - next: typestate for “typechecked query IR” at the REPL/CLI boundary.
3. Keep expanding snapshot/graph branding:
   - already implemented: `PathDB::db_token()` + DB-branded `AxiTyped*` wrappers
   - now implemented: `axiograph_pathdb::DbBranded<T>` + branded witnesses for:
     - reachability: `axiograph_pathdb::witness::reachability_proof_v2_from_relation_ids`
     - normalization/equivalence/reconciliation: `ProofProducingOptimizer::*_branded` variants
4. Expand certificate v2 into full rewrite/groupoid derivations and make the Rust
   engine emit those certificates for real operations (normalization, reconciliation).
