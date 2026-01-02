# Certificates (Rust → Lean)

**Diataxis:** Reference  
**Audience:** contributors

This repo follows **untrusted engine / trusted checker**:

- **Rust** computes results and emits a **certificate** (witness).
- **Lean** checks the certificate against the formal semantics.

Certificates are currently JSON for ease of inspection. The intent is to keep the
shape **stable and versioned**, and later add CBOR once the schema settles.

## Versions

### v1: reachability (float confidences)

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`Certificate`, version 1)
- `rust/crates/axiograph-pathdb/src/verified.rs` (`ReachabilityProof`)

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`ReachabilityProof`, `Certificate.reachabilityV1`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyReachabilityProof`)

Shape:

```json
{
  "version": 1,
  "kind": "reachability",
  "proof": {
    "type": "step",
    "from": 1,
    "rel_type": 10,
    "to": 2,
    "rel_confidence": 0.9,
    "rest": { "type": "reflexive", "entity": 2 }
  }
}
```

Lean treats v1 as transitional only: floats are *not* the long-term trusted representation.

### v2: reachability (fixed-point confidences)

v2 replaces float confidences with a **fixed-point numerator** (no floats in the checker).

Shared invariant:
- `0 ≤ rel_confidence_fp ≤ 1_000_000`
- The denominator is shared with Lean: `Axiograph.Prob.Precision = 1_000_000`

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`CertificateV2`, `FixedPointProbability`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_reachability_cert_v2.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`ReachabilityProofV2`, `Certificate.reachabilityV2`)
- `lean/Axiograph/Prob/Verified.lean` (`VProb`, `vMult`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyReachabilityProofV2`)

Shape:

```json
{
  "version": 2,
  "kind": "reachability_v2",
  "proof": {
    "type": "step",
    "from": 1,
    "rel_type": 10,
    "to": 2,
    "rel_confidence_fp": 900000,
    "rest": { "type": "reflexive", "entity": 2 }
  }
}
```

Semantics:
- Multiplication is fixed-point and rounds down:
  - `(a/P) * (b/P)` is encoded as `((a*b)/P)/P`, i.e. numerator `((a*b)/P)`.

#### Optional `.axi` anchors (snapshot-scoped)

Any v2 certificate may additionally carry an optional anchor:

```json
{
  "version": 2,
  "anchor": { "axi_digest_v1": "fnv1a64:..." },
  "kind": "...",
  "proof": { "...": "..." }
}
```

For **anchored reachability**, each step can also carry a `relation_id`:

- `relation_id = N` refers to `Relation_N` in a `PathDBExportV1` snapshot.
- Lean checks that every referenced `relation_id` exists in the snapshot `relation_info`
  table and matches `(from, rel_type, to, rel_confidence_fp)` (the snapshot stores float bits;
  Lean deterministically converts them to the fixed-point numerator to avoid trusting floats).

Note: this “anchored to `PathDBExportV1`” scheme is an end-to-end scaffold.
The long-term goal is to anchor certificates to canonical `.axi` inputs via stable
fact IDs (module digest + extracted fact ids), so verification does not require the
engine interchange snapshot format.

Samples:
- Anchor snapshot: `examples/anchors/pathdb_export_anchor_v1.axi`
- Anchored reachability cert: `examples/certificates/reachability_v2_anchored.json`

End-to-end:
- `make verify-lean-e2e-v2-anchored`

### v2: axi_well_typed_v1 (canonical `.axi` module typecheck gate)

This certificate kind is a small “trusted gate” for canonical `.axi` inputs:

- Rust parses the input module and emits a `axi_well_typed_v1` certificate **anchored to the module digest**.
- Lean re-parses the anchored module and re-checks it with a small decision procedure.
  In the trusted codebase, this corresponds to producing a `TypedModule` witness (a module packaged with a proof of well-typedness).

What is checked today (intentionally small):
- instances reference declared schemas
- assignments are either ident-sets (objects) or tuple-sets (relations), not mixed
- tuples have exactly the declared fields (no missing/extra/duplicate fields)
- tuple values may introduce objects implicitly, but subtyping-based reuse must not become ambiguous

Shape (sketch):

```json
{
  "version": 2,
  "anchor": { "axi_digest_v1": "fnv1a64:..." },
  "kind": "axi_well_typed_v1",
  "proof": {
    "module_name": "EconomicFlows",
    "schema_count": 1,
    "theory_count": 1,
    "instance_count": 1,
    "assignment_count": 42,
    "tuple_count": 100
  }
}
```

End-to-end:
- `make verify-lean-e2e-axi-well-typed-v1`

CLI usage:

```bash
axiograph cert typecheck examples/economics/EconomicFlows.axi --out build/axi_well_typed.json
make verify-lean-cert AXI=examples/economics/EconomicFlows.axi CERT=build/axi_well_typed.json
```

### v2: axi_constraints_ok_v1 (core theory constraints gate)

This certificate kind is a conservative ontology-engineering gate:

- Rust claims the anchored canonical `.axi` module satisfies a **small, high-ROI**
  subset of theory constraints.
- Lean re-parses the anchored module and re-checks the same subset (fail-closed).

For the design rationale (open-world semantics + what we can/can’t certify as a
single “module OK” gate), see `docs/explanation/CONSTRAINT_SEMANTICS.md`.

Certified subset:
- `constraint key Rel(field, ...)`
- `constraint functional Rel.field -> Rel.field`
  - Only **unary** FDs are canonical. Multi-field determinism should be written as a
    composite key: `constraint key Rel(a, b, ...)`.
- symmetry annotations:
  - `constraint symmetric Rel`
  - `constraint symmetric Rel where Rel.field in {A, B, ...}`
  - optional carrier-field clause: `... on (field0, field1)`
  - optional parameter-field clause: `... param (field0, field1, ...)`
  Semantics: the checker does **not** require inverse tuples to be explicitly present.
  Instead, it checks that the module’s **key/functional** constraints remain consistent
  under symmetric closure (adding swapped-endpoint tuples) on the carrier fields.
  By default the carrier fields are the first two relation fields; `on (field0, field1)`
  makes the choice explicit.
  If `param (..)` is present, symmetric closure is interpreted as operating on the
  carrier pair **within each fixed assignment** of the parameter fields (e.g. `ctx`, `time`),
  and other relation fields are treated as out-of-scope annotations/witnesses for the
  purposes of this certificate.
- transitivity annotations:
  - `constraint transitive Rel`
  - optional carrier-field clause: `constraint transitive Rel on (field0, field1)`
  - optional parameter-field clause: `constraint transitive Rel ... param (field0, field1, ...)`
  Semantics: the checker does **not** require the transitive closure to be explicitly
  materialized. Instead, it checks that the module’s **key/functional** constraints
  remain consistent under transitive closure on the relation’s carrier fields.
  By default the carrier fields are the first two relation fields; `on (field0, field1)`
  makes the choice explicit.
  If `param (..)` is present, transitive closure is interpreted as operating on the
  carrier pair within each fixed assignment of the parameter fields (e.g. `ctx`, `time`),
  without inventing new parameter values.
  If a key/functional constraint refers to non-carrier/non-param fields, the certificate
  check fails (witness construction is out of scope for this certificate).
- executable typing rules (small builtin set):
  - `constraint typing Rel: preserves_manifold_and_increments_degree`
  - `constraint typing Rel: preserves_manifold_and_adds_degree`
  - `constraint typing Rel: depends_on_metric_and_dualizes_degree`
  Semantics: the checker validates consistency against supporting “typing relations”
  (`FormOn`, `FormDegree`, `MetricOn`, `ManifoldDimension`) and treats output facts as
  derivable when omitted.

Notes:
- Opaque named blocks (`constraint Name:` followed by an indented body) are still
  **preserved**, but are not part of `axi_constraints_ok_v1` yet.
- Truly unknown constraints (`ConstraintV1.unknown`) are rejected by both:
  - accepted-plane promotion (hard error), and
  - `axi_constraints_ok_v1` (fail-closed).
- If you have dialect-ish constraint formatting (e.g. multi-line `... where` guards),
  run `axiograph check fmt --write your_file.axi` to canonicalize the constraint lines.

Shape (sketch):

```json
{
  "version": 2,
  "anchor": { "axi_digest_v1": "fnv1a64:..." },
  "kind": "axi_constraints_ok_v1",
  "proof": {
    "module_name": "ConstraintsOkDemo",
    "constraint_count": 10,
    "instance_count": 1,
    "check_count": 10
  }
}
```

End-to-end:
- `make verify-lean-e2e-axi-constraints-ok-v1`

CLI usage:

```bash
axiograph cert constraints examples/ontology/OntologyRewrites.axi --out build/axi_constraints_ok.json
make verify-lean-cert AXI=examples/ontology/OntologyRewrites.axi CERT=build/axi_constraints_ok.json
```

### v2: query_result_v1 (certified conjunctive queries: AxQL / SQL-ish)

This certificate kind supports “certified querying” for the *conjunctive query*
kernel used by AxQL (REPL) and the SQL-ish surface.

It proves **soundness of returned rows**:
- each returned row satisfies the query under the anchored snapshot
- it does **not** claim completeness (“these are all rows”)

Anchoring:
- the certificate carries an `.axi` digest anchor (`axi_digest_v1`)
- path witnesses use `relation_id` fact ids that refer to `Relation_<id>` in a
  `PathDBExportV1` snapshot

What Lean checks:
- type constraints against `entity_type`
- attribute constraints against `entity_attribute`
- path witnesses via anchored `ReachabilityProofV2` (each step must reference a real snapshot edge)
- RPQ semantics using mathlib `RegularExpression` (so path-meaning is not hand-rolled)
- optional context/world scoping (when present in the query) as an ordinary `path`
  atom over `axi_fact_in_context` (single-context scoping is certifiable today; multi-context unions are execution-only for now)

Shape (sketch):

```json
{
  "version": 2,
  "anchor": { "axi_digest_v1": "fnv1a64:..." },
  "kind": "query_result_v1",
  "proof": {
    "query": { "select_vars": ["?y"], "atoms": [ /* ... */ ], "max_hops": 5 },
    "rows": [
      { "bindings": [{ "var": "?y", "entity": 2 }], "witnesses": [ /* ... */ ] }
    ],
    "truncated": false
  }
}
```

End-to-end:
- `make verify-lean-e2e-query-result-v1`

### v2: query_result_v3 (axi-anchored query results)

`query_result_v3` removes the dependency on `PathDBExportV1` snapshot tables by
anchoring directly to a canonical `.axi` module:

- the certificate anchor is `axi_digest_v1` of the canonical module
- witnesses reference edges by their stable `axi_fact_id` (derived from the
  canonical tuple fact), rather than `relation_id` in a snapshot export

This keeps `.axi` as the canonical truth and avoids “DB export semantics drift”
in the checker.

End-to-end:
- `make verify-lean-e2e-query-result-module-v3`

CLI usage (canonical module → certificate; optional `--anchor-out` only for debugging):

```bash
axiograph cert query examples/manufacturing/SupplyChainHoTT.axi \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out build/supply_chain_query_cert_v3.json

make verify-lean-cert AXI=examples/manufacturing/SupplyChainHoTT.axi CERT=build/supply_chain_query_cert_v3.json
```

### v2: query_result_v2 (certified disjunction / UCQs)

This certificate kind extends `query_result_v1` with **top-level disjunction**
(`or`): a query is a union of conjunctive branches (UCQ).

It proves **soundness of returned rows**, but does not claim completeness.

What changes compared to `query_result_v1`:
- the query payload stores `disjuncts: [ [atoms...], [atoms...], ... ]`
- each row carries `disjunct: <index>` to indicate which branch it satisfies
- Lean checks each row against that branch using the same atom/witness rules

End-to-end:
- `make verify-lean-e2e-query-result-v2`

CLI usage (disjunction over a snapshot anchor):

```bash
axiograph cert query examples/anchors/pathdb_export_anchor_v1.axi \
  --lang axql \
  'select ?y where 0 -r1-> ?y or 0 -r1/r2-> ?y' \
  --out build/query_result_or_v2.json
make verify-lean-cert AXI=examples/anchors/pathdb_export_anchor_v1.axi CERT=build/query_result_or_v2.json
```

### v2: resolution (fixed-point)

This certificate claims a conflict-resolution decision and lets Lean re-compute
the decision using `Axiograph.Prob.decideResolution`.

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`ResolutionProofV2`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_resolution_cert_v2.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`ResolutionProofV2`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyResolutionProofV2`)

Shape:

```json
{
  "version": 2,
  "kind": "resolution_v2",
  "proof": {
    "first_confidence_fp": 800000,
    "second_confidence_fp": 600000,
    "threshold_fp": 200000,
    "decision": { "tag": "choose_first" }
  }
}
```

Decision tags:
- `choose_first`
- `choose_second`
- `need_review`
- `merge` (includes `w1_fp`, `w2_fp`)

Sample: `examples/certificates/resolution_v2.json`

### v2: normalize_path (groupoid normalization + derivations)

This certificate claims a normalized form of a path expression and (optionally)
provides an explicit rewrite derivation.

Expression constructors:
- `reflexive` (identity)
- `step` (generator edge)
- `trans` (composition)
- `inv` (formal inverse)

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`PathExprV2`, `NormalizePathProofV2`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_normalize_path_cert_v2.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`PathExprV2`, `NormalizePathProofV2`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyNormalizePathProofV2`)

Shape:

```json
{
  "version": 2,
  "kind": "normalize_path_v2",
  "proof": {
    "input": { "type": "trans", "left": { "type": "reflexive", "entity": 1 }, "right": { "...": "..." } },
    "normalized": { "type": "step", "from": 1, "rel_type": 10, "to": 2 },
    "derivation": [
      { "pos": [0, 1], "rule": "assoc_right" }
    ]
  }
}
```

Rewrite steps:
- `rule` is one of:
  - `assoc_right`, `id_left`, `id_right`
  - `inv_refl`, `inv_inv`, `inv_trans`
  - `cancel_head` (supports both `trans atom (trans invAtom rest)` and `trans atom invAtom`)
- `pos` is a path from the root:
  - `0` = `.trans.left`
  - `1` = `.trans.right`
  - `2` = `.inv.path`

Lean verifies `normalize_path_v2` by:
- Checking endpoints match.
- If `derivation` is present: replaying every step (congruence-aware via `pos`) and ensuring the
  final expression equals the claimed `normalized`.
- Always re-computing normalization and checking equality with the claimed `normalized`.

Sample: `examples/certificates/normalize_path_v2.json`

### v2: rewrite_derivation (replayable rewrite traces)

This certificate claims that `output` is reachable from `input` by replaying a
list of `(rule, position)` rewrite steps.

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`RewriteDerivationProofV2`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_rewrite_derivation_cert_v2.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`RewriteDerivationProofV2`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyRewriteDerivationProofV2`)

Shape:

```json
{
  "version": 2,
  "kind": "rewrite_derivation_v2",
  "proof": {
    "input": { "type": "trans", "...": "..." },
    "output": { "type": "step", "...": "..." },
    "derivation": [{ "pos": [], "rule": "id_left" }]
  }
}
```

Sample: `examples/certificates/rewrite_derivation_v2.json`

### v2: rewrite_derivation_v3 (first-class rule references: builtin + `.axi`)

`rewrite_derivation_v3` generalizes `rewrite_derivation_v2` so rewrite steps can
reference:

- builtin groupoid rules (`builtin:<tag>`), or
- rewrite rules declared in canonical `.axi` theories (`axi:<axi_digest_v1>:<theory>:<rule>`).

This is the certificate format we use for “**semantics = accepted rewrite rules**”:
Rust can apply rules during execution/optimization and emit an auditable derivation;
Lean replays the derivation by resolving each referenced rule against the anchored `.axi`.

Rust:
- `rust/crates/axiograph-pathdb/src/certificate.rs` (`RewriteDerivationProofV3`, `PathRewriteStepV3`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_rewrite_derivation_cert_v3.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`RewriteDerivationProofV3`)
- `lean/Axiograph/Certificate/Check.lean` (`verifyRewriteDerivationProofV3Anchored`)

Shape (sketch):

```json
{
  "version": 2,
  "anchor": { "axi_digest_v1": "fnv1a64:..." },
  "kind": "rewrite_derivation_v3",
  "proof": {
    "input": { "type": "trans", "...": "..." },
    "output": { "type": "step", "...": "..." },
    "derivation": [{ "pos": [], "rule_ref": "axi:fnv1a64:...:T:id_left_axi" }]
  }
}
```

End-to-end:
- `make verify-lean-e2e-rewrite-derivation-v3`

Anchor and demo inputs:
- `.axi` rule anchor: `examples/anchors/rewrite_rules_anchor_v1.axi`

### v2: path_equiv (groupoid equivalence via normalization + optional derivations)

This certificate claims that two path expressions are equivalent under the
groupoid rewrite laws.

Rust provides:
- `left` and `right` path expressions,
- a shared `normalized` form,
- and (optionally) explicit rewrite derivations for both sides.

Rust example emitters:
- `rust/crates/axiograph-pathdb/examples/emit_path_equiv_cert_v2.rs`
- `rust/crates/axiograph-pathdb/examples/emit_path_equiv_congr_cert_v2.rs` (congruence via post-composition)

Lean verifies `path_equiv_v2` by:
- checking endpoints match,
- replaying `left_derivation` and `right_derivation` when present,
- and recomputing normalization on both sides to ensure the claimed normal form
  is correct.

Sample: `examples/certificates/path_equiv_v2.json`

### v2: delta_f (functorial pullback / Δ_F)

This certificate claims the result of a **functorial data migration pullback**:

- Given a schema morphism (functor) `F : S₁ → S₂`, and
- a target instance `I : S₂ → Set`,
- compute the pulled-back instance `Δ_F(I) = I ∘ F : S₁ → Set`.

Rust:
- `rust/crates/axiograph-pathdb/src/migration.rs` (`SchemaMorphismV1`, `DeltaFMigrationProofV1`)
- `rust/crates/axiograph-pathdb/src/optimizer.rs` (`delta_f_v1`, `delta_f_certificate_v1`)
- Example emitter: `rust/crates/axiograph-pathdb/examples/emit_delta_f_cert_v1.rs`

Lean:
- `lean/Axiograph/Certificate/Format.lean` (`Migration.*` parsers)
- `lean/Axiograph/Certificate/Check.lean` (`Migration.verifyDeltaFMigrationProofV1`)

Shape:

```json
{
  "version": 2,
  "kind": "delta_f_v1",
  "proof": {
    "morphism": {
      "source_schema": "S1",
      "target_schema": "S2",
      "objects": [{ "source_object": "A", "target_object": "X" }],
      "arrows": [{ "source_arrow": "f", "target_path": ["g"] }]
    },
    "source_schema": { "name": "S1", "objects": ["A"], "arrows": [], "subtypes": [], "relations": [], "equations": [] },
    "target_instance": { "name": "I2", "schema": "S2", "objects": [], "arrows": [], "relations": [] },
    "pulled_back_instance": { "name": "I2_delta_f", "schema": "S1", "objects": [], "arrows": [], "relations": [] }
  }
}
```

Lean verifies `delta_f_v1` by recomputing `Δ_F` from `(morphism, source_schema, target_instance)`
and checking the claimed `pulled_back_instance` matches.

Sample: `examples/certificates/delta_f_v1.json`

## Running the checker

- v1 sample: `make verify-lean`
- v2 sample: `make verify-lean-v2`
- v2 resolution sample: `make verify-lean-resolution-v2`
- v2 normalize_path sample: `make verify-lean-normalize-path-v2`
- v2 path_equiv sample: `make verify-lean-path-equiv-v2`
- v2 delta_f sample: `make verify-lean-delta-f-v1`
- Rust→Lean v1: `make verify-lean-e2e`
- Rust→Lean v2: `make verify-lean-e2e-v2`
- Rust→Lean v2 resolution: `make verify-lean-e2e-resolution-v2`
- Rust→Lean v2 normalize_path: `make verify-lean-e2e-normalize-path-v2`
- Rust→Lean v2 path_equiv: `make verify-lean-e2e-path-equiv-v2`
- Rust→Lean v2 path_equiv congruence: `make verify-lean-e2e-path-equiv-congr-v2`
- Rust→Lean v2 delta_f: `make verify-lean-e2e-delta-f-v1`
- Focused suite: `make verify-semantics`

## Next (planned)

- Extend v2 rewrite derivations beyond `normalize_path_v2`:
  reconciliation proofs and domain rewrite certificates.
- Anchor certificates to canonical `.axi` input by referencing parsed `ModuleAST`
  (or a stable hash + extracted facts) so checks are always against the same source.
