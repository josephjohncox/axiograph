# Axiograph REPL

**Diataxis:** Tutorial  
**Audience:** users

The REPL maintains process-local PathDB state. It can import exact canonical
`.axi`, generate synthetic graphs, build ephemeral indexes, run AxQL, inspect
schemas, and evaluate proposal/evidence tooling. It has no durable PathDB
`load`, `save`, or reverse-export command.

## Start

```bash
cd rust
cargo run -p axiograph-cli -- repl
```

Run one or more commands non-interactively:

```bash
cargo run -p axiograph-cli -- repl \
  --cmd 'import_axi ../examples/Family.axi' \
  --cmd 'stats' \
  --cmd 'q select ?p where ?p : Person limit 20' \
  --quiet
```

Run a script:

```bash
cargo run -p axiograph-cli -- repl \
  --script ../examples/repl_scripts/family.txt
```

Scripts are fail-fast unless `--continue-on-error` is supplied.

## Import canonical meaning

```text
axiograph> import_axi ../examples/Family.axi
axiograph> schema
axiograph> validate_axi
axiograph> stats
```

The importer parses and validates the canonical module, then derives temporary
query rows and runtime indexes. The imported bytes remain the reviewable input;
PathDB cannot export them back later.

## Generate synthetic state

```text
axiograph> gen 10000 8 8 3 1
axiograph> gen scenario social_network 1000 3 1
axiograph> stats
axiograph> build_indexes
```

Synthetic state is useful for query/performance experiments and is discarded
when the process exits.

## Query with AxQL

```text
axiograph> q select ?x where ?x : Person limit 20
axiograph> q select ?y where name("Alice") -Parent-> ?y limit 20
axiograph> q select ?f where ?f = Flow(from=RawMetal_A, to=MachinedPart_A)
axiograph> q --elaborate select ?x where ?x : Person limit 20
axiograph> q --explain select ?x where ?x : Person limit 20
```

Elaboration exposes inferred types, typed holes, repair/refinement handles,
query IR, trust class, and execution-plan hints. Query output is an untrusted
runtime result unless a supported certificate is checked separately.

## Context and theory inspection

```text
axiograph> ctx list
axiograph> ctx use CensusData
axiograph> constraints Family Parent
axiograph> rules
axiograph> schema Family
```

Context selection affects subsequent process-local queries. Canonical context
fields and theory declarations remain in `.axi`.

## Proposal/evidence exploration

```text
axiograph> proposal use llm
axiograph> proposal propose build/proposals.json --goal "find missing parent links"
```

Generated proposals remain evidence-plane artifacts. The REPL does not commit
them into accepted state or a PathDB WAL.

## Visualization

The `viz` REPL command can emit a bounded derived graph from current process
state. For canonical file-based visualization, use:

```bash
axiograph tools viz examples/Family.axi \
  --out build/family.html --format html --plane both --all
```

## Persistence

To operate on durable query state, publish an authenticated SQLite
materialization through AxiStore using an explicit `AxpdBuildSpec`, then open it
by `MaterializationIdV2` with the DB server or MCP service. Do not copy REPL
state to a bare `.axpd` file.

See:

- `docs/explanation/PATHDB_DESIGN.md`
- `docs/howto/DB_SERVER.md`
- `docs/reference/QUERY_LANG.md`
