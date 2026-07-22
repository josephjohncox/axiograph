# Canonical Semantic Spine User Guide

**Diataxis:** How-to  
**Audience:** users, contributors, and coding agents

This guide is the public workflow map for the current system. The semantic
spine is:

```text
exact canonical .axi bytes + import closure + accepted snapshot handle
  -> axiograph_kernel::CanonicalCompiler
  -> CompiledKernelSnapshot
  -> KernelSnapshotIr + SchemaPresentationIr + InstanceModelIr
  -> derived runtime indexes/reports
  -> optional Lean certificate for the supported trusted fragment
```

Use this guide when you want a clean path through validation, runtime theory
checking, typed querying, software-authoring overlays, semantic VCS,
embedding/evidence overlays, and backend projections.

## Rules Of The Road

- Exact canonical `.axi` bytes plus an immutable accepted snapshot handle are
  the public semantic input.
- `CanonicalCompiler` is the only Rust meaning compiler. It requires the full
  import closure and returns `CompiledKernelSnapshot`.
- PathDB and `.axpd` are execution/query substrates, not semantic authority.
- Query certification uses `QueryCertificatePolicyV1`: `none`, `emit`,
  `verify`, or `require_verified`.
- DDD/fDDD, BDD, implementation surfaces, codegen, and coverage policies live in
  overlays/tools, not inside domain `.axi`.
- Embeddings produce advisory evidence overlays and refinement candidates; they
  do not mutate accepted ontology state.
- Backend projections are native-readable lower-tier views. Mutation authority
  stays in Axiograph.

## 0. Run The Primary Regulated-Shipment Workflow

```bash
make verify-regulated-shipment
```

This is the documented starting point. One canonical pharmaceutical-shipment
slice exercises the compiler and finite category IR, indexed/refined roles,
path equations and explanations, exact finite `query_result_v4` checking,
compiled-payload evolution, reviewed AxiStore merge, authenticated SQLite
materialization and restart, TypeDB/PathDB projection, and a generated Rust test
that is compiled and run.

Read `examples/regulated_shipment/README.md` for the evidence map and exact
non-claims. The strongest product-trusted result in this flow is exact answer
completeness for one bounded finite path query accepted by the `VerifyMain`
import closure. The finite category/dependent/groupoid executable, AxiStore,
projections, and code generation are adjacent theorem or operational evidence,
not additional trusted-kernel claims.

## 1. Validate A Canonical Module

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/software_authoring/OrderFulfillmentDomain.axi
```

Use `check validate` as the first gate. Public semantic flows accept canonical
modules, not derived storage snapshots.

## 2. Check Runtime Theory Closure

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check theory examples/software_authoring/OrderFulfillmentDomain.axi \
  --closure-tier finite_fragment \
  --json \
  --out build/examples/software_authoring/theory_check.json
```

The runtime checker claims only scoped operational facts:

- `well_typed`: refs resolve and endpoints/contexts are admissible.
- `admissible`: supported equations, rewrites, paths, and transports satisfy
  the runtime fragment.
- `closed`: supported in-scope obligations reached the declared closure tier.
- `complete`: every in-scope obligation was checked or explicitly residual.

Unsupported higher-order/dependent cases remain addressable residual
obligations. They are not silently accepted.

## 3. Inspect The Derived Runtime Index

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover kernel-surface examples/software_authoring/OrderFulfillmentDomain.axi \
  --out build/examples/software_authoring/kernel_surface.json
```

The `kernel-surface` command name is retained as a CLI report label, but its
output is a derived `RuntimeSemanticIndex` containing `RuntimeIrRef` citations.
Promotion and certification must also retain the immutable
`CompiledKernelSnapshot` anchor. User labels and derived refs cannot mint
semantic authority.

## 4. Run Typed Queries And Certificates

For server/tooling requests, prefer structured `query_ir_v1`.

```json
{
  "lang": "query_ir_v1",
  "query_ir_v1": {
    "version": 1,
    "select_vars": ["?x"],
    "where_atoms": [
      { "kind": "type", "term": "?x", "type": "Order" }
    ],
    "limit": 10
  },
  "certificate_policy": "emit"
}
```

Certificate policies:

- `none`: execute and return typed trust/non-claim reports.
- `emit`: emit envelope V3 / `query_result_v4` when certifiable.
- `verify`: emit and ask the configured Lean verifier to check it.
- `require_verified`: fail closed unless the accepted anchor, canonical text,
  certifiable fragment, verifier, and anchor match all succeed.

Server request parsing accepts only the `certificate_policy` field for query
certificate behavior. Boolean-style request fields are not part of the public
contract.

## 5. Use Software-Authoring Overlays

Keep domain meaning in `.axi`; keep implementation mapping in overlays.

```bash
./examples/software_authoring/run_authoring_flow.sh
```

The flow demonstrates:

- canonical module validation,
- runtime theory checking,
- question-first `.cq` authoring checks,
- advisory definition lookup,
- strict overlay validation,
- advisory coverage probes,
- behavior-case reports,
- enforced software coverage,
- codegen plan previews,
- continuous semantic coverage through the example crate.

Start with the unified workspace request. It validates the canonical import
closure, lowers and executes CQs, prepares the typed query, emits holes/repairs
and explanations, and returns fail-closed promotion review in one report:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring workspace \
  --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --out build/examples/software_authoring/authoring_workspace_report.json
```

Ask advisory definition questions for authoring context:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --prompt "define the shipment eligibility business rule" \
  --include-queries \
  --out build/examples/software_authoring/definition_query.json
```

Then validate the overlay and run an advisory coverage lookup:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover overlay-check examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/software_authoring/overlay_check.json

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover coverage-query examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --term "shipment eligibility" \
  --relation OrderEligibleForShipment \
  --cq-name accepted_order_is_shipment_eligible \
  --surface-hint shipping \
  --max-matches 8 \
  --out build/examples/software_authoring/coverage_query.json
```

Use strict coverage for CI:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check software-coverage examples/software_authoring/OrderFulfillmentDomain.axi \
  --behavior-case examples/software_authoring/order_fulfillment_behavior_case.json \
  --cq-file examples/software_authoring/order_fulfillment.cq \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/software_authoring/software_coverage.json
```

## 6. Review Semantic Merge And Rebase

```bash
make verify-lean-semantic-vcs
```

Semantic merge is typed reconciliation over slices, not text merge. Dry-run
plans expose:

- slice manifests and derived `RuntimeIrRef` citations under a compiled snapshot anchor,
- join/conflict decisions,
- resolver handles,
- CQ/trust/coverage/runtime-theory blockers,
- rebase transport success/failure,
- Lean-readable reduced conformance payloads.

Materialization is fail-closed when blockers, residual obligations, failed
required transports, stale refs, unresolved resolver steps, or CQ/trust
regressions remain.

The CLI dry-run models are untrusted analysis only. Accepted merge code must
construct `axiograph_store::SemReconciliationV2` from reviewed typed parent and
result candidates. Use `CompiledKernelSnapshot::payload_fingerprints()` for the
candidate payload indexes and record one typed keep/drop/introduce/transport
decision for every parent and result payload. Then call
`AxiStore::materialize_merge` with the current generation, named source ref,
and exact expected source tip. Do not call `promote`: it rejects merge commits.
The materialized `SemCommitV2` always has ordered
`[current_main_tip, current_source_tip]` parents.

The implemented acceptance check is finite payload-union accounting. It does
not claim an arbitrary categorical pushout, general dependent transport, HoTT
univalence, higher-path completeness, or ontology closure.

## 7. Use Embeddings As Evidence

Embedding outputs are sidecar evidence, not `.axi` truth.

The expected flow is:

1. promote or load accepted canonical state,
2. generate embedding sidecar manifests,
3. discover relationship candidates,
4. review the advisory `EmbeddingEvidenceOverlayV1`,
5. convert selected candidates into typed refinement/evolution previews,
6. promote only through normal ontology review gates.

See `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` for the manifest and overlay
contracts.

## 8. Generate Typed Backend Projections

Projection manifests are lower-tier, native-readable derived views of the one
immutable compiled kernel IR. They carry closed backend capabilities, finite
coverage, semantic losses, evidence-only readback, and Axiograph-only mutation
authority for PathDB, TypeDB, TerminusDB, RDF/OWL, and property graphs.

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend typedb --search-root examples/backend_projection \
  --out build/examples/backend_projection/typedb-manifest.json \
  --artifact-out build/examples/backend_projection/typedb.tql
```

Run the non-container checks:

```bash
cd rust && make test-projections
```

Run opt-in Docker readback tests only when you want live backend API coverage:

```bash
AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 \
  cargo test --manifest-path rust/Cargo.toml --test backend_container_tests -- --ignored --nocapture
```

## 9. Run The Main Verification Gates

```bash
make verify-regulated-shipment
make verify-canonical-spine
```

The first gate is the coherent user workflow and CI fixture. The second is the
broader subsystem regression gate across runtime theory checks, prepared
queries, semantic merge/rebase examples, software-authoring examples,
embedding overlays, typed projection/readback contracts, Lean semantic VCS
conformance, and diff hygiene.

Use both before treating a semantic-spine change as coherent.
