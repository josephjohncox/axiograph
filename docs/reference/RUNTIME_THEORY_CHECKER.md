# Runtime Theory Checker

**Diataxis:** Reference
**Audience:** contributors, tool authors, ontology reviewers

`RuntimeTheoryCheckReportV1` is the Rust-side **admissibility report** over
compiled `TheoryIr`. It is useful for authoring, CQ review, migration,
reconciliation, merge planning, and agent tooling.

`RuntimeTheoryCheckReportV1` is not a saturation engine and is not the Lean
trusted checker. A separate path does perform saturation:
`SchemaPresentationIr::category_formation` computes bounded finite generator
reachability, and anchored `category_kernel_v3` makes Lean reconstruct and check
the presentation, equations, congruence, and saturation. The two operations do
not share claims.

## Exact Contract

The runtime checker currently does four things:

1. resolves compiled theory obligations and their stable subjects;
2. checks the supported endpoint, variable, relation, role-axis, evidence, and
   transport conditions;
3. classifies each obligation as `checked`, `review_only`,
   `residual_obligation`, or `blocked`; and
4. emits typed dependency handles and an explanation trace of that scan.

It does **not** apply equations or rewrite rules to derive new facts or
obligations. It does not prove rewrite termination or confluence. The report
therefore contains `admissibility_scan`, not a legacy closure report. There are
no `complete`, `closed`, `fixpoint_reached`, `derived_obligations`,
`completeness_claim`, or `ontology_closure_claim` fields on the per-theory
report.

`admissibility_scan` records evidence-propagation iterations, considered and
excluded inputs, blockers, residuals, assumptions, and typed scan steps. The
final step is `admissibility_scan_complete`. CLI/module summaries expose
`admissibility_scopes` and `admissibility_trace`; their shared trust-contract
strings use `not_claimed_runtime_admissibility_only` for completeness and
ontology closure. `closure_engine_not_implemented` is always an explicit
residual/non-claim.

## Judgment Form

The runtime report is best read as:

```text
Gamma ; W ; E ; A |- obligation : Kind admissibility Status
```

Where:

- `Gamma` is the compiled schema/category/theory environment;
- `W` is caller-supplied finite world/context scope metadata;
- `E` is the evidence policy;
- `A` is the accepted ref/anchor/slice/import metadata;
- `Kind` is constraint, path equation, opaque equation, rewrite, or attached
  transport classification; and
- `Status` is the operational result below.

World/ref/slice/import fields narrow and explain the report. They are not by
themselves proof that the selected universe is canonical or exhaustive.

## Statuses

| Status | Meaning |
| --- | --- |
| `checked` | The obligation is well-scoped and admissible in the implemented runtime check. This is not a derived theorem or closure fact. |
| `review_only` | The obligation is typed/addressable, but no enforcement, closure, or certification claim is available. |
| `residual_obligation` | A named condition must be resolved before the operation can be treated as admissible. |
| `blocked` | The obligation violates the supported typed fragment or declared transport/scope conditions. |

The checker consumes the compiled `RuntimeTheoryObligationStatusV1`
classification. In particular:

- `functional`, `at_most`, and `key` may be runtime-enforced;
- `symmetric_where_in`, `symmetric`, and `transitive` remain review-only until
  an actual executor/certificate checks their finite fiber semantics;
- path equations and rewrite rules receive runtime endpoint/axis admissibility
  checks, but remain outside Lean certification unless a separate certificate
  is accepted; and
- opaque equations remain explicit review residuals.

Evidence filtering never erases a semantic residual. A low-weight opaque,
review-only, transported, or blocked obligation remains in
`admissibility_scan.residual_obligations`.

## Typed Runtime Surfaces

Every obligation may expose:

- stable theory/obligation/subject `RuntimeIrRef` citations;
- path-expression and path-step refs;
- variable and typed endpoint refs;
- context/temporal axis refs;
- transport-item refs;
- structured admissibility diagnostics; and
- residual obligations and non-claims.

These refs resolve in the derived runtime index retained under a
`CompiledKernelSnapshot`. They are operational citations, not proof terms,
accepted snapshot authority, or substitutes for canonical `.axi` bytes.

## Contexts And Transport

Context, world, temporal, parameter, and evidence roles survive compiled IR as
typed subjects. Rewrites that drop context or temporal axes block. Transport
plans attach one of:

- `preserved`;
- `transported` (still residual pending review/certification);
- `missing_object_image`;
- `missing_arrow_image`; or
- `opaque_or_out_of_fragment`.

The current runtime adapter does not prove naturality, functoriality, exhaustive
context closure, or preservation of arbitrary refinements. Those require a
checked transport certificate.

## Evidence Tiers

The existing tier names are report scopes, not closure results:

- `finite_fragment`: admissibility over the compiled finite obligation list;
- `evidence_weighted`: optional finite weight propagation and thresholding
  before the same admissibility classification; and
- `global_indexed`: caller-annotated finite ref/world/slice/import scope with
  fail-closed handling of declared unknown imports.

Weighted evidence propagation may reach a fixpoint in its finite numeric
calculation. That does not imply a theory or ontology fixpoint.

## Separate Bounded Finite Saturation

This section describes a different operation from the runtime admissibility
report above. `axiograph-kernel` and `lean/Axiograph/Theory/Finite.lean`
implement the same bounded category-presentation fragment. Rust stores the sole
presentation in `SchemaPresentationIr` and derived replay evidence in
`category_formation`; Lean reconstructs that presentation from exact anchored
`.axi` before checking the exported `category_kernel_v3` certificate.

- finite category presentations;
- relation objects with checked role-projection arrows;
- endpoint-indexed category paths and accepted parallel-path equations;
- mathlib-backed free-groupoid identity, associativity, and inverse laws;
- finite interpretations and dependent role witnesses;
- finite equality/membership refinements;
- context-indexed values and proof-carrying context transports;
- typed path holes and residual obligations; and
- bounded Floyd-Warshall reachability saturation with replayable explanation
  certificates.

`verifySaturationCertificate` checks that every claimed pair is explained by
identities, declared generators, and typed composition; that every seed is
present; and that the finite set is closed under composition.

The regulated-shipment test instantiates this with relation-object projections,
explicit shipment/batch/certificate generators, a parallel certificate path
equation, finite reviewer membership, and an explanation-certified
shipment-to-certificate reachability entry. A forged identity explanation from
shipment to context is rejected.

That certificate means **finite generator reachability only**. Presented path
equations identify parallel paths but do not create endpoints. The checker does
not infer ontology facts, execute arbitrary rewrites, prove confluence, close an
open world, establish univalence/HITs, or provide topos/sheaf completeness.

`Certificate.Format` imports this module and `VerifyMain` dispatches only the
strict anchored `category_kernel_v3` category family. It covers exact
presentation reconstruction, ordered projections and identities, typed arrow
composition, parallel equations, contextual congruence, and exact bounded
generator reachability. The rest of the module—full finite interpretations,
refinements, contexts, and transports—remains theorem support unless a
certificate kind invokes it.

Run the positive and adversarial theory gate with:

```bash
make verify-lean-theory
```

The gate checks valid relation-object projections and their declaration order,
dependent role/context witnesses, checked hole lifecycles, refinements, bounded
saturation, and explanation replay. It requires rejection of bad projection
targets, swapped role order, non-composable equations, unsupported or
out-of-range refinements, exceeded bounds, and tampered explanations. It also
runs the Rust kernel/runtime-theory regressions and the Rust-to-Lean regulated
shipment certificate test.

## User-Facing Runtime Command

```bash
axiograph check theory module.axi --json
axiograph discover theory-check module.axi --out report.json
```

Read these as admissibility/exploration reports. A report can guide the next
repair or review action; it cannot authorize claims of finite closure,
completeness, or certification.

## Authoring Integration

`AuthoringWorkspaceService` runs the same finite-fragment checker after the
canonical workspace import closure compiles. It embeds the module report in
`authoring_workspace_report_v1` and uses a fail-closed promotion gate:

- no compiled theories produces an explicit empty finite gate;
- blocking judgments block promotion review;
- residual obligations block promotion review rather than becoming warnings;
- CQ, query, olog, LSP, MCP, and HTTP adapters do not reimplement theory rules;
- authoring stores a canonical `Authoring` finite-theory receipt;
- prepared-query metadata stores a `Query` receipt when it has the accepted
  snapshot; and
- AxiStore candidate recompilation requires a `Merge` receipt.

These three receipts replay the canonical Rust IR and remain operational. The
separate anchored `category_kernel_v3` certificate is the trusted Lean category
claim, with the narrower scope above. Runtime admissibility does not become a
theorem merely because both checks pass.

## Explicit Non-Claims

Neither the Rust report nor the finite Lean reachability fragment proves:

- open-world or global ontology completeness;
- OWL/RDF/SHACL entailment completeness;
- arbitrary dependent type theory;
- arbitrary rewrite termination or confluence;
- univalence, higher inductive types, or unrestricted HoTT;
- topos/sheaf descent or modal completeness;
- source truth or evidence exhaustiveness; or
- correctness of the full Rust runtime.

For the trusted boundary, see `docs/reference/TRUSTED_KERNEL.md`. For the Lean
status matrix, see `docs/reference/LEAN_THEORY_EVALUATION.md`.
