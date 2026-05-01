# Canonical Semantic Spine User Guide

**Diataxis:** How-to  
**Audience:** users, contributors, and coding agents

This guide is the public workflow map for the current V1 system. The semantic
spine is:

```text
canonical .axi
  -> KernelModuleIr
  -> SchemaCategoryIr + TheoryIr + InstanceFunctorIr
  -> KernelSurfaceV1 refs
  -> typed runtime reports
  -> optional Lean certificate
```

Use this guide when you want a clean path through validation, runtime theory
checking, typed querying, software-authoring overlays, semantic VCS,
embedding/evidence overlays, and backend projections.

## Rules Of The Road

- Canonical `.axi` is the public semantic input.
- PathDB and `.axpd` are execution/query substrates, not semantic authority.
- Query certification uses `QueryCertificatePolicyV1`: `none`, `emit`,
  `verify`, or `require_verified`.
- DDD/fDDD, BDD, implementation surfaces, codegen, and coverage policies live in
  overlays/tools, not inside domain `.axi`.
- Embeddings produce advisory evidence overlays and refinement candidates; they
  do not mutate accepted ontology state.
- Backend projections are native-readable lower-tier views. Mutation authority
  stays in Axiograph.

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

## 3. Inspect The Compiled Kernel Surface

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover kernel-surface examples/software_authoring/OrderFulfillmentDomain.axi \
  --out build/examples/software_authoring/kernel_surface.json
```

Reports that matter for promotion, strict coverage, merge materialization, or
certification should cite `KernelRefV1` handles. User labels are ergonomics;
compiled refs are the runtime authority.

## 4. Run Typed Queries And Certificates

For server/tooling requests, prefer structured `query_ir_v1`.

```json
{
  "lang": "query_ir_v1",
  "query_ir_v1": {
    "version": 1,
    "select": ["?x"],
    "where": [
      { "kind": "type", "term": "?x", "type": "Order" }
    ],
    "limit": 10
  },
  "certificate_policy": "emit"
}
```

Certificate policies:

- `none`: execute and return typed trust/non-claim reports.
- `emit`: emit a `query_result_v3` typed query witness when certifiable.
- `verify`: emit and ask the configured Lean verifier to check it.
- `require_verified`: fail closed unless the accepted anchor, canonical text,
  certifiable fragment, verifier, and anchor match all succeed.

Legacy request booleans such as `certify`, `verify`,
`require_query_certs`, and `require_verified_queries` are intentionally
rejected by server request parsing.

## 5. Use Software-Authoring Overlays

Keep domain meaning in `.axi`; keep implementation mapping in overlays.

```bash
./examples/software_authoring/run_authoring_flow.sh
```

The flow demonstrates:

- canonical module validation,
- runtime theory checking,
- question-first `.cq` authoring checks,
- weak definition queries,
- strict overlay validation,
- weak coverage probes,
- behavior-case reports,
- enforced software coverage,
- codegen plan previews,
- continuous semantic coverage through the example crate.

Start with:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring competency-questions \
  --axi examples/software_authoring/OrderFulfillmentDomain.axi \
  --cq examples/software_authoring/order_fulfillment.cq \
  --out build/examples/software_authoring/competency_questions_authoring.json
```

Ask weak definition questions for authoring context:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --prompt "define the shipment eligibility business rule" \
  --include-queries \
  --out build/examples/software_authoring/definition_query.json
```

Then validate the overlay and run an exploratory coverage probe:

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
./examples/semantic_merge/run_merge_flow.sh
make verify-lean-semantic-vcs
```

Semantic merge is typed reconciliation over slices, not text merge. Dry-run
plans expose:

- slice manifests and `KernelRefV1` refs,
- join/conflict decisions,
- resolver handles,
- CQ/trust/coverage/runtime-theory blockers,
- rebase transport success/failure,
- Lean-readable reduced conformance payloads.

Materialization is fail-closed when blockers, residual obligations, failed
required transports, stale refs, unresolved resolver steps, or CQ/trust
regressions remain.

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

## 8. Generate Backend Projection Plans

Backend plans are lower-tier native-readable views from compiled IR.

- TypeDB is the primary high-fidelity typed target.
- TerminusDB is the RDF/VCS-shaped secondary target.
- Backend-native writes are drift unless re-imported through Axiograph review.

Run the non-container checks:

```bash
make test-backend-pushdown
```

Run opt-in Docker smoke tests only when you want live backend API coverage:

```bash
AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 \
  cargo test --manifest-path rust/Cargo.toml --test backend_container_tests -- --ignored --nocapture
```

## 9. Run The Main Verification Gate

```bash
make verify-canonical-spine
```

This validates the current V1 spine across runtime theory checks, prepared
queries, semantic merge/rebase examples, software-authoring examples,
embedding overlays, backend pushdown plans, Lean semantic VCS conformance, and
diff hygiene.

Use this gate before treating a cleanup tranche as coherent.
