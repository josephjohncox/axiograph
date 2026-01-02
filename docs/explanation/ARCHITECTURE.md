# Axiograph v6 Architecture

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph is a knowledge engine with an explicit trust boundary:

- **`.axi` is canonical** (schema + theory + instances in a reviewable format).
- **Rust** is the untrusted engine (ingest, store, query, optimize, reconcile).
- **Lean4 + mathlib** is the trusted checker/spec (semantics + certificate checking).

The core idea is: **untrusted engine, trusted checker**. High-value results are
only relied on when accompanied by a certificate that Lean verifies.

For a more “guided tour”, start with `./SYSTEM_OVERVIEW.md`.

## Design principles

1. **Small trusted core**: Lean checks certificates against semantics, not “the same algorithm again”.
2. **`.axi` is the meaning plane**: accepted knowledge is diffable, reviewable, and anchorable by digest.
3. **Evidence is not truth**: ingestion emits evidence/proposals with provenance; promotion into canonical `.axi` is explicit.
4. **Deterministic checking**: no floats in the trusted checker; certificates use fixed-point probabilities (`VProb`).
5. **Open world by default**: missing facts are usually **unknown**, not **false**.

## Planes and artifacts

Axiograph’s architecture is easiest to understand as a few “planes” of data,
each with a different trust level.

### 1) Meaning plane (canonical)

**Accepted `.axi` modules** are the canonical source of meaning:

- schemas (object/relationship declarations + constraints)
- theories (constraints + first-class rewrite rules)
- instances (facts, reified as typed tuples)

These are the inputs that certificates should ultimately be anchored to.

Practical tooling:
- validate: `axiograph check validate file.axi`
- certificate gates: `axiograph cert typecheck file.axi` and `axiograph cert constraints file.axi`

### 2) Evidence plane (untrusted, auditable)

Ingestion outputs evidence artifacts (provenance-first):

- `chunks.json` (DocChunks: bounded text, metadata)
- `proposals.json` (Evidence/Proposals schema; what *might* be true, with confidence)
- optional `facts.json` (raw extractor output, not canonical)

Evidence artifacts are designed to support:
- “show me the source” (chunk ids and provenance pointers)
- offline review + promotion into `.axi`
- hybrid retrieval (BM25-ish / embeddings / graph neighborhoods)

How-to: `../howto/KNOWLEDGE_INGESTION.md`.

### 3) Derived runtime plane (PathDB)

PathDB (`.axpd`) is a **derived, indexed** representation used for performance:

- fast querying (AxQL / SQL-ish)
- path search and reachability
- reconciliation / normalization / migrations (untrusted engine steps)
- caches and indexes (FactIndex, compiled-query cache, RPQ automata, etc.)

PathDB is rebuildable from accepted snapshots; it is not the canonical truth.

How-to: `../howto/SNAPSHOT_STORE.md` and `./PATHDB_DESIGN.md`.

### 4) Certificates (Rust → Lean)

Certificates are versioned JSON payloads emitted by Rust and checked by Lean.
They are the “proof-carrying” boundary between untrusted execution and trusted meaning.

Reference: `../reference/CERTIFICATES.md`  
How-to: `../howto/FORMAL_VERIFICATION.md`

## End-to-end data flow (typical)

```
sources
  → ingest (untrusted) → chunks.json + proposals.json
  → discover/promote (explicit) → candidate .axi modules
  → accept (append-only) → accepted-plane snapshot id
  → build-pathdb (derived) → .axpd snapshot + WAL overlays
  → query/ops (untrusted) → results + certificates
  → verify (trusted) → Lean accepts/rejects certificates
```

## Data model (how knowledge is represented)

### Entities, relations, and reified tuples

Axiograph uses a reified representation of n‑ary relations:

- **Objects** are entities with a declared object type (e.g. `Person`).
- **Relation instances** are represented as **fact nodes** (typed records), with
  outgoing edges for each field (e.g. `Parent(child=Dan,parent=Alice,ctx=CensusData,time=T2020)`).

This matters because it makes it possible to:
- attach provenance, context, and confidence to a specific fact,
- index facts by schema/relation/keys for efficient query planning,
- treat constraints as metadata about typed records (meta-plane as a type layer).

### Meta-plane as a “type layer”

PathDB imports schema/theory declarations into a meta-plane that supports:

- relation signatures (field names + object types)
- subtyping closure
- constraints (key/functionals, and certified closure-compatibility checks)
- rewrite rules (first-class rules declared in `.axi` theory blocks)

The meta-plane is used for:
- query elaboration (type/field checking + inferred constraints + good errors),
- planning (schema-directed joins, key/FD-driven indexing),
- visualization overlays (relation signatures, constraints, inferred supertypes).

### Contexts/worlds and modalities

Most real-world knowledge is scoped: by time, source, authority, policy, or even
conversation. In Axiograph this is modeled explicitly as **contexts** (a.k.a.
worlds / named graphs):

- A `Context` is a first-class object.
- Facts can be scoped to a context by an `axi_fact_in_context` edge from a fact node
  to a `Context` object.

This keeps the distinction between **unknown vs false** visible:

- “the fact is not present in this context” does **not** mean “false in this context”
  unless you explicitly model negation/closed-world intent.

Contexts are **optional but strongly suggested**. Some examples/demos intentionally
use contexts to make “time travel” and provenance exploration explicit.

Related docs:
- `./TOPOS_THEORY.md` (explanation-level roadmap for sheaf/topos semantics over contexts)
- `./VERIFICATION_AND_GUARDRAILS.md` (failure modes + guardrails)

### Confidence and approximate knowledge

Confidence values in Axiograph are a conservative evidence-weight calculus:

- Runtime may use floats internally, but **trusted checking is fixed-point** (`VProb`).
- Certificates represent confidences as fixed-point numerators with a shared precision.

Approximate knowledge is expected at the evidence/proposals stage, and can remain
present in overlays; promotion into accepted `.axi` is explicit.

### Paths, homotopies, and rewrite semantics

Paths are not “just graph reachability”. In Axiograph, paths are interpreted through
a HoTT/groupoid vocabulary:

- path composition (`p · q`)
- inverses (`p⁻¹`)
- equivalences/homotopies (“2‑cells”) as first-class justifications of equality of paths

The engine is free to implement fast normalization, reconciliation, or optimizations,
but any meaning-bearing result must be representable as a certificate that Lean checks.

Key idea: **rewrite rules are part of the ontology’s semantics**.

- Built-in rules cover the groupoid/path algebra (normalization of path expressions).
- Domain rules are declared in canonical `.axi` theory blocks as structured rule objects.
- Certificates reference rule applications by a stable rule reference (anchored to module digest).

Related docs:
- `./PATH_VERIFICATION.md`
- `./HOTT_FOR_KNOWLEDGE_GRAPHS.md`
- `../reference/CERTIFICATES.md` (rewrite derivations)

## Query architecture

### Query surfaces

Users and tools interact with the system through:

- **AxQL** (graph pattern + RPQ paths + shape-like macros)
- **SQL-ish** (compiled into the same core query IR)
- **Typed JSON Query IR** (`query_ir_v1`) for LLM/tooling integrations

Reference: `../reference/QUERY_LANG.md`  
How-to: `../howto/LLM_QUERY_INTEGRATION.md`

### Typed elaboration (meta-plane directed)

Before planning/execution, queries are elaborated against the meta-plane:

- checks relation/field names and produces good errors early,
- inserts implied type constraints (`?x : T`) and applies subtyping closure,
- computes endpoint typing for edge/path atoms from relation signatures,
- produces a typestate “typechecked query” representation that planning/execution requires
  in strict modes.

Elaboration is intentionally not “reasoning by running the optimizer”. It is a
small decision procedure guided by the schema/theory metadata.

### Planning and indexes

The planner uses PathDB’s derived indexes and meta-plane constraints:

- FactIndex keyed by `(axi_schema, axi_relation)` (and key-field prefixes where available)
- schema-directed join planning using keys/functionals (and, eventually, more constraints)
- compiled query cache keyed by `(query_ir, snapshot_digest)`
- RPQ compilation/caching for regular-path expressions

Related docs:
- `./PATHDB_DESIGN.md`
- `../howto/PERFORMANCE_PROFILING.md`

### Certified querying

Certified querying is an opt-in mode where results are accompanied by witnesses
that Lean can validate:

- per-row witnesses for reachability / RPQ path claims
- (evolving) witnesses that include equivalence chains + rewrite derivations, not only raw edges
- certificates anchored to canonical inputs (module digests + extracted fact ids), so the meaning
  being checked is “derivable from these accepted modules”

This is the key “high-value query” story: fast untrusted execution, with an
auditable and checkable justification at the boundary.

## Constraints and what we certify

Theory constraints serve multiple roles (quality gates, inference permissions, typing rules).
Only a subset is appropriate to certify as a global “module OK” gate under open-world semantics.

`axi_constraints_ok_v1` is intentionally conservative and **fail-closed**:

- unknown constraint kinds are rejected for canonical modules (promotion hazard)
- certified subset focuses on high-ROI integrity and closure-compatibility checks
- closure constraints support `param (...)` to interpret closure “within each fixed assignment”
  (fibered closure), which is the common case for context/time-scoped relations

Explanation: `./CONSTRAINT_SEMANTICS.md`

## Snapshot store + WAL workflow (construction loop)

Accepted knowledge is managed as an append-only log (“accepted plane”):

- promotion adds reviewed `.axi` modules to the accepted plane
- accepted-plane snapshots are identified by digest and can be rebuilt deterministically
- PathDB snapshots and overlays are derived artifacts, and can be committed as a WAL
  for fast iteration and server workflows

CLI entrypoints (high level):

- initialize: `axiograph db accept init --dir <accepted_dir>`
- promote: `axiograph db accept promote --dir <accepted_dir> --axi <file.axi>`
- rebuild `.axpd`: `axiograph db accept build-pathdb --dir <accepted_dir> --snapshot <id> --out <file.axpd>`
- commit WAL snapshot: `axiograph db accept pathdb-commit ...`
- build from WAL snapshot: `axiograph db accept pathdb-build ...`
- compute embeddings: `axiograph db accept pathdb-embed ...`

How-to: `../howto/SNAPSHOT_STORE.md`

## Server + visualization

The DB server loads a snapshot and exposes:

- query endpoints (AxQL/QueryIR)
- exploration endpoints (entity lookup/describe, context lists)
- optional LLM-assisted querying (tool loop)
- visualization rendering (HTML export and server-hosted views)

How-to: `../howto/DB_SERVER.md`  
Tutorial: `../tutorials/VIZ_EXPLORER.md`

## Lean checker (trusted core)

Lean is the trusted semantics and certificate checker:

- `.axi` parsing and well-formedness/typechecking (small decision procedures)
- HoTT/groupoid path semantics and normalization correctness
- fixed-point verified probabilities (`VProb`) used in certificate checking
- certificate replay/checking (reachability, query results, normalization, rewrite derivations, etc.)

Key directories:

- `.axi` parsers: `lean/Axiograph/Axi/*`
- certificate format + checking: `lean/Axiograph/Certificate/*`
- HoTT/groupoid semantics: `lean/Axiograph/HoTT/*`
- fixed-point probability: `lean/Axiograph/Prob/*`

This repo previously used Idris2 as a prototype proof layer; the Rust+Lean release
removes Idris/FFI compatibility. The Idris sources remain useful as a porting reference
in git history, but they are not part of the build.

## Rust engine (untrusted core)

Rust is where performance and operational complexity live:

- ingestion adapters (`axiograph ingest ...`) producing evidence artifacts
- PathDB storage/indexing and query execution (`.axpd`)
- proposal promotion and accepted-plane snapshot tooling
- certificate emission for high-value operations (untrusted, checked in Lean)

Workspace: `rust/`  
Crates: `rust/crates/*` (see `./SYSTEM_OVERVIEW.md` for a concise list)

Rust also uses “dependent-ish” patterns internally:

- DB/snapshot branding (DbToken) so witnesses can’t cross snapshots accidentally
- typestate wrappers for “typechecked query IR”, “normalized path”, etc.
- typed builders (CheckedDbMut, TypedFactBuilder) so common write paths are checked-by-construction

Design notes: `./RUST_DEPENDENT_TYPES.md` and `./TYPE_THEORY_DESIGN.md`

## What is (and is not) allowed in the kernel

### Allowed (untrusted tooling)

- ingestion heuristics (including LLM-assisted extraction)
- query planning and optimization
- visualization, analysis, and “quality report” tooling

These can be wrong. They become meaningful when:

- promoted into canonical `.axi`, or
- accompanied by certificates that Lean checks.

### Required (trusted gate)

- certificate checking is done in Lean
- semantics are defined by the Lean checker/spec and the canonical `.axi` meaning plane

## Extending the system

### Add a new ingestion source

1. Add an adapter under `rust/crates/axiograph-ingest-*` (or extend `axiograph ingest dir`).
2. Emit `chunks.json` (when applicable) and `proposals.json` with provenance pointers.
3. Ensure promotion into canonical `.axi` is explicit (candidates for review).
4. Add/extend demos so “grounding always has evidence” is the default.

### Add a new certificate kind

1. Define the certificate payload in Rust (untrusted) and emit it from an operation.
2. Add a Lean verifier that:
   - parses the anchored `.axi` inputs, and
   - checks the certificate against the semantics (fail-closed).
3. Add fixtures and end-to-end tests:
   - `make verify-semantics`
   - `make verify-lean-cert AXI=... CERT=...`

### Add a new semantics rule (rewrite)

1. Prefer first-class `.axi` rewrite rules (typed rule objects in theory blocks).
2. Import rules into the meta-plane so they are queryable/visualizable.
3. Have the optimizer apply rules untrusted, but emit rewrite-derivation certificates.

## Related documentation

- System overview: `./SYSTEM_OVERVIEW.md`
- Formal verification: `../howto/FORMAL_VERIFICATION.md`
- Certificates reference: `../reference/CERTIFICATES.md`
- Constraint semantics: `./CONSTRAINT_SEMANTICS.md`
- PathDB design: `./PATHDB_DESIGN.md`
- Distributed/replication notes: `./DISTRIBUTED_PATHDB.md`
