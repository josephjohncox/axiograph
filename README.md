# Axiograph

![Axiograph Logo](axiograph.png)

**Proof-carrying knowledge graphs with an untrusted Rust engine and a trusted Lean checker (mathlib).**

Axiograph is built around “untrusted engine, trusted checker”:

- **Canonical meaning plane:** `.axi` (schema + theory + instances; what we treat as “the input truth”)
- **Runtime/index plane:** `.axpd` (PathDB snapshot + indexes; derived/untrusted)
- **Certificates:** emitted by Rust, verified by Lean; anchored to `.axi` digests + snapshot-scoped fact ids

> A verified certificate proves *derivability from the declared inputs* (and invariants about the runtime), not that the inputs are “true”. Inputs can still be wrong.

## Why this system?

Axiograph is built to make knowledge work more like software:
versioned inputs, reproducible outputs, and checkable derivations when the stakes are high.
It’s designed for long-lived, evolving knowledge bases where schemas, sources, and optimizers will change over time.

Most knowledge systems either:

- **scale but become a black box** (hard to debug, hard to audit, hard to trust), or
- **are rigorous but hard to ship** (too slow/fragile to operate on real data and evolving schemas).

When something changes (data sources, schemas, heuristics, optimizers), you can still answer:
“why did the system say this?” and “is this derivation valid for these inputs?”

Practically, it supports a graph-grounded ingestion/discovery loop that can **graduate into a typed ontology**
with **proof-carrying** (certificate-backed) results for high-value queries.

What it enables in practice:

- **Meaningful + maintainable:** `.axi` is a typed, readable, versioned meaning plane (schema/theory/instances), so the ontology is not “whatever the importer emitted”.
- **Fast at scale:** `.axpd`/PathDB is a derived, indexed runtime format for ingestion, querying, and visualization (and can be rebuilt from canonical inputs).
- **Auditable by construction:** Rust is allowed to be “clever” (search, rewriting, optimization), but it can be required to emit certificates; Lean checks certificates against semantics.
- **Debuggable “why”:** the WAL/evidence plane carries DocChunks + provenance links + contexts/worlds; answers can cite evidence and you can time-travel snapshots.
- **Assisted exploration (optional):** integrations can drive tool calls and produce typed query IR that is validated/elaborated against the meta-plane before execution, with deterministic retrieval available offline.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                               AXIOGRAPH                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐     ┌───────────────────┐     ┌──────────────────┐│
│  │      Ingestion       │     │       PathDB      │     │   Lean checker   ││
│  │   (untrusted Rust)   │────►│   .axpd snapshot  │────►│ (trusted, mathlib││
│  │ docs/sql/json/proto  │     │ + WAL overlays    │     │  verifies certs) ││
│  │      → proposals     │     └───────────────────┘     └──────────────────┘│
│         │                          ▲                         ▲              │
│         ▼                          │ certificates            │              │
│  ┌─────────────────────┐           │                         │              │
│  │  Canonical `.axi`    │◄──────────┴─────────────────────────┘              │
│  │ (accepted snapshots) │     Rust emits result + certificate; Lean verifies │
│  └─────────────────────┘                                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick start

```bash
./scripts/setup.sh
make demo
make verify-semantics
```

### REPL

```bash
./bin/axiograph repl
```

### Server (HTTP API + `/viz`)

```bash
./bin/axiograph db serve --dir ./build/accepted_plane --listen 127.0.0.1:7878
```

### Browser explorer (viz + snapshots + optional LLM panel)

```bash
./scripts/graph_explorer_full_demo.sh

# Optional (requires `ollama serve`):
LLM_BACKEND=ollama LLM_MODEL=nemotron-3-nano KEEP_RUNNING=1 ./scripts/graph_explorer_full_demo.sh
```

## Demos

The repository has a lot of runnable scripts in `scripts/`. A few good entry points:

- `scripts/graph_explorer_full_demo.sh` — accepted plane + WAL overlays + contexts + snapshots + viz
- `scripts/network_quality_demo.sh` — network analysis + quality gates (for ontology engineering loops)
- `scripts/ontology_engineering_proto_evolution_ollama_demo.sh` — proto ingest + over-time discovery loop (LLM optional)
- `scripts/rdfowl_public_datasets_demo.sh` — RDF/OWL import demo (boundary adapter, not trusted kernel)

## Canonical `.axi` examples

These are the main “canonical” modules used throughout the Rust↔Lean parity checks and demos:

- `examples/economics/EconomicFlows.axi`
- `examples/learning/MachinistLearning.axi`
- `examples/ontology/SchemaEvolution.axi`

## Build + test

### Prereqs

- **Rust** (stable). If you see a Cargo error about `edition2024`, upgrade Rust/Cargo (our deps may use the Rust 2024 edition).
- **Lean4 + Lake** (optional; required for certificate checking).
  - macOS note: building the native Lean executable may require Xcode Command Line Tools (`xcode-select --install`).

### Common targets

```bash
make rust
make binaries

make lean
make lean-exe

make test
make verify-semantics
```

## Containers + Kubernetes

Build + run the server locally:

```bash
docker build -t axiograph .
docker run --rm -p 7878:7878 -v "$(pwd)/build/accepted_plane:/data/accepted" axiograph
```

Kubernetes manifests are in `deploy/k8s/` and the Helm chart is in `deploy/helm/axiograph/`.

## Documentation

- Docs index (Diataxis): `docs/README.md`
- End-to-end “book”: `docs/explanation/BOOK.md`
- Formal verification how-to: `docs/howto/FORMAL_VERIFICATION.md`
- REPL + scripts tutorial: `docs/tutorials/REPL.md`

## License

PolyForm Perimeter License 1.0.1
