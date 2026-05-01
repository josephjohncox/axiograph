# Ontology Engineering Roadmap (execution loop + quality gates)

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap turns ontology engineering in Axiograph into an operational
program, not a generic checklist.

Main connective insight:

> The missing abstraction is not “more validators”. It is one first-class
> evolution-preview contract that every ontology-changing workflow reuses:
> proposal review, typed olog authoring, migration preview, semantic merge, and
> accepted-plane promotion.

That contract must be anchored to accepted meaning, CQ-aware, trust-explicit,
and persistable under semantic history.

This roadmap is aligned with the repo’s current architecture and trust boundary:

- canonical `.axi` is the reviewable meaning plane,
- accepted snapshots are the source of semantic truth,
- PathDB and `.axpd` are derived execution substrates,
- Lean is the trusted checker for the current narrow kernel slice,
- Rust provides lifecycle, anchors, previews, and fail-closed workflow guardrails,
- evidence-plane artifacts remain untrusted until reviewed and promoted.

Companion docs:

- `docs/reference/TRUSTED_KERNEL.md`
- `docs/reference/KERNEL_IR.md`
- `docs/reference/RUST_LIFECYCLE_TYPES.md`
- `docs/reference/SEMANTIC_VCS.md`
- `docs/reference/QUERY_LANG.md`
- `docs/howto/SNAPSHOT_STORE.md`
- `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md`

For math-first theory and references, see `docs/explanation/BOOK.md`.

---

## 0. North Star

Treat ontology evolution like production code review:

1. start from a stable accepted snapshot anchor,
2. gather evidence or author a candidate delta,
3. preview the semantic effect before mutation,
4. gate the change with CQs, typing, and quality checks,
5. persist review artifacts and lineage in semantic history,
6. promote explicitly into the accepted plane,
7. rebuild/query derived substrates from accepted meaning.

The operative target is not “a perfect ontology”. It is:

- reviewable semantic deltas,
- explicit trust and scope claims,
- reproducible promotion,
- and explainable regression handling.

### 0.1 Runtime usefulness bar

This roadmap is not only about better ontology curation. It is about shipping a
runtime-usable ontology checker whose outputs are concrete enough to drive
business-rule review, coding-agent workflows, and ontology-driven development.

Near-term ontology-engineering surfaces should converge on a small report family:

| Report family | Normal producers | Must answer | Minimum contract |
| --- | --- | --- | --- |
| `EvolutionPreviewV1`-style review object | proposal preview, promotion preview, migration preview, merge/reconciliation preview | what changed semantically before mutation? | typed `schema` / `theory` / `instance` / `context` deltas, CQ before/after, trust, residual obligations |
| Business-rule applicability report | runtime checker, authoring review, implementation review, merge/promotion review | which accepted/review-state rules apply here? | matched rule/CQ ids, anchors, world/context scope, trust class, checked surfaces, next actions |
| Semantic coverage / drift report | runtime checker, implementation review, interop review | what is mapped, what is weak, and what is missing? | covered ontology objects/rules/CQs, uncovered areas, drift/gap reasons, anchors, trust/caveats |
| Agent-facing semantic report | REPL, server, tool-loop, review surfaces | what can an agent defensibly claim now? | proposition/task, matched ontology objects, anchors, trust contract, evidence/checks used, residual unknowns, next actions |

If a new ontology-engineering surface cannot say which report family it emits,
it is not yet aligned with this roadmap.

---

## 1. Non-Negotiables

These rules apply to every workstream in this document.

- `.axi` remains the canonical, reviewable meaning plane.
- Accepted snapshots, not PathDB byte layout, define semantic identity.
- Trust contracts are about soundness, coverage, scope, and anchors.
- Trust contracts must not imply completeness of answers or full ontology closure.
- Evidence-plane artifacts (`proposals.json`, `chunks.json`, LLM/world-model outputs) remain untrusted by default.
- No workflow may silently write evidence-plane outputs into accepted meaning.
- Semantic history must preserve explicit lifecycle transitions, not hide them behind mutable state.
- CQ regression handling must become a default gate for ontology-changing operations, not a best-effort report.
- Plan new ontology-engineering surfaces under a **greenfield default**:
  - do not preserve superseded authoring/query/review payloads by default,
  - do not carry parallel semantic vocabularies indefinitely,
  - and do not keep old workflow shapes alive unless they preserve trust-critical
    anchors or a defended migration path.
- When compatibility is kept, scope it explicitly:
  - compatibility is allowed for soundness/trust-preserving migration,
  - for anchor-preserving export/import,
  - or for short-lived operator migration windows with a defined removal target.
- Prefer explicit migration/drop-compat plans over silent adapter accretion:
  - new canonical contracts should replace old defaults,
  - old surfaces should become migration/import adapters or be removed,
  - and roadmap language should say when a superseded path is temporary rather than
    "supported" in the open-ended sense.

---

## 2. Current Implemented Substrate (2026-04)

The roadmap should build on the slices that already exist today.

| Slice | Current status | Why it matters |
| --- | --- | --- |
| Query trust contract | `query_ir_v1`, prepared queries, REPL, LLM tools, and `/query` expose `trust_class`, `soundness`, `coverage`, `scope`, plus explicit non-claims such as `completeness_claim = not_claimed` and `ontology_closure_claim = not_claimed` | This is the model for user-facing soundness language |
| Runtime theory checker | `RuntimeTheoryCheckReportV1`, `axiograph check theory`, `axiograph discover theory-check`, `/semantic/theory-check`, and `semantic_theory_check` expose scoped well-typedness, admissibility, closure, and completeness judgments over compiled `TheoryIr` | This is the model for theory obligations, fDDD invariants, behavior-case specs, and merge/rebase gates |
| Proposal preview validation | `proposals_validate` already does preview import, meta-plane typecheck, quality delta, optional CQ before/after reporting, and `fail_on_regression` / `fail_on_unsatisfied_after` policy | This is the first real CQ-gated evolution slice |
| Accepted-plane promotion preview | reviewed `.axi` modules can already be previewed against the current accepted snapshot with quality delta, optional CQ gate, stored validation report, and a trust narrative | This proves accepted-plane mutation can be gated before commit |
| Typed authoring draft path | `draft_axi_from_proposals` already distinguishes `draft_only` from `validated` canonical drafts and can surface an `axi_well_typed_proof_v1` summary | This is the first authoring surface that already speaks typing and review |
| Semantic history store | `sem/` layout exists, including `sem/world_model_runs/` and `sem/validations/`; semantic commit structs exist in the accepted-plane layer | This is enough scaffolding to stop inventing one-off lineage paths |
| Accepted plane + PathDB WAL split | accepted snapshots plus PathDB WAL/checkpoints are live, with explicit promotion and `pathdb-commit` flows | This is the core evidence-plane to accepted-plane operating model |

The roadmap below should close the gaps between these slices rather than invent a parallel workflow vocabulary.

The practical instruction for implementation is:

- reuse the existing trust / preview / semantic-summary seams as the starting point;
- widen them to business-rule and coverage usefulness;
- and resist introducing new one-off payload vocabularies unless they are clearly
  temporary migration adapters.

---

## 3. One Operational Loop

Every ontology change, regardless of source, should converge on the same loop.

### 3.1 Target loop

1. **Anchor the base world**
   - Start from an `AcceptedSnapshotId`.
   - When relevant, carry the derived `PathdbSnapshotId` too, but do not treat it as the semantic anchor.

2. **Create or collect a candidate**
   - Evidence-plane proposals (`proposals.json`)
   - Drafted canonical `.axi`
   - Olog fragment
   - Schema migration / merge candidate
   - Review branch delta

3. **Build an evolution preview**
   - Compare the base accepted snapshot against the candidate target.
   - Produce structured deltas across `schema`, `theory`, `instance`, and `context`.
   - Run quality checks and CQ before/after evaluation.
   - State an explicit trust contract and non-claims.

4. **Persist review artifacts**
   - Store the preview report under `sem/validations/`.
   - Attach proposal digests, world-model run ids, or source refs when relevant.
   - Make the preview report the review unit, not an incidental log line.

5. **Review on semantic history**
   - Review on `review/*`, `evidence/*`, or `wm/*` branches as appropriate.
   - Record lifecycle transitions explicitly.

6. **Promote or reject**
   - Promotion mutates the accepted plane and emits semantic history objects.
   - Rejection leaves the candidate in the evidence/review plane with diagnostics intact.

7. **Rebuild derived execution**
   - Rebuild/query PathDB or export interop views from the new accepted meaning.

### 3.2 Target evolution-preview contract

The repo now has multiple partial preview reports. They should converge on one
conceptual contract, regardless of entrypoint.

Minimum fields:

| Field | Meaning |
| --- | --- |
| `base_accepted_snapshot_id` | semantic source anchor |
| `candidate_kind` | `proposal_overlay`, `canonical_axi_module`, `olog_fragment`, `migration`, `semantic_merge` |
| `candidate_ref` | digest/path/id of the reviewed candidate |
| `delta.schema` | added/removed/changed schema objects, arrows, relation-objects, context axes |
| `delta.primitives[]` | explicit structural moves such as `reify_relation_object`, `introduce_dependent_relation_family`, `transport_along_schema_morphism`, `introduce_subtype`, `generalize_to_supertype`, `specialize_to_subtype`, `push_relation_role_to_subtype`, `pull_relation_role_to_supertype`, `factor_common_structure_to_supertype`, `split_type_into_subtypes`, `merge_types_under_supertype`, `lift_relation_to_carrier`, `add_path_equation`, `add_rewrite_rule`, `resolve_conflict_by_decision` |
| `delta.theory` | changed constraints, path equations, rewrite rules, opaque equations |
| `delta.instance` | facts/entities added, removed, or retyped |
| `delta.context` | world/context scoping additions, removals, or reinterpretations |
| `quality_delta` | only new findings introduced by the candidate |
| `cq.questions[]` | per-question before/after rows, satisfaction status, trust class, and reasons |
| `trust` | soundness/coverage/scope/reasons for the preview itself |
| `migration_obligations[]` | preserved/generated/dropped/unsupported transport obligations |
| `transport_basis` | named schema/theory map or other typed transport basis used by the preview |
| `reindexing_links[]` | source/target semantic ids that remain comparable across the evolution |
| `reconciliation_decisions[]` | typed conflict-set and decision inventory when the candidate is a semantic merge |
| `exploration_next_actions[]` | directed follow-on work suggested by the structural change: CQs to rerun, sibling relations to inspect, candidate shared supertypes, roles to re-home, code/docs/tests likely affected |
| `stored_report_path` | semantic-history location of the persisted report |

The important change is not merely storing more data. It is making the same
review object appear at every ontology mutation seam.

### 3.3 Structural evolution primitives

Ontology review gets materially better when the preview can say what kind of
structural move is being attempted, not only what strings changed.

The minimum structural primitive set should be:

- `reify_relation_object`
- `introduce_dependent_relation_family`
- `transport_along_schema_morphism`
- `introduce_subtype`
- `generalize_to_supertype`
- `specialize_to_subtype`
- `push_relation_role_to_subtype`
- `pull_relation_role_to_supertype`
- `factor_common_structure_to_supertype`
- `split_type_into_subtypes`
- `merge_types_under_supertype`
- `lift_relation_to_carrier`
- `add_path_equation`
- `add_rewrite_rule`
- `resolve_conflict_by_decision`

Why these specifically:

- relation-object reification and dependent/indexed-family primitives keep the
  categorical and dependent-type story operational rather than leaving it in
  docs only;
- subtype/generalize/specialize make the subtype lattice reviewable as a typed
  design object rather than a byproduct of renaming;
- push/pull relation role and lift relation to carrier preserve the repo's
  relation-as-object semantics and make role movement explicit before it causes
  query or migration drift;
- path-equation and rewrite-rule primitives make theory evolution explicit
  rather than mixing semantic laws into free-form notes;
- factor/split/merge cover the most common ontology refactorings that change CQ
  behavior and implementation mapping even when the vocabulary looks familiar.

Each primitive should carry:

- stable refs for the affected objects, relation-objects, and roles,
- a short rationale,
- migration obligations produced by the move,
- and directed exploration hints so the reviewer knows what to inspect next.

Directed exploration is part of the usefulness bar, not extra UI polish. If the
preview says "generalize `BoilerPump` and `CoolingPump` into `Pump`", the
runtime checker should also be able to suggest:

- sibling relations whose source/target types should be reconsidered,
- CQs likely to gain or lose answers,
- code/test/docs surfaces mapped to the changed semantic ids,
- and candidate follow-on edits such as "push maintenance interval onto the new
  supertype" or "lift `delivers_to` into `Delivery(...)` because time/carrier
  provenance is now in scope".

---

## 4. Workstream A: CQ-Gated Evolution

### Goal

Make competency questions the default behavioral gate for ontology evolution.

### Current slice

Implemented today:

- proposal preview validation can compare before/after CQ status,
- preview policy already supports `fail_on_regression`,
- preview policy already supports `fail_on_unsatisfied_after`,
- per-question deltas already include rows, satisfaction status, trust class, and reasons,
- accepted-plane promotion preview can already evaluate CQ effects before mutation.

### Gaps to close

- [ ] Reuse one evolution-preview object across:
  - proposal review,
  - accepted-plane promotion preview,
  - migration preview,
  - semantic merge/reconciliation preview.
- [ ] Make subtype/supertype evolution reviewable through explicit primitives instead of relying on before/after diff inference.
- [ ] Treat CQ suites as named review assets, not ad hoc JSON passed only at runtime.
- [ ] Preserve expected answer-shape references with the CQ asset:
  - exact set,
  - subset,
  - minimum rows,
  - or shape/type expectations.
- [ ] Include CQ trust deltas, not just row counts:
  - `certifiable -> mixed`,
  - `mixed -> execution_only`,
  - or the reverse.
- [ ] Persist CQ preview reports under `sem/validations/` and reference them from semantic commits.
- [ ] Require a CQ policy decision on merge/promotion paths rather than silently defaulting to permissive behavior.

### Operational deliverables

- [ ] Define a first-class `EvolutionPreviewV1` or equivalent report shape that subsumes current proposal and promotion preview reports.
- [ ] Make `EvolutionPreviewV1` carry rich structural primitives rather than only bucketed add/remove/change counts.
- [ ] Add one stable storage convention for CQ-bearing preview reports under `sem/validations/`.
- [ ] Add a CQ suite manifest location in the repo for reusable acceptance suites.
- [ ] Make every ontology-changing CLI/server surface accept the same CQ policy and emit the same per-question result structure.
- [ ] Emit `exploration_next_actions[]` from preview generation so authoring/review tooling can drive directed follow-up inspection instead of only printing deltas.

### Exit criteria

- A reviewer can answer “what domain questions changed?” from one persisted preview object.
- Merge and promotion can fail closed on CQ regression without custom wiring per surface.
- CQ behavior is visible in review history, not only in transient CLI output.

---

## 5. Workstream B: Typed Olog Authoring

### Goal

Provide an authoring surface that is intelligible to ontology authors but still
lowers into the canonical `.axi` and kernel-IR story.

### Current slice

Implemented today:

- evidence-plane proposals can be drafted into canonical `.axi`,
- `draft_axi_from_proposals` can distinguish plain rendered drafts from validated drafts,
- authoring responses can already include a Rust-side well-typedness summary.
- checked olog fragments now emit a compiled-IR-derived typed change summary and
  evolution preview with:
  - relation-object reification primitives,
  - indexed/dependent-family primitives over context/temporal/fiber roles,
  - path-equation primitives,
  - subtype-specialization primitives induced by actual box bindings,
  - and directed exploration next actions.
- migration preview builders now emit transport-along-morphism, transported
  path-equation, subtype-collapse, and merge-image primitives from real
  `SchemaMorphismV1` inputs plus residual transport obligations.
- reconciliation preview builders now emit typed conflict/decision previews from
  persisted reconciliation records without overstating merge completeness.
- accepted-plane reconciliation can now persist a stored reconciliation preview
  report under `sem/validations/` and emit a `SemCommitKindV1::Merge` semantic
  commit carrying typed delta/trust/rule/coverage summaries plus the cited
  preview path.
- compiled-IR directed exploration now emits relation-object, indexed-family,
  carrier-lift, rewrite-candidate, and subtype-factor opportunities beyond
  hand-authored olog fragments.

### Target behavior

Typed olog authoring should let authors create:

- object boxes,
- aspects/arrows,
- relation-objects for n-ary facts,
- commutative/path-equality obligations,
- context/time axes,
- CQ attachments,
- and provenance/evidence links.

The canonical output should be a review bundle, not merely editor state.

### 5.1 Usefulness bar for typed authoring

Typed authoring is only valuable if it answers operational questions while the
author is still editing, not only after a later batch validation pass.

The useful questions are:

- what semantic object or arrow did I just create or change?
- what accepted/review anchor am I editing against?
- which business rules, rewrite obligations, or CQs are now in scope?
- what changed in schema/theory/instance/context terms?
- what is well-typed now, what is only draft-shaped, and what is still
  unsupported?
- what trust class applies to this draft or preview, and what does it *not*
  claim?

Typed authoring therefore should not stop at "diagram editor plus export". It
should behave like a semantic IDE over canonical meaning.

### 5.2 Typed olog authoring modes

The authoring surface should support several distinct modes, because ontology
work is not only greenfield box-and-arrow editing:

- **Greenfield modeling**
  - create new object boxes, aspects, relation-objects, and context axes
    against an empty or skeletal accepted baseline.
- **Schema extension**
  - add new objects/arrows/rules to an existing accepted or review-state
    ontology while preserving stable ids and migration obligations.
- **Theory enrichment**
  - add path equations, rewrite rules, disjointness, cardinality, or business
    constraints to already-modeled structure.
- **Context/world refinement**
  - make scope explicit rather than hiding it in labels or comments:
    regulatory world, tenant, environment, temporal phase, or evidence scope.
- **Repair-driven editing**
  - start from quality/CQ/rule failures and author the minimal semantic delta
    that resolves them.

### 5.3 Ontology-driven development from typed authoring

Typed olog authoring should feed implementation work directly. The intended flow
is not:

> author ontology now, maybe use it later.

The intended flow is:

> author or refine ontology, preview the semantic delta, then drive code/tests/
> workflows/migrations from the resulting typed obligations.

That means the authoring bundle should be able to produce:

- business-rule checklists for APIs, workflows, reports, and jobs,
- candidate CQ suites or CQ deltas,
- migration preview obligations,
- code/test generation hints anchored to semantic ids,
- and semantic diff summaries that tell engineers what changed in business
  meaning, not only text.

### Required review bundle contents

- canonical `.axi` delta or draft text,
- stable object ids and arrow ids,
- schema/theory/instance/context change summary,
- source accepted snapshot anchor,
- evidence links and proposal digests,
- CQ attachments and before/after preview results,
- trust contract,
- repair-oriented typing diagnostics when validation fails.

### Gaps to close

- [ ] Define an olog-fragment serialization that can round-trip to canonical `.axi`.
- [ ] Make relation-as-object + projection arrows the default authoring model, not a hidden backend detail.
- [ ] Preserve explicit context/time roles and stable ids during lowering.
- [ ] Attach CQ references and evidence links directly at authoring time.
- [ ] Make editor, CLI, and server authoring surfaces return the same response contract.
- [ ] Add schema-aware completion and repair-oriented authoring diagnostics over:
  - object kinds,
  - aspect targets,
  - relation roles,
  - context axes,
  - and candidate theory obligations.
- [ ] Make typed authoring return stable semantic ids for:
  - objects,
  - arrows,
  - relation-objects,
  - roles,
  - theory rules,
  - and context/world discriminants.
- [ ] Add authoring modes for:
  - greenfield modeling,
  - extension of accepted structure,
  - theory/rule enrichment,
  - and context/world refinement.
- [ ] Make typed authoring produce ontology-driven-development artifacts:
  - business-rule checklists,
  - CQ deltas,
  - migration obligations,
  - backend projection obligations for advanced graph stores,
  - and code/test generation hints anchored to semantic ids.
- [ ] Keep advanced graph database compatibility above the storage layer:
  - authoring, ologs, CQ gates, semantic VCS refs/commits, and trust contracts
    stay in Axiograph,
  - graph databases receive anchored projections/materializations from accepted
    semantic state,
  - and backend-local schema or branch concepts do not replace semantic
    lifecycle state.
- [ ] Add one typed backend-projection plan per target materialization:
  - selected semantic ref / accepted snapshot anchor,
  - compiled IR digest,
  - target capability profile,
  - relation-object vs edge projection decisions,
  - context/world mapping,
  - and drift/round-trip caveats.
- [ ] Restrict “generic graph database support” to advanced engines that can
  actually preserve typed ontology structure or at least host disciplined
  projections:
  - `TypeDB` first for typed relation/role/n-ary semantics,
  - `TerminusDB` next for RDF/VCS-shaped graph projection,
  - and only then other advanced RDF/quad or constrained property-graph
    systems when their capability profiles justify it.
- [ ] Use backend-native strengths deliberately instead of flattening them into
  one compatibility story:
  - push rich type/constraint/query-validation fragments into `TypeDB`,
  - import TypeDB-style interface typing into the canonical IR:
    scoped role interfaces, subtype-inherited admissible players, and explicit
    single-change schema evolution/redefinition discipline,
  - push mirrored branch/history/diff collaboration views into `TerminusDB`,
  - treat property-graph execution/indexing as experimental until a backend
    clears the same long-term compatibility bar,
  - and keep semantic meaning, olog authoring, CQ gates, trust contracts, and
    lifecycle state in Axiograph above every backend.
- [ ] Treat backend-native VCS/history as advisory projection infrastructure,
  not ontology authority:
  - use it where it helps review and collaboration,
  - keep native backend query/read surfaces available for external tools as
    read-only projected views,
  - but keep Axiograph semantic refs/commits as the source of truth,
  - especially when a backend's native sync/history model does not transport
    schema evolution with the same guarantees as instance data.
- [ ] Make implementation review aware of backend projections:
  - semantic coverage should say which accepted rules/CQs are satisfied in the
    canonical semantic layer only,
  - which are also materialized in a target graph backend,
  - and where backend capabilities forced a weaker projection.
- [ ] Keep authoring trust contracts explicit about scope:
  - draft-only typing,
  - runtime-checked preview,
  - or certifiable/certified subset;
  - and continue to state `completeness_claim = not_claimed` and
    `ontology_closure_claim = not_claimed` unless a stronger claim is defended.

### Exit criteria

- An author can add or edit an olog fragment and immediately see:
  - the canonical `.axi` delta,
  - the typing status,
  - the CQ impact,
  - the trust contract,
  - and the supporting evidence links.
- The same authoring bundle can drive both ontology review and downstream
  implementation review without re-deriving business meaning from prose.

---

## 6. Workstream C: Uniform Trust Contracts

### Goal

Use one trust-language family across all user-facing ontology results.

### Current slice

Implemented today:

- query-facing surfaces already expose explicit trust fields,
- query trust surfaces explicitly state `completeness_claim = not_claimed`,
- query trust surfaces explicitly state `ontology_closure_claim = not_claimed`,
- proposal preview and promotion preview already emit trust metadata,
- proposal/promotion previews now also emit a runtime semantic summary:
  - visible rule inventory,
  - typed/quality/CQ coverage surfaces,
  - current review-only/runtime gaps,
  - and explicit non-claims about completeness/ontology closure,
- certificate endpoints already emit a trust contract for the certificate payload.

### Gaps to close

- [ ] Converge preview and promotion trust contracts onto the same field structure used by query/certificate surfaces where possible.
- [ ] Add anchor-bearing trust fields to preview, migration, authoring, and semantic-merge responses.
- [ ] Make query and authoring trust meta-aware rather than only query-shape-aware:
  - surface which business-rule/ontology claims were in scope,
  - which ones were runtime-used vs review-only,
  - and which gaps remain unsupported or outside the certified subset.
- [ ] Distinguish clearly between:
  - trust of a returned answer,
  - trust of a preview delta,
  - trust of a migration witness,
  - trust of a semantic merge decision.
- [ ] Keep explicit non-claims present in every user-facing trust narrative.

### Standard fields

All user-facing ontology-dependent outputs should converge on:

- `trust_class`
- `soundness`
- `coverage`
- `scope`
- `reasons`
- `anchors`

Answer-like outputs should also include:

- `claim_scope`
- `completeness_claim`
- `ontology_closure_claim`
- `notes`

### 6.1 Strong vs weak claims across versioned ontologies and worlds

Trust language becomes misleading if it is attached only to the output value and
not to the version/world in which that value was established.

Ontology-dependent claims should normally be read as:

> under ontology anchor `A`, context/world scope `W`, and trust contract `T`,
> proposition `P` holds with strength `S`.

The important operational distinction is not "certain vs uncertain". It is:

- accepted and certified under explicit anchors,
- accepted and execution-checked under explicit anchors,
- review-preview only,
- evidence-grounded but not semantically checked,
- or speculative/proposed.

This matters because ontology evolution often changes *how strong* a claim is
even when the surface proposition still "looks true".

Examples:

- a previously certifiable CQ answer becomes mixed after a schema change,
- a business rule remains accepted on `main` but is only preview-valid on
  `review/billing`,
- a proposition is valid in one regulatory world but not in another,
- or a migration preserves rows but weakens the semantic comparability claim.

### 6.2 Required claim-strength hygiene

Strong and weak claims should therefore be a structured part of the trust
contract family, not informal reviewer commentary.

- [ ] Add a shared claim-strength vocabulary for ontology-facing outputs:
  - `accepted_certified`
  - `accepted_execution_checked`
  - `review_preview`
  - `evidence_grounded`
  - `speculative`
- [ ] Require every strong claim to carry:
  - ontology/snapshot anchor,
  - branch/ref when relevant,
  - context/world scope,
  - trust class,
  - soundness clause,
  - coverage clause,
  - and residual unknowns.
- [ ] Add strengthening/weakening detection across versions/worlds:
  - same proposition, different trust class,
  - same CQ, different coverage,
  - same business rule, different scope or applicability.
- [ ] Ensure user-facing trust narratives say what changed without implying:
  - global truth,
  - exhaustive answer completeness,
  - or full ontology closure.

### Exit criteria

- A user does not have to infer trust semantics from ad hoc booleans.
- Query, preview, and promotion surfaces use a recognizably shared contract family.
- No trust payload implies completeness or ontology closure unless that stronger claim is explicitly justified.

---

## 7. Workstream D: Migration Preview and Witnesses

### Goal

Make schema and theory evolution reviewable before accepted meaning changes.

### Current slice

Implemented today:

- `delta_f_v1` exists as a recompute-and-compare migration scaffold,
- promotion preview already compares current accepted state against a candidate module,
- typed-ontology docs already define the required behavioral shape for useful migration preview.

### Required preview semantics

A migration preview must make these questions answerable before promotion:

- what schema structure is preserved,
- what theory obligations are preserved, generated, weakened, or unsupported,
- what instance facts are transported, dropped, or require reinterpretation,
- what context/world semantics were restricted, reified, or projected,
- what CQs regress or improve,
- what trust contracts change,
- what obligations remain unresolved.

### Deliverables

- [ ] Define `migration_witness_v1` as a first concrete preview/witness report for ontology evolution beyond path/query certificates.
- [ ] Classify migration effects by layer:
  - `schema`
  - `theory`
  - `instance`
  - `context`
- [ ] Distinguish transport outcomes:
  - `preserved`
  - `generated`
  - `dropped`
  - `unsupported`
  - `requires_review`
- [ ] Make migration preview name both:
  - the transport basis (`SchemaMorphismV1` or later richer schema/theory map),
  - and the reindexing links that preserve semantic comparability across anchors.
- [ ] Attach CQ before/after reporting and trust deltas to migration preview.
- [ ] Route migration preview through the same stored validation path as proposal and promotion preview.
- [ ] Fail closed on unresolved required obligations before accepted-plane mutation.
- [ ] Treat migration as the place where compatibility is made explicit rather than assumed:
  - if an old ontology/module/rule shape is being replaced, the preview should
    say what is preserved,
  - what is intentionally dropped,
  - what requires operator/data/code migration,
  - and what trust strength changes across the boundary.
- [ ] Avoid permanent dual-semantic support where one reviewed migration will do:
  - prefer one target canonical shape plus a reviewable transport report,
  - not an indefinite promise to support old and new semantics equally.

### Exit criteria

- Schema evolution is never described as “safe” without a typed impact report.
- Migration preview produces a stored artifact that can be cited during review and merge.
- `delta_f_v1` is no longer an isolated proof story; it participates in operator-facing evolution review.
- Migration becomes the explicit mechanism for retiring old semantic shapes while
  preserving only the trust-relevant continuity that is actually defended.

---

## 8. Workstream E: Semantic VCS as the Review Backbone

### Goal

Make ontology evolution a first-class history workflow instead of a sequence of
opaque accepted-plane mutations.

### Current slice

Implemented today:

- `sem/` layout exists,
- `sem/world_model_runs/` is already used,
- `sem/validations/` already exists as a persistence seam,
- semantic commit structs already exist in the accepted-plane layer,
- accepted-plane promotion already has enough metadata to reference validation artifacts.

### Required branch and object model

The target branch vocabulary remains:

- `refs/heads/main`
- `refs/heads/review/<topic>`
- `refs/heads/evidence/<source>`
- `refs/heads/wm/<experiment>`
- `refs/tags/<release>`

Required persisted objects remain:

- semantic commits,
- reconciliation objects,
- world-model run records,
- validation/preview reports.

### Gaps to close

- [ ] Emit semantic commit objects for accepted-plane promotions as the normal history unit.
- [ ] Persist validation report refs in semantic commits and use them as review evidence.
- [ ] Add ancestry-aware semantic history for review branches and merges.
- [ ] Make reconciliation objects carry conflict sets, decisions, attached certificates, and typed layer classification usable directly by preview/report tooling.
- [ ] Treat merge as typed reconciliation over semantic deltas rather than as commit ancestry plus free-form notes.
- [ ] Link world-model runs, proposal digests, validations, and resulting promotions through semantic history rather than loose filenames.
- [ ] Stop treating `sem/` as scaffolding only; make it the review audit trail for ontology change.

### Exit criteria

- A reviewer can trace any accepted ontology change back to:
  - its base snapshot,
  - its candidate artifacts,
  - its preview report,
  - its policy decision,
  - and its promotion commit.

---

## 9. Workstream F: Evidence Plane to Accepted Plane

### Goal

Make the evidence-to-meaning transition explicit, typed, and hard to bypass.

### Approved operating model

1. **Ingest or generate evidence**
   - Source artifacts become `proposals.json`, `chunks.json`, or world-model output.

2. **Optionally preserve evidence in the PathDB WAL**
   - Use WAL overlays for discovery, retrieval, grounding, and authoring assistance.
   - Do not treat these overlays as accepted meaning.

3. **Preview and validate**
   - Run proposal preview validation against the anchored current snapshot.
   - Record typing errors, quality delta, CQ deltas, and trust metadata.

4. **Draft canonical meaning**
   - Draft canonical `.axi` or a typed review bundle from the evidence candidate.

5. **Preview the would-be accepted state**
   - Compare the current accepted snapshot with the candidate reviewed module or migration delta.
   - Persist the preview report.

6. **Review on semantic history**
   - Review from `review/*` or `evidence/*`, not from a transient local file alone.

7. **Promote explicitly**
   - Promotion creates a new accepted snapshot and semantic-history event.

8. **Rebuild derived execution**
   - PathDB snapshots, viz, query services, and interop exports all rebuild from the accepted plane.

### Blocked transitions

These flows should remain impossible or explicitly unsupported:

- evidence-plane proposal bundle directly mutates accepted `.axi`,
- world-model run skips preview and writes to accepted meaning,
- migration bypasses typed preview and only shows resulting runtime state,
- review/promotion proceeds without a persisted validation artifact when policy requires one.

### Concrete next steps

- [ ] Require proposal bundles consumed by review tooling to carry accepted snapshot and evidence anchors.
- [ ] Require drafted `.axi` changes to carry provenance/evidence links into review.
- [ ] Make the accepted-plane promotion preview artifact the normal handoff from authoring to promotion.
- [ ] Thread the same lifecycle object model through REPL, server, and automated agent paths.
- [ ] Make promotion/review tooling say when a change is:
  - additive,
  - replacing a prior semantic shape,
  - or intentionally dropping a superseded path.
- [ ] Do not default to preserving older review/import payloads once the new
  canonical preview/review contract exists:
  - keep importers/exporters only where needed for migration or trust-preserving
    audit trails,
  - otherwise remove old defaults.

### Exit criteria

- There is one recognizable ladder from evidence to accepted meaning.
- Every step has an artifact, an anchor, and a review surface.
- High-value answers and promotions no longer depend on implicit operator memory.

---

## 10. Workstream G: Ontology-Engineering Best Practices

The classical ontology-engineering backlog remains important, but it should be
pulled into the execution loop above rather than live as isolated aspirations.

### 10.1 Requirements and CQ assets

- [ ] Add first-class CQ suite assets in the repo with stable names, expected answer-shape references, and context/snapshot assumptions.
- [ ] Add CI-friendly CQ execution for canonical examples and ontology regressions.
- [ ] Make CQ failure output explainable in terms of derivation, missing typing, or unsupported trust class.

### 10.2 Quality gates and pitfall scanning

- [ ] Add `axi lint` as a review-grade pitfall scanner with findings consumable by preview reports.
- [ ] Start with high-value ontology anti-patterns:
  - subtype cycles,
  - suspicious catch-all classes,
  - duplicate concept creation,
  - role inversion mistakes,
  - unsafe negation naming patterns,
  - missing disjointness or required constraints where policy demands them.
- [ ] Make quality delta reporting first-class in review and promotion history.

### 10.3 OntoClean-style taxonomy checks

- [ ] Add optional meta-properties for rigidity/identity/unity/dependence.
- [ ] Treat taxonomy-quality violations as review findings before attempting certificate status.
- [ ] Only promote a subset to the trusted checker when the semantics are crisp enough to defend.

### 10.4 Ontology design patterns

- [ ] Build a small pattern library that lowers to canonical `.axi` and relation-object IR:
  - n-ary relation / tuple object,
  - role pattern,
  - participation,
  - provenance/context,
  - part-whole variants.
- [ ] Make pattern instantiation produce reviewable canonical deltas, not hidden macro expansion only.

### 10.5 Modularization, metadata, and publishing

- [ ] Require module metadata for owner, purpose, version, dependencies, and provenance.
- [ ] Support reusable fragments and privacy/redaction workflows as reviewable module deltas.
- [ ] Tie changelog, supersession, and release tags to semantic-history objects rather than prose-only notes.
- [ ] Require release notes to say whether a change:
  - preserves prior semantic contracts,
  - introduces a reviewed migration,
  - or intentionally drops old authoring/query/rule shapes.

### 10.6 Alignment and mappings

- [ ] Represent alignment candidates with provenance and confidence in the evidence/review plane first.
- [ ] Distinguish alignment candidate from accepted mapping.
- [ ] Gate accepted mappings on satisfiability/policy checks and later migration preview.

### 10.7 OBDA and interop as boundary layers

- [ ] Keep RDF/OWL/SHACL/PROV import-export and OBDA rewriting as boundary layers, not the ontology kernel.
- [ ] Make mapping and validation artifacts reviewable and anchor-aware.
- [ ] Emit trust metadata that makes the boundary-layer status explicit.
- [ ] Align SHACL, RDF, and olog work on one internal semantic story:
  - SHACL shapes become validation/mapping artifacts over canonical relation-objects and projection arrows,
  - RDF remains an open-world boundary format with context/world structure preserved rather than erased,
  - and olog edits remain the human-facing authoring surface for the same objects and arrows.
- [ ] Require interop reports to say which parts are:
  - runtime-checked only,
  - accepted and reviewed but uncertified,
  - or Lean-certifiable for the supported fragment.

### 10.8 Checker-aware ontology engineering

The most useful near-term ontology-engineering surface is not "more theorem
proving everywhere". It is one **runtime-usable checker** in Rust that can be
called from authoring, query, migration, CQ, and interop workflows while
remaining explicit about its trust class.

That checker should be good at:

- business-rule lookup and applicability checking under accepted/review anchors,
- typed authoring validation and repair-oriented diagnostics,
- typed query elaboration and result-shape reporting,
- semantic coverage and drift reporting over implementation surfaces,
- CQ execution and expected-answer-shape comparison,
- and SHACL/RDF/olog preview alignment before review or promotion.

It should not be described as the trusted semantic authority. The trust split
must remain:

- Lean is authoritative for the certified semantic slice,
- Rust is the operational checker and authoring/query engine for the broader
  runtime-usable slice,
- and evidence-plane retrieval or heuristics remain explicitly weaker than both.

- [ ] Make every ontology-engineering surface return one checker/report family:
  - anchors,
  - stable semantic ids,
  - trust contract,
  - coverage statement,
  - findings/repairs,
  - and explicit non-claims.
- [ ] Make runtime checker output first-class review material rather than only
  transient CLI text or debug logs.
- [ ] Ensure authoring, query, CQ, migration preview, and interop validation all
  reuse the same typed status language instead of inventing local terms.

### 10.9 Ontology-driven development

Ontology engineering is most useful when it drives implementation work instead
of remaining a documentation sidecar.

Ontology-driven development in Axiograph should mean:

- accepted/review ontology objects define business concepts and obligations,
- theory/rule/CQ assets define what should hold operationally,
- semantic diffs explain what changed in business meaning across versions,
- and engineering surfaces consume those typed obligations directly.

The target is not automatic code generation everywhere. The target is a better
engineering contract:

- code reviews can ask which business-rule ids are affected,
- migrations can ask which semantic obligations weaken or strengthen,
- and agents can ask which claims are accepted, review-only, or still
  evidence-grounded.

- [ ] Add implementation-surface linkage from ontology objects/rules/CQs to:
  - APIs,
  - workflows,
  - reports,
  - jobs,
  - migrations,
  - and test suites.
- [ ] Make semantic diff report implementation impact:
  - what rule/check/CQ obligations changed,
  - which mapped code surfaces are affected,
  - and what new tests/reviews are required.
- [ ] Add ontology-driven-development artifacts generated from typed deltas:
  - test/checklist templates,
  - migration review checklists,
  - endpoint/report rule applicability summaries,
  - and agent-facing next-action hints.
- [ ] Ensure ontology-driven-development outputs inherit the same trust contract
  family:
  - anchor-scoped,
  - soundness/coverage explicit,
  - and without completeness/closure claims by default.

### 10.10 AI-assisted discovery and grounded axiomatization

AI assistance is useful when it helps surface candidate structure that humans
can review, not when it silently upgrades heuristics into accepted semantics.

In this roadmap, AI-assisted ontology discovery should mean:

- discovering candidate concepts, relations, scopes, and rules from grounded
  evidence,
- grouping them into reviewable olog fragments or canonical `.axi` deltas,
- previewing their typing/CQ/trust impact before review,
- and keeping them in the evidence plane until explicitly promoted.

Grounded axiomatization should mean:

> turning recurring grounded evidence and operational questions into explicit,
> reviewable semantic commitments.

It should not mean:

> treating model-generated text as ontology truth.

- [ ] Require every AI/world-model/LLM proposal set to carry:
  - source evidence refs,
  - accepted snapshot anchor,
  - context/world scope,
  - run/proposal digests,
  - and candidate target ontology objects when known.
- [ ] Group proposal streams into reviewable candidate units:
  - schema extensions,
  - theory/rule candidates,
  - CQ candidates,
  - olog fragments,
  - and migration obligations.
- [ ] Add grounded-axiomatization previews that report:
  - typing status,
  - CQ impact,
  - trust class,
  - coverage scope,
  - residual unknowns,
  - and why the candidate remains evidence-plane until review.
- [ ] Treat unsupported or weakly supported CQs as generators of candidate
  ontology work:
  - missing schema,
  - missing theory/rule structure,
  - missing context/world representation,
  - or missing implementation linkage.
- [ ] Require AI-assisted discovery surfaces to distinguish:
  - grounded evidence,
  - typed accepted meaning,
  - review-preview deltas,
  - and speculative suggestions.
- [ ] Keep AI-assisted trust claims honest:
  - soundness is about the preview/check performed,
  - coverage is about the candidate surface reviewed,
  - and neither completeness nor ontology closure is claimed by default.

### 10.11 Strong vs weak claims over ontology evolution

Ontology engineering needs a stable vocabulary for "what got stronger or weaker"
across accepted snapshots, review branches, and scoped worlds.

- [ ] Add claim-diff reports for propositions/rules/CQs across versions:
  - added,
  - removed,
  - strengthened,
  - weakened,
  - unchanged,
  - or scope-shifted.
- [ ] Distinguish at least these sources of weakening:
  - trust class downgrade,
  - coverage narrowing,
  - world/context restriction,
  - migration obligation left unresolved,
  - or fallback from typed checking to retrieval-only support.
- [ ] Make review/promotion surfaces say whether a change:
  - preserves prior claim strength,
  - intentionally weakens it,
  - or leaves the previous strength unknown.
- [ ] Preserve strong-vs-weak claim state in semantic history so version
  comparison does not depend on scraping prose or ad hoc notes.

### 10.12 Runtime checker and business-rule usefulness

The runtime checker should become the default operational answer surface for
business semantics outside the narrow certified fragment.

The useful question is not only:

> can the system certify this?

It is also:

> what can the system say now, under explicit anchors and scope, about business
> rules, applicability, residual gaps, and required next actions?

#### 10.12.1 What the runtime checker should answer

- which accepted or review-state business rules apply to this entity/action?
- which rules are currently runtime-checkable, Lean-certifiable, already
  Lean-certified, or only evidence-grounded?
- which world/context qualifiers matter?
- which CQ assets or implementation surfaces cover the rule?
- what is unknown because the ontology does not yet model it?

#### 10.12.2 Business-rule checking surfaces

The runtime checker should be usable over:

- APIs and endpoint handlers,
- workflow/state-transition steps,
- reports and metrics definitions,
- ingestion mappings and transformation jobs,
- migration previews,
- and semantic merge/reconciliation review.

#### 10.12.3 Coding-agent-usable outputs

Agents and operators should receive a structured report, not only prose:

- applicable rule ids,
- ontology/snapshot/world anchors,
- trust class, soundness, and coverage,
- implementation surfaces checked,
- residual unknowns and unsupported areas,
- and suggested next actions:
  - code change,
  - test change,
  - CQ addition,
  - ontology extension,
  - or manual review.

- [ ] Add first-class runtime business-rule applicability reports with:
  - matching rule ids,
  - scope qualifiers,
  - trust contract,
  - and residual-obligation summaries.
- [ ] Add runtime checker outputs for:
  - accepted state,
  - review candidate,
  - version diff,
  - and world/context-specific comparison.
- [ ] Make runtime checker reports storable under semantic history or review
  artifacts rather than remaining transient console output.
- [ ] Ensure business-rule checker surfaces do not overclaim:
  - runtime-checked is not the same as kernel-certified,
  - no claim of exhaustive rule discovery unless defended,
  - and no claim of full ontology closure by default.

---

## 11. Phasing

### Phase 0: unify what already exists

- [ ] Define the shared evolution-preview contract.
- [ ] Persist all preview reports under `sem/validations/`.
- [ ] Converge authoring, proposal preview, and promotion preview on one trust-language family.
- [ ] Require CQ policy to be carried explicitly through ontology-changing surfaces.
- [ ] Replace superseded review payloads as defaults once the shared contract lands;
  retain only explicit migration/audit adapters where justified.

### Phase 1: semantic review as the normal path

- [ ] Emit semantic commits for accepted promotions.
- [ ] Use review/evidence/world-model refs consistently.
- [ ] Link world-model runs, proposal digests, and validation reports through semantic history.
- [ ] Make promotion preview the standard handoff artifact from authoring/review to accepted mutation.
- [ ] Remove older one-off review/promotion report shapes from the default path
  once they are subsumed by the shared preview object.

### Phase 2: typed authoring and migration preview

- [ ] Add first-class typed olog/relation-object authoring bundles.
- [ ] Add `migration_witness_v1` and shared migration preview reporting.
- [ ] Route schema evolution and semantic merge through the same preview contract.
- [ ] Make typed authoring bundles feed ontology-driven-development artifacts:
  - business-rule summaries,
  - CQ deltas,
  - implementation checklists,
  - and migration obligations.

### Phase 3: best-practice maturity

- [ ] Add `axi lint`, reusable pattern instantiation, and richer CQ assets.
- [ ] Add publishing/release metadata on top of semantic history.
- [ ] Tighten accepted mapping, privacy, and interop workflows around the same review model.
- [ ] Add grounded AI-assisted discovery and axiomatization review bundles with
  explicit evidence anchors and non-claims.
- [ ] Add strong-vs-weak claim comparison across versions/worlds as a normal
  review artifact.
- [ ] Make runtime business-rule checking and coding-agent-facing semantic
  reports first-class engineering surfaces.

---

## 12. Axiograph For Coding Agents And Agentic Engineering

The ontology-engineering roadmap should explicitly serve implementation work,
not only ontology curation in isolation.

### 12.1 Why this matters

Coding agents need more than retrieval. They need a semantic operating model for:

- business rules,
- correctness obligations,
- semantic coverage,
- versioned policy/world changes,
- and distinctions between accepted truth, review-state candidates, and weak
  evidence-plane claims.

If Axiograph becomes useful here, it can act as:

- a typed business-rule registry,
- a queryable semantic/RAG layer with trust and anchor awareness,
- a change-impact engine across ontology/code/system revisions,
- and a type-directed programming substrate for implementation tooling.

### 12.2 Runtime-checker contract for engineering workflows

For coding agents and ontology engineers, the first operational requirement is a
Rust-side checker surface that is useful **before** and **outside** the narrow
Lean-certified subset.

That checker should answer questions like:

- which business rules apply to this API, workflow, batch job, or report?
- which of those rules are represented as accepted ontology/theory objects, and
  which are only inferred from weaker evidence?
- which claims are runtime-checked, which are Lean-certified, and which remain
  retrieval-backed only?
- what is the typed result shape of this query/authoring change/migration
  preview under the stated anchors and worlds?
- where is semantic coverage missing across code, tests, docs, interop shapes,
  or accepted ontology structure?

The trust contract has to stay explicit:

- the runtime checker in Rust is the default useful engineering surface for
  authoring/querying/coverage/business-rule preview,
- Lean remains authoritative only for the supported certified semantics,
- and agent/tool answers must preserve the distinction instead of collapsing
  everything into one "semantic confidence" number.

### 12.3 Minimum useful agent report

If Axiograph is genuinely useful to coding agents, the agent should be able to
request one structured semantic report family across REPL, tool-loop, and
server surfaces.

Minimum fields:

- the proposition, question, or engineering task,
- ontology anchors:
  - accepted snapshot,
  - branch/ref when relevant,
  - and context/world scope,
- the matched schema/theory/rule/CQ objects,
- trust class, soundness clause, and coverage clause,
- explicit non-claims:
  - `completeness_claim = not_claimed`
  - `ontology_closure_claim = not_claimed`
  - unless a stronger claim is actually defended,
- evidence and checks actually used,
- residual unknowns or unsupported areas,
- and suggested next actions:
  - code change,
  - test change,
  - CQ addition,
  - ontology change,
  - or manual review.

- [ ] Define one agent-facing semantic report family shared across:
  - REPL,
  - LLM tool-loop,
  - DB server,
  - and review/promotion previews.
- [x] First runtime/API slice:
  - `POST /semantic/agent-report` now returns a typed engineering report over a
    task, mapped implementation surfaces, matched rule ids/scope ids, semantic
    coverage, residual unknowns, and suggested next actions under the current
    accepted snapshot.
- [ ] Make agent reports preserve the distinction between:
  - accepted semantic fact,
  - review-preview result,
  - runtime-checked obligation,
  - evidence-grounded hypothesis,
  - and speculative repair suggestion.

### 12.4 Core work items

Canonical hard example for this workstream:

- a chemical plant where physics, optimization, ERP/MRP, vendor certification,
  pricing, delivery, reporting, PLC/HMI behavior, and operational procedures
  all co-evolve.

That example forces the ontology-engineering surface to become useful across:

- simulators and digital-twin models,
- PLC/control logic and HMI/operator workflows,
- APIs, jobs, reports, and pricing/ERP integrations,
- and wiki/SOP/certificate/document evidence.

If a proposed feature cannot explain how it helps that class of system co-evolve
under typed semantic control, it is probably still too abstract.

- [ ] Add a coding-agent query profile that can ask for:
  - applicable business rules,
  - required evidence/provenance,
  - expected state transitions,
  - CQ assets that cover the rule,
  - and accepted-vs-review-vs-evidence status.
- [ ] Make the runtime checker return typed rule/application reports over:
  - accepted snapshots,
  - review candidates,
  - version deltas,
  - and scoped worlds/contexts.
- [ ] Add semantic coverage reporting:
  - which accepted rules are exercised by code/tests,
  - which ontology objects/arrows are not mapped into implementation,
  - which CQs have operational enforcement,
  - where semantic drift exists between ontology and code,
  - and which interop-layer artifacts (RDF/SHACL/OBDA mappings) are not aligned
    with accepted ontology structure.
- [ ] Add implementation-correctness preview/reporting:
  - what business-rule consequences changed across versions,
  - what code obligations are now missing or inconsistent,
  - which changes weaken a previously stronger claim,
  - and which changes are only weak/evidence-plane suggestions.
- [ ] Extend implementation surfaces beyond the current minimal set when
  modeling industrial/business systems:
  - simulation models,
  - optimizer artifacts,
  - PLC routines and mode/state machines,
  - HMI views,
  - regulatory/reporting outputs,
  - and document/SOP sections as typed surfaces rather than only generic notes.
- [ ] Make ontology-backed retrieval first-class for LLM/tool loops:
  - accepted facts/rules/contexts/provenance,
  - trust contracts and anchors,
  - weak-vs-strong claim separation,
  - and version-aware querying across ontologies/worlds.
- [ ] Add type-directed programming surfaces driven by the same IR:
  - validators,
  - query builders,
  - review checklists,
  - migration previews,
  - and code-generation hints for business systems.
- [ ] Add hole-driven IDE/API behavior for engineering, not only query repair:
  - typed holes in olog authoring,
  - typed holes in CQ authoring,
  - typed holes in migration/evolution authoring,
  - and machine-applicable refinement actions rather than only diagnostic prose.
- [ ] Make typed authoring/querying first-class for engineers as well as
  ontology authors:
  - authoring surfaces return stable object/arrow/role ids plus repair
    suggestions,
  - query surfaces return typed result-shape and trust information,
  - and both should be consumable by agents without scraping prose.
- [ ] Align SHACL, RDF, and olog-facing workflows with the same agent-facing
  checker/report model:
  - SHACL reports remain boundary-layer validation artifacts,
  - RDF remains a boundary-layer source/query format,
  - ologs remain the human authoring surface,
  - but all three must lower into the same canonical delta/query/coverage
    vocabulary.
- [ ] Treat "LLM-assisted" as explicit typed integration surfaces:
  - tool-loop endpoints,
  - stable APIs,
  - plugin/skill contracts,
  - and future MCP-style adapters,
  - rather than free-form rewrite-only behavior.

### 12.5 Strong and weak claims across versioned worlds

- [ ] Support querying under one or more versioned ontology/world anchors and
  report:
  - accepted claims,
  - review-state candidate claims,
  - retracted/superseded claims,
  - and evidence-plane suggestions.
- [ ] Make trust payloads explicit about whether a result is:
  - accepted and anchor-scoped,
  - review-preview only,
  - execution-only,
  - or heuristic/evidence-plane.
- [ ] Add comparison tooling for rule changes across versions:
  - what was added,
  - what was removed,
  - what became weaker/stronger,
  - and what implementation obligations changed.

### 12.6 Exit criteria

- A coding agent can use Axiograph as a business-rule and implementation
  understanding engine rather than only as a certificate checker.
- Rust authoring/query/checker surfaces are useful enough for day-to-day
  engineering without pretending to be the trusted kernel.
- SHACL/RDF/olog flows are visibly aligned on one semantic vocabulary and one
  trust-language family.
- Semantic coverage and rule drift are reviewable alongside code/test changes.
- Ontology-backed retrieval becomes anchor-aware and trust-explicit by default.

---

## 13. Toward A Fuller Dependently Typed Ontology Engine

The current system should still be described as a typed ontology workbench with
a narrow trusted kernel. The roadmap to a fuller dependently typed ontology
engine needs to be explicit.

### 13.1 Necessary repo conditions

- [ ] One compiled schema/category IR must become the common semantic currency
  for:
  - authoring,
  - querying,
  - migration,
  - certification,
  - and semantic VCS history.
- [ ] Lean must check more than replay-shaped witnesses:
  - accepted `.axi` well-formedness for the defended fragment,
  - rewrite admissibility,
  - certifiable query obligations,
  - and certifiable migration obligations.
- [ ] Rust must make semantic indices first-class in the public workflow:
  - lifecycle state,
  - accepted snapshot/module anchors,
  - schema/theory/context ids,
  - typed query handles,
  - typed migration handles,
  - and certified-result handles.
- [ ] Typed authoring surfaces must emit stable-id semantic deltas with
  provenance, CQ attachments, and trust contracts.
- [ ] Semantic VCS must track state+delta objects and review/merge decisions over
  typed ontology objects, not only module blobs.

### 13.2 What this does not mean

- [ ] Do not collapse the runtime into Lean.
- [ ] Do not imply completeness or ontology closure by default.
- [ ] Do not present heuristic discovery/LLM/world-model outputs as part of the
  trusted kernel.

### 13.3 Exit criteria

- The repo’s strongest operational seams all reduce to typed, anchored,
  checkable artifacts.
- The phrase “full dependently typed ontology engine” becomes honest for the
  supported fragment, not merely aspirational.

---

## 14. Anti-Goals

This roadmap should not be misread as a license to overclaim.

Do not describe the result as:

- a full HoTT ontology engine,
- a complete theorem-backed migration calculus today,
- a system that proves query completeness by default,
- or a workflow where PathDB/WAL state defines ontology meaning.

The honest target claim remains:

> Axiograph should become a proof-carrying ontology engineering workbench in
> which accepted `.axi` defines meaning, Lean checks a narrow trusted kernel
> slice, Rust enforces anchor-aware workflow discipline, and ontology evolution
> is mediated by CQ-gated, trust-explicit, semantically versioned review
> artifacts.
