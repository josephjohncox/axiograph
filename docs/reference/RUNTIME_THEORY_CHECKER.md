# Runtime Theory Checker

`RuntimeTheoryCheckReportV1` is the Rust-side report family for operational
theory checking. It turns compiled theory obligations into scoped judgments that
are useful for ontology authoring, CQ gates, fDDD wrappers, semantic VCS merge,
and coding-agent planning.

It is not the Lean trusted kernel. It is a runtime checker for well-typedness,
admissibility, closure attempts, and explicitly scoped completeness claims.

## Judgment Form

The intended runtime judgment is:

```text
Gamma ; W ; E ; A |- obligation : Kind closed_under Fragment
```

Where:

- `Gamma` is the compiled schema/category/theory context.
- `W` is the declared world/context assumption.
- `E` is the evidence policy.
- `A` is the accepted ref, anchor, slice, or import universe.
- `Kind` is constraint, path equation, opaque equation, rewrite, transport,
  behavior case, CQ/spec, or business invariant.
- `Fragment` is the supported runtime theory fragment and closure tier.

## Trust Levels

Runtime judgments use four operational statuses:

- `checked`: the obligation is well-scoped and admissible in the supported
  runtime fragment.
- `review_only`: the obligation is addressable and typed enough for review, but
  no runtime closure/certification claim is made.
- `residual_obligation`: the checker can name the unresolved condition needed
  before stronger claims are allowed.
- `blocked`: the obligation violates the declared fragment or world/evidence
  assumptions and must not be promoted as accepted strong semantics.

Severity is `info`, `warning`, or `error`.

## Supported Fragment

The first runtime fragment supports:

- structured constraints with stable relation/role refs,
- path equations whose relation refs resolve and whose endpoints were checked
  during compiled-IR elaboration,
- rewrite rules whose variables, relation refs, endpoints, context roles, and
  temporal roles remain admissible,
- theory transport plans over schema morphisms with preserved/transported/
  missing/opaque classifications,
- opaque equations as addressable review obligations.

Unsupported higher-order/dependent obligations are not discarded. They remain
runtime-addressable with explicit non-claim status so exploration, CQ repair,
and semantic merge can point at them.

## Closure Tiers

`finite_fragment` is the default. It claims closure only for a finite accepted
world, terminating supported rules, explicit context/world indices, and declared
imports.

`evidence_weighted` first filters obligations through a thresholded evidence
world. Optional semiring/lattice propagation can be enabled later; when disabled
the report says so as a non-claim.

`global_indexed` is closure over a finite declared semantic VCS/world/slice/
import universe. It refuses undeclared imports. It is not unrestricted global
ontology truth.

## Completeness And Closure

`CompletenessClaimV1` means every obligation in the declared scope was either
checked in the fragment or explicitly classified as residual/review/blocking.

`OntologyClosureClaimV1` means the checker saturated the supported obligation
fragment to a fixpoint under the declared world/evidence/ref assumptions.

Neither claim means:

- proof of completeness for arbitrary ontology truth,
- proof of OWL/RDF closure,
- full HoTT/topos semantics,
- Lean certification,
- or absence of unknown facts under open-world semantics.

## JSON Shape

```json
{
  "version": "runtime_theory_check_report_v1",
  "schema_id": "Family",
  "theory_ref": {"kind": "theory", "theory_id": "Family.FamilyTheory"},
  "fragment": {
    "closure_tier": "finite_fragment",
    "supports_path_equations": true,
    "supports_rewrites": true,
    "supports_transports": true,
    "supports_weighted_evidence": false,
    "terminating_fragment": true
  },
  "world": {
    "world_id": "accepted:current",
    "finite": true,
    "included_refs": ["current_module"]
  },
  "evidence_policy": {
    "policy_id": "thresholded_world_default",
    "threshold_ppm": 500000,
    "semantics": "thresholded_world",
    "weighted_propagation_enabled": false
  },
  "judgments": [],
  "closure": {},
  "completeness_claim": {},
  "ontology_closure_claim": {},
  "non_claims": []
}
```

## User-Facing Surfaces

CLI:

```bash
axiograph check theory module.axi --json
axiograph check theory module.axi --closure-tier finite_fragment --out report.json
axiograph discover theory-check module.axi --out report.json
```

Tool-loop/server surface:

- `semantic_theory_check`

Shared summary sidecar:

- `RuntimeTheoryCheckSummaryV1` is the compact attachment used by semantic
  coverage, agent-engineering reports, bounded-context reports, behavior-case
  reports, CQ coverage, migration/reconciliation previews, and semantic
  merge/rebase plans.
- A sidecar may be supplied directly, or derived from `RuntimeTheoryCheckInputV1`
  when a tool/server request carries canonical `.axi` text plus an optional
  theory filter and closure tier.

Reports should be embedded or linked by:

- behavior-case reports,
- bounded-context/fDDD reports,
- CQ gates,
- migration preview,
- reconciliation preview,
- semantic merge/rebase plans,
- semantic coverage and coding-agent engineering reports.

## fDDD Mapping

Bounded-context invariants become theory obligations. Context maps become
schema/theory morphisms with transport reports. Behavior cases cite executable
CQ/spec obligations. Aggregates and business invariants become runtime-checked,
evidence-weighted, or review-only theory judgments.

The checker is therefore not only a verifier. It is also a typed planning
surface for ontology-driven development: it tells agents which business rules
are checked, which are weak, which require evidence, and which code/test/schema
changes are now required.

## Research Basis

See `docs/research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md` for the
mathematical design package behind this report family.
