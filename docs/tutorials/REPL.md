# Axiograph REPL (PathDB)

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

The Axiograph REPL is a lightweight interactive shell for working with:

- PathDB snapshots (`.axpd`)
- Reversible PathDB snapshot exports (`.axi`, schema `PathDBExportV1`)
- Canonical `.axi` modules (`axi_v1`, schema/theory/instance), imported into PathDB for querying

It’s intended for quick experiments and debugging (not a polished end-user UI).

If built with default features, the REPL supports **tab completion** and basic
line editing via `rustyline`.

For “type theory / certificates / proof relevance” walkthroughs (Rust emits, Lean verifies),
see `docs/tutorials/TYPE_THEORY_DEMOS.md`.

## Start

```bash
cd rust
cargo run -p axiograph-cli -- repl
```

Non-interactive (run a script or a list of commands and then exit):

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/enterprise_demo.repl
```

Approximate + tacit knowledge demo (confidence thresholds + fuzzy/fts querying + viz HTML):

```bash
./scripts/approximate_tacit_query_demo.sh
```

If you want a scripted run to keep going even after an error (useful for demos that
depend on optional tools like Ollama), add `--continue-on-error`.

More scripts live in `examples/repl_scripts/` (e.g. `economic_flows_demo.repl`,
`machinist_learning_demo.repl`, `schema_evolution_demo.repl`, `schema_evolution_axi_demo.repl`, `continuous_ingest_demo.repl`, `proto_api_demo.repl`,
`proto_import_enterprise_demo.repl`, `social_network_demo.repl`, `supply_chain_demo.repl`,
`physics_knowledge_demo.repl`, `supply_chain_hott_axi_demo.repl`, `sql_schema_discovery_axi_demo.repl`,
`proto_schema_discovery_axi_demo.repl`, `family_hott_axi_demo.repl`, `social_network_axi_demo.repl`,
`economic_flows_axi_demo.repl`, `context_scoping_family_demo.repl`).

The `social_network_demo.repl` script also demonstrates `viz` and writes `build/social_network_viz.dot`
and `build/social_network_viz.html`.

The `ontology_rewrites_axi_demo.repl` script also demonstrates **ontology-engineering tooling**
available inside the REPL:

- `quality` (lint + constraint checks; JSON/text report)
- `analyze network` (graph metrics: components, hubs, PageRank, communities)

Or run one-off commands:

```bash
cd rust
cargo run -p axiograph-cli -- repl --cmd "gen enterprise 5 3 1" --cmd "stats"
```

Minimal build (no `rustyline` / no completion):

```bash
cd rust
cargo run -p axiograph-cli --no-default-features -- repl
```

To preload an existing snapshot:

```bash
cd rust
cargo run -p axiograph-cli -- repl --axpd path/to/snapshot.axpd
```

## Walkthrough: Generate → Query → Export → Reload

Start the REPL:

```bash
cd rust
cargo run -p axiograph-cli -- repl
```

## Tooling: Quality + Network Analysis (in-REPL)

These operate over the current in-memory DB (after `import_axi`, `load`, or `gen`).

```text
axiograph> import_axi examples/ontology/OntologyRewrites.axi
axiograph> quality build/quality.json format json profile strict plane both no_fail
axiograph> analyze network plane both skip_facts communities top 10
axiograph> analyze network build/network.json format json plane both skip_facts communities
```

### 1) Generate a synthetic graph

Generate a graph with:

`gen <entities> <edges_per_entity> <rel_types> [index_depth] [seed]`

Example:

```text
axiograph> gen 10000 8 8 3 1
```

This creates:
- `entities` entities of type `Node`
- `edges_per_entity` outgoing relations per entity
- relation types named `rel_0`, `rel_1`, …, `rel_{rel_types-1}`
- a `PathIndex` up to `index_depth` hops, then builds indexes

### 1b) Generate a realistic scenario graph (shapes + homotopies)

For demos that exercise **typed shapes**, **equivalences**, and explicit
“multiple-derivation” structures, use the scenario generator:

`gen scenario <name> [scale] [index_depth] [seed]`

Shorthand: if the first argument is non-numeric, `gen <name> ...` works too.

Example:

```text
axiograph> gen scenario enterprise 5 3 1
axiograph> gen enterprise 5 3 1
```

The `enterprise` scenario generates:
- `Person`, `Team`, `Service`, `Endpoint`, `Table`, `Column`, `Doc`
- equivalence classes (e.g. `SameService`, `SameApiSurface`)
- explicit `Homotopy` / commuting-diagram artifacts via `PathWitness` nodes

It also prints a short list of suggested queries to try.

Other built-in scenarios:
- `economic_flows` (EconomicFlows-style path algebra)
- `machinist_learning` (MachinistLearning-style concepts/guardrails)
- `schema_evolution` (SchemaEvolution-style migrations + commuting diagrams)
- `proto_api` (Proto/gRPC surface + HTTP annotations + tacit workflows)
- `social_network` (higher-groupoid-ish social dynamics)
- `supply_chain` (route alternatives + “path independence”)

### 1c) Mutate the DB (continuous ingest ticks)

The REPL includes small mutation commands (tooling / evidence-plane; not certified):

```text
axiograph> add_entity Doc doc_stream_0 kind=slack
axiograph> add_edge mentionsService doc_stream_0 svc_0 confidence 0.72
axiograph> add_equiv path_doc_stream_0_direct path_doc_stream_0_via_endpoint HomotopicPath
```

For a full worked example that combines mutation + visualization, run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/continuous_ingest_demo.repl
```

### 2) Inspect basic stats

```text
axiograph> stats
```

### 3) Follow a relation path

```text
axiograph> follow 0 rel_0 rel_1 rel_2
```

You can also pass a single **path expression** (RPQ) instead of a list:

```text
axiograph> follow 0 rel_0/rel_1/rel_2
axiograph> follow 0 (rel_0|rel_1)* max_hops 5
```

This prints the number of reachable targets and the first few entity ids.

### 3b) Query with AxQL (pattern matching)

AxQL is a small datalog-ish pattern language with **conjunctive query / homomorphism**
semantics.

Performance note: the REPL keeps a small in-memory cache of **compiled AxQL queries**
(lowered query + candidate bitmaps + join order + RPQ automata) keyed by the current
snapshot and the query IR digest. The cache is cleared on `load`, `import_*`, and `gen`.
The `q` command prints cache hit/miss + elapsed time.

- type constraint: `?x : TypeName`
- path constraint: `?x -<rpq>-> ?y` where `<rpq>` supports:
  - concatenation: `rel_0/rel_1/rel_2`
  - alternation: `(a|b)`
  - repetition: `*`, `+`
  - optional: `?`
  - grouping: `( … )`
  - bounded search: `max_hops N` (applies to RPQ atoms)
- confidence threshold: `min_conf 0.8` (ignores edges below this confidence)
- attribute equality: `attr(?x, "key", "value")`
- (sugar) attribute equality: `?x.key = "value"` (also supports single quotes: `'value'`)
- n-ary relation (fact) atoms (canonical `.axi` import shape):
  - `Flow(from=a, to=b)`
  - `?f = Flow(from=a, to=b)`
- optional context/world scoping (recommended for `.axi` modules using `@context` / `ctx=...`):
  - `... in CensusData` (single context; certifiable)
  - `... in {CensusData, FamilyTree}` (multi-context union; not certifiable yet)
- approximate attribute queries (REPL/discovery only; **not certifiable**):
  - `contains(?x, "name", "titan")`
  - `fuzzy(?x, "name", "titainum", 2)`
- (sugar) type constraint: `?x is TypeName`
- (sugar) outgoing edge existence: `?x has rel_0`
- shape macros (expand into conjunctions):
  - `has(?x, rel_0, rel_1, ...)`
  - `attrs(?x, name="node_42", ...)`
- shape literal (expand into conjunctions):
  - `?x { rel_0, rel_1, name="node_42", is Node }`
- lookup terms (desugar into fresh vars + `attr(...)`):
  - `name("node_42")`
  - `entity("key", "value")`
  - bare identifiers are treated as `name("...")` for convenience (e.g. `b` ≡ `name("b")`)

Examples:

```text
axiograph> q select ?y where 0 -rel_0/rel_1-> ?y
axiograph> q select ?y where 0 -[rel_0/rel_1]-> ?y
axiograph> q select ?y where 0 -(rel_0|rel_1)-> ?y
axiograph> q select ?y where 0 -rel_0*-> ?y
axiograph> q select ?y where 0 -rel_0*-> ?y max_hops 5
axiograph> q select ?y where 0 -rel_0/rel_1-> ?y min_conf 0.8
axiograph> q select ?x ?y where ?x : Node, ?x -rel_0-> ?y limit 5
axiograph> q select ?x where ?x : Node, attr(?x, "name", "node_42")
axiograph> q select ?x where ?x is Node, ?x.name = "node_42"
axiograph> q select ?x where ?x is Node, ?x has rel_0
axiograph> q select ?x where ?x { is Node, rel_0, name="node_42" }
axiograph> q select ?x where ?x : Node, has(?x, rel_0), attrs(?x, name="node_42")
axiograph> q select ?x where ?x : Node, ?x.name = "a" or ?x : Node, ?x.name = "b" limit 10
axiograph> q select ?x where ?x -rel_0-> name("b")
axiograph> q select ?x where ?x -rel_0-> b
axiograph> q select ?f where ?f = Flow(from=a, to=b)
axiograph> q select ?f where ?f = Parent(child=Carol) in CensusData
axiograph> q select ?x where ?x : Node, contains(?x, "name", "b")
axiograph> q select ?x where ?x : Material, fuzzy(?x, "name", "titainum", 2)
```

Schema-aware type elaboration (meta-plane)

If the PathDB snapshot contains an imported canonical `.axi` meta-plane, you can
ask the REPL to **typecheck + elaborate** a query and show what the system
inferred:

```text
axiograph> q --elaborate select ?dst where ?f = Flow(from=a, to=?dst)
```

This prints:
- the elaborated AxQL query text (with implied `?x : Type` atoms inserted),
- inferred types per variable (including supertypes),
- and ambiguity notes when a relation name exists in multiple schemas.

To stop after checking/elaboration (no execution), use:

```text
axiograph> q --typecheck select ?dst where ?f = Flow(from=a, to=?dst)
```

This mode is intentionally user-facing: it catches common typos early (unknown
types/relations, or `Flow(foo=...)` where `foo` is not a declared field).

#### Context scoping (worlds)

Many canonical `.axi` corpora scope facts to a *context/world* (e.g. `ctx=CensusData`,
`ctx=FamilyTree`). In PathDB, fact tuples are reified as nodes, and the importer
derives a uniform edge:

```text
fact_node -axi_fact_in_context-> context_entity
```

In the REPL you can:

- scope individual queries with `in <context>` / `in {a,b,...}`
- or set a default scope for subsequent queries:

```text
axiograph> ctx list
axiograph> ctx use CensusData
axiograph> q select ?f where ?f = Parent(child=Carol)
axiograph> ctx clear
```

### 3c) Visualize / explore a neighborhood graph

For quick exploration beyond raw `show`/`follow`, the REPL can export a small
neighborhood graph around an entity as:

- Graphviz DOT (best layout; render to SVG/PNG with `dot`)
- a self-contained offline HTML explorer (simple radial graph view + node inspector)

Examples:

```text
axiograph> viz build/graph.dot focus 0 hops 2
axiograph> viz build/graph.html format html focus_name Alice_0 hops 2 max_nodes 120
axiograph> viz build/graph_typed.html format html focus_name Alice_0 hops 2 max_nodes 120 typed_overlay
axiograph> viz build/schema.dot format dot plane meta focus_name SupplyChainHoTT hops 3 max_nodes 220
axiograph> viz build/schema.html format html plane meta focus_name SupplyChainHoTT hops 3 max_nodes 220
```

Tip: if multiple entities share a `name`, add `focus_type <TypeName>` to disambiguate.
This also works for “virtual types” like `Morphism` and `Homotopy`.

With `typed_overlay`, the visualization annotates data-plane nodes using the
`.axi` meta-plane as a type layer (supertypes, relation signatures, and theory
constraints). In DOT output this shows up as node tooltips; in HTML output it
shows up in the node attribute inspector.

In the HTML explorer:
- scroll to zoom
- Alt+drag to pan
- shift-click two nodes to highlight a shortest path (within the currently filtered subgraph)
- use the Filters panel to hide/show node/edge kinds (and planes: accepted/evidence/data)
- if facts are context-scoped, use the context dropdown to filter fact nodes by world/context

To render DOT to SVG:

```bash
dot -Tsvg build/graph.dot -o build/graph.svg
```

The same functionality is available from the CLI (without starting the REPL):

```bash
cd rust
cargo run -p axiograph-cli -- tools viz path/to/snapshot.axpd --out build/graph.dot --focus-name Alice_0 --hops 2
```

## Importing canonical `axi_schema_v1` modules

`import_axi` accepts both `PathDBExportV1` snapshot exports and canonical schema
modules (like `examples/machining/PhysicsKnowledge.axi` or
`examples/manufacturing/SupplyChainHoTT.axi`).

When importing a canonical schema module, the REPL maps instance data into PathDB:

- object elements become entities of the corresponding type (with `name=...`)
- n-ary relation tuples become first-class “tuple entities” with field edges:
  - `tuple -field-> value`
- when a relation has clear endpoints (binary, or `from/to`, etc), the importer
  also adds a derived edge `source -RelationName-> target` for direct traversal

For HoTT-style examples, the importer also adds lightweight “higher structure”
hooks:

- relations named `*Equiv*` / `*Equivalence*` are indexed as `Homotopy` and get
  alias edges `lhs` / `rhs` (so you can query them generically)
- many endpoint-bearing relation tuples are indexed as `Morphism` and get alias
  edges `from` / `to`

### Validate imported instance data (schema-directed)

After importing a canonical module, you can ask the REPL to type-check the
imported **fact nodes** (n-ary relation tuples) against the imported schema
meta-plane:

```text
axiograph> validate_axi
```

This catches obvious “dependent typing” mismatches like:
- a relation tuple missing a declared field edge
- a field pointing to an entity of the wrong type (not a subtype of the declared field type)

### Extract learning structures (concept graph)

If the imported schema uses the canonical learning vocabulary (`Concept`,
`requires`, `explains`, `demonstrates`, `conceptDescription`), you can extract a
typed “learning graph” view:

```text
axiograph> learning_graph MachiningLearning
```

### Inspect schema metadata (meta-plane)

When you import a canonical schema module, the REPL also imports its
schema/theory metadata into PathDB as a *meta-plane*.

List imported modules + schemas:

```text
axiograph> schema
```

Inspect a schema by name:

```text
axiograph> schema SupplyChain
```

Inspect imported theory constraints (keys/functionals/etc):

```text
axiograph> constraints SupplyChain
axiograph> constraints SupplyChain BOM
```

### Export canonical modules (round-trip)

If the meta-plane is present, you can export a canonical `axi_v1` module
back out of PathDB:

```text
axiograph> export_axi_module build/SupplyChainHoTT_roundtrip.axi SupplyChainHoTT
```

### 3c) Query with SQL-ish (compiled into AxQL)

This is a constrained SQL-like surface intended for familiarity and tooling.

```text
axiograph> sql SELECT y FROM Node AS y WHERE FOLLOW(0, 'rel_0/rel_1', y) LIMIT 10;
axiograph> sql SELECT x FROM Node AS x WHERE HAS(x, 'rel_0') AND ATTR(x, 'name') = 'a' LIMIT 5;
```

### 3d) Ask (natural-language-ish templates → AxQL)

`ask` is a small, deterministic template parser that compiles into AxQL (no network/LLM).
It’s intended as a convenience layer for REPL usage.

```text
axiograph> ask find Node named b
axiograph> ask find nodes has rel_0
axiograph> ask from 0 follow rel_0 then rel_1 max hops 5
```

### 3e) LLM-assisted questions

For more “generic” questions, the REPL supports an **optional** LLM layer:

1. LLM proposes an AxQL query
2. Axiograph executes the proposed query against the loaded snapshot
3. (optional) LLM summarizes the results

For more robust workflows, prefer **tool-loop mode**:

- `llm agent ...` (LLM calls tools like `fts_chunks` / `axql_run`; Rust executes; LLM answers)

The tool-loop agent is given a **schema/meta-plane hint pack** (relation signatures + key/functional
constraints + context/time samples) so it can propose correctly typed facts and choose the right
endpoint field mappings (and it can also call tools like `lookup_relation` when unsure).

Tool-loop mode is also **RAG-like by default**:

- before the first model step, Axiograph pre-runs `db_summary` + `semantic_search`, plus a small
  “expansion pack” (`lookup_relation`/`lookup_type` when the question mentions known schema terms,
  `describe_entity` for top entity hits, and `docchunk_get` for top DocChunk hits; with a fallback
  `fts_chunks` when DocChunks exist but semantic retrieval returns none) and includes the results in
  the transcript
- the model then uses tools (`describe_entity`, `axql_run`, `fts_chunks`, etc.) to drill down

For large snapshots, the backend also keeps prompts bounded by truncating older transcript steps and
trimming large tool outputs in the prompt (the full tool transcript is still returned in the JSON
outcome). You can tune this with:

- `AXIOGRAPH_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS`
- `AXIOGRAPH_LLM_PROMPT_MAX_JSON_STRING_CHARS`
- `AXIOGRAPH_LLM_PROMPT_MAX_JSON_ARRAY_LEN`
- `AXIOGRAPH_LLM_PROMPT_MAX_JSON_OBJECT_KEYS`
- `AXIOGRAPH_LLM_PROMPT_MAX_JSON_DEPTH`
- `AXIOGRAPH_LLM_PREFETCH_DESCRIBE_ENTITIES` (0 disables entity neighborhood prefetch; default 2)
- `AXIOGRAPH_LLM_PREFETCH_DOCCHUNKS` (0 disables chunk expansion; default 1)
- `AXIOGRAPH_LLM_PREFETCH_LOOKUP_RELATIONS` (0 disables schema relation prefetch; default 1)
- `AXIOGRAPH_LLM_PREFETCH_LOOKUP_TYPES` (0 disables schema type prefetch; default 1)

This supports:
- a built-in Ollama backend (local models), and/or
- an external command plugin (no network required).

See `docs/reference/LLM_REPL_PLUGIN.md` for the plugin protocol.

### 3f) World model proposals (JEPA / objective-driven)

The REPL also supports a **world model** proposal flow (untrusted, evidence-plane):

```text
axiograph> wm use stub
axiograph> wm status
axiograph> wm propose build/wm_proposals.json --goal "predict missing parent links" --max 50
```

MPC-style planning loop (multi-step):

```text
axiograph> wm plan build/wm_plan.json --steps 3 --rollouts 2 --goal "fill missing parent links" --cq "has_parent=select ?p where ?p is Person limit 1"
```

To commit proposals into the PathDB WAL (store-backed workflows), pass `--commit-dir`:

```text
axiograph> wm propose build/wm_proposals.json --commit-dir build/accepted_plane --message "wm: parent predictions"
```

Built-in deterministic backend (good for demos/tests):

```text
axiograph> llm use mock
axiograph> llm ask find Node named b
```

Single-shot query generation (no tool loop):

```text
axiograph> llm query find Node named b
```

Built-in Ollama backend (local models via Ollama):

```text
axiograph> llm use ollama nemotron-3-nano
axiograph> llm ask find Node named b
axiograph> llm ask what RPCs does acme.svc0.v1.Service0 have?
```

Full demo script (non-interactive, uses Ollama + `nemotron-3-nano` by default):

```bash
./scripts/llm_ollama_nemotron_demo.sh
```

If Ollama isn't running, start it with `ollama serve` (or launch the Ollama app).
To point at a non-default host/port, set `OLLAMA_HOST` before starting the REPL.

External command plugin (reference mock implementation in this repo):

```text
axiograph> llm use command python3 scripts/axiograph_llm_plugin_mock.py
axiograph> llm ask from 0 follow rel_0 then rel_1
axiograph> llm answer find Node named b
```

Model selection is passed through to plugins:

```text
axiograph> llm model qwen2.5:3b
```

### 4) Show an entity

```text
axiograph> show 0
```

### 4b) Describe an entity (discovery-friendly)

`describe` is a richer, discovery-friendly entity inspector. It resolves either:
- a numeric entity id (`123`), or
- a `name("...")` identifier / bare name (`Alice`, `acme.svc0.v1.Service0`).

It prints:
- plane classification (meta / accepted / evidence / data),
- attributes (truncated),
- contexts (if facts are context-scoped),
- equivalences/homotopies (if present),
- evidence links (e.g. `has_evidence_chunk`),
- grouped inbound/outbound edges.

```text
axiograph> describe Alice
axiograph> describe acme.svc0.v1.Service0 --out 8 --in 8 --attrs 20
```

### 4c) Open evidence / doc chunks

When you have ingested evidence into the snapshot (e.g. repo/docs/proto ingestion),
use `open` to jump to the underlying text.

```text
axiograph> open chunk doc_proto_api_overview_0 --max_chars 800
axiograph> open chunk doc_proto_service_0 --max_chars 800
axiograph> open chunk doc_proto_rpc_0_1 --max_chars 800
axiograph> open doc Document_0 --max_chunks 10
axiograph> open evidence some_entity
```

### 4d) Diff contexts (world-scoped facts)

If your `.axi` imports include contexts/worlds, you can diff what facts exist in
one context but not the other:

```text
axiograph> diff ctx CensusData FamilyTree rel Parent limit 20
```

### 5) Export snapshot as `.axi` (reversible)

```text
axiograph> export_axi build/snapshot_pathdb_export_v1.axi
```

Notes:
- This `.axi` uses the **engineering snapshot schema** `PathDBExportV1`.
- It is **not** the domain `.axi` DSL (like `EconomicFlows.axi`).

### 6) Save snapshot as `.axpd`

```text
axiograph> save build/snapshot.axpd
```

### 7) Quit and reload

```text
axiograph> exit
```

Reload:

```bash
cd rust
cargo run -p axiograph-cli -- repl --axpd build/snapshot.axpd
```

Then:

```text
axiograph> stats
```

## Walkthrough: Import a `PathDBExportV1` `.axi`

If you have a reversible snapshot export:

```text
axiograph> import_axi build/snapshot_pathdb_export_v1.axi
axiograph> stats
```

## Command Reference (quick)

```text
help | ?                       Show help
exit | quit                    Exit

load <file.axpd>               Load a PathDB snapshot
save <file.axpd>               Save the current PathDB snapshot

import_axi <file.axi>          Import either a `PathDBExportV1` snapshot or a canonical `axi_v1` module
export_axi <file.axi>          Export current PathDB as `PathDBExportV1` `.axi`
export_axi_module <file.axi> [module_name]
                               Export a canonical `axi_v1` module from the meta-plane (if imported)
schema [name]                  Inspect imported `.axi` schema/theory metadata (meta-plane)
constraints <schema> [relation]
                               Show imported theory constraints (keys/functionals/etc) for a schema/relation
validate_axi                   Type-check imported canonical `.axi` instance data against the meta-plane schema

gen <entities> <edges> <types> [index_depth] [seed]
                               Generate a synthetic graph and build indexes
gen scenario <name> [scale] [index_depth] [seed]
                               Generate a scenario graph (typed shapes + homotopies)
build_indexes                  Build PathDB indexes for the current DB
stats                          Print current DB stats

show <entity_id>               Show entity (type + attributes)
describe <entity_id|name>      Rich entity inspector (attrs + contexts + in/out edges + evidence)
open <kind> <ref>              Open evidence (chunk/doc/entity); see: `open chunk|doc|evidence|entity ...`
diff ctx <c1> <c2> ...         Diff fact nodes between contexts/worlds
find_by_type <type_name>       List entity ids of a type (first 20)
follow <start_id> <rel...>     Follow a relation path
follow <start_id> <path_expr>  Follow an RPQ path expression (`rel_0/rel_1`, `(a|b)*`)
                               Optional: `max_hops N`
find_paths <from> <to> <depth> Find paths (first 10)
q <AxQL query>                 Pattern-match query language (datalog-ish)
sql <SQL query>                SQL-ish dialect compiled into the same query core
ask <query>                    Natural-language-ish templates compiled into AxQL
llm <subcommand>               LLM-assisted query translation / answering
neigh <entity> ...             Summarize + (optional) write a neighborhood viz graph (HTML/DOT/JSON)
```
