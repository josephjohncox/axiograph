# Axiograph Examples

Examples are teaching artifacts first. They should demonstrate usable typed
ontology workflows instead of acting as hidden compatibility fixtures.

The machine-readable catalog is `examples/catalog.json`. The canonical parser
and Rust/Lean parity corpus is `examples/canonical/corpus.json`.

## Start Here

| Goal | Example | What It Teaches |
| --- | --- | --- |
| Minimal ontology | `examples/Family.axi` | objects, relations, context/time roles, constraints |
| HoTT/path flavor | `examples/family/FamilyHoTT.axi` | typed paths, path witnesses, relation composition |
| Rewrites and equations | `examples/ontology/OntologyRewrites.axi` | runtime theory graph, rewrite/constraint surfaces |
| Schema evolution | `examples/ontology/SchemaEvolution.axi` | migration-preview and typed transport examples |
| Business process | `examples/industrial/RegulatedProductionLine.axi` | BDD/DDD/fDDD, CQs, coverage, implementation surfaces |
| Software authoring | `examples/software_authoring/OrderFulfillmentDomain.axi` | pure domain `.axi` plus DDD/fDDD tooling overlays, weak definition queries, typed theory checks, continuous semantic coverage, code skeleton previews |
| Physics/domain modeling | `examples/physics/PhysicsOntology.axi` | scientific ontology and typed relation design |
| Backend/interop | `examples/rdfowl/` | RDF/SHACL boundary-layer examples, not the kernel |

## Core Commands

Validate a single canonical `.axi` module:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/Family.axi
```

Emit a typed theory-obligation graph:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover theory-graph examples/ontology/OntologyRewrites.axi \
  --out build/examples/ontology_rewrites_theory_graph.json
```

Emit a query certificate from a canonical module:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  cert query examples/manufacturing/SupplyChainHoTT.axi \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out build/examples/supply_chain_query_cert.json
```

Run the end-to-end teaching demo:

```bash
./examples/run_demo.sh
```

Run the industrial domain harness:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-industrial \
  --bin axiograph-industrial-example \
  -- run-regulated-seed \
  --axi examples/industrial/RegulatedProductionLine.axi \
  --cache-root build/examples/industrial \
  --run-id demo-001 \
  --created-at-unix-secs 1713810000 \
  --json
```

Check a JSON behavior case over a canonical module:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover behavior-case examples/industrial/RegulatedProductionLine.axi \
  --request examples/behavior_cases/regulated_ship_release.json \
  --out build/examples/regulated_ship_release_behavior_case_report.json
```

Run the software-authoring DDD/fDDD example and emit code skeleton previews:

```bash
./examples/software_authoring/run_authoring_flow.sh
```

Run the software-authoring example crate over a generated behavior report:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-software-authoring \
  --bin axiograph-software-authoring-example -- \
  continuous-check \
  --behavior-report build/examples/software_authoring/behavior_case_report.json \
  --repo-root . \
  --out build/examples/software_authoring/example_crate_continuous_coverage.json
```

Run the full software-authoring/codegen suite:

```bash
./examples/software_authoring/run_codegen_examples.sh
```

Run only weak definition queries for authoring and agent planning:

```bash
./examples/software_authoring/run_definition_queries.sh
```

## Pedagogical Map

| Directory | Role |
| --- | --- |
| `examples/behavior_cases/` | JSON BDD/DDD cases that compile into receipts and test skeleton previews |
| `examples/canonical/` | selected canonical `.axi` corpus for parser and semantics parity |
| `examples/competency_questions/` | CQ fixtures for typed coverage and review gates |
| `examples/demo_data/` | small constraint/fibered-closure modules for focused tests |
| `examples/economics/` | business/economic flow ontology |
| `examples/family/` | HoTT/path-oriented family examples |
| `examples/industrial/` | co-evolving industrial process, business, and implementation example |
| `examples/software_authoring/` | pure-domain software-authoring example with typed tooling overlays |
| `examples/manufacturing/` | supply-chain HoTT/modal examples |
| `examples/ontology/` | rewrites, schema evolution, and migration-preview examples |
| `examples/physics/` | scientific ontology and measurement examples |
| `examples/proto/` | proto/API semantics example |
| `examples/rdfowl/` | RDF/SHACL boundary-layer examples |
| `examples/repl_scripts/` | smoke scripts and historical REPL demos |
| `examples/schema_discovery/` | proposal-to-canonical `.axi` examples |

## Greenfield Example Rules

- Prefer canonical `.axi`, typed reports, certificates, and semantic previews.
- Keep domain harnesses outside core CLI unless they define reusable runtime
  infrastructure.
- Example crates should consume the reusable runtime/library surfaces and teach
  application integration; they should not become parallel ontology kernels.
- Keep mutations through Axiograph semantic workflows. Example harnesses may
  write reports and cache artifacts, but they should not directly mutate
  accepted ontology state.
- `PathDBExportV1` and `export_axi` are debug/parity surfaces, not teaching
  anchors. New examples should not foreground them.
- Every nontrivial example should state which type, trust, CQ, coverage,
  merge/VCS, or backend-projection surface it exercises.
