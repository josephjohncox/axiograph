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
# From repo root, with rustc 1.88.0: exact publication decision
make release-gate

# From repo root: focused semantics suite (Rust + Lean)
make verify-semantics

# From repo root: focused canonical semantic spine gate
make verify-canonical-spine

# Exact-byte compiler properties plus Rust/Lean parser/typechecker parity
make verify-w02-compiler

# Primary regulated-shipment usefulness fixture (Rust + VerifyMain + Lean support)
make verify-regulated-shipment

# Finite category/dependent/groupoid semantics plus runtime non-closure regressions
make verify-lean-theory

# Focused exact-byte category formation/congruence/saturation parity and rejection
make verify-lean-e2e-category-kernel-v3

# Rust semantic VCS runtime plans checked against Lean theory
make verify-lean-semantic-vcs

# Authenticated SQLite materialization and verified PathDB hydration
cd rust
cargo test -p axiograph-store --test materialization
cargo test -p axiograph-pathdb --test materialization_tests
cd ..

# Reject unsafe code in every first-party Rust package and source file
make check-no-unsafe

# Focused untrusted-boundary regressions
cargo test --manifest-path rust/Cargo.toml -p axiograph-security
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli --bin axiograph
python3 -m unittest scripts.tests.test_validate_release_archive

# Rust tests
cd rust
cargo test

# Performance runners (run in release mode)
cargo run -p axiograph-cli --release -- tools perf pathdb --entities 200000 --edges-per-entity 8 --rel-types 8 --queries 50000
cargo run -p axiograph-cli --release -- tools perf axql --entities 200000 --edges-per-entity 8 --rel-types 8 --mode star --queries 2000 --limit 200

# Scenario-based in-memory perf (typed model-like graphs)
cargo run -p axiograph-cli --release -- tools perf scenario --scenario proto_api --scale 10000 --index-depth 3
```

## Choosing The Right Gate

| Gate | Use it for | Notes |
| --- | --- | --- |
| `make release-gate` | The only binary/container publication decision | Requires rustc 1.88.0 exactly, then runs catalog validation, Rust formatting, the no-unsafe gate, full locked workspace tests, the CLI feature matrix, `make verify-semantics` (including the regulated-shipment fixture), and `git diff --check`. Publication workflows must depend on this result. |
| `make verify-regulated-shipment` | Primary usefulness and CI fixture | Compiles baseline/candidate canonical modules; checks runtime theory, CQ, evolution, behavior/codegen, TypeDB/PathDB projections, VerifyMain type/constraint certificates and exact bounded query completeness; materializes a reviewed typed merge; reopens authenticated SQLite/PathDB state; compiles the generated Rust test; and requires adversarial reviewer, path, query, explanation, and materialization cases to reject. |
| `make check-no-unsafe` | First-party Rust safety policy | Verifies every workspace package inherits `unsafe_code = "forbid"`, scans every checked-in Rust source file for the `unsafe` keyword outside comments and literals, then checks all targets and features with the compiler lint enabled. |
| Focused security commands below | Untrusted I/O, parser, process, network, saturation, and mutation boundaries | Covers no-follow same-handle reads, atomic outputs, JSON/CBOR depth, process descendants and floods, public/loopback peer pinning, Git URL/ref policy, MCP/LSP frames, SQLite substitution/limits, and strict archive extraction. See `docs/reference/SECURITY_BOUNDARIES.md`. |
| `make verify-canonical-spine` | Current user/agent cleanup across the canonical spine | Runs the no-unsafe gate, Rust formatting, runtime theory checker tests, prepared-query tests, semantic VCS tests, software-authoring examples, embeddings tests, typed projection/readback tests, Lean `SemanticVCS`, `verify-lean-semantic-vcs`, and `git diff --check`. |
| `make verify-w02-compiler` | Exact-byte canonical compiler changes | Runs canonical compiler unit/property/source-gate tests, including imported-schema visibility, builds Rust and Lean parser/typechecker executables, and checks all W02 positive/adversarial corpus expectations in both implementations. The full workspace suite additionally checks canonical-backed runtime-index retention and import-aware REPL loading. |
| `make verify-semantics` | Broad Rust + Lean semantic verification | Includes positive certificate fixtures, approved-checker adversarial rejection tests, query-result V4 checks, parser/digest parity, finite merge/rebase conformance, and externally anchored V2 lineage parity/adversarial cases. |
| `make verify-lean-theory` | Finite category/dependent/groupoid semantics | Builds and runs `axiograph_finite_theory_tests`; runs canonical-kernel tests; checks authoring/query/merge finite-theory receipts; accepts ordered relation-object projections, dependent role/context witnesses, lifecycle-checked holes, refinements, bounded reachability saturation, and explanation replay; rejects malformed projections/order/equations/refinements/bounds/explanations; runs the anchored regulated-shipment Rust-to-Lean certificate; then runs runtime non-closure regressions. |
| `make verify-lean-e2e-category-kernel-v3` | Focused trusted finite category slice | Rust and Lean must agree on the shared category formation corpus; Rust emits an Envelope V3 certificate for the exact regulated-shipment `.axi`; Lean reconstructs 23 objects, 43 arrows, identities, and one equation, replays congruence plus 70 reachable endpoint explanations, and rejects presentation, congruence, and saturation tampering. |
| `make verify-lean-semantic-vcs` | Runtime merge/rebase plan conformance against Lean theory | Checks reduced Rust semantic VCS payloads with the Lean `SemanticVCS` checker. |
| `make verify-lean-e2e-query-result-module-v4` | Query-bound Rust/Lean digest parity | Builds the V2 stdio checker and covers conjunction, disjunction, Boolean, hidden-variable, projection-order, and limit cases. |
| `cd rust && make test-projections` | Typed projection and readback validation without containers | Exercises PathDB/TypeDB/TerminusDB/RDF/OWL/property-graph manifests generated directly from `CompiledKernelSnapshot`, including semantic loss and adversarial evidence-only readback. |
| `cargo test -p axiograph-store --test materialization` | Authenticated SQLite `.axpd` contract | Covers deterministic images, exact/logical digests, limits, corruption, substitution, recovery, fault injection, and arbitrary-byte rejection. |

The no-unsafe policy covers Axiograph-owned Rust. It does not claim that every
transitive dependency is implemented without `unsafe`; dependency review is a
separate supply-chain concern. There is no first-party exception or allowlist.
A new workspace package that fails to inherit the lint, an `unsafe` token hidden
behind an inactive `cfg`, and compiler-visible unsafe constructs all fail the
gate.

For what these gates mean, read `docs/reference/KERNEL_IR.md`,
`docs/reference/RUNTIME_THEORY_CHECKER.md`,
`docs/reference/SOFTWARE_AUTHORING_TOOLS.md`,
`docs/reference/SEMANTIC_VCS.md`,
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`, and
`docs/howto/FORMAL_VERIFICATION.md`.

## Production Hardening Gates

Canonical `.axi` modules are public semantic input. SQLite `.axpd` files are
immutable derived images under AxiStore and must be verified before hydration.

Run focused gates from `rust/`:

```bash
# Shared bounded file/JSON/process primitives.
cargo test -p axiograph-security

# CLI network, Git, query, MCP/LSP, verifier, and output boundaries.
cargo test -p axiograph-cli --bin axiograph

# Verified CBOR and reconciliation bounds.
cargo test -p axiograph-llm-sync

# Strict release archive validation and extraction.
cd ..
python3 -m unittest scripts.tests.test_validate_release_archive
cd rust

# Store-family deterministic/authenticated materialization contract.
cargo test -p axiograph-store --test materialization

# PathDB hydration is reachable only after store-family receipt/image checks.
cargo test -p axiograph-pathdb --test materialization_tests

# Runtime evidence staging has no custom WAL, changelog, or PathDB writer.
cargo test -p axiograph-storage

# Compile every repository-owned caller after legacy API deletion.
cargo check -p axiograph-pathdb --all-targets
cargo check -p axiograph-storage --all-targets
cargo check -p axiograph-llm-sync --all-targets
cargo check -p axiograph-cli --all-targets

make check-no-unsafe
make verify-canonical-spine
make verify-semantics
make release-gate
make verify-lean-theory
make verify-lean-semantic-vcs
make verify-axi-store
```

The materialization suite checks insertion-order invariance, semantic-row digest
sensitivity, exact N/N+1 count/string/fanout/page/file limits, SQLite header and
V2 schema validation, accepted-manifest and protected-main membership,
review-candidate rejection, anchor/digest mutation, truncation, file
substitution, rejection of old binary formats, quarantine/rebuild behavior,
image/receipt fault points, exact cache bindings, deterministic deletion/rebuild,
and bounded arbitrary-byte inputs.

Additional hardening gates belong in executable targets before they count as
finished. Certificate determinism and Rust/Lean parity remain separate from the
`.axpd` contract: a verified materialization is still not a Lean proof.

## Test Categories

### 1. Unit Tests (`cargo test --lib`)

Per-crate unit tests in `src/lib.rs` or `src/tests.rs`:

| Crate | Tests | Description |
| ------- | ------- | ------------- |
| `axiograph-dsl` | Parsing | .axi syntax parsing |
| `axiograph-pathdb` | Storage | Entity/relation storage, indexing |
| `axiograph-storage` | Evidence staging | In-memory typed validation and rollback |
| `axiograph-llm-sync` | Sync | Extraction, validation, grounding |

### 2. Integration Tests (`tests/integration_tests.rs`)

Cross-crate integration tests:

- **Canonical `.axi` parsing**: Rust parses the canonical corpus
- **AxiStore ↔ PathDB**: Receipt-checked materialization and verified hydration
- **LLM Sync ↔ Storage**: Extraction into process-local evidence staging
- **Complete Pipeline**: Review, promotion, materialization, and query regressions
- **Persistence**: AxiStore restart and old-or-new transaction outcomes
- **Concurrency**: Generation-CAS writers and read-only materialization access

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
- Verified `.axpd` hydration
- Rebuildable indexes
- Bitmap operations
- Large scale (100k entities)

#### Storage E2E (`axiograph-storage/src/tests.rs`)

- Typed evidence validation
- Source segregation
- In-memory rollback
- Batch operations
- Concept/guideline staging

## Test Scenarios

### Regulated Shipment Primary Flow

```text
1. Compile RegulatedShipmentBaseline.axi and RegulatedShipment.axi from exact bytes.
2. Check finite relation objects, role projections, indexed/refined roles, equations, rewrites, and CQs.
3. Execute ShipmentContainsBatch / BatchHasCertificate with max_hops=2.
4. Require VerifyMain to accept exactly CoA_RX_42 and reject a missing-row answer.
5. Review the finite compiled-payload evolution diff.
6. Publish the candidate on a review ref and materialize an exact-two-parent typed AxiStore merge.
7. Build an immutable SQLite .axpd image, reopen the store, and hydrate PathDB only after receipt checks.
8. Emit TypeDB/PathDB projections and compile/run the generated Rust behavior test.
```

The exact-completeness theorem is scoped to the bounded finite query denotation.
The merge is exact payload accounting, not an arbitrary pushout. The
`category_kernel_v3` presentation, equation-congruence, and bounded
reachability check is in `VerifyMain`; broader finite interpretations,
refinements, contexts, holes, and transports remain theorem support.

### Machinist Knowledge Flow

```
1. User has conversation with LLM about titanium cutting
2. LLM sync extracts facts:
   - "Titanium is a Material"
   - "Always use coolant when cutting titanium"
   - "Carbide tools recommended"
3. Facts validated against schema
4. All extracted facts remain typed evidence proposals
5. Reviewer promotes accepted canonical `.axi` changes
6. AxiStore records accepted objects and immutable semantic history
7. An authenticated SQLite `.axpd` image is derived from accepted state
8. User queries "titanium cutting parameters"
9. Grounding context is built only after verified PathDB hydration
```

### Test Coverage

```
┌─────────────────┬────────────────────────────────────────┐
│ Layer           │ What's Tested                          │
├─────────────────┼────────────────────────────────────────┤
│ Parsing         │ .axi syntax, edge cases, errors        │
│ Certificates    │ Rust emission, Lean verification       │
│ Storage         │ AxiStore objects, refs, audit, receipts│
│ PathDB          │ SQLite auth, hydration, indexes, query │
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
cargo test -p axiograph-store --test materialization
cargo test -p axiograph-pathdb --test materialization_tests
```

### Typed Projections And Advanced Graph Backend Containers

Run the non-container projection contract and CLI tests first:

```bash
cd rust
make test-projections
```

These tests compile exact canonical `.axi` through the single
`CompiledKernelSnapshot` authority and project it to PathDB, TypeDB,
TerminusDB, RDF/OWL, and portable property-graph manifests. They check complete
finite `KernelRefV2` record coverage, backend capability declarations,
dependent/refinement structure, semantic-loss classes, read-only artifacts,
and evidence-only readback. Adversarial cases cover removed versions, empty
adapter provenance, wrong projection anchors, wrong backend kinds, duplicate
records, missing records, payload/native-key drift, unexpected records, unknown
authority fields, and forbidden native higher-path claims.

The separate container tests bring up the currently prioritized external graph
backends with Docker Compose and exercise a minimal native operation against
each one:

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

The current backend regression checks are deliberately narrow:

- `TypeDB`: create a database, define a minimal schema, insert data, query it back
- `TerminusDB`: boot the server, create a database, verify it appears in the CLI listing

These tests validate backend bootability and basic native API behavior only.
They do not consume `ProjectionManifestV1` and are not projection-semantic or
readback-contract tests. They do not elevate any backend into the semantic
kernel; Axiograph still treats these systems as typed projection/execution
targets rather than the source of truth. Container smoke tests are an explicit
opt-in gate for image/API behavior, not a prerequisite for typed projection
validation.

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
```

### Example Session

```text
axiograph> gen 10000 8 8 3 1
axiograph> stats
axiograph> follow 0 rel_0 rel_1 rel_2
axiograph> exit
```

For a longer walkthrough, see `docs/tutorials/REPL.md`.

## Scripted Demos (end-to-end)

These scripts live under `scripts/` and are intended to be runnable from repo root:

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

The checked-in workflows are the executable policy; do not copy an abbreviated
workflow from this document:

- `.github/workflows/ci.yml` runs `make release-gate` on pinned Ubuntu 24.04
  with rustc 1.88.0 and the checked-in Lean/Lake manifest. Native container
  lanes run the checked-in smoke and checker-integrity policy.
- `.github/workflows/release.yml` is the only tag entry point. Bundle jobs and
  the reusable container publisher depend on the same successful
  `make release-gate` job.
- `.github/workflows/docker.yml` has no push or manual trigger. It publishes
  only when called by the tag workflow, pushes architecture images by digest,
  runs the image smokes, and creates tags only after both digests pass.
- Third-party actions use immutable commit SHAs and checkout does not retain
  credentials. Job permissions are narrowed to the publication operation.

A package job derives its archive name from `rustc -vV`'s host triple. It then
checks the outer archive SHA-256, extracts into a fresh directory, checks the
inner checker SHA-256, checks Unix mode `0755`, runs the CLI, directly checks
the durable envelope V3 / `query_result_v4` exact fixture, and accepts it through
`axiograph-verifier-stdio-v2` with build id `axiograph-verify-main-v3`. The
receipt decision is `accepted`.

### Local CI

```bash
# Exact release decision; requires rustc 1.88.0.
PATH="$(dirname "$(rustup which --toolchain 1.88.0 rustc)"):$PATH" \
  make release-gate

# Focused Rust+Lean semantics suite.
make verify-semantics

# Runtime semantic merge/rebase plans checked against Lean theory.
make verify-lean-semantic-vcs
```

### Hosted negative release rehearsal

The release workflow exposes `inject_verify_failure` only on
`workflow_dispatch`. Run it against a temporary tag that points at the exact
candidate commit:

```bash
gh workflow run release.yml \
  --ref v0.6.0-rehearsal-fail \
  -f inject_verify_failure=true
```

The injected verify step exits before any build or publication job. A hosted
rehearsal is complete only after the run records `verify` as failed, all bundle
and publish jobs as skipped, `gh release view v0.6.0-rehearsal-fail` reports no
release, and the registry has no matching GHCR tag. Delete the temporary remote
tag after collecting that evidence. A local run or a dispatch against a branch
does not satisfy the hosted-tag requirement.

### Release platform claims

A platform is supported only after its exact tag-workflow lane has built,
extracted, and passed the packaged smoke for that release. Configured candidate
lanes are Linux x86_64 (`x86_64-unknown-linux-gnu`, Ubuntu 24.04 ABI with
OpenSSL 3), macOS arm64 (`aarch64-apple-darwin`, deployment target 13.0), and
Windows x86_64 (`x86_64-pc-windows-msvc`, Windows Server 2025 build lane).
Until a tag run records those results, they remain release candidates rather
than supported platforms.

Linux arm64 is currently a container lane only, not a binary bundle. macOS
Intel, Windows arm64, and every other unexecuted runner path are unsupported.
A Docker multi-architecture manifest does not imply native binary support.

### Release evidence map

| Release requirement | Executable evidence |
| --- | --- |
| One decision | `make release-gate`; both publication jobs depend on the release workflow's `verify` job. |
| Pinned language inputs | rustc 1.88.0 is asserted; `rust/Cargo.lock` is checked in and every release command uses `--locked`; Lean uses `lean-toolchain` and `lake-manifest.json` without `lake update`. |
| Binary integrity | The package audit verifies the host-triple name, deterministic archive, outer and inner SHA-256, extraction, mode, CLI version, and an accepted stdio V2 receipt. |
| Container integrity | CI and release run BuildKit `--check`, installed-checker checksum, non-root/mode checks, canonical `.axi` validation, authenticated DB command checks, and bare-`.axpd` rejection. |
| Fail-closed server readiness | Unit tests `verifier_status_requires_explicit_operator_approval` and `verifier_status_reports_complete_v2_readiness`. |
| Format/protocol claim | Envelope V3, `query_result_v4`, stdio V2, exact-finite build id V3, and receipt decision `accepted` are asserted by the package audit. |
| Platform support | Support is per successful tag-lane artifact; missing native lanes remain unsupported. |
| No premature publication | Release assets require all bundle audits and the container manifest. Architecture images are pushed by digest without a tag; manifest tags are created only after both image audits. |
| Hosted failure injection | `workflow_dispatch` can fail the `verify` job through `inject_verify_failure` while running at a temporary tag ref. Evidence remains open until the hosted run records all dependent build/publish jobs skipped and confirms that no release asset or GHCR tag appeared. |

The query gate has no legacy reader. Package evidence must exercise the V4
exact fixture plus the missing-row, duplicate-row, and truncated adversarial
fixtures; an old artifact cannot satisfy the current claim.

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

### CLI Performance Runner (PathDB)

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
