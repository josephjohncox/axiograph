# Formal Verification

**Diataxis:** How-to  
**Audience:** contributors

Axiograph verifies narrow, high-value claims by reducing runtime work to small
Lean-checkable witnesses:

```text
exact accepted canonical .axi bytes
  -> revision_digest_v2 cryptographic anchor
  -> envelope V3 / query_result_v4
  -> axiograph-verifier-stdio-v2
  -> Lean receipt with decision accepted
```

Non-query V2 certificate families remain separate typed contracts. The query
boundary has no FNV anchor, V3 query reader, or compatibility upgrade path.

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

The active query certificate is envelope V3 / `query_result_v4`: a typed query
witness bound to exact canonical `.axi` bytes, the prepared query, and the
ordered returned answer. Derived PathDB rows and `.axpd` images are not
verifier inputs or semantic authority.

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
anchors and assumptions. V4 proves exact completeness only for its declared
bounded finite denotation. It does not prove source truth, approximate-search or
evidence completeness, global ontology closure, unrestricted dependent/HoTT
semantics, or correctness of the whole Rust runtime.

## Current Lean-Checked Surfaces

The shipped verifier path is centered on:

- `lean/Axiograph/VerifyMain.lean`
- `lean/Axiograph/Certificate/Format.lean`
- `lean/Axiograph/Certificate/Check.lean`
- `lean/Axiograph/Certificate/Invariants.lean`
- `lean/Axiograph/Certificate/PathRewriteSoundness.lean`
- `lean/Axiograph/Theory/Finite.lean`
- `lean/Axiograph/HoTT/FreeGroupoid.lean`
- `lean/Axiograph/Axi/AxiV1.lean`
- `lean/Axiograph/Axi/TypeCheck.lean`
- `lean/Axiograph/Axi/ConstraintsCheck.lean`
- `lean/Axiograph/Prob/Verified.lean`

Main checked families:

- `axi_well_typed_v1`: canonical `.axi` module well-typedness gate.
- `axi_constraints_ok_v1`: conservative theory-constraint gate.
- `query_result_v4`: query-and-answer-bound witness plus exact completeness for
  the declared finite decidable type/derived-attribute/bounded-RPQ fragment.
- `reachability_v3`: low-level canonical path witness over stable `axi_fact_id`.
- `category_kernel_v3`: exact-byte-anchored category reconstruction with typed
  objects, relation objects, ordered projections, identities, composable
  arrows, parallel equations, contextual congruence, exact signed cancellation
  traces for both formal inverse laws of every generator, and complete bounded
  generator-reachability saturation.
- `rewrite_derivation_v3`: `.axi`-anchored rewrite trace.
- `normalize_path_v2` and `path_equiv_v2`: endpoint-indexed groupoid path
  fragments with mandatory traces; imported Lean theorems show that accepted
  replay preserves denotation after endpoint retyping. Fixed-point confidence
  is a separate non-associative fold and is not transported through path
  equality.
- `resolution_v2`: fixed-point reconciliation decision.
- `delta_f_v1`: finite array-level transport recompute parity, not a general
  functorial pullback theorem.

`Axiograph.Theory.Finite` checks finite relation-object category presentations,
ordered role projections, indexed paths/free-groupoid laws, dependent
role/refinement/context/transport witnesses, checked typed-hole lifecycles, and
explanation-certified finite generator reachability. `VerifyMain` imports it
for `category_kernel_v3`. The dispatched certificate checks exact presentation
reconstruction, endpoint-aware equation congruence, formal inverse-law
normalization traces, and bounded generator saturation; broader interpretation,
refinement, and transport definitions remain theorem support. The V3 wire
checker is decision procedure plus replay: no theorem currently retypes its
index words as `GroupoidPath` or derives denotation preservation from
acceptance.

Semantic VCS has separate external checkers:

- `lean/Axiograph/SemanticVCS.lean`
- `lean/Axiograph/SemanticVCS/Json.lean`
- `lean/Axiograph/SemanticVCS/Lineage.lean`
- `lean/Axiograph/SemanticVCS/CheckMain.lean`

The first pair checks reduced merge/rebase payloads for finite operational
conformance. `Lineage.lean` independently recomputes V2 accepted-tree,
snapshot-lineage, and complete semantic-commit SHA-256 identities and checks a
bounded ancestry path. Neither path is a global ontology-completeness proof or
part of the `VerifyMain` trusted closure.

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

Finite category/dependent/groupoid theory, including Rust kernel gates, the
regulated-shipment Rust-to-Lean certificate, adversarial explanation replay,
and Rust non-closure regressions:

```bash
make verify-lean-theory
```

Run only the anchored category-kernel certificate slice:

```bash
make verify-lean-e2e-category-kernel-v3
```

That target checks the shared positive/rejection formation corpus, writes the
regulated-shipment certificate under `build/category-kernel/`, and succeeds
only if Lean accepts the exact presentation and rejects presentation,
congruence, groupoid-normalization-trace, and saturation tampering.

Run the generic endpoint-indexed path normalization/equivalence parity and
rejection slice:

```bash
make verify-lean-indexed-path-theory
```

Semantic VCS finite merge/rebase conformance:

```bash
make verify-lean-semantic-vcs
```

Transactional accepted-state and lineage integrity:

```bash
make verify-axi-store
```

AxiStore lineage proofs are runtime-authenticated objects, not Lean
certificates. They require an exact current state digest for accepted main or an
independently pinned subject commit; repository identity alone is rejected.
`Axiograph.SemanticVCS.CheckMain` now checks only finite merge/rebase
conformance and remains outside the trusted `VerifyMain` import closure.

Full semantics gate:

```bash
make verify-semantics
```

Exact release decision, including the full locked Rust workspace, CLI feature
matrix, adversarial certificate and AxiStore fixtures, and the semantics gate:

```bash
PATH="$(dirname "$(rustup which --toolchain 1.88.0 rustc)"):$PATH" \
  make release-gate
```

The release target consumes the checked-in Lean toolchain and Lake manifest; it
does not run `lake update`. Dependency refresh is an explicit, reviewable
`make lean-update` operation.

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

Typed query witness. Prefer `.cq`/question-first authoring for humans. The
direct emission-only query command was removed; exercise the bound V4 checker
through the semantic MCP route, `axiograph check finite-query`, or the focused
Rust/Lean gate:

```bash
axiograph discover competency-questions examples/manufacturing/SupplyChainHoTT.axi \
  --from-cq examples/competency_questions/supply_chain.cq \
  --no-schema \
  --out build/supply_chain_competency_questions.json

make verify-lean-e2e-query-result-module-v4
make verify-lean-certificate-rejections
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

`RuntimeTheoryCheckReportV1` is an operational admissibility report from Rust.
It classifies typed obligations, evidence scope, transport status, blockers, and
residuals. It does not saturate equations/rewrites or claim a theory fixpoint,
completeness, or ontology closure. Legacy closure/claim fields were removed;
`admissibility_scan` records unresolved judgments; the structured
`closure_engine_not_implemented` entry is a non-claim, not a residual.

`Axiograph.Theory.Finite` does perform finite saturation, but only for generator
reachability and only under its explicit object and generator bounds. Its
explanation certificate is replayable proof data. The `category_kernel_v3`
projection of that data is dispatched by `VerifyMain`; the broader finite
instance, refinement, context, and transport definitions remain theorem support.

Runtime reports remain promotion-sensitive engineering artifacts. Their module
summary uses typed scope, coverage counts, transport classifications, residual
ids, and structured non-claims; it does not expose synthetic completeness or
ontology-closure claim fields. A product proof claim requires an anchored
payload dispatched through the supported Lean verifier boundary.

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
