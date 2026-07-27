# Regulated Shipment: Primary End-to-End Workflow

This is Axiograph's primary usefulness fixture. It follows one pharmaceutical
shipment from canonical meaning through finite typed checks, an exact bounded
query, review artifacts, accepted persistence, typed merge, backend projection,
and a generated test.

The fixture is deliberately small enough to decide exhaustively. It is not a
complete pharmaceutical, customs, cold-chain, or regulatory ontology.

## Scenario

`Shipment_RX_1007` contains `Batch_RX_42`. The batch has a reviewed certificate
of analysis, a release authorization, in-band temperature evidence, a Canadian
customs declaration, and an approved dispatch review. `Shipment_RX_1008` keeps
its blocked state and temperature-excursion evidence; it has no certificate or
release fact.

The candidate evolves the baseline by adding:

- customs declarations and jurisdiction;
- a `DispatchReview` relation object;
- an indexed role whose value must be a `ShipmentContainsBatch` fact in the
  selected context; and
- a finite reviewer refinement (`QA_Lee | QA_Mora`).

The path equation and rewrite relate the composed path
`ShipmentContainsBatch / BatchHasCertificate` to the direct
`ShipmentHasCertificate` path.

## Run It

From the repository root:

```bash
make verify-regulated-shipment
```

For an explicit output directory:

```bash
./examples/regulated_shipment/run_regulated_shipment_workflow.sh \
  build/examples/regulated-shipment-local
```

The output includes:

- baseline and candidate runtime-theory reports;
- unified authoring reports, including a finite compiled-payload evolution diff;
- `axi_well_typed_v1`, `axi_constraints_ok_v1`, and anchored
  `category_kernel_v3` certificates plus actual `Axiograph.VerifyMain` outputs;
- baseline and candidate `finite_query_verification_report_v1` files containing
  the exact `query_result_v4` certificate and accepted answer-bound V2 receipt;
- overlay, behavior-case, and software-coverage reports;
- a generated Rust behavior test that is compiled and executed;
- TypeDB and PathDB projection manifests/artifacts;
- an AxiStore catalog, reviewed exact-two-parent merge, immutable SQLite
  materialization, and restart report; and
- `regulated_shipment_usefulness_report_v2`, the compact evidence index.

The gate also requires both adversarial `.axi` modules to fail, requires the
shared Rust/Lean category formation corpus to agree, requires Lean's finite
presentation/explanation tests to accept the valid shipment model, requires the
exported category certificate to reject presentation, congruence, signed
groupoid-normalization-trace, and saturation tampering, and requires
`query_result_v4` to reject a missing-row answer. The AxiStore scenario rejects
a placeholder query-receipt identity before protected main can advance.

## What Is Actually Proved

The strongest trusted result is the `query_result_v4` receipt emitted by the
`VerifyMain` import closure. For this fixture, it proves that the one returned
certificate (`CoA_RX_42`) is the exact answer to the declared finite bounded
path query over the accepted candidate bytes. It checks both soundness and
missing/extra-row completeness for that denotation. The workflow parses the
strict `CertificateV3` payload, stages the independently approved checker bytes
whose SHA-256 was supplied by the operator, reruns that checker with a fresh
nonce, and checks the returned revision/query/answer/certificate identities
before using the immutable evidence digest as the reviewed-candidate and merge
gate.

The same `VerifyMain` closure checks an independent second claim. Rust emits an
exact-byte-anchored `category_kernel_v3` certificate; Lean reconstructs the
candidate schema as 23 category objects and 43 arrows, checks identities, the
parallel path equation and contextual congruence, replays 86 exact formal
inverse-law normalization traces, then replays all 70 reachable endpoint
explanations and checks exact finite closure.

The Lean finite-theory executable also checks the broader matching finite
presentation:

- relation objects and role projections are well formed and preserve declared
  role order;
- the shipment-certificate equation is between parallel typed paths;
- reviewer membership is decidable in the declared finite refinement;
- finite generator reachability saturates under the configured bound; and
- every accepted reachability entry replays from identities, generators, and
  composition; and
- open typed holes remain residual until a candidate enters the checked
  lifecycle.

Only anchored finite category formation, the forward schema equation and its
contextual congruence, formal inverse-law normalization, and generator
saturation dispatch through `VerifyMain`.
Reviewer refinement, context transport, Rust's relation-span groupoid equation,
the complete `InstanceModelIr`, AxiStore merge, SQLite authentication, restart,
PathDB hydration, evolution diffing, projection, and code generation remain
theorem support or Rust operational evidence as marked. The shared runtime
finite-theory receipt reports exact coverage for the candidate: 23 identities,
70 saturation explanations, finite refinement predicates, dependent role and
context witnesses, and identity scope transports. Its non-identity transport
certification count is zero, and the receipt is reproduced by AxiStore during
merge validation; it is not a Lean proof.

## Non-Claims

This workflow does **not** prove:

- an arbitrary categorical pushout, colimit, or complete merge lattice;
- general dependent type theory or dependent transport;
- univalence, higher inductive types, or unrestricted HoTT path equality;
- open-world, evidence, regulatory, backend-query, or ontology closure
  completeness;
- semantic equivalence of a backend projection; or
- correctness of generated application logic. The generated code is a typed
  test seam carrying scenario receipt anchors, not business logic.

## Files

| File | Role |
| --- | --- |
| `RegulatedShipmentBaseline.axi` | Accepted baseline used by evolution and merge |
| `RegulatedShipment.axi` | Canonical candidate and query anchor |
| `regulated_shipment.cq` | Three finite competency questions |
| `release_certificate_query.json` | Exact bounded query used by the trusted merge gate |
| `authoring_baseline_request.json` | Baseline query/CQ report request |
| `authoring_request.json` | Candidate query/CQ/evolution report request |
| `regulated_shipment_behavior_case.json` | Dispatch behavior case |
| `regulated_shipment_tooling_overlay.json` | Implementation mapping and codegen plan |
| `../../fixtures/adversarial/regulated_shipment/BadReviewer.axi` | Out-of-refinement reviewer rejection |
| `../../fixtures/adversarial/regulated_shipment/BadPathEquation.axi` | Non-parallel path equation rejection |
