# Formal Verification

**Diataxis:** How-to  
**Audience:** contributors

Axiograph verifies narrow, high-value claims by reducing runtime work to small
Lean-checkable witnesses:

```text
canonical .axi
  -> axi_digest_v1 anchor
  -> typed certificate / gate payload
  -> Lean verifier
```

Rust remains the operational engine. Lean is the trusted checker for the
supported certificate and gate families.

## Boundaries

- Trust boundary: `docs/reference/TRUSTED_KERNEL.md`.
- Certificate families: `docs/reference/CERTIFICATES.md`.
- Compiled semantic IR: `docs/reference/KERNEL_IR.md`.
- Runtime theory reports: `docs/reference/RUNTIME_THEORY_CHECKER.md`.
- Semantic VCS conformance: `docs/reference/SEMANTIC_VCS.md`.
- Software-authoring coverage/codegen: `docs/reference/SOFTWARE_AUTHORING_TOOLS.md`.
- Embedding/evidence boundaries: `docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.
- Test gate selection: `docs/howto/TESTING.md`.

The active query certificate is `query_result_v3`: a canonical `.axi`-anchored
typed query witness. Storage snapshot exports are debug/parser parity material,
not verifier authority.

## Trusted vs Untrusted

Rust:

- parses and elaborates `.axi`,
- builds compiled IR and runtime reports,
- runs queries, coverage, merge/rebase, projections, and codegen planning,
- emits certificates for supported fragments.

Lean:

- parses the supplied canonical `.axi` anchor,
- checks the certificate/gate payload,
- rejects unsupported, under-anchored, or malformed claims.

A Lean-accepted certificate proves the stated narrow claim under the supplied
anchors and assumptions. It does not prove source truth, query completeness,
global ontology closure, or correctness of the whole Rust runtime.

## Current Lean-Checked Surfaces

The shipped verifier path is centered on:

- `lean/Axiograph/VerifyMain.lean`
- `lean/Axiograph/Certificate/Format.lean`
- `lean/Axiograph/Certificate/Check.lean`
- `lean/Axiograph/Axi/AxiV1.lean`
- `lean/Axiograph/Axi/TypeCheck.lean`
- `lean/Axiograph/Axi/ConstraintsCheck.lean`
- `lean/Axiograph/Prob/Verified.lean`

Main checked families:

- `axi_well_typed_v1`: canonical `.axi` module well-typedness gate.
- `axi_constraints_ok_v1`: conservative theory-constraint gate.
- `query_result_v3`: typed query row-soundness witness.
- `reachability_v3`: low-level canonical path witness over stable `axi_fact_id`.
- `rewrite_derivation_v3`: `.axi`-anchored rewrite trace.
- `normalize_path_v2` and `path_equiv_v2`: supported groupoid path fragments.
- `resolution_v2`: fixed-point reconciliation decision.
- `delta_f_v1`: finite functorial pullback check.

Semantic VCS has a separate finite conformance checker:

- `lean/Axiograph/SemanticVCS.lean`
- `lean/Axiograph/SemanticVCS/Json.lean`
- `lean/Axiograph/SemanticVCS/CheckMain.lean`

It checks reduced merge/rebase payloads for finite operational conformance. It
is not a global ontology-completeness proof.

## Run The Current Gates

Focused canonical spine:

```bash
make verify-canonical-spine
```

Lean build:

```bash
make lean
```

Certificate fixtures:

```bash
make verify-lean-certificates
```

Current Rust-to-Lean certificate suite:

```bash
make verify-lean-e2e-suite
```

Semantic VCS conformance:

```bash
make verify-lean-semantic-vcs
```

Full semantics gate:

```bash
make verify-semantics
```

## Verify A Custom Certificate

Most useful certificate families are anchored to a canonical `.axi` module:

```bash
make verify-lean-cert AXI=examples/manufacturing/SupplyChainHoTT.axi CERT=build/supply_chain_query_cert.json
```

The verifier rejects the certificate if the digest anchor cannot be matched to
the supplied module.

## Generate And Verify Common Certificates

Typecheck gate:

```bash
axiograph cert typecheck examples/economics/EconomicFlows.axi --out build/axi_well_typed.json
make verify-lean-cert AXI=examples/economics/EconomicFlows.axi CERT=build/axi_well_typed.json
```

Constraint gate:

```bash
axiograph cert constraints examples/ontology/OntologyRewrites.axi --out build/axi_constraints_ok.json
make verify-lean-cert AXI=examples/ontology/OntologyRewrites.axi CERT=build/axi_constraints_ok.json
```

Typed query witness:

```bash
axiograph cert query examples/manufacturing/SupplyChainHoTT.axi \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out build/supply_chain_query_cert.json

make verify-lean-cert AXI=examples/manufacturing/SupplyChainHoTT.axi CERT=build/supply_chain_query_cert.json
```

## Parse/Digest Parity

Canonical `.axi` parser parity:

```bash
make verify-lean-axi-v1
make verify-axi-parse-e2e
make verify-axi-digest-e2e
```

Parser/digest parity is useful for keeping Rust and Lean aligned. It is not the
same thing as proving that every runtime report is certified.

## Runtime Theory Reports Are Not Certificates

`RuntimeTheoryCheckReportV1` is an operational report from Rust. It can claim
well-typedness, admissibility, finite closure, completeness, and residual
obligations only under declared closure tiers, anchors, contexts, worlds, and
evidence policies.

Those reports are promotion-sensitive engineering artifacts. They become trusted
proof claims only when reduced to a supported Lean certificate/gate.

## Rust-Side Verification

The optional Verus-oriented crate is:

```bash
cd rust/verus
verus src/lib.rs
```

Use it for selected Rust invariants such as probability bounds and low-level
shape checks. It is complementary to the Lean certificate checker and does not
replace the trusted semantic boundary.

## Hardening Guidance

- Keep the verifier target small and audited.
- Add Lean-readable projections for runtime objects before calling them trusted.
- Fail closed when an anchor, compiled ref, certificate policy, or verifier is
  missing.
- Use fuzz/property/golden tests around `.axi`, AxQL, certificate JSON, `.axpd`,
  semantic merge payloads, and backend projection artifacts.
