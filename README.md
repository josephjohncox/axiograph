# Axiograph

![Axiograph logo](axiograph.png)

[Read the book](https://josephjohncox.github.io/axiograph/) ·
[Run the primary example](examples/regulated_shipment/README.md) ·
[Develop Axiograph](docs/DEVELOPMENT.md)

Axiograph is a typed ontology workbench. It compiles reviewable `.axi` modules
into one immutable semantic package, uses that package for queries, review,
storage, and projections, and can attach Lean-checked certificates to selected
bounded claims.

## What Axiograph Does

- **Models domains with explicit types.** Define objects, relation objects,
  ordered roles, constraints, equations, rewrites, contexts, worlds, and finite
  instances in canonical `.axi` modules.
- **Compiles one operational meaning package.** The canonical compiler produces
  a `CompiledKernelSnapshot` shared by queries, competency questions, theory
  reports, semantic evolution, authoring tools, and backend projections.
- **Keeps evidence separate from accepted meaning.** Embeddings, language-model
  output, imported graphs, and generated code remain evidence, proposals, or
  derived artifacts until typed review promotes them.
- **Checks narrow claims independently.** Rust emits certificates for supported
  finite claim families. The trusted Lean checker reconstructs the relevant
  source fragment and accepts or rejects the stated claim.

Axiograph is not a general dependent type theory, an unrestricted HoTT engine,
or an open-world ontology-completeness prover. Its strongest claims are finite,
explicitly anchored, and scoped by their certificate format.

## How It Works

```text
evidence and proposals
        │ typed validation, review, and promotion
        ▼
exact accepted .axi bytes + ordered import closure
+ repository and accepted-snapshot anchor
        │
        │ CanonicalCompiler (Rust; operational, not trusted as a proof kernel)
        ▼
CompiledKernelSnapshot
  ├── KernelSnapshotIr
  ├── SchemaPresentationIr
  ├── TypedTheoryIr
  └── InstanceModelIr
        │
        ├── typed queries, competency questions, and theory reports
        ├── authenticated AxiStore / SQLite / PathDB execution state
        ├── backend projections, code generation, and evidence sidecars
        └── certificates for supported claim families

exact canonical .axi anchor + supported anchored certificate
        │
        ▼
import closure of lean/Axiograph/VerifyMain.lean
        │
        └── accept or reject the stated narrow claim
```

Accepted exact-byte `.axi` modules are the reviewable meaning plane.
`CompiledKernelSnapshot` is the common operational package, but Rust
compilation is not proof-theoretic authority. PathDB, `.axpd`, TypeDB,
TerminusDB, RDF/OWL, property-graph output, embeddings, and generated code are
derived surfaces.

The trusted checker is the import closure of
`lean/Axiograph/VerifyMain.lean`, not every Lean module in the repository.
Ontology and query certificate families bind exact canonical `.axi` anchors;
payload-only path and resolution families instead validate their declared
payloads and do not claim an accepted ontology anchor. `query_result_v4` can
establish exact answer completeness only for its declared
bounded finite denotation. `category_kernel_v3` establishes finite
presentation reconstruction and replay, not general category theory, arbitrary
HoTT semantics, or reversibility of ontology relations.

For the precise boundary, read:

- [Trusted kernel](docs/reference/TRUSTED_KERNEL.md)
- [Compiled kernel IR](docs/reference/KERNEL_IR.md)
- [Certificate families](docs/reference/CERTIFICATES.md)

## Use Axiograph

### Run the executable tour

```bash
make verify-regulated-shipment
```

This is the primary cross-layer fixture. It compiles canonical modules, checks
runtime obligations and competency questions, verifies supported certificates,
previews evolution, applies a reviewed typed merge, reopens authenticated
SQLite state, emits derived backend projections, and compiles a generated Rust
behavior test.

Within that fixture, `VerifyMain` proves that `CoA_RX_42` is the exact answer to
one declared bounded finite query. Merge accounting, persistence, projection,
grounding, and generated code remain operational evidence. The
[regulated-shipment evidence map](examples/regulated_shipment/README.md)
separates each claim from its non-claims.

### Validate a canonical module

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/Family.axi
```

### Inspect runtime theory admissibility

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check theory examples/ontology/OntologyRewrites.axi \
  --closure-tier finite_fragment \
  --json \
  --out build/readme/ontology-rewrites-theory-check.json
```

This command returns a Rust admissibility report. It does not compute ontology
closure, prove rewrite confluence, or issue a Lean certificate.

### Run competency questions

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/manufacturing/SupplyChainHoTT.axi \
  --from-cq examples/competency_questions/supply_chain.cq \
  --no-schema \
  --out build/readme/supply-chain-cqs.json
```

### Run the software-authoring workflow

```bash
./examples/software_authoring/run_authoring_flow.sh
```

### Emit a derived backend projection

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend typedb \
  --search-root examples/backend_projection \
  --out build/readme/typedb-projection.json \
  --artifact-out build/readme/typedb-projection.tql
```

The projection manifest records finite transport coverage and semantic loss.
The backend artifact is readable through its native interface, but it is not
ontology authority.

## Choose A Path

| Goal | Start here |
| --- | --- |
| Understand the architecture | [System overview](docs/explanation/SYSTEM_OVERVIEW.md) |
| Follow the canonical workflow | [Canonical semantic spine](docs/howto/CANONICAL_SEMANTIC_SPINE.md) |
| Run a certified query | [Certified querying tutorial](docs/tutorials/CERTIFIED_QUERYING_101.md) |
| Audit a trust claim | [Trusted kernel](docs/reference/TRUSTED_KERNEL.md) |
| Browse examples | [Example catalog](examples/README.md) |
| Build software-authoring tools | [Software authoring](docs/reference/SOFTWARE_AUTHORING_TOOLS.md) |
| Review semantic evolution | [Semantic VCS](docs/reference/SEMANTIC_VCS.md) |
| Emit backend projections | [Backend projections](docs/reference/BACKEND_PROJECTIONS.md) |
| Develop, test, or release Axiograph | [Development guide](docs/DEVELOPMENT.md) |

Useful entry points include the
[regulated-shipment workflow](examples/regulated_shipment/),
[`Family.axi`](examples/Family.axi),
[software-authoring examples](examples/software_authoring/), and
[backend-projection examples](examples/backend_projection/). The
[example catalog](examples/README.md) lists the complete maintained set.

## Development

Source builds, toolchains, verification gates, documentation checks, platform
support, and release policy are documented in the
[development guide](docs/DEVELOPMENT.md).

## License

[BSD 3-Clause License](LICENSE). Copyright © 2026 Joseph Cox.
