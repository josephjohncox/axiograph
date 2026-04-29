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

# From repo root: focused canonical semantic spine V1 gate
make verify-canonical-spine

# Rust semantic VCS runtime plans checked against Lean theory
make verify-lean-semantic-vcs

# PathDBExportV1 snapshot `.axi` parse parity (debug/live-byte/parser parity only)
make verify-pathdb-export-axi-v1

# Rust tests
cd rust
cargo test

# Performance harnesses (run in release mode)
cargo run -p axiograph-cli --release -- tools perf pathdb --entities 200000 --edges-per-entity 8 --rel-types 8 --queries 50000
cargo run -p axiograph-cli --release -- tools perf axql --entities 200000 --edges-per-entity 8 --rel-types 8 --mode star --queries 2000 --limit 200

# Scenario-based perf (typed “model-like” graphs) + live `.axpd` roundtrip
cargo run -p axiograph-cli --release -- tools perf scenario --scenario proto_api --scale 10000 --index-depth 3 --out-axpd build/proto_api.axpd
```

## Choosing The Right Gate

| Gate | Use it for | Notes |
| --- | --- | --- |
| `make verify-canonical-spine` | Current user/agent cleanup across the canonical spine | Runs Rust formatting, runtime theory checker tests, prepared-query tests, semantic VCS tests, software-authoring examples, embeddings tests, backend pushdown tests, Lean `SemanticVCS`, `verify-lean-semantic-vcs`, and `git diff --check`. |
| `make verify-semantics` | Broad Rust + Lean semantic verification | Includes certificate fixtures, parser/digest parity, semantic VCS conformance, and the debug-only PathDB export parity gate. |
| `make verify-lean-semantic-vcs` | Runtime merge/rebase plan conformance against Lean theory | Checks reduced Rust semantic VCS payloads with the Lean `SemanticVCS` checker. |
| `cd rust && make test-backend-pushdown` | Backend projection plan-shape validation without containers | Exercises TypeDB/TerminusDB pushdown plans generated from `CompiledSchemaIr` and capability profiles. |
| `make verify-pathdb-export-axi-v1` | Debug/live-byte/parser parity for `PathDBExportV1` only | Do not use this as semantic, query, certificate, teaching, or accepted-plane promotion evidence. |

For what these gates mean, read `docs/reference/KERNEL_IR.md`,
`docs/reference/RUNTIME_THEORY_CHECKER.md`,
`docs/reference/SOFTWARE_AUTHORING_TOOLS.md`,
`docs/reference/SEMANTIC_VCS.md`,
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`, and
`docs/howto/FORMAL_VERIFICATION.md`.

## Production Hardening Gates

These gates are the current pass/fail surface for greenfield cleanup. Canonical
`.axi` modules are public semantic input. `PathDBExportV1` remains a
debug/live-byte/parser-parity format only; it must not become a semantic,
query, certificate, teaching, or accepted-plane promotion surface.

Run the focused gates from `rust/` unless the command says `make`:

```bash
# Canonical examples stay canonical; catalog/README must not foreground exports.
cargo test -p axiograph-cli --test examples_e2e \
  example_catalog_paths_exist_and_stay_teaching_oriented
cargo test -p axiograph-cli --test examples_e2e \
  examples_readme_keeps_storage_debug_roundtrips_out_of_teaching_path

# Canonical-only public semantic surfaces reject derived PathDBExportV1 snapshots.
cargo test -p axiograph-cli --test examples_e2e \
  canonical_only_cert_commands_reject_pathdb_export_snapshots
cargo test -p axiograph-cli --test examples_e2e \
  accept_promote_rejects_pathdb_export_snapshot_without_mutating_store
cargo test -p axiograph-cli --test examples_e2e \
  querycert_rejects_pathdb_export_snapshot_smoke

# REPL teaching scripts do not emit removed debug snapshot exports or commands.
cargo test -p axiograph-cli --test examples_e2e \
  repl_scripts_canonical_smoke
cargo test -p axiograph-cli --test examples_e2e \
  repl_rejects_removed_export_axi_command

# PathDBExportV1 remains deterministic/reversible for debug parity only.
cargo test -p axiograph-pathdb --test axi_export_tests \
  pathdb_export_v1_roundtrip_is_deterministic_and_reversible
cargo test -p axiograph-pathdb --test axi_export_property_tests

# Live `.axpd` remains v1-readable while deterministic scaffolding hardens.
cargo test -p axiograph-pathdb stable_live_snapshot_digest_v1_survives_v1_envelope_roundtrip
cargo test -p axiograph-pathdb canonical_fact_log_v1_is_deterministic_and_certified_for_imported_axi
cargo test -p axiograph-pathdb certified_canonical_fact_log_v1_rejects_fact_id_mismatch
cargo test -p axiograph-pathdb typed_fact_builder_can_commit_certified_only_fact_id
cargo test -p axiograph-pathdb test_axpd_convergence_status_stays_explicit

# Rust/Lean digest and parser parity for the supported semantic/parity lanes.
make verify-canonical-spine
make verify-semantics
make verify-lean-semantic-vcs
make verify-pathdb-export-axi-v1
```

Additional hardening gates should be introduced as executable targets before
they are considered done:

- **Deterministic certificate JSON**: a fixed canonical `.axi` fixture and fixed
  query/check command must emit byte-identical certificate JSON on repeated
  runs. The test should compare bytes, not only parsed JSON.
- **Stable canonical ids**: compiled canonical `.axi` facts, relation objects,
  projection arrows, and snapshots must expose deterministic ids derived from
  module digest plus local/content identity, then assert stability across parse,
  import, export-module, and accepted-plane promotion.
- **Live `.axpd` convergence**: an actual production checkpoint must be created
  from accepted canonical state, reloaded or served from bytes, and shown to keep
  the same canonical module digest and accepted snapshot anchor. Do not satisfy
  this gate with only a `PathDBExportV1` `.axi` roundtrip. Current code reports
  the status through `axpd_convergence_status_v1()`: live reads use the v1
  `PathDB::to_bytes/from_bytes` envelope, while the sectioned v2 header in
  `verified.rs` is scaffolding and not yet the runtime format.
- **Canonical fact log/snapshot gate**: accepted-plane promotion must append an
  immutable JSONL event, write a snapshot manifest, update `HEAD` only after the
  manifest is durable, and reject derived snapshot exports without mutating any
  of those files. Current scaffolding is `CanonicalFactLogV1` plus
  `stable_live_snapshot_digest_v1`; the full accepted-plane JSONL/manifest gate
  is still pending.
- **Optional deep lanes**: fuzz/property/Miri/Kani/Loom/Shuttle targets are
  optional until the tools and surfaces exist, but each target must either skip
  explicitly when the tool is missing or run a named minimal suite. Property
  tests that already live under `axiograph-pathdb/tests/*_property_tests.rs`
  remain part of normal Rust test coverage.

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

# Debug-only PathDBExportV1 `.axpd` ↔ `.axi` snapshot roundtrip
cargo test -p axiograph-llm-sync --test pathdb_snapshot_export_tests
```

### Advanced Graph Backend Containers

Run the backend pushdown plan-shape tests before using containers:

```bash
cd rust
make test-backend-pushdown
```

These non-container tests verify that the TypeDB and TerminusDB planners consume
`CompiledSchemaIr` plus backend/projection capability profiles, preserve the
read-only native projection notes, generate native-readable TypeQL/RDF/WOQL
projection artifacts, and keep mutation authority in Axiograph. The generated
artifacts are tested as plan surfaces only; container smoke tests remain the
separate opt-in check for backend image/API compatibility.

These smoke tests bring up the currently prioritized external graph backends with Docker Compose and
exercise a minimal typed operation against each one:

- `TypeDB` as the primary high-fidelity typed backend target
- `TerminusDB` as the RDF/VCS-shaped secondary target

The compose file lives at
[rust/tests/docker/graph_backends.compose.yml](../../rust/tests/docker/graph_backends.compose.yml).
The test target is intentionally `ignored` by default because it requires Docker,
image pulls, and a slower startup path than the normal crate tests.

Run from repo root:

```bash
make test-backend-containers
```

Run from `rust/` directly:

```bash
AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 cargo test --test backend_container_tests -- --ignored --nocapture
```

Useful environment flags:

- `AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1` enables the ignored backend test.
- `AXIOGRAPH_KEEP_BACKEND_TEST_CONTAINERS=1` leaves the compose project running
  after the test for manual inspection.

The current smoke checks are deliberately narrow:

- `TypeDB`: create a database, define a minimal schema, insert data, query it back
- `TerminusDB`: boot the server, create a database, verify it appears in the CLI listing

These tests validate backend bootability and basic compatibility. They do not
elevate any backend into the semantic kernel; Axiograph still treats these
systems as typed projection/execution targets rather than the source of truth.
Container smoke tests are an explicit opt-in gate for image/API compatibility,
not a prerequisite for ordinary backend pushdown plan validation.

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

# Runtime semantic merge/rebase plans checked against Lean theory
make verify-lean-semantic-vcs

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
