# Axiograph v6

**Proof-carrying knowledge graphs with Rust + Lean (mathlib)**

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           AXIOGRAPH v6                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐   │
│  │   Ingestion       │     │     PathDB       │     │   Lean checker   │   │
│  │ (untrusted Rust)  │────►│  (.axpd, untrusted│────►│ (trusted, mathlib│   │
│  │  docs/sql/json →  │     │   indexes)       │     │  verifies certs) │   │
│  │  proposals.json   │     └──────────────────┘     └──────────────────┘   │
│         │                          ▲                         ▲              │
│         ▼                          │ certificates            │              │
│  ┌──────────────────┐              │                         │              │
│  │ Canonical `.axi`  │◄─────────────┴─────────────────────────┘              │
│  │ (accepted facts)  │   Rust emits result + certificate; Lean verifies      │
│  └──────────────────┘                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Build everything
make all

# Run demo
make demo

# Run tests
make test

# Focused semantics verification (Rust + Lean)
make verify-semantics
```

## Components

### Rust Crates (`rust/`)

| Crate | Description |
|-------|-------------|
| `axiograph-cli` | CLI for validation/ingestion/promotion/snapshots |
| `axiograph-dsl` | Canonical `.axi` parser (`axi_v1`) |
| `axiograph-pathdb` | Binary graph DB (`.axpd`) + certificates |
| `axiograph-llm-sync` | Untrusted LLM sync scaffolding |
| `axiograph-storage` | Storage helpers (`.axi` + `.axpd`) |
| `axiograph-ingest-docs` | Docs/convos → `proposals.json` (+chunks/facts) |
| `axiograph-ingest-sql` | SQL DDL → `proposals.json` |
| `axiograph-ingest-json` | JSON schema → `proposals.json` |
| `axiograph-ingest-proto` | Buf/Protobuf APIs → `proposals.json` (+chunks) |

### Lean (`lean/`)

The trusted checker/spec lives in `lean/` (mathlib-backed). It verifies certificates emitted by Rust:

- `make lean`
- `make verify-lean-certificates`
- `make verify-lean-e2e-suite`

## Building

### Prerequisites

- **Rust** 1.75+ (with cargo)
- **Lean4 + Lake** (optional, for certificate checking)

## Formal Verification

- Start here: `docs/README.md`
- How to run verification: `docs/howto/FORMAL_VERIFICATION.md`
- Certificate formats: `docs/reference/CERTIFICATES.md`

### Commands

```bash
# Full build
make all

# Rust only
make rust

# Lean checker (optional)
make lean
make verify-lean
make verify-lean-e2e
make verify-semantics

# Install to /usr/local/bin
make install
```

## Examples

### 1. Machining Knowledge Graph

```bash
# Run E2E demo
cargo run --release --example e2e_demo
```

### 2. LLM Grounding

```rust
// Build knowledge graph
let kg = KnowledgeGraph::new();
kg.ingest_pdf("machining_handbook.pdf")?;

// Ground LLM response
let answer = kg.ground("What speed for titanium?");
println!("{}", answer);
// "Based on verified knowledge:
//  - Cutting speed: 60-150 SFM (conf: 95%)
//  - Expert tip: Watch for blue chips (conf: 92%)"
```

### 3. Lean certificate checking (Rust emits, Lean verifies)

```bash
make verify-semantics
```

## Configuration

### Environment Variables

```bash
# LLM API (choose one)
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...
export LOCAL_LLM_URL=http://localhost:11434

# Data directory
export AXIOGRAPH_DATA_DIR=./data
```

## Containers + Kubernetes

The Docker image defaults to running the PathDB server (`/viz` + `/query`):

```bash
docker run --rm -p 7878:7878 \
  -v "$(pwd)/build/accepted_plane:/data/accepted" \
  ghcr.io/axiograph/axiograph:latest
```

Kubernetes manifests are in `deploy/k8s/` and a Helm chart lives in
`deploy/helm/axiograph/` (StatefulSet + PVC by default). Example values presets:
`deploy/helm/axiograph/values-replicas.yaml` and
`deploy/helm/axiograph/values-ingress.yaml`, plus
`deploy/helm/axiograph/values-rwx-sync.yaml` for RWX/shared storage.

### Feature Flags

```toml
[features]
pdf = ["pdf-extract"]        # PDF ingestion
rdf = ["sophia"]             # OWL/RDF parsing
openai = ["reqwest"]         # OpenAI API
full = ["pdf", "rdf", "openai", "anthropic", "local"]
```

## Documentation

- Start here (Diataxis index): `docs/README.md`
- End-to-end “book”: `docs/explanation/BOOK.md`

## Testing

```bash
# All tests
make test

# Property-based tests
make test-property

# E2E tests
make test-e2e
```

## License

MIT
