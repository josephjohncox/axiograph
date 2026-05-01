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

Operational note:

- `axiograph db serve` now accepts structured `query_ir_v1` at `POST /query`,
  and can echo the canonical compiled `query_ir_v1` alongside elaboration output.
- `/query` accepts `certificate_policy`; server agent/tool-loop paths accept
  `query_certificate_policy`. Both fields use the shared
  `QueryCertificatePolicyV1` values `none`, `emit`, `verify`, and
  `require_verified`. Legacy `certify`, `verify`, `require_query_certs`, and
  `require_verified_queries` boolean aliases are not part of the greenfield
  public wire contract.
- raw AxQL remains a human-facing REPL/debug surface, not the machine-facing
  HTTP/tool boundary.
- `query_ir_v1` is now the preferred execution seam for tooling: `QueryIrV1::prepare_with_meta`
  returns a typed prepared query handle (`PreparedQueryV1`) that exposes:
  - stable handle ids (`prepared_query_id`, `query_ir_id`, `elaborated_query_ir_id`)
  - execution via prepared statement
  - elaborated plan output (`explain_plan_lines`)
  - prepared-query introspection (`disjunct_count`, `selected_vars`, `limit`, `context_count`)
  - trust-class classification (`certifiable`, `execution-only`, or `mixed`)
  - attached trust/semantic profile carried with the prepared handle itself
  - direct certificate request (`certify_typed_with_anchor` / anchor-bound
    `certify_typed`) against the same prepared state
- `PreparedQueryV1::metadata_with_meta` returns the common report envelope
  (`PreparedQueryMetadataV1`) for query, refinement, CQ, and agent surfaces:
  - IR/prepared-query ids,
  - inferred variable types,
  - certifiability and query trust,
  - explicit non-claims (`completeness_claim`, `ontology_closure_claim`),
  - machine-applicable runtime refinement handles,
  - and, when the caller already has canonical compiled `KernelModuleIr`,
    optional `KernelRefV1` handles via `metadata_with_meta_and_kernel`.
    The default metadata path never invents kernel refs from PathDB/meta state.
- query-facing trust surfaces now make the boundary explicit:
  - `QueryIrV1::trust_contract`
  - `PreparedQueryV1::trust_contract`
  - `PreparedQueryV1::metadata_with_meta`
  - `PreparedQueryV1::semantic_coverage`
  - `PreparedQueryV1::semantic_claims`
  - `PreparedQueryV1::trust_gaps`
  - LLM tools `axql_elaborate` / `axql_run` (typed `query_ir_v1` only)
  - REPL `q --elaborate` / `q --typecheck`
  return the core trust fields plus:
  - `claim_scope = returned_rows_within_snapshot_and_context`
  - `completeness_claim = not_claimed`
  - `ontology_closure_claim = not_claimed`
  - `notes` explaining that the contract is scoped returned-row soundness, not exhaustive answer completeness or full ontology closure
- `axiograph db serve /query` now surfaces the same classification as a wire-level
  `trust` contract:
  - `trust_class`
  - `soundness`
  - `coverage`
  - `scope`
  so clients do not need to infer trust semantics from ad hoc booleans.
- accepted-anchor certifiable `/query` / `axql_run` executions may also return
  `support_summary` behind the existing wire field:
  - it is a runtime-layer contract, not a kernel claim,
  - its basis is `query_result_v3` witness rows,
  - `supported_facts[*].witness_rows` point back to the supporting witness rows,
  - and `contexts` / `evidence` remain attachment-layer enrichments rather than
    the support basis itself.
- `require_verified` is deliberately fail-closed: it requires a fully
  certifiable query, an accepted `.axi` anchor, canonical text for that anchor,
  certificate emission, and a successful Lean verification result. Standalone
  runtime exports may still support `emit`/`verify`, but they do not satisfy
  `require_verified` because they are not accepted-anchor-bound.

`PreparedQueryV1` keeps the parsed AxQL body with the prepared low-level runtime handle
and the trust/semantic profile computed at preparation time. Its metadata
envelope is the citation surface for reports, so callers don’t have to re-parse
queries, re-run trust classification, or carry raw strings plus optional
meta-plane state around for repeated execution and refinement.

For hands-on demos (scenario generation + proof-relevant certificates), see
`docs/tutorials/TYPE_THEORY_DEMOS.md`.

## Today (implemented)

### 1) AxQL (REPL language)

AxQL is a small datalog-ish pattern language implemented for the REPL.

Key idea: a query is a **conjunction of atoms** (a basic graph pattern),
evaluated as a **graph homomorphism** (pattern match) over PathDB.

AxQL also supports top-level **disjunction** (`or`): a query can be a union of
conjunctive branches (UCQ). The preferred certificate path is the canonical
`.axi`-anchored typed query-witness family (wire kind `query_result_v3`).

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
Current `chunks.json` files are typed `EvidenceChunkBundleV1` evidence-plane
bundles, not bare arrays and not accepted ontology truth.
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
canonical typed query-witness certificates (wire kind `query_result_v3`).

User-facing type elaboration (REPL)

In the REPL, you can ask AxQL to typecheck and show the elaborated query:

```text
q --elaborate <AxQL query>
q --explain <AxQL query>
q --typecheck <AxQL query>
```

This prints the elaborated query text (with implied type atoms inserted),
inferred types per variable, ambiguity notes, and a query trust block. The trust
block is explicit about:

- what the claim is scoped to (`claim_scope`)
- the fact that completeness is **not** claimed
- the fact that full ontology closure is **not** claimed

This is primarily a UX feature
to make schema-directed planning *visible* and to catch typos early (unknown
types/relations, or `Flow(foo=...)` where `foo` is not a declared field).

With `--explain`, the REPL also prints a small **execution plan** summary
(join order, candidate domain sizes, and FactIndex hints). This is untrusted
debug output, but it helps explain performance and schema-directed inference.

#### Type inference and hole-driven exploration

Current implemented slice:

- `q --elaborate`, `q --typecheck`, and prepared queries already expose inferred
  variable types, ambiguity notes, schema-qualification choices, typed holes,
  variable-centric exploration suggestions, and trust classification before
  execution.
- The same structured payload is also available to tool-loop/agent surfaces,
  including `axql_elaborate` and the focused exploration tool `axql_explore`.
- exploration suggestions now carry typed refinement handles rather than only
  pasteable query fragments:
  - `exploration_suggestions[*].refinement_candidates[*].handle`
  - each handle has a stable id plus a typed operation (`add_type_guard`,
    `add_edge_atom`, `add_fact_atom`) over machine-usable terms;
- `PreparedQueryV1::apply_refinement_handle` /
  `PreparedQueryV1::apply_refinement_by_id` apply one of those handles and
  return:
  - base and refined `PreparedQueryMetadataV1`,
  - refined `query_ir_v1`,
  - refined elaborated IR,
  - trust/introspection before+after,
  - and refreshed exploration suggestions for the new prepared query.
- This makes elaboration usable as a real exploration loop instead of only as a
  hidden planner step.

Remaining gaps:

- partial queries should preserve even more unresolved relation names, role
  fillers, projection targets, or context restrictions as typed repair sites
  rather than collapsing immediately into opaque errors;
- the current typed apply protocol is conservative:
  it applies only to a single conjunctive query body and does not yet target
  individual disjuncts or higher-order theory objects;
- and tooling should show how each repair changes result shape and
  certifiability (`execution-only`, `mixed`, `certifiable`).

This is runtime dependent-type usefulness rather than a replacement for the
semantic kernel: the elaborator keeps schema/context indices and repair
obligations explicit so humans and agents can explore the ontology safely,
while accepted `.axi` anchors and Lean-checked certificates remain the
authority for strong semantic claims.

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

Human AxQL and SQL-ish forms are frontends. Machine and report flows should
compile to `query_ir_v1`, prepare a `PreparedQueryV1`, and certify from that
prepared handle against a canonical accepted `.axi` anchor. In
**proof-producing mode**:

- Rust emits canonical `.axi`-anchored typed query witnesses (wire kind `query_result_v3`)
- Lean verifies that each returned row satisfies the query under that anchor

This certificate is intentionally **soundness-only** (no completeness claim): it
proves “these rows satisfy the query”, not “these are all the satisfying rows”.

Certifiability in this seam is explicit:

- `certifiable`: all disjuncts are in the currently supported certificate subset
- `execution-only`: every disjunct has unsupported operators
  (approximate string operators, multi-context union, etc.)
- `mixed`: some disjuncts are certifiable while others are execution-only

For mixed queries, the prepared query introspection includes the mixed trust class
and per-branch classification counts so callers can choose whether to:

- run only certifiable branches through the certificate path, or
- execute whole query in untrusted mode and report trust caveats explicitly.

Certificate policy is also explicit:

- `none`: execute and report trust metadata, but do not emit a certificate.
- `emit`: emit a `query_result_v3` certificate when the query is certifiable.
- `verify`: emit and attempt Lean verification, returning the verification
  status/output without treating a failed check as an accepted result.
- `require_verified`: fail closed unless the accepted-anchor/canonical-text
  preconditions hold and Lean verification succeeds.

The runtime currently marks `contains(...)`, `fts(...)`, and `fuzzy(...)` as
execution-only even if other parts of the query are certifiable. Certified
querying therefore still applies only to the supported fragment and does not
upgrade approximate search into kernel semantics.

The shared query-facing trust contract is intentionally stronger about what it
does **not** say:

- `soundness` is about returned rows within the current snapshot/context scope
- `completeness_claim = not_claimed` means the runtime is not asserting that all
  satisfying rows were found or returned
- `ontology_closure_claim = not_claimed` means the runtime is not asserting full
  closure under ontology rules, open-world completion, or exhaustive entailment
- `notes` restate these non-claims in human-readable form so callers do not have
  to infer them from enum values

Important: approximate query atoms (`contains`, `fuzzy`, future similarity
operators) are **not** part of the certified kernel. They are treated as
evidence-plane tooling for discovery and should not be conflated with
certificate-checked derivability.

E2E:
- Emit cert from Rust (canonical module): `axiograph cert query <module.axi> --lang axql '<query>'`
- Verify in Lean: `make verify-lean-e2e-query-result-module-v3`

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
