# Axiograph Test Suite

**Diataxis:** How-to  
**Audience:** contributors

## Overview

The Axiograph test suite provides comprehensive coverage across all layers:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           TEST PYRAMID                                    │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│                        ┌─────────────────┐                               │
│                        │   E2E Tests     │                               │
│                        │  (Full Pipeline)│                               │
│                        └────────┬────────┘                               │
│                                 │                                        │
│                    ┌────────────┴────────────┐                           │
│                    │   Integration Tests     │                           │
│                    │   (Cross-crate)         │                           │
│                    └────────────┬────────────┘                           │
│                                 │                                        │
│           ┌─────────────────────┴─────────────────────┐                  │
│           │              Unit Tests                    │                  │
│           │         (Per-crate, per-module)           │                  │
│           └───────────────────────────────────────────┘                  │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# From repo root: focused semantics suite (Rust + Lean)
make verify-semantics

# PathDB snapshot export `.axi` parse parity (Rust ↔ Lean)
make verify-pathdb-export-axi-v1

# Rust tests
cd rust
cargo test

# Performance harnesses (run in release mode)
cargo run -p axiograph-cli --release -- tools perf pathdb --entities 200000 --edges-per-entity 8 --rel-types 8 --queries 50000
cargo run -p axiograph-cli --release -- tools perf axql --entities 200000 --edges-per-entity 8 --rel-types 8 --mode star --queries 2000 --limit 200

# Scenario-based perf (typed “model-like” graphs) + `.axpd` roundtrip
cargo run -p axiograph-cli --release -- tools perf scenario --scenario proto_api --scale 10000 --index-depth 3 --out-axpd build/proto_api.axpd
```

## Test Categories

### 1. Unit Tests (`cargo test --lib`)

Per-crate unit tests in `src/lib.rs` or `src/tests.rs`:

| Crate | Tests | Description |
|-------|-------|-------------|
| `axiograph-dsl` | Parsing | .axi syntax parsing |
| `axiograph-pathdb` | Storage | Entity/relation storage, indexing |
| `axiograph-storage` | Unified | Dual-format persistence |
| `axiograph-llm-sync` | Sync | Extraction, validation, grounding |

### 2. Integration Tests (`tests/integration_tests.rs`)

Cross-crate integration tests:

- **Canonical `.axi` parsing**: Rust parses the canonical corpus via `axi_v1`
- **Storage ↔ PathDB**: Persistence and restarts
- **LLM Sync ↔ Storage**: Extraction scaffolding integrates with storage
- **Complete Pipeline**: End-to-end smoke tests
- **Persistence**: Data survives restart
- **Concurrency**: Parallel writes/reads

### 3. E2E Tests

#### LLM Sync E2E (`axiograph-llm-sync/tests/e2e_tests.rs`)

- Full extraction pipeline
- Facts land in .axi and PathDB
- Grounding context retrieval
- Review workflow (approve/reject)
- Conflict detection
- Event emission
- Statistics tracking

#### PathDB E2E (`axiograph-pathdb/tests/pathdb_tests.rs`)

- String interning
- Entity storage
- Relation storage
- Serialization roundtrip
- Persistence
- Bitmap operations
- Large scale (100k entities)

#### Storage E2E (`axiograph-storage/src/tests.rs`)

- Dual-format writes
- Source segregation
- Changelog persistence
- Batch operations
- Concept/guideline storage

## Test Scenarios

### Machinist Knowledge Flow

```
1. User has conversation with LLM about titanium cutting
2. LLM sync extracts facts:
   - "Titanium is a Material"
   - "Always use coolant when cutting titanium"
   - "Carbide tools recommended"
3. Facts validated against schema
4. High-confidence facts auto-integrated
5. Low-confidence facts queued for review
6. Data written to:
   - llm_extracted.axi (human-readable)
   - knowledge.axpd (PathDB binary)
7. User queries "titanium cutting parameters"
8. Grounding context built from PathDB
9. LLM responds with cited facts
```

### Test Coverage

```
┌─────────────────┬────────────────────────────────────────┐
│ Layer           │ What's Tested                          │
├─────────────────┼────────────────────────────────────────┤
│ Parsing         │ .axi syntax, edge cases, errors        │
│ Certificates    │ Rust emission, Lean verification       │
│ Storage         │ Atomic writes, dual format, changelog  │
│ PathDB          │ Indexes, queries, serialization        │
│ LLM Sync        │ Extraction, validation, conflicts      │
│ Grounding       │ Context building, guardrails           │
│ Review          │ Approve/reject workflow                │
│ Persistence     │ Restart survival, concurrent access    │
└─────────────────┴────────────────────────────────────────┘
```

## Running Tests

### All Unit Tests
```bash
cargo test --workspace --lib
```

### Specific Crate
```bash
cargo test -p axiograph-storage
cargo test -p axiograph-llm-sync
cargo test -p axiograph-pathdb
```

### Integration Tests
```bash
cargo test --test integration_tests
```

### E2E Tests
```bash
cargo test -p axiograph-llm-sync --test e2e_tests
cargo test -p axiograph-pathdb --test pathdb_tests

# PathDB `.axpd` ↔ `.axi` snapshot roundtrip (via UnifiedStorage + PathDBExportV1)
cargo test -p axiograph-llm-sync --test pathdb_snapshot_export_tests
```

### With Output
```bash
cargo test -- --nocapture
```

### Specific Test
```bash
cargo test test_full_extraction_pipeline
```

### Slow/Ignored Tests
```bash
cargo test -- --ignored
```

## REPL

```bash
cd rust
cargo run -p axiograph-cli -- repl

# Optionally preload a snapshot
cargo run -p axiograph-cli -- repl --axpd path/to/snapshot.axpd
```

### Example Session

```text
axiograph> gen 10000 8 8 3 1
axiograph> stats
axiograph> follow 0 rel_0 rel_1 rel_2
axiograph> export_axi build/snapshot_pathdb_export_v1.axi
axiograph> save build/snapshot.axpd
axiograph> exit
```

For a longer walkthrough, see `docs/tutorials/REPL.md`.

## Scripted Demos (end-to-end)

These scripts live under `scripts/` and are intended to be runnable from repo root:

- Offline ontology-engineering (all source types): `scripts/ontology_engineering_all_sources_offline_demo.sh`
- Large `.axpd` performance demo: `scripts/perf_large_proto_api_axpd.sh`
- Public RDF/OWL/SHACL dataset ingest: `scripts/rdfowl_public_datasets_demo.sh`
- Web ingest demos:
  - small list: `scripts/web_mixed_sources_demo.sh`
  - Wikipedia crawl: `scripts/web_wikipedia_crawl_demo.sh`

## Test Utilities

### Temp Directory
```rust
use tempfile::tempdir;

let dir = tempdir().unwrap();
// Files automatically cleaned up when `dir` drops
```

### Test Storage
```rust
fn test_storage() -> (UnifiedStorage, TempDir) {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };
    let storage = UnifiedStorage::new(config).unwrap();
    (storage, dir)
}
```

### Test Conversation
```rust
fn machinist_conversation() -> Vec<ConversationTurn> {
    vec![
        ConversationTurn {
            role: Role::Assistant,
            content: "Titanium is a Material...".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
    ]
}
```

## CI Integration

### GitHub Actions
```yaml
name: CI
on: [push, pull_request]
jobs:
  rust-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: make rust-test

  semantics:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Lean (elan)
        run: |
          if [ ! -x "$HOME/.elan/bin/elan" ]; then
            curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh -s -- -y
          fi
          echo "$HOME/.elan/bin" >> "$GITHUB_PATH"
      - run: make verify-semantics

  k8s-manifests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: azure/setup-helm@v4
      - uses: azure/setup-kubectl@v4
      - run: helm lint deploy/helm/axiograph
      - run: helm lint deploy/helm/axiograph -f deploy/helm/axiograph/values-replicas.yaml
      - run: helm lint deploy/helm/axiograph -f deploy/helm/axiograph/values-ingress.yaml
      - run: helm lint deploy/helm/axiograph -f deploy/helm/axiograph/values-rwx-sync.yaml
      - run: helm template axiograph deploy/helm/axiograph > /tmp/axiograph.yaml
      - run: kubectl apply --dry-run=client -f deploy/k8s
      - run: kubectl apply --dry-run=client -f /tmp/axiograph.yaml

  container-build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/build-push-action@v5
        with:
          context: .
          file: Dockerfile
          load: true
          tags: axiograph:ci
      - run: |
          docker run --rm -v /tmp:/out axiograph:ci \
            tools perf pathdb \
            --entities 200 --edges-per-entity 2 --rel-types 2 \
            --index-depth 1 --path-len 1 --queries 10 \
            --out-axpd /out/ci.axpd
      - run: |
          cid=$(docker run --rm -d -p 7878:7878 -v /tmp:/data axiograph:ci \
            db serve --axpd /data/ci.axpd --listen 0.0.0.0:7878)
          for i in $(seq 1 30); do
            if curl -fsS http://127.0.0.1:7878/status >/dev/null; then
              docker stop "$cid"
              exit 0
            fi
            sleep 1
          done
          docker logs "$cid" || true
          docker stop "$cid" || true
          exit 1
```

### Local CI
```bash
# Focused Rust+Lean semantics suite
make verify-semantics

# All tests (Rust + any optional layers installed)
make test
```

## Coverage

### With Tarpaulin
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --workspace --out Html
open tarpaulin-report.html
```

### With llvm-cov
```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --html
```

## Performance Testing

### CLI Harness (PathDB)
```bash
cd rust
cargo run -p axiograph-cli --release -- tools perf pathdb \
  --entities 200000 --edges-per-entity 8 --rel-types 8 --index-depth 3 --path-len 3 --queries 50000
```

Perf scripts in `scripts/` build with release + LTO and default to
`-C target-cpu=native` (set `PERF_NATIVE=0` to disable).

### Ignored Performance Tests (PathDB)
```bash
cd rust
cargo test -p axiograph-pathdb --release -- --ignored
```

### Benchmarks
```bash
# When benchmarks are added
cargo bench
```

## Debugging Tests

### With Logging
```rust
#[test]
fn test_with_logging() {
    tracing_subscriber::fmt::init();
    // Test code...
}
```

### Print Statements
```bash
cargo test test_name -- --nocapture
```

### GDB/LLDB
```bash
cargo test test_name --no-run
# Find binary in target/debug/deps/
gdb ./target/debug/deps/axiograph_storage-xxxxx
```

## Adding New Tests

### Unit Test
```rust
// In src/lib.rs or src/tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_feature() {
        // ...
    }
}
```

### Integration Test
```rust
// In tests/integration_tests.rs
#[test]
fn test_cross_crate_feature() {
    use axiograph_storage::*;
    use axiograph_llm_sync::*;
    // ...
}
```

### Async Test
```rust
#[tokio::test]
async fn test_async_feature() {
    // ...
}
```

## Test Checklist

Before submitting changes:

- [ ] All unit tests pass: `cargo test --workspace --lib`
- [ ] Integration tests pass: `cargo test --test integration_tests`
- [ ] No clippy warnings: `cargo clippy --workspace`
- [ ] Formatting correct: `cargo fmt --check`
- [ ] Doc tests pass: `cargo test --doc`
- [ ] New features have tests
- [ ] Edge cases covered
