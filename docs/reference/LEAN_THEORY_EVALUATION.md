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
- `Axiograph.Theory.Finite` implements a checked finite category/dependent/
  groupoid fragment with explanation-certified generator reachability. Its
  anchored `category_kernel_v3` checker is in the `VerifyMain` import closure
  and checks exact presentation reconstruction, equations, congruence, and
  saturation; broader interpretation and transport definitions are not
  automatically product claims.
- Semantic merge/lattice preservation has a finite Lean conformance slice in
  `lean/Axiograph/SemanticVCS.lean` plus an executable conformance checker.
- Authenticated V2 commit-path checking now has a separate strict Rust/Lean
  parity slice in `lean/Axiograph/SemanticVCS/Lineage.lean`. It requires a
  caller-supplied repository id or expected head.
- Both Semantic VCS checkers remain outside the shipped `VerifyMain`
  certificate boundary.

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
- `query_result_v4` witness replay plus exact-completeness decision procedure
  for the declared bounded finite UCQ/RPQ denotation against canonical `.axi`;
- `category_kernel_v3` reconstruction of relation-object category
  presentations from exact anchored `.axi`, including ordered projections,
  identities, typed composition, parallel equations, contextual congruence,
  exact signed cancellation traces for both formal inverse laws of every
  generator, and exact-closure checking for bounded finite generator reachability;
- `rewrite_derivation_v3` replay against canonical `.axi` rewrite rules;
- `normalize_path_v2` and `path_equiv_v2` mandatory rewrite traces, with
  acceptance-to-free-groupoid-denotation theorems imported through
  `Axiograph.Certificate.Invariants`;
- `delta_f_v1` migration certificate that checks recomputed parity over the
  supported fragment.

The umbrella Lean library also contains theorem-support and research-support
modules for HoTT/groupoid paths, path congruence, probability, topos/sheaf
orientation, and now operational semantic VCS slices.

Those modules are valuable, but they do not become product-trusted merely
because they exist. They must be imported by the verifier target or referenced
by a checked certificate path before they participate in a trusted claim.

The checker boundary also depends on operational input safety without treating
that safety as a theorem. Exact-byte file reads, certificate/receipt byte and
nesting limits, approved-executable staging, bounded stdio, timeout, and process
concurrency are Rust controls documented in
`docs/reference/SECURITY_BOUNDARIES.md`. They protect delivery of the checked
claim; they do not extend what Lean proves.

## Lean-Encoding Status Matrix

| Area | Lean status | Runtime status | Product claim allowed now |
| --- | --- | --- | --- |
| Canonical `.axi` parser | In verifier boundary | Used by CLI/server gates | Trusted parsing for verifier inputs |
| `.axi` well-typedness gate | In verifier boundary; Rust/Lean parser and formation corpora cover W02 role/type/generator syntax | Rust canonical compiler has broader finite-model checks | Conservative well-typedness only |
| `.axi` constraints gate | In verifier boundary | Rust has richer theory checks | Conservative constraint subset only |
| Fixed-point probability | In verifier boundary | Used by certs and evidence scoring | Deterministic checked arithmetic where certificates use it |
| Reachability v3 | In verifier boundary | Used as the path-witness syntax inside V4 | Replay soundness for anchored canonical facts |
| Query result v4 | In verifier boundary with `ensureExactFiniteCompletenessV4_sound` | `CompiledFiniteQuery` is the only executable/certifiable family | Exact completeness for bounded finite type/derived-attribute/RPQ UCQs; no open-world ontology-closure claim |
| Path normalization/equivalence | In verifier boundary with mandatory traces and endpoint-retyped replay-soundness theorems | Runtime uses the same endpoint and rewrite vocabulary | Denotational equality in the supported free-groupoid syntax after endpoint retyping; no ontology-fact or confidence-equality claim |
| Rewrite derivation v3 | In verifier boundary | Runtime rewrite admissibility exists | Replay against accepted canonical rules |
| Delta-F migration | In verifier boundary as recomputed witness | Runtime transport plans exist | Narrow recompute parity, not full functorial migration theory |
| Runtime theory checker | Not in verifier boundary | `RuntimeTheoryCheckReportV1` is an admissibility scan | Typed admissibility/residual report only; no saturation, fixpoint, completeness, or closure claim |
| Finite category/dependent/groupoid theory | `Axiograph.Theory.Finite` is in `VerifyMain` through strict `category_kernel_v3` dispatch | Canonical IR stores the presentation, executable saturation, formal equations, traced normalization, dependent witnesses, holes, lifecycle states, and shared gate receipts | Trusted anchored presentation reconstruction plus finite decision procedures and wire replay for equation congruence, formal inverse cancellation, and bounded generator reachability; no category-wire denotation theorem, and broader finite interpretation/transport and Rust gate claims remain untrusted |
| `RuntimeSemanticIndex` / `RuntimeIrRef` | Not encoded fully | Derived runtime citation surface exists after canonical compilation | Typed runtime citation, not Lean proof or semantic authority |
| Semantic slices and merge lattice | Finite Lean conformance slice exists outside verifier | Runtime merge lattice exists | Finite operational preservation slice only |
| Semantic rebase transport | Finite Lean conformance slice exists outside verifier | Runtime rebase transport plans exist | Runtime transport classification, not certified rebase yet |
| Semantic VCS commits/refs | Not encoded in the trusted checker; finite merge/rebase conformance remains outside `VerifyMain` | Transactional `axiograph-store` validates accepted state, audit chain, refs, tags, and lineage under exact state/subject pins | Rust operational integrity only; not Lean authority, author identity, historical ref authenticity, or ontology closure |
| DDD/fDDD overlays | Not encoded fully | Tooling overlay crate exists | Runtime authoring/coverage aid, not kernel semantics |
| Embedding/evidence sidecars | Not encoded fully | Advisory evidence overlays exist | Evidence-plane suggestions only |
| Backend projections | Not encoded fully | TypeDB/TerminusDB pushdown plans exist | Native-readable projection with caveats, not semantic authority |

## Exact Finite Query Theorem And Scope

`Axiograph.Query.finiteQueryDenotationV4` reconstructs the finite canonical
object/fact universe from the accepted module and evaluates each query disjunct.
The supported atom denotation is:

- subtype-aware type membership;
- canonical derived-attribute equality;
- canonical tuple-field, context-projection, and binary-relation paths;
- regular-expression paths, with explicit `max_hops` required for `*` and `+`.

The checker caps disjuncts, atoms, regex nodes, hops, and candidate assignments.
Unary type/attribute constraints narrow candidate domains before Cartesian
enumeration. `rowsExactlyDenotationV4` compares every `(disjunct, full binding)`
assignment in both directions and rejects missing, duplicate-substituted, extra,
or truncated results. The theorem
`ensureExactFiniteCompletenessV4_sound` proves that successful exact-checker
acceptance implies `ExactFiniteCompletenessV4`.

This theorem is exact for that finite executable denotation. It does not prove
completeness of approximate operators, multi-context unions rejected by the
compiler, unbounded path repetition, the runtime's full storage image,
evidence discovery, open-world ontology closure, general dependent type theory,
univalence, higher inductive types, or unrestricted HoTT equality.

The regulated-shipment fixture instantiates this theorem with
`ShipmentContainsBatch / BatchHasCertificate`, `max_hops = 2`, and one selected
certificate. `VerifyMain` accepts the exact row `CoA_RX_42`; an answer with that
row removed but with internally recomputed answer digest is rejected by the
trusted finite denotation check.

## Canonical Compiler And Lean Parity Boundary

Lean's `SchemaV1` and `TypeCheck` modules now parse and formation-check module
headers, ordered imports, relation objects, explicit projection generators,
role kinds, indexed/refined types, labeled relation facts, subtype cycles,
duplicate declarations, unknown targets, typed theories, and generator
interpretations over the shared adversarial corpus.

This is not full compiler equivalence. Rust alone currently constructs
`KernelSnapshotIr`, `SchemaPresentationIr`, and validated finite
`InstanceModelIr`; checks total functions, injective subtype maps, stable fact
ids, finite refinements, and equation satisfaction; and binds them to the
immutable accepted snapshot handle. Lean does not deserialize or certify that
complete IR yet. The trusted product claim remains limited to the certificate
fragments imported by `VerifyMain`.

## Finite Category, Dependent, And Groupoid Fragment

`lean/Axiograph/Theory/Finite.lean` is the first executable finite semantics
module that combines the category, dependent, and path layers without claiming
a general theorem that the implementation cannot support.

It provides:

- finite objects and arrows, with relation objects and role projections checked
  against source/target types, exact zero-based declaration order, and unique
  earlier-role names in every dependent index;
  `compileAxiSchemaPresentation` derives this
  slice from Lean's canonical `.axi` schema AST rather than inventing a second
  authoring authority;
- endpoint-indexed category paths and parallel-path presentation equations;
- free-groupoid completion whose unit, inverse, associativity, composition
  congruence, and inverse congruence laws are proved by denotation into
  mathlib's `Quiver.FreeGroupoid`;
- finite-set interpretations, dependent role witnesses, equality/membership
  refinements, context-indexed values, context transports, and a theorem that
  typed paths preserve declared context visibility;
- typed path holes whose selection requires the `Residual` lifecycle plus a
  matching nonempty typed-hole obligation before producing
  `ExplanationVerified`; and
- deterministic finite reachability saturation with replayable explanation
  certificates under the same fixed bounds used by Rust.

The explanation checker verifies seed inclusion, typed replay, and closure under
composition. Since every explanation is built only from identities, declared
generators, and composition, accepted entries cannot invent an endpoint pair.
It accepts any valid typed explanation tree for that pair; it does not require
literal equality with Rust's chosen tree. The
certificate is exact only for that finite reachability relation. Equations do
not create endpoints; arbitrary rewrite application, termination, confluence,
ontology fact closure, and open-world completeness remain non-claims. Surface
`.axi` theory equations enter this fragment only when both sides are explicit,
endpoint-correct schema-generator paths (`id(X)`, a named generator, or
semicolon composition). Relation-span/groupoid and opaque theory equations
remain outside this certificate claim.

The module is imported by `lean/Axiograph.lean`, exercised by
`axiograph_finite_theory_tests`, and imported into `VerifyMain` through
`Certificate.Format`. Envelope V3 kind `category_kernel_v3` carries the
compiler's finite name/index presentation, contextual congruence certificates,
exact signed cancellation traces for both inverse laws of every generator, and
a strict saturation certificate under a V2 exact-byte revision anchor.
`Certificate.Check` independently reconstructs the schema presentation from the
anchored `.axi`, requires exact equality with the compiler projection, replays
all equation replacements, formal inverse cancellations, and reachability
explanations, and enforces exact identity/generator/composition closure under
the 64-object/4,096-arrow bounds. Congruence also checks the selected equation's
source and target at the replacement offset, including empty identity paths,
and requires one one-step forward witness per equation.

This V3 path is a decision procedure plus replay, not theorem-backed replay.
Its wire words are not retyped as dependent `GroupoidPath` values, and no
acceptance theorem connects successful cancellation replay to
`GroupoidPath.denote` or `PathEquiv`. The denotational laws proved earlier in the
module apply to the dependent path type, not automatically to this wire format.
The exact-byte anchor names one defining module. Rust refuses V3 export when a
forward equation on that schema comes from an importing module, because Lean
cannot reconstruct that extension from the single anchored file. Import-closure
certification requires a future closure anchor.

It does not deserialize the complete Rust `KernelSnapshotIr` or certify full
instances, `ObjectMembershipWitnessIr`, `RoleIndexedWitnessIr`,
`TypedConstraintWitnessIr`, `DependentContextIr`, non-identity transports, or
relation-span groupoid equations. Those payloads are replayable Rust
finite-decision evidence with explicit non-claims, not trusted proof terms.

The finite executable also contains a regulated-shipment presentation with
three relation objects (`ShipmentContainsBatch`, `BatchHasCertificate`, and
`DispatchReview`), eight projection arrows, explicit shipment-to-batch and
batch-to-certificate generators, one parallel path equation, finite reviewer
membership, complete bounded generator saturation, and adversarial explanation
replay. This is the strongest decidable presentation fragment implemented; it
is not a proof that the Rust compiled package or projected backend is equivalent
to that hand-checked finite presentation.

Run:

```bash
make verify-lean-theory
make verify-lean-e2e-category-kernel-v3
```

The first target runs the broader finite-theory regressions. The second checks
Rust/Lean exact-presentation equality for the canonical regulated-shipment
schema and proves that formation, congruence, signed normalization-trace, and
saturation tampering rejects.

The theory gate also runs Rust regressions proving that `RuntimeTheoryCheckReportV1`
uses typed scope, coverage, residual, transport, and non-claim fields rather
than synthetic completeness/closure/fixpoint claims; never reports checked
obligations as derived obligations; preserves evidence-filtered semantic
residuals; and keeps transitivity review-only without an executor.

`authoring_workspace_report_v1` exposes the matching Rust finite fragment to
CLI, LSP, MCP, and HTTP: compiled relation objects/projections, typed
refinements, endpoint-safe paths, finite CQ/query execution, exact compiled
payload fingerprints, and an `Authoring` finite-theory gate receipt. The receipt
contains typed scope and exact replay coverage for category formation,
identities, saturation explanations, refinements, contexts, and identity
transports, plus residuals and non-claims. Prepared query metadata carries a
separate `Query` receipt when the accepted snapshot is available. Every
AxiStore typed candidate carries a `Merge` receipt that is reproduced during
canonical recompilation. These Rust receipts are not Lean encodings. The
separate anchored `category_kernel_v3` certificate covers only
the finite category presentation, contextual equation congruence, formal
inverse-law normalization, and generator saturation; neither it nor the Rust
gate receipts establish category
equivalence between arbitrary presentations, naturality, univalence,
higher-path equality, rewrite confluence, or ontology closure.

## Operational Semantic VCS Lean Conformance Slice

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
shape. It is a Rust+theory conformance checker: Rust exports a reduced
merge/rebase plan, and Lean checks finite materialization semantics. These JSON
parsers are not imported by `VerifyMain` yet, so this is not the same trust
class as canonical query/rewrite certificates.

AxiStore lineage validation is deliberately not dispatched through this Lean
executable. `axiograph-store` checks full SHA-256 typed identities, parent
ordering, maximal-common-ancestor semantics, audit closure, and exact
state/subject pins as a Rust operational contract. Moving any of those claims
into the trusted product boundary requires a new certificate format imported by
`VerifyMain`; the runtime check is not described as a Lean proof.

The current lineage slice does not check signed ref-update receipts, historical
ref state, author identity, a transparency log, or the default V1 CLI history.
Its complete-DAG merge-base selection is implemented on the Rust side; compact
Lean paths prove only ancestry/common ancestry. It remains outside
`VerifyMain` pending explicit trust review.

Rust now has matching adapter functions in
`rust/crates/axiograph-cli/src/semantic_merge_lattice.rs`:

- `semantic_merge_plan_lean_json_v1(...)`;
- `semantic_rebase_plan_lean_json_v1(...)`.

They intentionally emit a reduced Lean-check shape rather than the full runtime
report. Full `SemanticMergePlanV1` / `SemanticRebasePlanV1` remains the
operator-facing report; the Lean shape is the current external conformance
contract and the candidate verifier interchange if promoted into `VerifyMain`.
The removed `axiograph sem` command family no longer exports this shape.
Library/tests serialize the reduced plan directly for the focused checker; any
future operator adapter must resolve refs through AxiStore.

Run the focused conformance path with:

```bash
make verify-lean-semantic-vcs
```

That target builds `axiograph_semantic_vcs_check`, runs the Rust semantic-merge
unit suite, accepts the clean merge/rebase fixtures, and requires rejection of
conflicting, blocked, failed-transport, cross-lineage, dropped-ref, and
missing-target fixtures. It does not create filesystem semantic refs or invoke
the removed broad semantic CLI.

The fixture set is complete only for those named finite conformance cases. It
does not cover every field of `SemanticMergePlanV1` or
`SemanticRebasePlanV1`, prove resolver-policy or competency-question
preservation, or participate in the `VerifyMain` certificate boundary.

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

- Extend the current semantic VCS conformance checker from reduced example
  payloads to canonical `CompiledKernelSnapshot` manifests plus declared
  `RuntimeIrRef` citations, then check query/CQ/merge anchor consistency.
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

The Rust runtime report currently makes no completeness or ontology-closure
claim. Its tier names scope an admissibility scan:

- `finite_fragment`: classify the compiled finite obligation list;
- `evidence_weighted`: optionally propagate finite weights and threshold the
  same list without erasing semantic residuals; and
- `global_indexed`: annotate the scan with a caller-declared finite
  ref/world/slice/import universe and reject declared unknown imports.

A numeric evidence propagation may itself stop changing; that is not a theory
fixpoint. Honest closure requires a derivation state, applicability semantics,
termination/fuel, and replayable derived artifacts. The new finite Lean module
provides that contract only for generator reachability.

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

The Lean `SemanticOperationKind` and `OperationPreservationClaim` vocabulary is
the first common finite conformance surface for proving these
operation-specific statements.

## Concrete Lean Roadmap

1. **Finite slice certificate.** Export `SemanticSliceManifestV1` and
   `SemanticMergePlanV1` to Lean JSON, then check conservative join/ref
   preservation and materialization blockers using
   `checkMergePlanJson` / `checkMergePlanMaterialization`.
2. **Compiled-snapshot declaration check.** Export the canonical snapshot
   manifest and derived citation set to Lean; reject query/merge/CQ certificates
   that cite undeclared refs or a mismatched immutable snapshot handle.
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
