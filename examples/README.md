# Axiograph Examples

Examples are teaching artifacts first. Canonical `.axi` modules are the public
semantic input, and examples should demonstrate usable typed ontology workflows
instead of acting as hidden regression fixtures.

The machine-readable catalog is `examples/catalog.json`. The canonical parser
and Rust/Lean parity corpus is `examples/canonical/corpus.json`.

Check that every public example directory is cataloged with a teaching purpose
and every Axiograph-owned JSON fixture carries an explicit top-level version:

```bash
python3 examples/check_catalog.py
```

## Start Here

| Goal | Example | What It Teaches |
| --- | --- | --- |
| Minimal ontology | `examples/Family.axi` | objects, relations, context/time roles, constraints |
| HoTT/path flavor | `examples/family/FamilyHoTT.axi` | typed paths, path witnesses, relation composition |
| Rewrites and equations | `examples/ontology/OntologyRewrites.axi` | runtime theory graph, rewrite/constraint surfaces |
| Schema evolution | `examples/ontology/SchemaEvolution.axi` | migration-preview and typed transport examples |
| Semantic merge | `examples/semantic_merge/` | realistic plant-operations merge/rebase plans checked against Lean theory |
| Business process | `examples/industrial/RegulatedProductionLine.axi` | regulated-process domain model plus CQ/coverage overlays |
| Software authoring | `examples/software_authoring/OrderFulfillmentDomain.axi` | pure domain `.axi` plus DDD/fDDD tooling overlays, weak definition queries, typed theory checks, continuous semantic coverage, code skeleton previews |
| Physics/domain modeling | `examples/physics/PhysicsOntology.axi` | scientific ontology and typed relation design |
| Backend/interop | `examples/backend_projection/`, `examples/rdfowl/` | native-readable TypeDB/TerminusDB projection contracts plus RDF/SHACL boundary-layer examples |

## Recommended Flow

Use examples in this order unless you are testing a specific subsystem:

1. Validate the canonical `.axi` module.
2. Run a runtime-theory check or emit a theory graph.
3. Explore meaning through typed query, definition, overlay, or coverage
   reports.
4. For software-authoring examples, generate code plans and continuous
   coverage reports before materializing skeletons.
5. For semantic VCS examples, build accepted-plane review refs, dry-run
   merge/rebase, then check the reduced Lean payloads.
6. Treat backend-specific artifacts as read-only projections from compiled IR.
   They are useful for native inspection, not semantic authority or mutation.

## JSON Fixtures

Example JSON is a wire format for typed Axiograph contracts, not an invitation
to pass anonymous ad hoc objects around. Public Axiograph-owned fixtures carry
an explicit top-level `version`: most user-facing reports use string versions
such as `tooling_overlay_bundle_v1`, while low-level parser/certificate parity
fixtures may use numeric protocol versions consumed by Rust and Lean.

One exception class is intentional:

- `examples/software_authoring/host_integrations/*.json` are generic host
  launcher examples for MCP/LSP clients. They follow host configuration shapes,
  not Axiograph report schemas.

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

Emit the compiled kernel surface used by query, coverage, merge, and backend
projection tooling:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover kernel-surface examples/Family.axi \
  --out build/examples/family_kernel_surface.json
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
  --cq-file examples/behavior_cases/regulated_ship_release.cq \
  --overlay examples/behavior_cases/regulated_ship_release_overlay.json \
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

Run the semantic VCS merge/rebase example and Lean theory conformance checks:

```bash
./examples/semantic_merge/run_merge_flow.sh
make verify-lean-semantic-vcs
```

Inspect backend-projection contracts as implementation/test surfaces:

```bash
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli backend_pushdown
```

## Pedagogical Map

| Directory | Role |
| --- | --- |
| `examples/anchors/` | narrow verification anchors for parser/certificate boundary tests, not authoring examples |
| `examples/behavior_cases/` | JSON BDD/DDD cases that compile into receipts and test skeleton previews |
| `examples/backend_projection/` | TypeDB/TerminusDB native-read projection contracts and caveats |
| `examples/canonical/` | selected canonical `.axi` corpus for parser and semantics parity |
| `examples/certificates/` | low-level Lean/Rust certificate fixtures, not the first teaching path |
| `examples/competency_questions/` | human-authored `.cq` suites for typed coverage and review gates |
| `examples/demo_data/` | focused constraint/fibered-closure modules for checker tests |
| `examples/economics/` | business/economic flow ontology |
| `examples/family/` | HoTT/path-oriented family examples |
| `examples/industrial/` | co-evolving industrial process, business, and implementation example |
| `examples/ingest_fixtures/` | source fixtures for evidence-plane ingestion tests and tutorials |
| `examples/learning/` | learning and guardrail ontology material used by evidence/review flows |
| `examples/llm_sync/` | evidence-overlay guidance for LLM/agent extraction flows |
| `examples/machining/` | manufacturing and machining knowledge fixtures for learning and ingestion |
| `examples/manufacturing/` | supply-chain HoTT/modal examples |
| `examples/modal/` | compact context/world/modal example |
| `examples/ontology/` | rewrites, schema evolution, and migration-preview examples |
| `examples/physics/` | scientific ontology and measurement examples |
| `examples/proto/` | proto/API semantics example |
| `examples/rdfowl/` | RDF/SHACL boundary-layer examples |
| `examples/repl_scripts/` | REPL smoke scripts for canonical imports, typed queries, and derived module exports |
| `examples/schema_discovery/` | proposal-to-canonical `.axi` examples |
| `examples/semantic_merge/` | plant-operations semantic VCS merge/rebase with Lean conformance fixtures |
| `examples/social/` | compact relationship/path query domain |
| `examples/software_authoring/` | pure-domain software-authoring examples with typed tooling overlays |

## Greenfield Example Rules

- Prefer canonical `.axi`, typed reports, certificates, and semantic previews.
- Keep domain harnesses outside core CLI unless they define reusable runtime
  infrastructure.
- Example crates should consume the reusable runtime/library surfaces and teach
  application integration; they should not become parallel ontology kernels.
- Keep mutations through Axiograph semantic workflows. Example harnesses may
  write reports and cache artifacts, but they should not directly mutate
  accepted ontology state.
- Storage/debug roundtrips are not public semantic inputs, teaching anchors,
  accepted-plane promotion inputs, or certificate/query authorities.
- Every nontrivial example should state which type, trust, CQ, coverage,
  merge/VCS, or backend-projection surface it exercises.
- Any example not listed in `examples/catalog.json` should be either cataloged
  with a clear teaching purpose or removed from the public examples path.
- Avoid examples whose only lesson is “the command runs.” Prefer examples that
  expose typed refs, obligations, reports, resolver handles, coverage deltas,
  backend projection caveats, or explicit evidence-plane boundaries.
