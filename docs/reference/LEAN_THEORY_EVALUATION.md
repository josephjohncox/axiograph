# Lean Theory Evaluation

**Diataxis:** Reference
**Audience:** contributors, verifier/tooling implementers

This document evaluates how much of Axiograph's ontology/type theory is encoded
in Lean today, what is feasible to certify next, and where the system must keep
runtime-only or research-level non-claims explicit.

The current answer is precise:

- Lean already checks important narrow certificate fragments.
- Rust already has richer operational theory surfaces than Lean certifies.
- The whole Axiograph theory is not yet encoded in Lean.
- Semantic merge/lattice preservation now has a small Lean scaffold in
  `lean/Axiograph/SemanticVCS.lean` plus an executable conformance checker,
  but it is not yet part of the shipped `VerifyMain` certificate boundary.

## Current Lean Boundary

The shipped trusted checker is the import closure of:

```text
lean/Axiograph/VerifyMain.lean
```

That boundary currently checks:

- canonical `.axi` parsing and anchored module loading;
- conservative `.axi` well-typedness certificates;
- conservative `.axi` constraint certificates;
- fixed-point probability arithmetic used in certificates;
- `query_result_v3` row-soundness replay against canonical `.axi`;
- `rewrite_derivation_v3` replay against canonical `.axi` rewrite rules;
- `normalize_path_v2` and `path_equiv_v2` path witnesses;
- `delta_f_v1` migration recompute scaffolding.

The umbrella Lean library also contains theorem-support and research-support
modules for HoTT/groupoid paths, path congruence, probability, topos/sheaf
orientation, and now operational semantic VCS slices.

Those modules are valuable, but they do not become product-trusted merely
because they exist. They must be imported by the verifier target or referenced
by a checked certificate path before they participate in a trusted claim.

## Lean-Encoding Status Matrix

| Area | Lean status | Runtime status | Product claim allowed now |
| --- | --- | --- | --- |
| Canonical `.axi` parser | In verifier boundary | Used by CLI/server gates | Trusted parsing for verifier inputs |
| `.axi` well-typedness gate | In verifier boundary | Rust has broader runtime checks | Conservative well-typedness only |
| `.axi` constraints gate | In verifier boundary | Rust has richer theory checks | Conservative constraint subset only |
| Fixed-point probability | In verifier boundary | Used by certs and evidence scoring | Deterministic checked arithmetic where certificates use it |
| Reachability v3 | In verifier boundary | Canonical-anchor query/cert path exists | Replay soundness for anchored facts, not query completeness |
| Query result v3 | In verifier boundary | Prepared query metadata exists | Returned-row soundness for supported fragment, not completeness |
| Path normalization/equivalence | In verifier boundary for v2 certs | Runtime path/rewrite checks exist | Narrow path witness replay |
| Rewrite derivation v3 | In verifier boundary | Runtime rewrite admissibility exists | Replay against accepted canonical rules |
| Delta-F migration | In verifier boundary as recompute scaffold | Runtime transport plans exist | Narrow recompute parity, not full functorial migration theory |
| Runtime theory checker | Not in verifier boundary | `RuntimeTheoryCheckReportV1` exists | Well-typed/admissible/closed under declared runtime assumptions |
| `KernelSurfaceV1` / `KernelRefV1` | Not encoded fully | Runtime shared ref surface exists | Typed runtime citation, not Lean proof |
| Semantic slices and merge lattice | Lean scaffold exists outside verifier | Runtime merge lattice exists | Finite operational preservation scaffold only |
| Semantic rebase transport | Lean scaffold exists outside verifier | Runtime rebase transport plans exist | Runtime transport classification, not certified rebase yet |
| Semantic VCS commits/refs | Not encoded fully | Partial Rust implementation exists | Runtime lifecycle object, not certified history calculus |
| DDD/fDDD overlays | Not encoded fully | Tooling overlay crate exists | Runtime authoring/coverage aid, not kernel semantics |
| Embedding/evidence sidecars | Not encoded fully | Advisory evidence overlays exist | Evidence-plane suggestions only |
| Backend projections | Not encoded fully | TypeDB/TerminusDB pushdown plans exist | Native-readable projection with caveats, not semantic authority |

## New Semantic VCS Lean Scaffold

`lean/Axiograph/SemanticVCS.lean` encodes the finite operational core used by
semantic merge/rebase planning:

- `SemanticRefKind` and `SemanticRef`;
- `SemanticAnchor`;
- `SemanticSliceManifest` and denotation to `SemanticSlice`;
- `refSubset`, `disjoint`, `overlaps`;
- finite operational `join` and `meet`;
- upper/lower-bound preservation lemmas;
- conservative merge plans and materialization predicates;
- checked merge-plan wrappers for conservative joins;
- executable fail-closed merge/rebase materialization gates with soundness
  lemmas;
- rebase transport items and transport preservation predicates;
- an operation vocabulary spanning query, CQ, behavior case, authoring,
  migration, reconciliation, merge, rebase, promotion, supersession,
  retraction, backend projection, and embedding evidence lift.

`lean/Axiograph/SemanticVCS/Json.lean` adds the first strict Lean-readable JSON
shape for runtime exports:

- semantic refs and anchors;
- semantic slice manifests;
- merge blockers, resolver steps, and trust classes;
- merge plans accepted by `checkMergePlanJson`;
- transport items and rebase plans accepted by `checkRebasePlanJson`.

`lean/Axiograph/SemanticVCS/CheckMain.lean` is an executable checker over that
shape. It is a Rust+theory conformance harness: Rust exports a reduced
merge/rebase plan, and Lean checks finite materialization semantics. These JSON
parsers are not imported by `VerifyMain` yet, so this is not the same trust
class as canonical query/rewrite certificates.

Rust now has matching adapter functions in
`rust/crates/axiograph-cli/src/semantic_merge_lattice.rs`:

- `semantic_merge_plan_lean_json_v1(...)`;
- `semantic_rebase_plan_lean_json_v1(...)`.

They intentionally emit a reduced Lean-check shape rather than the full runtime
report. Full `SemanticMergePlanV1` / `SemanticRebasePlanV1` remains the
operator-facing report; the Lean shape is the future certificate/checker input.
The CLI exposes the same reduced shape through:

```bash
axiograph sem merge --dry-run --source heads/review/demo --target heads/main --lean-json
axiograph sem rebase --source heads/review/demo --onto heads/main --lean-json
```

Run the focused conformance path with:

```bash
make verify-lean-semantic-vcs
```

That target builds `axiograph_semantic_vcs_check`, runs Rust semantic-merge
tests, runs the plant-operations semantic merge script, checks clean
merge/rebase fixtures, checks a Rust-generated dry-run merge payload, and
ensures conflicting merge, blocked rebase, and required failed-transport
payloads are rejected. The script writes
`semantic_vcs_conformance_coverage_v1`, which is the current machine-readable
coverage statement for this finite Rust+Lean conformance surface.

That coverage statement is complete for the claimed conformance cases only:
static clean merge acceptance, static clean rebase acceptance, Rust-generated
clean merge acceptance, static conflicting merge rejection, static blocked
rebase rejection, static failed-transport-without-blocker rejection, and
Rust-generated blocked rebase rejection. It is not a claim that the plant flow
currently generates a clean rebase, covers every field of `SemanticMergePlanV1`
or `SemanticRebasePlanV1`, proves resolver-policy completeness, proves
competency-question or trust-regression preservation, or participates in the
`VerifyMain` certificate boundary.

The most important checked facts are:

- `join_contains_left`;
- `join_contains_right`;
- `join_least`;
- `meet_subset_left`;
- `meet_subset_right`;
- `meet_greatest`;
- `disjoint_not_overlaps`;
- `conservative_join_preserves_left`;
- `conservative_join_preserves_right`;
- `conservative_join_can_materialize`;
- `checkMergePlanMaterialization_sound`;
- `checkRebasePlanMaterialization_sound`;
- `checkRebasePlanMaterialization_transport_sound`;
- `rebase_preservation_from_parts`.

These are intentionally small. They prove that the denotation of the
conservative finite join preserves left/right slice refs and that the finite
meet behaves as an intersection. The rebase checker also rejects required
transport items whose status is missing, opaque, or blocked. These facts do not
prove merge optimality, CQ preservation, complete ontology closure, or global
lattice completeness.

## Feasibility Assessment

### Feasible now

These should be the next Lean targets because they match existing Rust objects
and avoid broad theorem-prover overreach:

- Extend the current semantic VCS conformance checker from reduced fixture
  payloads to `KernelRefV1` / `KernelSurfaceV1` snapshots, then check that
  query/CQ/merge reports cite declared refs.
- Check `SemanticMergePlanV1` as a Lean-readable certificate for conservative
  disjoint/idempotent joins.
- Check that materialization gates are fail-closed: unresolved blockers,
  resolver steps, residual obligations, CQ regressions, trust regressions, or
  runtime-theory blockers imply no materialization certificate.
- Extend `delta_f_v1` from recompute parity toward selected schema-morphism
  transport obligations over objects, arrows, and path equations.
- Check rewrite-rule admissibility over declared variables, endpoints,
  contexts, worlds, temporal roles, and touched relation roles.

### Feasible but deeper

These are plausible but require more IR export and proof design:

- Encode a fuller `TheoryIr` in Lean with addressable constraints, equations,
  rewrites, obligations, dependent contexts, and transport items.
- Prove finite runtime closure soundness for a terminating supported fragment:
  every in-scope obligation is either checked, saturated to fixpoint, or
  reported as residual.
- Prove query metadata consistency: `PreparedQueryMetadataV1` refs are declared
  in the compiled kernel surface and the returned rows use only supported
  typed paths.
- Prove selected semantic rebase preservation: required transport items are
  preserved/transported and failed transports become residual obligations.
- Prove selected backend projection/lifting checks: TypeDB/TerminusDB projected
  refs lift back to declared kernel refs under the manifest's caveats.

### Research-grade or explicit non-claims

These should not be claimed without a much larger formalization:

- arbitrary ontology merge as a complete lattice;
- globally complete open-world ontology closure;
- full HoTT/univalence semantics for all ontology equivalence;
- full topos/sheaf semantics for every context/world operation;
- general OWL/RDFS/RDF entailment completeness;
- SHACL recursion semantics;
- backend-native query completeness relative to canonical `.axi`.

## Completeness And Closure Semantics

Runtime theory reports may say:

```text
Γ ; W ; E ; A ⊢ obligation : Kind closed_under Fragment
```

The intended reading is:

- `Γ`: declared compiled schema/theory/context environment;
- `W`: finite declared world/context/slice/import universe;
- `E`: evidence policy or evidence-weight threshold;
- `A`: accepted anchor and lifecycle state;
- `Fragment`: supported runtime fragment such as `finite_fragment`,
  `evidence_weighted`, or `global_indexed`.

Lean should eventually mirror this judgment with data types for:

- environments and anchors;
- finite world/import universes;
- evidence policies;
- obligations and obligation subjects;
- admissibility checks;
- closure worklists and residual obligations.

Until then, Rust completeness claims are operational:

- `finite_fragment`: every in-scope supported obligation was checked or marked
  residual, and supported rules reached a bounded fixpoint;
- `evidence_weighted`: the finite world was filtered by declared evidence
  policy, with optional weight propagation;
- `global_indexed`: every included ref/world/slice/import anchor was declared
  explicitly.

They are not global ontology completeness claims.

## Preservation Across Operations

The same preservation vocabulary should be used across all operations:

| Operation | Preservation shape |
| --- | --- |
| Query | returned refs/rows are typed under the prepared-query anchor |
| CQ | executable question cites declared kernel refs and returns checked/pass/fail/residual status |
| Behavior case | behavior obligations cite CQ/theory/overlay refs and do not mutate domain `.axi` |
| Authoring delta | proposed refs lower to typed preview handles before promotion |
| Migration | source obligations transport, fail, or become residual obligations |
| Reconciliation | conflicts, decisions, and resolver handles cite typed refs |
| Merge | conservative joins preserve source refs and fail closed on blockers |
| Rebase | required transports preserve/transport or become residuals |
| Promotion | accepted mutation occurs only after typed gates pass |
| Supersede/retract | lifecycle transitions cite previous accepted refs and preserve audit history |
| Backend projection | native-readable output cites projection manifest and lifting caveats |
| Embedding evidence lift | vector relationships become advisory proposals, not accepted facts |

The Lean `SemanticOperationKind` and `OperationPreservationClaim` scaffold is
the first common vocabulary for proving these operation-specific statements.

## Concrete Lean Roadmap

1. **Finite slice certificate.** Export `SemanticSliceManifestV1` and
   `SemanticMergePlanV1` to Lean JSON, then check conservative join/ref
   preservation and materialization blockers using
   `checkMergePlanJson` / `checkMergePlanMaterialization`.
2. **Kernel ref declaration check.** Export `KernelSurfaceV1` to Lean and reject
   query/merge/CQ certificates that cite undeclared refs.
3. **Theory obligation address model.** Encode `TheoryIr` obligations, subjects,
   path expressions, rewrite rules, dependent context axes, and touched roles.
4. **Runtime closure witness.** Encode finite closure traces so Lean can verify
   checked/residual partitioning for the supported terminating fragment.
5. **Transport and rebase.** Extend `delta_f_v1` into typed transport witnesses
   for path equations, rewrite obligations, and selected dependent-context
   transports, then feed `SemanticRebasePlanV1` into
   `checkRebasePlanJson` / `checkRebasePlanMaterialization`.
6. **Merge compiler soundness.** Prove that accepted conservative merge plans
   preserve input refs and cannot materialize with blockers/residuals.
7. **Query/CQ consistency.** Tie prepared-query metadata, CQ reports, returned
   rows, and kernel refs into one certifiable witness.
8. **Evidence/backend fragments.** Add advisory-only checks that evidence and
   backend projections cite declared refs and do not mutate accepted meaning.

## References

- David I. Spivak, "Ologs: a categorical framework for knowledge
  representation": <https://arxiv.org/abs/1102.1889>
- David I. Spivak, "Functorial Data Migration": <https://arxiv.org/abs/1009.1166>
- Ryan Wisnesky et al., "Functorial Data Migration: From Theory to Practice":
  <https://arxiv.org/abs/1502.05947>
- Brendan Fong and David I. Spivak, "Seven Sketches in Compositionality":
  <https://arxiv.org/abs/1803.05316>
- Peter Dybjer, "Internal Type Theory": categories with families:
  <https://www.cse.chalmers.se/~peterd/papers/ITT.pdf>
- Stanford Encyclopedia of Philosophy, "Intuitionistic Type Theory":
  <https://plato.stanford.edu/entries/type-theory-intuitionistic/>
- The Univalent Foundations Program, "Homotopy Type Theory":
  <https://homotopytypetheory.org/book/>
- Joseph A. Goguen and Rod M. Burstall, "Institutions":
  <https://doi.org/10.1145/5397.5398>
- W3C RDF 1.1 Semantics: <https://www.w3.org/TR/rdf11-mt/>
- W3C SHACL Recommendation: <https://www.w3.org/TR/shacl/>
- Martin Fowler, "Bounded Context":
  <https://martinfowler.com/bliki/BoundedContext.html>
- Scott Wlaschin, "Domain Modeling Made Functional":
  <https://pragprog.com/titles/swdddf/domain-modeling-made-functional/>
