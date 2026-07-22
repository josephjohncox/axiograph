# Axiograph Examples

Examples are teaching artifacts first. Canonical `.axi` modules are the public
semantic input, and examples should demonstrate usable typed ontology workflows
instead of acting as hidden test data.

The teaching map is this README plus the per-directory README files. The
machine-readable `examples/catalog.json` is an index fixture for agents and
tests; it is not the authoring surface. The canonical parser and
Rust/Lean parity corpus is `fixtures/canonical/corpus.json`. Operational
maintenance scripts live under `scripts/ops/`; keep those out of the first
teaching path unless a user is explicitly testing CLI breadth or
integration behavior.

Check that every public example directory has an indexed teaching purpose and
every runnable Axiograph-owned JSON artifact carries an explicit top-level
version:

```bash
python3 examples/check_catalog.py
```

## Start Here

| Goal | Example | What It Teaches |
| --- | --- | --- |
| Full primary workflow | `examples/regulated_shipment/` | one regulated-shipment slice across finite category/dependent/path semantics, exact query completeness, evolution, typed merge, restart, projection, and generated tests |
| Minimal ontology | `examples/Family.axi` | objects, relations, context/time roles, constraints |
| HoTT/path flavor | `examples/family/FamilyHoTT.axi` | typed paths, path witnesses, relation composition |
| Rewrites and equations | `examples/ontology/OntologyRewrites.axi` | runtime theory graph, rewrite/constraint surfaces |
| Schema evolution | `examples/ontology/SchemaEvolution.axi` | migration-preview and typed transport examples |
| Semantic merge | `examples/semantic_merge/` | realistic plant-operations merge/rebase plans checked against Lean theory |
| Business process | `examples/industrial/RegulatedProductionLine.axi` | regulated-process domain model plus CQ/coverage overlays |
| Software authoring | `examples/software_authoring/OrderFulfillmentDomain.axi` | pure domain `.axi` plus DDD/fDDD tooling overlays, advisory definition lookup, typed theory checks, continuous semantic coverage, code skeleton previews |
| Physics/domain modeling | `examples/physics/PhysicsOntology.axi` | scientific ontology and typed relation design |
| Backend/interop | `examples/backend_projection/`, `examples/rdfowl/` | native-readable TypeDB/TerminusDB projection contracts plus RDF/SHACL boundary-layer examples |

## Recommended Flow

Start with `make verify-regulated-shipment`. It is the only primary fixture that
crosses every major layer under one canonical scenario. Then use narrower
examples when isolating a subsystem:

1. Validate the canonical `.axi` module.
2. Run a runtime-theory check or emit a theory graph.
3. Explore meaning through typed query, definition, overlay, or coverage
   reports.
4. For software-authoring examples, generate code plans and continuous
   coverage reports before materializing skeletons.
5. For semantic VCS examples, review typed candidates and materialize only
   through AxiStore; reduced Lean merge payloads remain an external finite
   conformance slice.
6. Treat backend-specific artifacts as read-only projections from compiled IR.
   They are useful for native inspection, not semantic authority or mutation.

## JSON Artifacts

Example JSON is a wire format for typed Axiograph contracts, not an invitation
to pass anonymous ad hoc objects around. Public Axiograph-owned JSON artifacts
carry an explicit top-level `version`, usually a string version such as
`tooling_overlay_bundle_v1`. Low-level parser/certificate parity payloads live
under `fixtures/`.

One exception class is intentional:

- `examples/software_authoring/host_integrations/*.json` are generic host
  launcher examples for MCP/LSP clients. They follow host configuration shapes,
  not Axiograph report schemas.

## Core Commands

Run the complete regulated-shipment workflow:

```bash
make verify-regulated-shipment
```

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

Run question-first competency questions over a canonical module:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/manufacturing/SupplyChainHoTT.axi \
  --from-cq examples/competency_questions/supply_chain.cq \
  --no-schema \
  --out build/examples/supply_chain_competency_questions.json
```

Run the breadth-first repo tour across validation, theory, CQ, REPL,
behavior-case, regulated production line example-crate, and Lean surfaces:

```bash
./examples/run_demo.sh
```

Run the regulated production line example crate:

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

Run only advisory definition queries for authoring and agent planning:

```bash
./examples/software_authoring/run_definition_queries.sh
```

Run the semantic merge/rebase Rust and Lean conformance checks:

```bash
make verify-lean-semantic-vcs
```

Compile and inspect a capability-declared derived backend projection:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend typedb \
  --search-root examples/backend_projection \
  --out build/examples/backend_projection/typedb-manifest.json \
  --artifact-out build/examples/backend_projection/typedb.tql
```

The same command accepts `pathdb`, `terminusdb`, `rdf-owl`, and
`property-graph`. The manifest carries finite coverage, semantic-loss, trust,
and readback contracts; backend output is never accepted authority.

## Pedagogical Map

| Directory | Role |
| --- | --- |
| `examples/behavior_cases/` | JSON BDD/DDD cases that compile into receipts and test skeleton previews |
| `examples/backend_projection/` | TypeDB/TerminusDB native-read projection contracts and caveats |
| `examples/competency_questions/` | human-authored `.cq` suites for typed coverage and review gates |
| `examples/economics/` | business/economic flow ontology |
| `examples/family/` | HoTT/path-oriented family examples |
| `examples/industrial/` | co-evolving industrial process, business, and implementation example |
| `examples/ingest_sources/` | source documents for evidence-plane ingestion tutorials |
| `examples/learning/` | learning and guardrail ontology material used by evidence/review flows |
| `examples/llm_sync/` | evidence-overlay guidance for LLM/agent extraction flows |
| `examples/machining/` | manufacturing and machining knowledge examples for learning and ingestion |
| `examples/manufacturing/` | supply-chain HoTT/modal examples |
| `examples/modal/` | compact context/world/modal example |
| `examples/ontology/` | rewrites, schema evolution, and migration-preview examples |
| `examples/physics/` | scientific ontology and measurement examples |
| `examples/proto/` | proto/API semantics example |
| `examples/rdfowl/` | RDF/SHACL boundary-layer examples |
| `examples/regulated_shipment/` | primary end-to-end usefulness fixture and CI gate |
| `examples/repl_scripts/` | interactive scripts; start with the cataloged canonical walkthrough, then use review-plane/proposal scripts only when that is the lesson |
| `examples/runtime_theory/` | focused constraint and fibered-closure examples for runtime theory checks |
| `examples/schema_discovery/` | proposal-to-canonical `.axi` examples |
| `examples/semantic_merge/` | plant-operations semantic VCS merge/rebase with Lean conformance checks |
| `examples/social/` | compact relationship/path query domain |
| `examples/software_authoring/` | pure-domain software-authoring examples with typed tooling overlays |

## Greenfield Example Rules

- Prefer canonical `.axi`, typed reports, certificates, and semantic previews.
- Keep domain example crates outside core CLI unless they define reusable runtime
  infrastructure.
- Example crates should consume the reusable runtime/library surfaces and teach
  application integration; they should not become parallel ontology kernels.
- Keep mutations through Axiograph semantic workflows. Example crates may
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
