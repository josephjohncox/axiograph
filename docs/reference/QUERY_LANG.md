# Query Languages (AxQL, SQL-ish, and Certified Querying)

**Diataxis:** Reference  
**Audience:** users (and contributors)

Axiograph aims to support **multiple query surfaces** over the same core graph
semantics:

- a **human-first** REPL language (fast to type, expressive)
- a **structured** query format for tooling/LLMs (`query_ir_v1`, JSON)
- a **SQL-ish** language for familiarity and integration

All of these should share the same *meaning* and be able to run in:

- **fast mode** (proof-irrelevant execution)
- **certified mode** (proof-producing execution; Lean checks certificates)

For hands-on demos (scenario generation + proof-relevant certificates), see
`docs/tutorials/TYPE_THEORY_DEMOS.md`.

## Today (implemented)

### 1) AxQL (REPL language)

AxQL is a small datalog-ish pattern language implemented for the REPL.

Key idea: a query is a **conjunction of atoms** (a basic graph pattern),
evaluated as a **graph homomorphism** (pattern match) over PathDB.

AxQL also supports top-level **disjunction** (`or`): a query can be a union of
conjunctive branches (UCQ). This is certificate-checkable via `query_result_v2`
(each returned row is proved to satisfy *some* branch).

Supported atoms:

- Type constraint: `?x : TypeName`
- (sugar) type constraint: `?x is TypeName`
  - **Schema-qualified** types are supported: `?x is Fam.Person`
    - this is elaborated into: `?x is Person, attr(?x,"axi_schema","Fam")`
- Path constraint (RPQ): `?x -<rpq>-> ?y` where `<rpq>` supports:
  - (sugar) bracketed RPQ: `?x -[<rpq>]-> ?y`
  - concatenation: `rel_0/rel_1/rel_2`
  - alternation: `(a|b)`
  - repetition: `*`, `+`
  - optional: `?`
  - grouping: `( … )`
  - bounded search: `max_hops N` (applies to RPQ atoms)
  - confidence threshold: `min_conf 0.8` (ignores edges below this confidence)
- Attribute equality: `attr(?x, "key", "value")`
- (sugar) attribute equality: `?x.key = "value"` (also supports single quotes: `'value'`)
- N-ary relation (fact) atom (canonical `.axi` import shape):
  - `Flow(from=a, to=b)` (implicit fact/tuple node)
  - `?f = Flow(from=a, to=b)` (bind the tuple node)
  - Schema-qualified fact atoms are supported: `Flow` can be written as `Fam.Flow(...)`.
- Optional **context/world scoping** (recommended when your `.axi` uses `@context` / `ctx=...`):
  - `... in CensusData` (single context; lowered into core atoms and **certifiable**)
  - `... in {CensusData, FamilyTree}` (multiple contexts; execution-time union filter; **not certifiable yet**)
- Approximate attribute queries (REPL/discovery only; **not certifiable**):
  - `contains(?x, "name", "titan")` (case-insensitive substring)
  - `fts(?x, "text", "capture payment")` (token-based full-text-ish search over a chosen attribute key; AND semantics)
  - `fts(?x, "search_text", "PaymentService GetPayment")` (same operator, but commonly used for semantic metadata + identifiers)
  - `fuzzy(?x, "name", "titainum", 2)` (case-insensitive Levenshtein)

`fts(...)` is most useful when you import evidence chunks into a snapshot (e.g.
proto/doc ingestion): `axiograph db pathdb import-chunks <in.axpd> --chunks <chunks.json> --out <out.axpd>`.
This importer stores:

- `DocChunk.text` (the chunk body / doc comment text)
- `DocChunk.search_text` (semantic metadata + identifiers: chunk/doc/span ids, kind/fqn/message/field/etc)

so you can search either “what was said” (`text`) or “what it refers to” (`search_text`).
- (sugar) outgoing edge existence: `?x has rel_0`
- Shape macros (expand into conjunctions):
  - `has(?x, rel_0, rel_1, ...)`
  - `attrs(?x, name="node_42", ...)`
- Shape literal (expand into conjunctions):
  - `?x { rel_0, rel_1, name="node_42", is Node }`
- Lookup terms (desugar into fresh vars + `attr(...)`):
  - `name("node_42")`
  - `entity("key", "value")`
  - bare identifiers are treated as `name("...")` for convenience (e.g. `b` ≡ `name("b")`)

Examples:

```text
q select ?y where 0 -rel_0/rel_1-> ?y
q select ?y where 0 -[rel_0/rel_1]-> ?y
q select ?y where 0 -(rel_0|rel_1)-> ?y
q select ?y where 0 -rel_0*-> ?y
q select ?y where 0 -rel_0*-> ?y max_hops 5
q select ?y where 0 -rel_0/rel_1-> ?y min_conf 0.8
q select ?x ?y where ?x : Node, ?x -rel_0-> ?y limit 5
q select ?x where ?x : Node, attr(?x, "name", "node_42")
q select ?x where ?x is Node, ?x.name = "node_42"
q select ?x where ?x is Node, ?x has rel_0
q select ?x where ?x { is Node, rel_0, name="node_42" }
q select ?x where ?x : Node, has(?x, rel_0), attrs(?x, name="node_42")
q select ?x where ?x -rel_0-> name("b")
q select ?x where ?x -rel_0-> b
q select ?f where ?f = Flow(from=a, to=b)
q select ?f where ?f = Parent(child=Carol) in CensusData
q select ?p where ?p is Fam.Person limit 10
q select ?p where name("Carol") -Fam.Parent-> ?p limit 10
q select ?x where ?x : Material, contains(?x, "name", "titan")
q select ?c where ?c : DocChunk, fts(?c, "text", "capture payment")
q select ?c where ?c : DocChunk, fts(?c, "search_text", "PaymentService CapturePayment")
q select ?x where ?x : Material, fuzzy(?x, "name", "titainum", 2)
q select ?x where ?x : Person or ?x : Organization limit 10
```

See `docs/tutorials/REPL.md`.

#### Schema-directed enrichment (when meta-plane is present)

When a PathDB was built by importing canonical `.axi` modules (so the meta-plane
schema/theory graph is available), the AxQL planner enriches queries with
*implied* type constraints:

- fact atoms add a type constraint for the tuple node (`Flow` / `FlowFact`)
- tuple field edges add type constraints for the field values (from the `.axi` relation declaration)
- key/functional constraints are used as lightweight join-planning hints (ordering) and for
  best-effort *fact-node pruning* when keys are fully bound to constants.

In addition, the executor uses PathDB’s **FactIndex** so queries that filter on
`axi_relation` (including all fact atoms like `Flow(from=a, to=b)`) do not have to
scan the attribute column repeatedly.

#### Multi-schema “one universe” behavior (schema-qualified names)

It is common to load multiple schemas into a single snapshot (e.g. `Fam` and
`Census`) that share names like `Person` or `Parent`.

AxQL supports **schema-qualified names** to disambiguate intentionally:

- `?x is Fam.Person` (type constraint scoped to the `Fam` schema)
- `?x -Fam.Parent-> ?y` (edge traversal that matches only the derived traversal
  edges for that schema)
- `?f = Fam.Parent(child=Carol, parent=Bob)` (fact atom scoped to the schema)

Unqualified edge labels are treated as “best-effort”:

- If a relation name is unambiguous across loaded schemas, the derived traversal
  edge is emitted unqualified (e.g. `Parent`).
- If a relation name is ambiguous across schemas, PathDB emits the derived
  traversal edges schema-qualified (e.g. `Fam.Parent`, `Census.Parent`).
- If you query with an ambiguous unqualified edge label (e.g. `-Parent->` when
  both `Fam.Parent` and `Census.Parent` exist), elaboration will either:
  - **pick a schema** when it can be inferred from other constraints (e.g.
    schema-qualified facts/types that imply `axi_schema=Fam`), or
  - treat it as a **union** (RPQ alternation) across schemas, and add an
    elaboration note recommending explicit qualification.

You can also get a meta-plane by running a schema-discovery step over structured
ingestion artifacts:

- ingest → `proposals.json` (evidence plane)
- `axiograph discover draft-module …` → a candidate canonical `.axi` module
- import that `.axi` into PathDB to explore it with schema-directed AxQL planning

This is an optimization that also makes certified queries more explicit: the
extra type atoms become part of the core query IR and are checked by Lean for
`query_result_v1` certificates.

User-facing type elaboration (REPL)

In the REPL, you can ask AxQL to typecheck and show the elaborated query:

```text
q --elaborate <AxQL query>
q --explain <AxQL query>
q --typecheck <AxQL query>
```

This prints the elaborated query text (with implied type atoms inserted),
inferred types per variable, and ambiguity notes. This is primarily a UX feature
to make schema-directed planning *visible* and to catch typos early (unknown
types/relations, or `Flow(foo=...)` where `foo` is not a declared field).

With `--explain`, the REPL also prints a small **execution plan** summary
(join order, candidate domain sizes, and FactIndex hints). This is untrusted
debug output, but it helps explain performance and schema-directed inference.

#### Context/world scoping (`in ...`)

Canonical `.axi` supports **world/context** scoping by annotating relations with
`@context` (which expands into an ordinary tuple field, conventionally `ctx`).
When importing into PathDB, the importer derives an extra edge:

- `fact_node -axi_fact_in_context-> context_entity`

This keeps `.axi` as the canonical truth (context is still a normal field), but
lets PathDB build fast indexes and lets AxQL scope fact-node matches.

AxQL syntax:

- `... in CensusData` (single context; lowered into certified core atoms)
- `... in {CensusData, FamilyTree}` (union of contexts; execution-time filter, not certifiable yet)

REPL ergonomics:

- `ctx use CensusData` (sets the default scope for subsequent queries)
- `ctx clear` / `ctx show` / `ctx list`

### 1b) `ask` templates (REPL-only convenience)

The REPL also includes a small `ask` command that parses **deterministic**
natural-language-ish templates and compiles them into AxQL (no network/LLM):

```text
ask find Node named b
ask find nodes has rel_0
ask from 0 follow rel_0/rel_1 max_hops 5
```

### 2) SQL-ish surface (compiled into AxQL)

We also support a constrained SQL-like surface (parsed via `sqlparser`) that
compiles into the same core query IR as AxQL:

- `SELECT … FROM Type AS x WHERE … LIMIT N;`
- `FOLLOW(x, 'rel_0/rel_1', y)` for RPQ/path atoms
- `HAS(x, 'rel_0', ...)` / `HAS_OUT(...)` for shape macros
- `ATTR(x, 'name') = 'value'` for attribute equality

This is intended for familiarity and tooling integration, not “full SQL”.

### 3) Certified querying (Rust emits, Lean verifies)

AxQL/SQL-ish queries can be run in a **proof-producing mode**:

- Rust emits a `query_result_v1` certificate (versioned JSON)
- certificates are anchored to canonical `.axi` snapshots (`PathDBExportV1`) via module digest
- Lean verifies that each returned row satisfies the query under that snapshot

This certificate is intentionally **soundness-only** (no completeness claim): it
proves “these rows satisfy the query”, not “these are all the satisfying rows”.

Important: approximate query atoms (`contains`, `fuzzy`, future similarity
operators) are **not** part of the certified kernel. They are treated as
evidence-plane tooling for discovery and should not be conflated with
certificate-checked derivability.

E2E:
- Emit cert from Rust (snapshot export): `axiograph cert query <snapshot_export.axi> --lang axql '<query>'`
- Emit cert from Rust (canonical module): `axiograph cert query <module.axi> --anchor-out <derived_snapshot_export.axi> --lang axql '<query>'`
- Verify in Lean: `make verify-lean-e2e-query-result-v1`

## Roadmap (next iterations)

### A) Expand SQL-ish coverage

- richer WHERE expressions (beyond the function-style predicates)
- joins and named constants (still compiling into the same core IR)
- better error messages (surface → core IR mapping)

### B) Better homomorphism / pattern queries

AxQL should grow toward a “conjunctive query” engine:

- better join planning (index-driven ordering)
- explicit constants and named entities (e.g. look up by `name`)
- partial evaluation and incremental query results

### C) Path expressions beyond fixed sequences

Support regular-path queries (RPQs):

- `rel*`, `rel+`, alternation `(a|b)`, optional `?`, grouping
- optional bounded paths and cost models
- Lean semantics should reuse mathlib’s regular-expression definitions (so “RPQ meaning” is not hand-rolled).

### D) Certified querying

For “Rust computes, Lean verifies”, we want query results to be optionally
certificate-backed:

- each edge/path witness is part of a certificate
- certificates are anchored to snapshot-scoped fact IDs / module digests
- Lean checks that returned bindings/results are derivable from the canonical inputs

Next tightening steps:
- expand certificates beyond *soundness* into optional completeness claims (where feasible)
- add “unknown vs false” shape validation as certificate-checked ingestion/promotion
