# Certificates

**Diataxis:** Reference  
**Audience:** contributors

Axiograph uses an untrusted-runtime / trusted-checker split:

- Rust elaborates canonical `.axi`, runs queries, builds runtime reports, and
  emits certificates for supported fragments.
- Lean re-checks the certificate against the formal semantics and the supplied
  canonical `.axi` anchor.

Certificates are JSON today so they can be inspected, tested, and fed to the
Lean executable. The verified service route is:

```text
exact accepted canonical .axi bytes
  -> envelope-specific anchor (V3 query envelopes use RevisionDigestV2)
  -> typed witness over stable facts / rewrite rules / compiled refs
  -> caller expectations + Lean verifier receipt
```

There is no direct emission-only query-certificate command. Query
certification is exposed only where the caller can bind exact accepted `.axi`
bytes, the compiled query digest, the exact answer digest, and an approved Lean
checker receipt. Semantic MCP `verify` / `require_verified` is the service
route. `axiograph check finite-query` is the file-oriented bound route used by
the regulated-shipment acceptance fixture; it always invokes the approved
checker and returns one report that distinguishes certifiable shape,
certificate emission, and an accepted receipt bound to the exact answer.

## Active Families

The active families are the only public certificate surfaces. Some wire names
carry explicit version suffixes because they are serialized contract names, but
the docs and examples should teach the semantic role rather than a migration
story.

| Family | Role | Claim |
| --- | --- | --- |
| `axi_well_typed_v1` | Canonical module type gate | The anchored `.axi` module satisfies the supported well-typedness decision procedure. |
| `axi_constraints_ok_v1` | Conservative theory-constraint gate | The anchored `.axi` module satisfies the supported key, functional, at-most, symmetry/transitivity closure, and builtin typing constraints. |
| `query_result_v4` in envelope V3 | Query-and-answer-bound exact finite witness | Lean checks every row witness and proves equality with the declared bounded finite denotation under the exact accepted `.axi` bytes. The claim is `finite_exact_complete`; ontology closure and all out-of-fragment completeness claims remain excluded. |
| `category_kernel_v3` in envelope V3 | Anchored finite category/groupoid presentation | Lean reconstructs the indexed presentation from exact `.axi`, checks endpoint-aware contextual equation congruence, syntactically replays both formal inverse-law cancellation traces for every generator, and checks bounded generator-reachability closure. This is decision procedure plus replay, not a wire-path denotation theorem. |
| `reachability_v3` | Low-level canonical path witness | A path witness is valid against stable canonical `axi_fact_id` facts. Public verified query surfaces use `query_result_v4`. |
| `rewrite_derivation_v3` | Anchored rewrite trace | Rewrite steps are replayable using builtin rules or rewrite rules declared in the anchored `.axi` module. |
| `normalize_path_v2` | Groupoid path normalization | A mandatory endpoint-preserving rewrite trace reaches the checker-computed normal form; for the endpoint-retyped input, accepted replay preserves free-groupoid denotation. |
| `path_equiv_v2` | Path equivalence | Mandatory traces take two endpoint-retyped expressions to one normal form; accepted replay implies equal free-groupoid denotation. Confidence is not part of this equality. |
| `resolution_v2` | Fixed-point resolution decision | Lean recomputes the reconciliation decision from fixed-point inputs. |
| `delta_f_v1` | Functorial pullback | Lean recomputes a finite `Delta_F` pullback for the supported schema/instance payload. |

Unsupported or obsolete certificate payloads should fail closed. They should not
be kept as public examples, Make targets, REPL flows, server options, or MCP
tool outputs.

## Anchors And Envelope Dispatch

Envelope versions are semantic protocol versions, not aliases:

- envelope V2 hosts the non-query certificate families that still have V2 wire contracts;
- envelope V3 requires `CertificateAnchorV2 { revision_digest_v2 }` and hosts
  `query_result_v4` or `category_kernel_v3`;
- a V2 payload, a missing anchor, an unknown field, or a changed version cannot
  satisfy a V3 check.

A V3 query anchor has this shape:

```json
{
  "version": 3,
  "kind": "query_result_v4",
  "anchor": {
    "revision_digest_v2": "axi:revision:v2:sha256:<64-lowercase-hex>"
  }
}
```

Rust and Lean independently hash the exact accepted UTF-8 bytes with the shared
`AXIOGRAPH-ID` V2 framing. The accepted bytes remain the reviewable meaning
plane. The digest identifies those bytes; PathDB, snapshots, indexes, and
storage paths do not become semantic authority.

Stable fact references use the same domain-separated SHA-256 identity family as
module/query/answer/certificate identities. The Lean index rejects any
same-id/different-tuple ambiguity before it verifies a V4 row. No FNV query
anchor, legacy query reader, or prefix-upgrade shim exists.

## Module Type Gate

`axi_well_typed_v1` is a small decision procedure for canonical `.axi` modules.
It checks that:

- instances reference declared schemas,
- assignments use the expected object/relation shape,
- relation tuples have exactly the declared fields,
- tuple values and subtype reuse do not create ambiguous typed references.

Emit and verify a module type certificate directly from canonical `.axi`:

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
- supported symmetry/transitivity closure checks over carrier fields
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

`query_result_v4` binds three objects into one checked result:

1. the exact accepted `.axi` module bytes through `revision_digest_v2`;
2. the prepared lowered query through `prepared_query_digest_v1`; and
3. the ordered selected projections plus `runtime_truncated` through
   `answer_digest_v1`.

`PreparedQueryBindingV1` contains only:

- `version = 1`;
- ordered selected variables;
- ordered disjuncts, atoms, terms, and recursive regular-expression structure;
- optional `max_hops`;
- fixed-point `min_confidence_fp` numerator;
- `row_limit`; and
- `claim_kind = finite_exact_complete`.

The digest excludes source text, diagnostic elaborated text, plans, inferred
types, runtime kernel refs, DB tokens, rows, and truncation. Rust constructs and
stores the binding from the prepared lowered AST. Certificate emission does not
reparse diagnostic text or independently re-elaborate the source query.

### Canonical digest framing

Both prepared and answer digests use the V2 identity preimage from
`Axiograph.Util.Sha256`: magic `AXIOGRAPH-ID`, version 2, a lowercase ASCII
domain, an ordered field count, and each field as an unsigned 64-bit
big-endian byte length followed by exact bytes.

The prepared digest uses kind/domain `query`/`query`. Its ordered fields are:

1. `prepared_query_binding_v1`, `1`, and `finite_exact_complete`;
2. `select_count`, the decimal count, then `select`, variable for each selected
   variable in order;
3. `disjunct_count`, then for each branch `disjunct`, decimal branch index,
   decimal atom count, and recursive atom fields in source-lowered order;
4. atom tags `atom_type`, `atom_attr_eq`, or `atom_path`; term tags
   `term_var`/`term_const`; and regex tags `regex_epsilon`, `regex_rel`,
   `regex_seq`, `regex_alt`, `regex_star`, `regex_plus`, or `regex_opt`, with
   decimal child counts for sequence and alternation;
5. `max_hops_none` or `max_hops_some`, decimal value;
6. `min_confidence_none` or `min_confidence_some`, decimal fixed-point
   numerator; and
7. `row_limit`, decimal value.

The answer digest uses kind/domain `answer`/`answer`. Its ordered fields are
`query_answer_v1`, the prepared digest, selected-variable names, row count,
each row index and `(variable, stable entity id)` projection in selected-variable
order, then `runtime_truncated_true` or `runtime_truncated_false`. Row order and
duplicates are significant. Reordering, dropping, duplicating, or changing a
row changes the digest.

Lean recomputes both digests, validates every row, enforces
`rows.size <= row_limit`, rejects `runtime_truncated = true`, evaluates the
bounded finite query denotation, and proves exact row equality. The executable
fragment is a finite UCQ over type, canonical derived-attribute equality, and
regular paths; repeated paths require explicit `max_hops`. Syntax, regex, hop,
and candidate-assignment bounds make the decision procedure total.
`QueryAnswer<CertificateEmitted>` retains the exact certificate serialization
and `CertificateIdV2`; only a matching accepted receipt transitions it to
`LeanVerified` and permits the finite completeness claim.

For service, REPL, and MCP query surfaces, policy remains controlled by
`QueryCertificatePolicyV1`:

- `none`: run without a returned certificate;
- `emit`: emit envelope V3 / `query_result_v4`;
- `verify`: run the stdio V2 checker and return its full receipt;
- `require_verified`: fail closed unless the query is certifiable, the accepted
  bytes and prepared/answer expectations match, and Lean accepts.

The direct query-certificate CLI was removed rather than preserve an
emission-only path that could be confused with a Lean-verified result.

### Verifier protocol and receipt

Query-bound verification uses `axiograph_verify --stdio-v2`, protocol
`axiograph-verifier-stdio-v2`, and build id `axiograph-verify-main-v3`. The
request requires non-null, well-formed `expected_prepared_query_digest` and
`expected_answer_digest` strings. Missing, null, malformed, non-string, or
unknown fields reject.

`VerifierReceiptV2` returns the nonce, approved checker SHA-256 and build id,
`anchor_digest_v2`, exact-certificate `certificate_digest_v2`, prepared digest,
answer digest, certificate kind, claim kind, decision, and message. Rust accepts
the receipt only when all fields match its caller expectations and the process
exit status agrees with `accepted` or `rejected`. An old stdio V1 receipt or old
build id cannot satisfy this protocol.

## Rewrite And Path Families

`rewrite_derivation_v3` is the active rewrite trace for accepted `.axi` rules.
Each step references either a builtin rule or an `.axi` rule:

```json
{
  "rule_ref": "axi-rule-v2|axi:revision:v2:sha256:<64-hex>|TheoryName|rule_name"
}
```

Lean resolves the rule against the anchored module, replays each step, checks
endpoints, and rejects derivations that do not produce the claimed output.

`normalize_path_v2` and `path_equiv_v2` use a closed builtin groupoid rewrite
vocabulary. Their traces are mandatory. The parser rejects missing traces,
unknown fields, non-composable endpoints, invalid positions, and injected
confidence fields. `Certificate.Invariants`, imported by `VerifyMain`, proves
that accepted builtin traces preserve denotation after endpoint retyping in
mathlib's free groupoid.
Fixed-point confidence remains a separate syntax-directed fold because rounding
at each multiplication is not associative.

`category_kernel_v3` additionally carries exact-byte-anchored signed generator
words. Rust emits one deterministic cancellation trace for `g ; g⁻¹` and one
for `g⁻¹ ; g` for every presented generator. Lean reconstructs the same indexed
presentation and requires exact coverage before accepting the certificate. It
also requires one one-step forward congruence witness per equation and checks
the equation's source and target at the replacement offset, so `id(A)` cannot
be applied at `id(B)`.

The V3 category anchor covers one exact defining module. Export rejects a schema
whose forward equations were added by an importing module; the current envelope
does not pretend to bind an ordered import closure. The cancellation replay is
not connected to `GroupoidPath.denote` by an acceptance theorem, so this family
is replay-only. A different valid reachability explanation for the same typed
endpoint pair may be accepted; invalid or incomplete explanations reject.

Useful targets:

```bash
make verify-lean-e2e-rewrite-derivation-v3
make verify-lean-e2e-normalize-path-v2
make verify-lean-e2e-path-equiv-v2
make verify-lean-e2e-path-equiv-congr-v2
make verify-lean-indexed-path-theory
make verify-lean-e2e-category-kernel-v3
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
- query completeness outside the explicit bounded finite denotation of an
  accepted `query_result_v4`,
- that the full Rust runtime is correct,
- that external graph DB projections are authoritative,
- that embedding/LLM evidence is accepted ontology truth,
- or that Axiograph has global ontology closure.

For runtime closure tiers and completeness claims, use
`docs/reference/RUNTIME_THEORY_CHECKER.md`.

## Derived Storage Boundary

Derived PathDB rows and SQLite `.axpd` materializations are never certificate
anchors or semantic interchange. Certificate checks bind to exact canonical
`.axi` bytes; obsolete snapshot-export fixtures and fallback readers are not
retained.
