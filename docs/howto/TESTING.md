# Axiograph Test Suite

**Diataxis:** How-to  
**Audience:** contributors

## Read-only database client workflow

From the repo root, build the runtime assets and run the nonempty temporary-store
client/server workflow (requires Node/npm, the existing locked frontend install,
and Rust; exact release pins still apply to release acceptance):

```bash
(cd frontend/viz && npm ci --ignore-scripts && npm run build && npm test)
cargo build --locked --offline --manifest-path rust/Cargo.toml -p axiograph-cli --bin axiograph
cargo run --locked --offline --manifest-path rust/Cargo.toml -p axiograph-cli \
  --example read_only_client_workflow
# Separate default Rust coverage: controlled temporary assets, no Node/npm needed.
cargo test --locked --offline --manifest-path rust/Cargo.toml -p axiograph-cli \
  --test db_server_e2e
```

The fixture creates fresh AxiStores and publishes/opens materializations using
production repository APIs. Its logical entities and supporting report blobs are
synthetic; the materialization receipts are genuine repository-bound image
receipts, **not Lean checker receipts or evidence of canonical semantic validity**.
The explicit `read_only_client_workflow` integration gate starts actual loopback
servers, fetches `/viz`, and invokes the production TypeScript client. It fails on
missing prerequisites rather than skipping. Default Rust e2e tests instead use
small controlled temporary assets for HTTP/template checks and never invoke
frontend tools; a workspace test pass does not cover the production-client gate.
Logs show nonempty exact/approximate queries, truncation,
status and complete runtime trust/non-claims, malformed/legacy query rejection,
unsupported read-probed routes, and headless query availability after missing
assets or UI-budget rejection. Children and temporary stores are cleaned up.
No mutation/LLM request is sent. This is HTTP/client coverage, not browser/layout
or accessibility coverage.

For an existing legitimately published receipt-bound image, start the server:

```bash
rust/target/debug/axiograph db serve --dir "$AXISTORE_DIR" \
  --materialization "$MATERIALIZATION_ID" --listen 127.0.0.1:7878
# Open http://127.0.0.1:7878/viz in a browser.
curl --fail http://127.0.0.1:7878/capabilities
curl --fail http://127.0.0.1:7878/status
curl --fail -H 'content-type: application/json' http://127.0.0.1:7878/query \
  --data '{"query":{"version":1,"select_vars":["entity"],"where_atoms":[{"kind":"attr_eq","term":"?entity","key":"axiograph.value","value":"Alice"}],"limit":10}}'
# Deliberate rejection: legacy AxQL is not this HTTP protocol (HTTP 400).
curl -i -H 'content-type: application/json' http://127.0.0.1:7878/query \
  --data '{"query":"select ?x where ...","certify":true}'
```

`Alice` is an example value, not a guaranteed entity in arbitrary images. Use the
IR editor directly; Rust owns path parsing, name resolution, typing and execution.
Local graph/context selections are not implicitly sent. The shared transport
profile is `frontend/viz/src/server/read-only-api.json`, consumed by Rust discovery
and the strict TypeScript boundary. The nested QueryIr schema is **descriptive**:
its explicit-object branches are regression-checked, but it is not complete
serde/semantic equivalence. For example serde permits omitted versions and some
extra inner fields that the documented profile rejects; the browser requires an
explicit version but delegates atom validation to Rust. This is not a complete
OpenAPI description, generated SDK or browser query compiler.

The client validates closed capability/status/result/trust envelopes, integer
counts/u32 rows, row-variable consistency, runtime claims and required non-claims.
Receipt internals and the descriptive schema remain explicitly opaque; nested
semantic summary strings are shape-checked, not reinterpreted as checked refs.
It rejects unknown versions/fields, bounds streamed JSON to 16 MiB, input to 1 MiB,
and response structure to depth 64 / 500,000 visited values, with a 30-second
request timeout. Superseded replies/editor changes cannot replace current output;
failures visibly retain only the labelled previous successful output. Fixed
same-origin API paths and redirect rejection prevent capability-supplied URLs.
The older optional offline `?data=` graph loader is outside this API boundary;
whole graph JSON validation remains open. No graph is relabeled authenticated
from sampled status, and no remote query/evidence IDs are highlighted locally.

Unsupported controls are disabled with CLI guidance, and their programmatic
callbacks also deny remote requests. Existing local draft/graph inspection is
preserved; cached legacy storage keys are not migrated from status or used as
identity/authority. `/viz` caches one bounded export at startup after authenticated
opening: no static directory routes, CORS, request-selected roots or HTML sanitizer.
Missing/unbuilt/over-budget assets produce bounded 503 and `ui_available=false`;
health/status/query continue. UI-only preflight caps are 1,000 entities / 4,000
relations, 8 KiB per referenced string, 1 MiB aggregate repeated referenced text,
and 16,384 attribute entries. Extraction selects at most 250 nodes / 1,000 edges,
without meta/theory/equivalence expansion; JSON/template/script caps are
1 MiB / 1 MiB / 2 MiB with a checked 4 MiB inlined payload ceiling. These bound
intermediates and label amplification; 4 MiB payload is not a peak-memory claim.
Build assets from the same checkout before starting the server.

### Read-only client recovery evidence (review required)

The timeout recovery's fresh available-tool run passed clean installation,
script-disabled audit (zero vulnerabilities), both TypeScript configurations,
**29 frontend tests**, debug then production builds (**38 modules / 91,425 JS
bytes**), **999 workspace tests / 0 failures / 3 existing ignored / 96 groups**,
strict default workspace all-target Clippy, strict no-default CLI/query all-target
Clippy and formatting. Focused selections were nonempty: **7 viz**, **3 DB**,
**6 genuine receipt-bound HTTP**, **1 unsupported-browser-boundary** and **1
certificate-policy documentation** tests; counts overlap the workspace suite.
The production TS/client example separately exercised all three available,
UI-budget-unavailable and missing-asset cases. A fresh CLI Family export copied
and inlined the exact production script (SHA-256
`63d59683208d439c4e9322a2155ec7d4e6bbf198baba02fa4e09660198d5835e`), with
22 nodes / 86 edges and unchanged Family input; temporary output was removed.

`build/engineering-quality/read-only-client-recovery-final-01/` retains the first
complete hash-guarded run. After this documentation update, the same full command
list is rerun under `read-only-client-recovery-final-02/` before freezing, so the
manifest must establish final documentation/source byte equality too. Review
entry points are `read-only-client-recovery-review.diff` (complete original-stage
increment, including new files), `read-only-client-recovery-only.diff`,
`read-only-client-recovery-manifest.json`,
`read-only-client-recovery-source-index.json`, and
`read-only-client-recovery-handoff.md` under `build/engineering-quality/`.
Review exact `.txt` copies only; do not invoke source tools on live TS/MJS/Rust
after validation. The read-only path-restricted verifier is:

```bash
python3 build/engineering-quality/read-only-client-recovery-verify.py
```

Any drift blocks acceptance; expected hashes are not refreshed to hide changes.
Available Node 26.1.0/npm 11.13.0 still fail the required 26.8.1/11.19.0 pin gate
with actual exit **2**. This is development evidence, **not acceptance**, release,
browser/layout/accessibility, live-provider or globally clean diagnostics evidence.
The old 993-test integration is historical; all nine feature configurations and
semantic/canonical integration gates still require the separate final integration.
EQ-02/EQ-05 remain partial; the historical gitleaks timeout remains inconclusive.

### Read-only client final integration evidence (review required)

After independent source review, the separate integration rerun passed **999
workspace tests / 0 failed / 3 existing ignored / 96 groups**, default strict
workspace all-target Clippy and **all nine** strict CLI/query feature
configurations. `verify-canonical-spine` passed **144 Rust tests**;
`verify-semantics` passed **395 Rust tests / 2 existing ignored**;
`verify-regulated-shipment` passed **12 Rust tests**; `verify-w02-compiler`
passed **47 Rust tests and 11 Rust/Lean cases**. Counts overlap. Required query
selectors actually executed **6 prepared-query**, **13 verifier-bridge**, **1
prepared-AST golden**, and **1 shipment query** tests after the named gates'
checker prerequisites. All **7 query-selection guard regressions** passed.
Focused package/source-diagnostic/public-query parity passed **3 tests** and
software-authoring passed **7**; these supplement, not replace, workspace checks.

Fresh script-disabled frontend install/audit (zero vulnerabilities), both
typechecks, **29 tests**, debug then production (**38 modules / 91,425 JS bytes**),
the opt-in production client **three-case** HTTP workflow and fresh CLI byte-exact
Family export passed again. The export retained **22 nodes / 86 edges**, copied
and inlined SHA-256
`63d59683208d439c4e9322a2155ec7d4e6bbf198baba02fa4e09660198d5835e`, and preserved
Family source bytes. Default Rust HTTP tests remain frontend-tool-independent;
controlled test assets are not production frontend evidence. The explicit
example above is still required for production-client coverage.

New evidence uses `build/engineering-quality/read-only-client-integration-*`:
`final-01/` preserves the initial 30-command run; the same 30-command list runs
again in `final-02/` after this documentation update. The final manifest requires
all **736 declared input hashes** to match before/after every command. Review
starts at `read-only-client-integration-handoff.md`: it maps the complete
original-stage `review.diff` (including new files), additional integration-only
`only.diff`, exact `.txt` source copies and log ranges. Full hash maps are separate.
The read-only verifier is `read-only-client-integration-verify.py`; any later
drift blocks acceptance, and expectations must not be repaired. Prior recovery
and rendering artifacts remain unchanged historical checkpoints.

The pin prerequisite still **fails with actual exit 2** on available Node
26.1.0/npm 11.13.0 rather than required 26.8.1/11.19.0. No override, pinned
frontend/release pass, browser/layout/accessibility or live-provider claim is
made. Synthetic HTTP fixture support blobs are not checker receipts; the real
semantic gates' checker results do not confer proof authority on the HTTP UI.
Independent final review and parent verification remain required. EQ-02/EQ-05
remain partial; global diagnostic cleanliness and the historical inconclusive
gitleaks timeout are not resolved by this integration.

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
# From repo root, with rustc 1.98.0 plus the pinned fuzz tools: publication decision
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

# Focused exact-byte category/groupoid formation, congruence, trace parity, and rejection
make verify-lean-e2e-category-kernel-v3

# Generic endpoint-indexed normalization/equivalence Rust-Lean parity and rejection
make verify-lean-indexed-path-theory

# Rust semantic VCS runtime plans checked against Lean theory
make verify-lean-semantic-vcs

# Authenticated SQLite materialization and verified PathDB hydration
cd rust
cargo test -p axiograph-store --test materialization
cargo test -p axiograph-pathdb --test materialization_tests
cd ..

# Reject unsafe code in every first-party Rust package and source file
make check-no-unsafe

# Reject unreviewed unwrap, expect, panic, and unreachable paths in production Rust
make check-no-panics

# Exact-Node install, advisory audit, typechecks, tests, and both frontend builds
make verify-viz

# Exact-lock RustSec audit with no ignored vulnerabilities
make verify-rustsec

# Bounded libFuzzer smoke over .axi, certificates, REPL/plugin input, and .axpd bytes
make verify-fuzz

# Miri over pure identity/framing/domain-separation kernel tests
make verify-miri

# Loom model over the production child-process concurrency reservation algorithm
make verify-loom

# Kani proof that all u32 fixed-point construction paths enforce the bound
make verify-kani

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

## Nested authoring drilldown

The focused production projection regressions cover every nested collection,
lossless full/page unions, deterministic ordering, empty and singleton
collections, N/N+1 entry limits, exact byte boundaries, oversized items,
malformed and cross-workspace cursors, changed imports, hidden compilation
failures, closed response schemas, and CLI/MCP/HTTP/LSP parity:

```bash
cargo test --manifest-path rust/Cargo.toml --locked --offline \
  -p axiograph-cli --bin axiograph authoring_workspace::projection::tests
```

Measure the two checked-in packages through the production CLI. Generate the
high-cardinality input under `build/` so it cannot become accepted source:

```bash
python3 examples/software_authoring/generate_high_cardinality_authoring.py \
  --out-dir build/authoring-high-cardinality --relations 180
cargo run --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli -- \
  authoring workspace --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --detail full --out build/software-authoring-full.json
cargo run --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli -- \
  authoring workspace --workspace . \
  --request examples/regulated_shipment/authoring_request.json \
  --detail full --out build/regulated-shipment-full.json
cargo run --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli -- \
  authoring workspace --workspace . \
  --request build/authoring-high-cardinality/high_cardinality_request.json \
  --detail summary --nested-collection competency_evaluations \
  --nested-limit 20 --nested-byte-limit 1048576 \
  --out build/high-cardinality-cq-page.json
```

Use `wc -c` and a JSON parser for measurement; do not infer capacity from a
teaching fixture or from HTTP's 16 MiB rejection. The generated 180-family input
is a bounded stress fixture, not an ontology-completeness or peak-memory claim.
For real HTTP client parity, start the documented read-only authoring server and
run:

```bash
python3 examples/software_authoring/compact_authoring_client.py \
  --request examples/software_authoring/authoring_workspace_request.json \
  --check-full
```

The client checks top-level references and nested runtime closure steps against
an explicit full report. It verifies byte counts, SHA-256 identities, offsets,
totals, and source identity on every page. It does not mint receipts or mutate
accepted state.

## Required Query-Library Selections

These named gates use `scripts/run_required_query_tests.py` against
`axiograph-query --lib`, not the CLI crate:

| Gate | Cargo substring filter |
| --- | --- |
| `make verify-lean-certificate-rejections` | `verifier_bridge::tests` |
| `make verify-canonical-spine` | `prepared_query` |
| `make verify-lean-e2e-query-result-module-v4` | `approved_lean_checker_matches_prepared_ast_goldens` |
| `make verify-regulated-shipment` | `regulated_shipment_exact_path_query_is_complete_and_missing_rows_reject` |

The helper preserves Cargo/test failures and requires a nonempty successful
libtest summary backed by matching named passed tests. Missing/blank filters,
zero matches, all-ignored selections, and missing/unrecognized output cannot pass.
It uses the existing bounded subprocess runner (600 seconds, 8 MiB per output
stream); exceeding those bounds fails the gate. The checker-dependent Make gates
build Lean first; running the helper alone is not a substitute for that prerequisite.

```bash
# Focused guard regressions (including empty/ignored and failed commands)
python3 -m unittest scripts.tests.test_run_required_query_tests

# Process-free prepared-query library tests, with nonempty-selection protection
python3 scripts/run_required_query_tests.py --package axiograph-query --filter prepared_query
```

## Strict CLI Feature Boundaries

From the repository root, with the locked dependencies cached:

```bash
# Nine strict all-target configurations for axiograph-query and axiograph-cli:
# default, minimal, six individual features, all-features.
make lint-cli-feature-matrix

# Actual subprocess entrypoints; no credentials or external provider fixtures.
cargo test --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli --test feature_boundaries_cli
cargo test --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli --no-default-features --test feature_boundaries_cli
for feature in repl-rustyline llm-ollama llm-openai llm-anthropic profiling proposal-adapter-http; do
  cargo test --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli --no-default-features --features "$feature" --test feature_boundaries_cli || exit
done
cargo test --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli --all-features --test feature_boundaries_cli

# Minimal library/binary regressions and default integration checks:
cargo test --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli --no-default-features --bin axiograph --lib
cargo test --manifest-path rust/Cargo.toml --locked --offline --workspace
cargo clippy --manifest-path rust/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

`lint-cli-feature-matrix` is an opt-in maintenance gate, not a new release
prerequisite or a substitute for `release-gate`. Each Cargo invocation uses
`--all-targets --locked --offline -- -D warnings`; the six single-feature runs
use `--no-default-features --features axiograph-cli/<feature>`. Defaults and the
original compile-only `check-cli-feature-matrix` are unchanged. This covers nine
supported configurations, not every subset of optional features.

`feature_boundaries_cli.rs` requires unavailable-provider errors from actual draft,
augmentation and predictive-plugin commands, no connection to a loopback canary,
and no output/trace mutation. Child environments are cleared. Mock output still
requires exact canonical input bytes; tampered source rejects. No-provider
candidate generation and exclusive-provider selection are checked in every build.
Expected counts are 2 for default/all-features, 4 for a single LLM provider, and 5
for minimal or a non-LLM single feature; unavailable-provider tests are compiled
only when that provider is absent. These tests do not call enabled live providers.

Current bounded EQ-19 evidence: all nine strict lint configurations and all nine
subprocess test runs pass (36 overlapping passes); minimal CLI lib/bin tests pass
254; fresh default workspace tests pass 975, with 3 existing ignored tests across
94 groups. The earlier 40-diagnostic no-default lint failure and 973-test import
integration remain historical in the engineering-quality roadmap. Existing parser,
endpoint/peer-pinning and adapter tests are retained. Full rendered book, pinned
frontend/release and live-provider/backend checks are separate, not implied here.

The later bounded source-diagnostics integration rerun passed **993 workspace
tests, 0 failed, 3 ignored across 96 groups**, default strict workspace Clippy,
all nine strict feature configurations, and workspace formatting. It also passed
`verify-w02-compiler` (47 Rust tests and 11 Rust/Lean cases),
`verify-canonical-spine` (144 Rust tests), `verify-semantics` (394 Rust tests,
2 existing ignored), and `verify-regulated-shipment` (12 Rust tests). Counts
across commands overlap. The named query guards executed 6 prepared-query,
13 rejection, 1 prepared-AST golden, and 1 shipment-query tests, not empty
selections. See the [engineering-quality integration record](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md#diagnostics-integration-eq-06eq-07-bounded-eq-19)
for exact logs, real adapter checks, preservation evidence and open review scope.
This is not broader diagnostic/LSP completion or a release-gate result.

## No-unsafe ownership gate

The scanner walks the filesystem. Git does not select the Rust input set.
The scan includes ignored, untracked, unattached, and disabled Rust source.
It prunes only `.codebase-index`, `.git`, `.lake`, `.pi-subagents`, `node_modules`, and `target` on ordinary branches.

Two fixed Kani 0.67.0 library caches have a narrow ownership exception.
`scripts/no_unsafe_external_cache_manifest_v1.json` pins every directory, file size, and file SHA-256.
The scanner removes the exception if Git or Cargo owns any candidate path.
It also removes the exception for a missing, extra, changed, linked, or special object.
A matching cache proves distribution identity only. It does not prove vendor safety.

The scanner uses Linux type-first file access.
It opens metadata with `O_PATH` and no-follow flags before it opens file data.
It does not data-open a FIFO, socket, device, link, or other rejected type.
It then opens only the held regular inode through `/proc/self/fd` with `O_NONBLOCK`.
Directory reads use bounded incremental `getdents64` batches. Repository and candidate
counters are charged before names enter a sortable buffer. An exact byte budget permits
one EOF observation; the next nonempty batch fails before it yields a name. Declared
candidate and source bytes are charged before data allocation or read. The generator
limits an output-parent read to 65,536 bytes and stops at the first non-dot entry.
The scanner and generator fail closed with their family-specific unsupported code if
the host cannot provide these operations.

Run the focused tests before the full gate:

```bash
python3 -m unittest scripts.tests.test_check_no_unsafe \
  scripts.tests.test_no_unsafe_policy
AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE=1 \
  python3 -m unittest \
  scripts.tests.test_no_unsafe_policy.OfficialGeneratorCliTests
make check-no-unsafe
```

The second command reads the preserved 137,826,806-byte official archive.
It uses the production bounded gzip and tar parser.

### Hosted capability fixtures

`.github/workflows/no-unsafe-capability-tests.yml` runs only on pushes to
`ci/no-unsafe-capability-tests-*`. It uses the standard GitHub-hosted
`ubuntu-24.04` and `ubuntu-24.04-arm` runners. The workflow downloads the pinned
Kani ARM64 archive with a 137,826,806-byte transfer ceiling, then verifies its
exact size and SHA-256 identity. It never executes an archive payload.

The ordinary runner user prepares isolated repositories and runs exact-byte
copies of the production scanner and generator. The scanner fixture has a
minimal Cargo workspace, an offline lockfile, and an owned Git baseline. The
candidate cache stays untracked.

`sudo` runs only two fixture helper commands. The setup command creates one
mode `000` character device with identity `1:3`. It also creates one read-only
`nosuid,nodev,noexec` bind mount. The cleanup command unmounts the recorded
mount and removes the recorded device. The production CLIs never use `sudo`.

The setup command records each observed identity before it starts the next
privileged operation. Mount hardening validates and preserves the pre-mount
target identity in the persisted ready state. It rolls back the mount before
the device after a setup failure. Cleanup checks the mount ID, mount root,
backing device, and directory identities. It also checks the device inode and
device identity. Cleanup
refuses an unknown or replaced object. These checks cover observed operations
only. They do not give crash-atomic cleanup or authority over a future object
at the same path.

The tests fail unless setup and test UIDs differ. They also require a real
device and a real hardened bind mount. The production CLIs must report the
exact TYPE-05 and CHECK-03 leaf causes.

A post-checkout step creates the evidence directory before the Rust action.
Thus, a toolchain setup failure can retain cleanup evidence. An `always()` step
copies only allowlisted, nonsymlink regular evidence files to a new upload
directory. Each input has a 1 MiB limit. Missing success files and a failed
cleanup remain visible in the evidence manifest. Upload safety does not change
the job result or claim semantic success. The upload never contains a fixture
tree.

Local unprivileged contract tests do not substitute for those hosted results:

```bash
python3 -m unittest scripts.tests.test_hosted_no_unsafe_capabilities
```

A hosted pass proves only real-device TYPE-05 and real-mount-crossing CHECK-03
behavior for the exact tested commit, fixture, and runner architectures. It is
not a full no-unsafe suite, roadmap acceptance, or a release decision.
It compares the result with the checked-in manifest and writes to a fresh empty directory.
Synthetic archive tests use the production parser for parser limits and extension-header accounting.
They also test path grammar, kind and size precedence, special kinds, framing, and inventory rejection.
A persistent global PAX `path` applies to each later ordinary member.
A local PAX `path` overrides the global value for one ordinary member only.
The next ordinary member uses the persistent global value again.
The parser accepts only the supported `path` key and the inert `comment` key.
It rejects PAX `size`, `linkpath`, unknown semantic keys, and sparse keys instead of ignoring them.

The row evidence runner reads `scripts/no_unsafe_row_cases_v1.json`.
It rejects absent, unexecuted, unknown, and wrong-level evidence.
It executes each assignment separately for its normative row.
It records the actual passing `unittest.TestCase` assertion calls, including their arguments and test source coordinates, under that row and level.
It rejects a passing row if an assigned execution has no observed assertion or if stored row observations differ from the executed outcome.
It does not copy `required_evidence` into an observed-result field.
It also records the command, working directory, allowed environment, exit, standard output, and standard error.
Use a new output path for each run because the runner uses exclusive creation:

```bash
python3 scripts/run_no_unsafe_row_evidence.py \
  --case-map scripts/no_unsafe_row_cases_v1.json \
  --output build/engineering-quality/no-unsafe-row-evidence/run-01.json
```

The case map contains all 81 normative rows.
A row count or a callable test name is not execution evidence.

Use the offline generator only with its three fixed absolute paths:

```bash
mkdir -p build/engineering-quality/no-unsafe-regeneration/manual-run
python3 scripts/generate_no_unsafe_external_cache_manifest.py \
  --archive "$PWD/build/engineering-quality/release-roadmap/prepare-compatible-release-20260908T035930Z/tools/kani-home/kani-0.67.0-aarch64-unknown-linux-gnu.tar.gz" \
  --out "$PWD/build/engineering-quality/no-unsafe-regeneration/manual-run/regenerated-kani-library-inventory.json" \
  --check-manifest "$PWD/scripts/no_unsafe_external_cache_manifest_v1.json"
```

The output directory must exist and must be empty.
The generator uses exclusive creation and never replaces or deletes an object.
Remove a failed-run residue only after a person inspects it.
The generator has no network, install, extraction, or tracked-manifest write mode.

## Choosing The Right Gate

| Gate | Use it for | Notes |
| --- | --- | --- |
| `make release-gate` | The only binary/container publication decision | Requires rustc 1.98.0, Node.js 26.8.1, npm 11.19.0, cargo-audit 0.22.2, `nightly-2026-09-01` with Miri and `rust-src`, `cargo-fuzz 0.13.2`, and `cargo-kani 0.67.0` exactly; then runs catalog validation, Rust formatting, the no-unsafe and no-panic gates, full locked workspace tests, the CLI feature matrix, locked frontend and RustSec advisory audits, bounded fuzz targets, pure identity-kernel Miri tests, the Loom child-limiter model, the Kani fixed-point-constructor proof, `make verify-semantics` (including the regulated-shipment fixture), and `git diff --check`. Publication workflows must depend on this result. |
| `make verify-regulated-shipment` | Primary usefulness and CI fixture | Compiles baseline/candidate canonical modules; checks runtime theory, CQ, evolution, behavior/codegen, TypeDB/PathDB projections, VerifyMain type/constraint/category certificates; runs `axiograph check finite-query` for baseline and candidate; binds the accepted exact-answer receipt into each reviewed trust gate; materializes a reviewed typed merge; reopens authenticated SQLite/PathDB state; builds accepted-derived grounding bound to the reopened receipt and exact query; compiles the generated Rust test; and requires adversarial reviewer, path, query, placeholder-receipt, explanation, and materialization cases to reject. |
| `make check-no-unsafe` | First-party Rust safety policy | Verifies workspace `unsafe_code = "forbid"` inheritance. Scans ordinary filesystem Rust, including ignored and untracked files. Exempts only two exact unowned Kani cache inventories. Then checks all locked targets and features with the compiler lint. |
| `make check-no-panics` | First-party production panic policy | Runs Clippy over every workspace library and binary with all features and rejects `unwrap`, `expect`, `panic!`, and `unreachable!`. Three closed, resource-impossible or serializer-infallible invariants carry local reviewed lint exceptions; test-only assertion paths are outside this production target. |
| `make verify-viz` | Frontend dependency, typecheck, test, and build gate | Requires Node.js 26.8.1 and npm 11.19.0 from `.node-version` and `.npm-version`, installs only `package-lock.json` with lifecycle scripts disabled, rejects moderate-or-higher npm advisories, runs typechecks and Node regression tests, then builds debug and production bundles (production last). Both build scripts also typecheck before invoking Vite. Current pins are Vite 8.2.2, TypeScript 7.0.2, esbuild 0.28.2, PostCSS 8.5.26, and Rolldown 1.2.4; obsolete vulnerable Rollup is absent. |
| `make book` | Published documentation gate | Downloads the pinned mdBook 0.5.4 binary for the current host, verifies the platform-specific SHA-256 digest, validates the curated chapter graph, builds the static site, and rejects broken rendered links or missing search/theme artifacts. Pull requests build the same book; pushes to `main` publish it through immutable GitHub Pages actions. |
| `make verify-rustsec` | Exact Rust lockfile advisory gate | Requires cargo-audit 0.22.2 and audits both workspace and isolated fuzz lockfiles without ignored vulnerabilities. RDF/XML uses Oxigraph's immutable upstream quick-xml 0.42 migration commit until a patched crate release; the structural preflight and semantic parser therefore share the patched XML line. The remaining `ttf-parser` notice is informational and unmaintained, not a RustSec vulnerability. |
| `make verify-fuzz` | Bounded adversarial parser smoke | Requires pinned `nightly-2026-09-01` and `cargo-fuzz 0.13.2`; first compiles all five harnesses under a separate 600-second process-group bound, then copies checked seed corpora into a temporary directory and runs named `.axi`, certificate JSON, REPL-command, predictive-proposal adapter response, and authenticated `.axpd` byte targets with case-time, process-time, output, input-size, run-count, RSS, and single-artifact bounds. CI and release verification install and run the exact tool versions. |
| `make verify-miri` | Interpreter-level identity-kernel hardening | Runs seven pure tests for exact-byte identities, authenticated-field sensitivity, framing, registry closure, Rust/Lean domain parity, scoped semantic refs, and strict ID parsing under Miri. The lane forces SHA-2's portable software backend because Miri must interpret Rust code rather than target-specific cryptographic intrinsics; production builds retain runtime-selected acceleration. It explicitly reports `SKIP` when Miri or `rust-src` is unavailable; `release-gate` instead uses `verify-miri-required` and fails if either pinned component is missing. |
| `make verify-loom` | Exhaustive small-state concurrency model | Runs the production child-slot reservation algorithm with Loom atomics under two-thread contention and proves the configured maximum is never exceeded and every acquired slot is released. This models real shared mutable state rather than a synthetic concurrency example. |
| `make verify-kani` | Bounded model checking over the complete constructor input domain | With `cargo-kani 0.67.0` and its bundled Rust 1.93 compiler, proves for every `u32` that `FixedPointProbability::try_new` accepts exactly the numerators at or below the shared denominator and preserves accepted values exactly. Workspace packages declare Rust 1.93 as their minimum while normal release compilation uses Rust 1.98.0. It explicitly reports `SKIP` when Kani is unavailable; `release-gate` uses `verify-kani-required` and fails if the exact version is missing. |
| Focused security commands below | Untrusted I/O, parser, process, network, saturation, and mutation boundaries | Covers no-follow same-handle reads, atomic outputs, JSON/CBOR depth, process descendants and floods, public/loopback peer pinning, Git URL/ref policy, MCP/LSP frames, SQLite substitution/limits, and strict archive extraction. See `docs/reference/SECURITY_BOUNDARIES.md`. |
| `make verify-canonical-spine` | Current user/agent cleanup across the canonical spine | Runs the no-unsafe gate, Rust formatting, runtime theory checker tests, prepared-query tests, semantic VCS tests, software-authoring examples, embeddings tests, typed projection/readback tests, Lean `SemanticVCS`, `verify-lean-semantic-vcs`, and `git diff --check`. |
| `make verify-w02-compiler` | Exact-byte canonical compiler changes | Runs canonical compiler unit/property/source-gate tests, including imported-schema visibility, builds Rust and Lean parser/typechecker executables, and checks all W02 positive/adversarial corpus expectations in both implementations. The full workspace suite additionally checks canonical-backed runtime-index retention and import-aware REPL loading. |
| `make verify-semantics` | Broad Rust + Lean semantic verification | Includes positive certificate fixtures, approved-checker adversarial rejection tests, query-result V4 checks, parser/digest parity, finite merge/rebase conformance, and externally anchored V2 lineage parity/adversarial cases. |
| `make verify-lean-theory` | Finite category/dependent/groupoid semantics | Builds and runs `axiograph_finite_theory_tests`; runs canonical-kernel tests; checks endpoint-indexed unit/inverse/associativity/congruence laws, mandatory normalization traces, authoring/query/merge finite-theory receipts, ordered relation-object projections, dependent role/context witnesses, lifecycle-checked holes, refinements, bounded reachability saturation, and explanation replay; rejects malformed paths, trace offsets, projections/order/equations/refinements/bounds/explanations; runs the anchored regulated-shipment Rust-to-Lean certificate; then runs runtime non-closure regressions. |
| `make verify-lean-e2e-category-kernel-v3` | Focused trusted finite category/groupoid slice | Rust and Lean must agree on the shared category formation corpus, including valid identity equations and rejection of repeated dependent indices; Rust emits an Envelope V3 certificate for the exact regulated-shipment `.axi`; Lean reconstructs 23 objects, 43 arrows, identities, and one equation, replays endpoint-aware one-witness-per-equation congruence, 86 formal inverse cancellation traces, and 70 reachable endpoint explanations, then rejects presentation, congruence, groupoid-trace, and saturation tampering. This wire path is decision procedure plus replay, not a denotation theorem. |
| `make verify-lean-indexed-path-theory` | Generic endpoint-indexed path certificate slice | Rust emits mandatory normalization/equivalence/congruence traces; Lean accepts all three and rejects missing or tampered traces, non-composable endpoints, invalid positions, and confidence-field injection. |
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

## Frontend Context And Typecheck Regressions

From the repository root:

```bash
cd frontend/viz
npm ci --ignore-scripts
node --test tests/context.test.mjs   # focused production context functions
npm run typecheck
npm test
npm run build:debug
npm run build
cd ../..
make verify-viz                     # requires the exact Node/npm pins
```

The Node built-in runner imports production TypeScript using Node's native type
stripping; this is not typechecking. `npm run typecheck` checks all files below
`src` with `strict: true`. It then runs the smaller `tsconfig.context.json` check
for compatibility with the accepted focused gate. No production module uses a
TypeScript suppression directive. This is whole-frontend strict type coverage,
not complete JSON validation.

`context.test.mjs` checks numeric ordering, cross-fact deduplication, empty sets,
re-enabling controls, selection/name/badge consistency, and hostile labels. It
uses the production option builder plus small DOM write spies that reject HTML
sinks and check literal `textContent`; no browser emulator or duplicate context
logic is involved. Browser rendering, keyboard/accessibility, review, query, and
promotion flows still need their own integration coverage.

`typecheck-gate.test.mjs` checks the whole-source include and rejects suppression
or explicit `any` boundaries. It copies the real sources, scripts, and
configuration into a temporary directory, checks valid sources, and then injects
invalid graph-node, graph-edge, UI, context-map, DOM, and read-only draft state.
The actual `typecheck`, `build`, and `build:debug` scripts must fail with
TypeScript diagnostics without touching a bundle sentinel. The test cleans up its
temporary directory and never alters checkout sources. Local results with
different Node/npm versions are development evidence only; do not override the
exact-toolchain check to claim `make verify-viz` passed.

### Whole-frontend strict migration (pending parent review)

The production state contract is in `src/types.ts`. It gives graph nodes and
edges numeric identities and gives review, draft, layout, path, component, run,
and local describe state explicit types. Review and draft state uses an
`empty`/`loaded` discriminant, so selected proposals cannot exist without a
validated overlay. `src/dom.ts` binds each required static
control to its actual HTML or SVG element type and fails when the document omits
one. App context extensions use typed staged composition instead of unchecked
object casts. Embedded, fetched, and stored graph or draft values must pass the
production boundary predicates before they enter typed state.

`types.test.mjs` covers accepted and rejected graph and draft boundary values.
The source-policy regression also rejects unparameterized empty `Map` and `Set`
construction because their default type arguments can reintroduce `any` inside a
strict build. All app-context collection constructors now state their key and
value types. Unknown values remain only at JSON, storage, callback, and rendering
boundaries and must be narrowed before typed state uses them. The staged context
assertion is paired with `Object.assign`; production code has no assertion cast.

The existing rendering, query, status, draft-selection, and read-only tests still
exercise production functions. This migration does not validate every nested
wire payload and does not provide browser layout or accessibility evidence.
Those EQ-02 items remain separate. The documentation checks for this work are
`make book-validate` and `make book`: they validate the curated chapter graph,
build with pinned mdBook, and check rendered links and required artifacts. There
is no `lint-docs` target or `scripts/lint_docs.py`; those names are not project
gates. This implementation is pending parent review; it does not close a roadmap
marker or authorize accepted mutation.

## Frontend Rendering And Draft Selection Regressions

```bash
cd frontend/viz
node --test tests/rendering.test.mjs
# User-oriented local scenario: select one proposal and see remote draft,
# commit and promotion blocked for the read-only server.
node --test --test-name-pattern='real draft-before-add' tests/rendering.test.mjs
npm test                         # includes context and injected-error build gates
npm run build:debug
npm run build                    # production assets last
```

The following describes the historical accepted rendering slice; its mocked
remote-success cases were subsequently migrated to zero-request rejection and
local prefill assertions by the read-only client lane above. Current routing and
production-client validation are described there, not by the old mocked flows.

`rendering.test.mjs` bundles and imports the real list/detail/status/draft/add/LLM
modules using the already locked esbuild dependency. Small DOM operation spies
reject HTML sinks; they are not a browser, layout engine or accessibility tree.
Coverage includes hostile labels/IDs/attributes/status text; grouping and virtual
slice replacement; highlights and shift-click selection; rich detail sections,
tabs, tuple fields and safe numeric same-page links; review filtering, checkbox
selection, truncation, clearing, evidence lookup and initialization order. Draft
selection copies only changed arrays/envelopes; read-only retained records and
nullable chunk fields remain intact. Invalid selector shapes and stale IDs give
actionable errors before requests or output clearing, not empty success. Backend
proposal validation and accepted promotion authority are unchanged; this is not a
complete frontend wire validator. Tests use no real credentials or accepted stores.

The actual `llmAskBtn` callback is also exercised with mocked generated overlays:
Review prefill must continue through clear/highlight/rerender with status `ok`.
Persisted auto-commit preferences cannot enable the disabled option, send mutation
fields/admin tokens, interpret commit-looking data as success, or navigate.
Query-certificate policy precedence remains covered (`emit`, `verify`, and
`require_verified`). Generated overlays receive evidence-only Review/CLI guidance.
These are callback regressions, not live `/llm/agent` endpoint coverage.

Commit/promote buttons explicitly say **CLI only**: at that historical checkpoint,
`db_server.rs` exposed only `GET /healthz`, `GET /status` and `POST /query`. Its V2 status
has no mutation role/capability. Both buttons block for every status result,
including unavailable/malformed status and legacy `role=master`; no admin POST
branch remains. They give canonical `.axi` check/authoring and AxiStore CLI
guidance without clearing local drafts. The V2-shaped unit fixture deliberately
has no authoritative receipt and is not a genuine server response. At that checkpoint, other
frontend request routes (including `/discover/draft-axi`, proposal generation and
DocChunk lookup) did not match the backend; mocked responses were not successful
live workflows. The read-only client lane now removes/disables these remote paths
rather than restoring unsupported endpoints.

The compatibility context configuration still checks `render/dom.ts`,
`core/status.ts`, and `core/draft-selection.ts`. The whole-source configuration
now strict-checks every production module. The negative gate tests reject
mutation of the read-only selection, object-shaped fake DOM/text values, and
string graph, path, and context identities. Full nested JSON validation, real
browser interactions, keyboard focus, screen-reader labels/roles,
layout/scrolling, and accessibility remain open under EQ-02. Only runs with the
pinned Node 26.8.1 and npm 11.19.0 satisfy this frontend source gate; they do not
constitute a new release gate or release.

To inspect the resulting offline UI manually after a production build:

```bash
out=$(mktemp -d)
rust/target/debug/axiograph tools viz examples/Family.axi \
  --out "$out/family.html" --format html --all --typed-overlay
# Open "$out/family/index.html"; inspect grouped nodes, shift-click a path,
# switch detail tabs, and confirm review remains evidence-only/offline.
# Remove the temporary output when finished: rm -r "$out"
```

The CLI resolves and reads current `frontend/viz/dist` at runtime, copies assets
and inlines its script for offline export; a Rust rebuild does not embed new JS.
An export/inlining byte check is integration evidence, not a browser smoke pass.

### Frozen rendering evidence recovery

The original rendering and correction manifests describe historical checkpoints:
native automatic formatting changed reviewed inputs after validation twice.
The second drift affected only `tests/rendering.test.mjs` (tested SHA-256
`7edce0bbdffb369e42602cbdf425424c1e446b3aeb6153fb83a8231a47ddbd4e`,
then `8623682cf541c386b864780ab5c8c2585dc8240680b74ae03f40ed5f8c5d527e`,
1,051 physical lines). The latest independent rereview found no additional
behavioral issue, but could not establish final-byte evidence. Earlier logs and
manifests remain unchanged; their final-byte claims do not cover that drift.

The scoped recovery reran clean `npm ci --ignore-scripts`,
`npm audit --audit-level=moderate --ignore-scripts` (zero vulnerabilities), both
TypeScript configurations, all **21 tests** (including the real click handlers
and isolated injected-error build gate), debug then production builds, the real
CLI byte-exact export, **3 Rust visualization tests** and **1 query-policy test**.
All **43 frontend inputs** matched before and after every command. This used the
available toolchain only; `make check-node-toolchain` still failed with exit 2.
No production/test edits or accepted-state mutations were part of this recovery.

Review entry points under `build/engineering-quality/` are
`rendering-frozen-manifest.json`, `rendering-frozen-combined.diff`,
`rendering-frozen-recovery-only.diff` and `rendering-frozen-source-index.json`.
After all validation and documentation, exact source/document copies are frozen
as `.txt` files in `rendering-frozen/source/`, with original-path mappings,
SHA-256, byte counts and physical line counts. Review those copies, not live
TS/MJS through source tools that can invoke automatic formatting. From repo root:

```bash
python3 build/engineering-quality/rendering-frozen-verify.py
```

The standard-library verifier only reads declared originals, manifest and frozen
copies; it fails on mismatches and never repairs bytes or refreshes expected
hashes. Independent frozen review and subsequent parent hash verification remain
required; this record is **not acceptance**. Historical primary LSP and cached
auxiliary results remain scoped supplemental evidence, not fresh global scans.
The historical gitleaks 120-second timeout remains inconclusive. A fresh lexical
count across all declared `frontend/viz/src` inputs finds **24 empty catch sites**;
this is a different scope from the earlier auxiliary report's 18 findings, not a
fresh analyzer result. Persistence/error reporting, backend route drift, browser/
accessibility coverage, exact pins and broader EQ-02 work remain open.

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
make check-greenfield-surface
make verify-fuzz
make verify-canonical-spine
make verify-semantics
make release-gate
make verify-lean-theory
make verify-lean-semantic-vcs
make verify-axi-store
```

`make verify-fuzz` requires pinned `nightly-2026-09-01` and `cargo-fuzz
0.13.2`. It builds all named harnesses under a separate 600-second cold-build
bound, copies each checked seed corpus to a private temporary directory, runs
with hard input, case-time, process-time, run-count, RSS, and artifact bounds,
and removes generated corpus entries and artifacts after the run. A build or
fuzz timeout, output overflow, missing tool or seed, lockfile drift, crash, or
nonzero exit fails the target. The fuzz driver requires POSIX process-group
containment. Release verification runs it on Ubuntu before any Linux, macOS, or
Windows bundle job can start.

`make check-greenfield-surface` parses every tracked shell script and rejects
retired binary aliases, bare `.axpd` loading/materialization commands, immutable
image mutation commands, and missing scripts advertised by `scripts/ops/README.md`.
It also scans production source for deprecated APIs, Serde or CLI aliases,
public re-export aliases, unversioned aliases for versioned APIs, retired
fixed-point aliases, backward-compatibility branches, retired frontend
storage-key migration, and the removed pre-`/api/embed` Ollama endpoint.

The materialization suite checks insertion-order invariance, semantic-row digest
sensitivity, exact N/N+1 count/string/fanout/page/file limits, SQLite header and
V2 schema validation, accepted-manifest and protected-main membership,
review-candidate rejection, anchor/digest mutation, truncation, file
substitution, rejection of old binary formats, quarantine/rebuild behavior,
image/receipt fault points, exact cache bindings, deterministic deletion/rebuild,
and bounded arbitrary-byte inputs.

Additional hardening gates belong in executable targets before they count as
finished. The CLI E2E suite emits typecheck and constraint certificates twice
and compares exact bytes; the query lifecycle test does the same for
`query_result_v4` and its certificate digest. Certificate determinism and
Rust/Lean parity remain separate from the `.axpd` contract: a verified
materialization is still not a Lean proof.

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
5. Parse the bound query receipt and make its immutable object the reviewed
   baseline/candidate/merge trust gate; reject placeholder receipt identities.
6. Review the finite compiled-payload evolution diff and its typed `Merge`
   finite-theory scope/coverage receipt.
7. Publish the candidate on a review ref and materialize an exact-two-parent typed AxiStore merge.
8. Build an immutable SQLite `.axpd` image, reopen the store, and hydrate PathDB only after receipt checks.
9. Build accepted-derived grounding whose private provenance binds that reopened receipt, accepted snapshot, stable ids, and exact query digest.
10. Emit TypeDB/PathDB projections and compile/run the generated Rust behavior test.
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
[rust/tests/docker/graph_backends.compose.yml](https://github.com/josephjohncox/axiograph/blob/main/rust/tests/docker/graph_backends.compose.yml).
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
  with rustc 1.98.0 and the checked-in Lean/Lake manifest. Native container
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
# Exact release decision; requires rustc 1.98.0.
PATH="$(dirname "$(rustup which --toolchain 1.98.0 rustc)"):$PATH" \
  make release-gate

# Focused Rust+Lean semantics suite.
make verify-semantics

# Runtime semantic merge/rebase plans checked against Lean theory.
make verify-lean-semantic-vcs
```

### Hosted negative release rehearsal

The release workflow exposes `inject_verify_failure` only on
`workflow_dispatch`. Run it against a disposable rehearsal tag that points at
the exact candidate commit. The injected verify step must fail before any build
or publication job, and neither GitHub Releases nor GHCR may contain the
rehearsal version.

The complete tag grammar, command sequence, evidence requirements, cleanup,
and never-reuse policy are in [Release Axiograph](RELEASING.md). A local run or
a dispatch against a branch does not satisfy the hosted-tag requirement.

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
| Pinned language inputs | rustc 1.98.0 is asserted; `rust/Cargo.lock` is checked in and every release command uses `--locked`; Lean uses `lean-toolchain` and `lake-manifest.json` without `lake update`. |
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
