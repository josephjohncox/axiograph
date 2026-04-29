# Certificates

**Diataxis:** Reference  
**Audience:** contributors

Axiograph uses an untrusted-runtime / trusted-checker split:

- Rust elaborates canonical `.axi`, runs queries, builds runtime reports, and
  emits certificates for supported fragments.
- Lean re-checks the certificate against the formal semantics and the supplied
  canonical `.axi` anchor.

Certificates are JSON today so they can be inspected, tested, and fed to the
Lean executable. The public certificate route is:

```text
canonical .axi
  -> axi_digest_v1 anchor
  -> typed witness over stable facts / rewrite rules / compiled refs
  -> Lean verifier result
```

## Active Families

The active families are the only public certificate surfaces. Some wire names
carry explicit version suffixes because they are serialized contract names, but
the docs and examples should teach the semantic role rather than a migration
story.

| Family | Role | Claim |
| --- | --- | --- |
| `axi_well_typed_v1` | Canonical module type gate | The anchored `.axi` module satisfies the supported well-typedness decision procedure. |
| `axi_constraints_ok_v1` | Conservative theory-constraint gate | The anchored `.axi` module satisfies the supported key, functional, at-most, symmetry/transitivity-compatibility, and builtin typing constraints. |
| `query_result_v3` | Typed query witness | Returned rows are sound for the supported query fragment under the canonical `.axi` anchor. It does not claim completeness. |
| `reachability_v3` | Low-level canonical path witness | A path witness is valid against stable canonical `axi_fact_id` facts. Public query surfaces should prefer `query_result_v3`. |
| `rewrite_derivation_v3` | Anchored rewrite trace | Rewrite steps are replayable using builtin rules or rewrite rules declared in the anchored `.axi` module. |
| `normalize_path_v2` | Groupoid path normalization | The normalized path expression is replayable/recomputable for the supported groupoid fragment. |
| `path_equiv_v2` | Path equivalence | Two path expressions normalize to the same supported form. |
| `resolution_v2` | Fixed-point resolution decision | Lean recomputes the reconciliation decision from fixed-point inputs. |
| `delta_f_v1` | Functorial pullback | Lean recomputes a finite `Delta_F` pullback for the supported schema/instance payload. |

Unsupported or obsolete certificate payloads should fail closed. They should not
be kept as public examples, Make targets, REPL flows, server options, or MCP
tool outputs.

## Anchors

Anchored certificate families carry:

```json
{
  "anchor": { "axi_digest_v1": "fnv1a64:0123456789abcdef" }
}
```

The verifier must be invoked with the exact canonical `.axi` module that
produces that digest:

```bash
make verify-lean-cert AXI=examples/manufacturing/SupplyChainHoTT.axi CERT=build/supply_chain_query_cert.json
```

If the module is missing, the digest does not match, or the certificate family
requires an anchor but none is supplied, verification fails.

## Module Type Gate

`axi_well_typed_v1` is a small decision procedure for canonical `.axi` modules.
It checks that:

- instances reference declared schemas,
- assignments use the expected object/relation shape,
- relation tuples have exactly the declared fields,
- tuple values and subtype reuse do not create ambiguous typed references.

Emit and verify:

```bash
axiograph cert typecheck examples/economics/EconomicFlows.axi --out build/axi_well_typed.json
make verify-lean-cert AXI=examples/economics/EconomicFlows.axi CERT=build/axi_well_typed.json
```

End-to-end target:

```bash
make verify-lean-e2e-axi-well-typed-v1
```

## Constraint Gate

`axi_constraints_ok_v1` is the conservative Lean-backed theory gate. It checks
only the supported subset:

- `constraint key Rel(field, ...)`
- `constraint functional Rel.field -> Rel.field`
- `constraint at_most N Rel.field -> Rel.field`
- optional `param (...)` fibers for at-most, symmetry, and transitivity checks
- symmetry/transitivity compatibility with keys/functionals over carrier fields
- selected builtin typing rules used by the examples

Opaque theory blocks remain runtime-addressable obligations but are not
certified by this gate. Unknown constraints fail closed.

Emit and verify:

```bash
axiograph cert constraints examples/ontology/OntologyRewrites.axi --out build/axi_constraints_ok.json
make verify-lean-cert AXI=examples/ontology/OntologyRewrites.axi CERT=build/axi_constraints_ok.json
```

End-to-end target:

```bash
make verify-lean-e2e-axi-constraints-ok-v1
```

## Typed Query Witness

`query_result_v3` is the active query certificate family. It proves row
soundness for returned rows under the supplied canonical `.axi` anchor.

Policy is controlled by `QueryCertificatePolicyV1`:

- `none`: run without a certificate.
- `emit`: emit a certificate when the query fragment is certifiable.
- `verify`: emit and verify with Lean.
- `require_verified`: fail closed unless the query is certifiable and Lean
  verification succeeds under the expected anchor.

Emit and verify:

```bash
axiograph cert query examples/manufacturing/SupplyChainHoTT.axi \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out build/supply_chain_query_cert.json

make verify-lean-cert AXI=examples/manufacturing/SupplyChainHoTT.axi CERT=build/supply_chain_query_cert.json
```

End-to-end target:

```bash
make verify-lean-e2e-query-result-module-v3
```

## Rewrite And Path Families

`rewrite_derivation_v3` is the active rewrite trace for accepted `.axi` rules.
Each step references either a builtin rule or an `.axi` rule:

```json
{
  "rule_ref": "axi:fnv1a64:0123456789abcdef:TheoryName:rule_name"
}
```

Lean resolves the rule against the anchored module, replays each step, checks
endpoints, and rejects derivations that do not produce the claimed output.

Useful targets:

```bash
make verify-lean-e2e-rewrite-derivation-v3
make verify-lean-e2e-normalize-path-v2
make verify-lean-e2e-path-equiv-v2
make verify-lean-e2e-path-equiv-congr-v2
```

## Reconciliation And Migration Families

`resolution_v2` and `delta_f_v1` are finite checker families used by
reconciliation/migration examples:

- `resolution_v2` recomputes a fixed-point reconciliation decision.
- `delta_f_v1` recomputes a finite functorial pullback.

Targets:

```bash
make verify-lean-e2e-resolution-v2
make verify-lean-e2e-delta-f-v1
```

## Running The Checker

Recommended current gates:

```bash
make verify-canonical-spine
make verify-lean-certificates
make verify-lean-e2e-suite
make verify-lean-semantic-vcs
```

Custom certificate:

```bash
make verify-lean-cert AXI=path/to/module.axi CERT=path/to/certificate.json
```

The `AXI=` argument is required for anchored families. Unanchored checker
families are intentionally narrow and should not be used to make accepted
ontology/query claims.

## Non-Claims

A Lean-accepted certificate does not prove:

- that source facts are true,
- that a query answer is complete,
- that the full Rust runtime is correct,
- that external graph DB projections are authoritative,
- that embedding/LLM evidence is accepted ontology truth,
- or that Axiograph has global ontology closure.

For runtime closure tiers and completeness claims, use
`docs/reference/RUNTIME_THEORY_CHECKER.md`.

## Debug/Parser Parity Boundary

Reversible PathDB snapshot exports are storage/debug artifacts. They are useful
for live-byte and parser-parity tests, but they are not certificate anchors,
query authority, accepted-plane promotion inputs, or teaching examples for the
semantic spine.
