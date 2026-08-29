# Axiograph

![Axiograph Logo](axiograph.png)

[Read the published Axiograph book](https://josephjohncox.github.io/axiograph/)

**Runtime-typed ontology engineering for software, business rules, semantic VCS,
and proof-carrying high-value claims.**

Axiograph is a greenfield typed ontology workbench built around one public
semantic spine:

```text
exact canonical .axi bytes + import closure + accepted snapshot handle
  -> CanonicalCompiler
  -> CompiledKernelSnapshot
  -> KernelSnapshotIr + SchemaPresentationIr + InstanceModelIr
  -> derived runtime indexes/reports
  -> optional Lean verifier for the supported fragment
```

The accepted `.axi` module is the reviewable meaning plane. Runtime storage,
graph backends, embeddings, LLM outputs, and generated code are derived or
advisory surfaces until they are lowered through the compiled IR and accepted by
the semantic workflow.

## What It Is For

- Author typed ontologies with relation-as-object semantics, projection roles,
  equations, rewrites, constraints, contexts, and worlds.
- Check runtime theory obligations with explicit anchors, closure tiers,
  residual obligations, and non-claims.
- Run typed queries and competency questions over canonical modules, with
  optional Lean-checked certificates for supported fragments.
- Co-evolve ontology and code through DDD/fDDD overlays, behavior cases,
  continuous semantic coverage, and codegen plans.
- Review semantic changes with branch/tag style semantic VCS, typed merge and
  rebase plans, resolver handles, CQ gates, trust gates, and Lean conformance
  checks for finite operational fragments.
- Use embeddings, RDF/SHACL, TypeDB, TerminusDB, PathDB, and LLM/MCP tools as
  adapters or evidence/projection layers, not as ontology authority.

## Trust Boundary

- **Rust** is the operational runtime: parsing, elaboration, querying,
  projections, coverage, authoring flows, diagnostics, and report generation.
- **Lean** is the trusted checker for selected certificate/gate fragments.
- **PathDB** and `.axpd` are derived execution formats.
- **Graph DB projections** are native-readable read surfaces; mutation authority
  stays in Axiograph.
- **Embedding/LLM output** is evidence or proposal material until promoted
  through typed review.

A verified certificate proves a narrow claim under explicit anchors and
assumptions. `query_result_v4` can prove exact answer completeness for its
bounded finite denotation; that is not source truth, open-world completeness,
global ontology closure, or full HoTT semantics.

## Quick Start

Run the primary end-to-end regulated-shipment fixture:

```bash
make verify-regulated-shipment
```

It compiles one canonical scenario, verifies the supported finite query in the
`VerifyMain` boundary, checks finite category/dependent/path theorem support,
previews evolution, materializes a reviewed typed merge, restarts AxiStore and
PathDB from an authenticated SQLite image, emits backend projections, and
compiles the generated Rust behavior test.

Validate a canonical module:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/Family.axi
```

Run a runtime theory check:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check theory examples/ontology/OntologyRewrites.axi \
  --closure-tier finite_fragment \
  --json \
  --out build/readme/ontology_rewrites_theory_check.json
```

Run question-first competency questions over canonical `.axi`:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/manufacturing/SupplyChainHoTT.axi \
  --from-cq examples/competency_questions/supply_chain.cq \
  --no-schema \
  --out build/readme/supply_chain_competency_questions.json
```

Run the software-authoring DDD/fDDD flow:

```bash
./examples/software_authoring/run_authoring_flow.sh
```

Emit a capability-declared derived backend projection:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend typedb --search-root examples/backend_projection \
  --out build/readme/typedb-projection.json \
  --artifact-out build/readme/typedb-projection.tql
```

Run the semantic merge/rebase Rust and Lean conformance gate:

```bash
make verify-lean-semantic-vcs
```

Run the current canonical-spine gate:

```bash
make verify-canonical-spine
```

Run the exact publication gate with the pinned Rust toolchain:

```bash
PATH="$(dirname "$(rustup which --toolchain 1.88.0 rustc)"):$PATH" \
  make release-gate
```

## Main User Paths

| Goal | Start Here |
| --- | --- |
| Read the published book | <https://josephjohncox.github.io/axiograph/> |
| Learn the current workflow | `docs/howto/CANONICAL_SEMANTIC_SPINE.md` |
| Understand the architecture | `docs/explanation/SYSTEM_OVERVIEW.md` |
| Use examples | `examples/README.md` |
| Run tests | `docs/howto/TESTING.md` |
| Verify certificates and Lean gates | `docs/howto/FORMAL_VERIFICATION.md` |
| Work on compiled IR | `docs/reference/KERNEL_IR.md` |
| Work on runtime theory checking | `docs/reference/RUNTIME_THEORY_CHECKER.md` |
| Work on semantic VCS and merge | `docs/reference/SEMANTIC_VCS.md` |
| Work on backend projections and readback | `docs/reference/BACKEND_PROJECTIONS.md` |
| Work on software authoring/codegen | `docs/reference/SOFTWARE_AUTHORING_TOOLS.md` |
| Work on embeddings and evidence | `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` |

## Example Catalog

- `examples/regulated_shipment/` — primary realistic workflow across finite
  category/dependent/path semantics, exact bounded-query completeness,
  explanations, evolution, typed merge, persistence/restart, projection, and a
  compiled generated test.
- `examples/Family.axi` — compact typed ontology for relation-as-object basics,
  context/time roles, constraints, type-directed queries, and runtime-theory
  reports.
- `examples/ontology/OntologyRewrites.axi` — equations, rewrites, and runtime
  theory obligations.
- `examples/competency_questions/` — question-first `.cq` suites for typed CQ
  coverage and review gates.
- `examples/semantic_merge/` — plant-operations semantic VCS merge/rebase.
- `examples/software_authoring/` — pure domain `.axi` plus DDD/fDDD overlays,
  advisory definition lookup, continuous coverage, and codegen previews.
- `examples/industrial/RegulatedProductionLine.axi` — process, certification,
  and operational/business constraints.
- `examples/physics/` — scientific ontology and measurement modeling.
- `examples/rdfowl/` — RDF/SHACL boundary adapters.
- `examples/backend_projection/` — capability-declared PathDB, TypeDB,
  TerminusDB, RDF/OWL, and property-graph derived projection workflow.

## Release Status

Release candidates are built as `x86_64-unknown-linux-gnu`,
`aarch64-apple-darwin`, and `x86_64-pc-windows-msvc` bundles. A platform is
supported only for a release whose exact hosted-runner lane records successful
archive extraction, checksum, mode, CLI, and anchored checker smokes. No support
is inferred before that evidence exists.

Linux arm64 is a container candidate only. Native Linux arm64, macOS Intel,
Windows arm64, and all unexecuted runner paths are unsupported. The Linux bundle
uses the Ubuntu 24.04 glibc/OpenSSL 3 ABI baseline; the macOS arm64 deployment
target is 13.0.

## Development

Prerequisites:

- Rust 1.88.0 for the release gate; newer toolchains may be used for local
  development only when the 1.88.0 gate remains green.
- The Lean toolchain and mathlib revisions pinned by `lean/lean-toolchain` and
  `lean/lake-manifest.json`.
- Docker only for container and optional backend projection/readback tests.
- The book target downloads the pinned mdBook binary and verifies its SHA-256
  digest before use.

Common checks:

```bash
cargo fmt --manifest-path rust/Cargo.toml --check
cargo check --manifest-path rust/Cargo.toml -p axiograph-cli -p axiograph-pathdb
lake build Axiograph.VerifyMain
make book
git diff --check
```

## License

PolyForm Perimeter License 1.0.1
