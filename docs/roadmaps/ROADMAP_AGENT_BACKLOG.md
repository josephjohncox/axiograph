# Agent Backlog

This roadmap preserves the active priorities and living checklist that were
previously embedded in `AGENTS.md`. Keep `AGENTS.md` short; update this file or
the domain roadmaps when priorities change.

Related roadmaps:

- `docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md`
- `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`
- `docs/roadmaps/ROADMAP_MATHEMATICAL.md`
- `docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md`
- `docs/roadmaps/ROADMAP_SEMANTIC_MERGE_LATTICE.md`

## Highest-Priority Work

- [~] Compile one canonical schema/category IR with deterministic ids for
  schema, theory, instance, context, relation-objects, projection arrows,
  rewrite rules, and anchors shared by Rust, Lean, semantic diff, migration
  preview, backend projection, and certificates. Current first slice includes
  `SchemaCategoryIr` and `InstanceFunctorIr` in `axiograph_pathdb::kernel_ir`;
  the remaining work is to make it the universal spine for query, migration,
  certification, semantic VCS, and Lean export.
- [~] Make Rust theory checking first-class and useful: current slice adds
  `RuntimeTheoryCheckReportV1`, closure tiers (`finite_fragment`,
  `evidence_weighted`, `global_indexed`), `axiograph check theory`,
  `axiograph discover theory-check`, `/semantic/theory-check`, and
  `semantic_theory_check`. Remaining work is to feed the report into every
  behavior-case, CQ, context, migration, reconciliation, merge/rebase, and
  semantic coverage gate by default.
- [ ] Make typed authoring and ontology exploration return one preview contract:
  canonical `.axi` deltas, stable ids, provenance/evidence/context anchors,
  CQ attachments, trust metadata, structural evolution primitives, directed
  next actions, and typed repair suggestions.
- [ ] Keep typed query and certification first-class across REPL, server, and
  tooling through `query_ir_v1`, prepared query handles, structured
  diagnostics, anchor-aware answers, trust contracts, and soundness-scoped
  certificates.
- [~] Treat migration as typed transport plus explicit reindexing over stable
  semantic ids, including preserved, reindexed, split, merged, dropped, and
  residual comparability obligations. Current slice adds
  `TheoryTransportPlanIr` over compiled `TheoryIr` plus `SchemaMorphismV1`, so
  migration preview can distinguish preserved obligations, transported
  obligations, missing object images, missing arrow images, and opaque
  out-of-fragment theory obligations before emitting resolver handles.
- [ ] Treat reconciliation as typed evolution over conflict sets and decisions,
  with CQ/trust consequences and unresolved obligations preserved as review
  objects.
- [~] Make type inference and hole-driven exploration a shared service over the
  compiled IR. Current slices exist for queries, typed olog authoring,
  migration preview, reconciliation review, and CQ repair; next slices should
  cover migration authoring, implementation-surface mapping, and richer theory
  obligations.
- [ ] Make CQ-gated evolution the shared mutation-review primitive across
  proposals, promotion, migration preview, semantic merge, and review branches.
- [ ] Make semantic VCS the reviewable unit of ontology change: refs, branches,
  ancestry, typed deltas, validation refs, reconciliation, tags, supersession,
  and retraction.
- [~] Make semantic merge/rebase operate through typed slices and a finite
  runtime merge lattice. Current first slice adds `SemanticSliceManifestV1`,
  `SemanticMergeLatticeV1`, `SemanticMergePlanV1`, conservative auto-join
  analysis, persisted `sem/slices/` manifests, CLI slice/merge/rebase
  planning, resolver-step extraction, and MCP/tool surfaces. `sem slice build`
  now enriches stored manifests from accepted canonical modules compiled to
  `KernelModuleIr`, including schema/category refs, relation objects, role
  projections, subtype inclusions, theory obligations, and instance-functor
  refs. Next work is using those enriched refs as the default input to merge
  materialization gates and resolver application loops.
- [ ] Make the typed lifecycle useful for co-evolving real engineering systems:
  simulators, optimizers, PLC logic, HMI views, reports, ERP/MRP surfaces, SOPs,
  code, tests, and deployment artifacts.
- [ ] Keep AI/world-model outputs evidence-plane but richly typed: run anchors,
  proposal-set digests, grounded evidence links, candidate schema/theory/olog
  deltas, and explicit preview failures.
- [ ] Keep verification language pinned to accepted `.axi` anchors and the
  actual checked production `.axpd` path until verified-v2 storage converges.
- [ ] Finish the greenfield example cleanup by rewriting export-era REPL
  scripts and `repl_scripts_export_and_querycert_smoke` so examples foreground
  canonical `.axi`, typed reports, certificates, behavior cases, and semantic
  previews instead of requiring every script to emit `*_export_v1.axi`.

## Current Slices To Build On

- [x] Typed anchors, lifecycle wrappers, and initial `kernel_ir.rs` compiled
  semantics scaffolding exist in Rust.
- [x] First runtime `SchemaCategoryIr` / `InstanceFunctorIr` slice exists for
  relation-as-object category semantics, role projection arrows, subtype
  inclusions, closed object memberships, stable fact-id relation objects, and
  role-value arrow images.
- [x] JSON `BehaviorCaseV1` exists as a BDD/DDD/coding-agent wrapper over
  bounded-context reports, CQ coverage, trust contracts, `CaseReceiptV1`, and
  Rust/TypeScript test skeleton previews.
- [x] Bounded contexts, behavior cases, and DDD/fDDD context maps now emit
  `SemanticSliceSelectorV1` payloads, so context reports, BDD specs, and
  context maps can feed semantic merge/rebase planning without a parallel DDD
  merge system.
- [x] Query trust contracts and query-result certification expose a first
  soundness-scoped trust surface.
- [x] CQ-gated proposal preview exists for evidence-plane overlays.
- [x] Accepted-plane code already has `sem/commits`, `sem/refs`,
  `sem/validations`, and `sem/world_model_runs`.
- [x] Migration preview builders emit transport-along-morphism, transported
  path-equation, subtype-collapse, and merge-image primitives.
- [x] Runtime theory transport plans classify compiled theory obligations under
  schema morphisms before migration preview emits residual obligations and
  resolver handles.
- [x] `TheoryObligationGraphV1` exposes theory/obligation/subject nodes and
  support/touches edges so type-directed exploration, CQ repair, migration
  authoring, and reconciliation can share typed handles instead of string
  patches. `axiograph discover theory-graph` now emits this graph for
  canonical `.axi` modules, and `semantic_theory_graph` exposes it through the
  semantic tool-loop surface.
- [x] Reconciliation records lower into the shared preview language and can
  persist preview reports plus merge commits with typed summaries.
- [x] Compiled-IR exploration emits candidates for relation-object reification,
  dependent families, carrier lifts, rewrite candidates, and subtype factoring.
- [x] First semantic merge-lattice runtime contracts exist for typed slices,
  conservative merge/rebase plans, and resolver handles over existing semantic
  VCS dry-runs.
- [x] CLI-built semantic slice manifests are enriched from accepted
  `KernelModuleIr` so semantic VCS slicing is grounded in schema/category,
  theory, and instance-functor handles rather than commit metadata only.
- [x] Industrial engineering harness split out of `axiograph-cli` into
  `rust/crates/axiograph-example-industrial`, with example docs and a
  standalone teaching binary over canonical `.axi`.
- [x] Examples now have a teaching catalog, directory guides, a BehaviorCaseV1
  fixture, and an expanded canonical `.axi` corpus so agents can route examples
  by feature instead of by historical script names.

## Lean And Certificates

- [ ] Port remaining knowledge-graph transport, quotient, equivalence, and
  migration scaffolding into Lean.
- [ ] Finish inverse paths, equivalence congruence, normalization, and
  functoriality proofs, preferably using mathlib groupoid/free-groupoid
  machinery.
- [ ] Port remaining probability and reconciliation verification modules as
  needed for the trusted checker.
- [ ] Converge Rust and Lean parsers to a shared `Axiograph.ModuleAST` anchored
  to `examples/canonical/corpus.json`.
- [ ] Extend rewrite derivation certificates beyond normalization into
  reconciliation proofs and domain rewrites.
- [ ] Anchor certificates to canonical `.axi` inputs with stable module hashes
  and extracted facts.
- [ ] Keep certificate scope conservative: returned-row, derivation, and
  migration soundness under explicit anchors, not completeness or ontology
  closure.

## Rust Runtime Type System

- [ ] Make lifecycle and anchor typestate universal for major artifacts:
  `Parsed`, `Validated`, `Reviewed`, `Accepted`, `Certified`, `Superseded`,
  `Retracted`, plus stable anchors on modules, proposals, snapshots, answers,
  runs, and reports.
- [ ] Promote schema-scoped and snapshot-scoped wrappers to default public APIs;
  de-emphasize raw `u32` and `String` surfaces in core execution.
- [ ] Introduce first-class workflow types such as `FactId<A>`,
  `TypedFact<S, R, A>`, `ProposalSet<Validated, A>`, `WorldState<A>`,
  `WorldModelRun<A>`, and `CertifiedAnswer<A>`.
- [ ] Lift module kind into the type surface: distinguish canonical `.axi`
  modules from derived `PathDBExportV1` snapshots.
- [ ] Expose first-class prepared/typechecked query handles so REPL, server,
  LLM, and tool-loop surfaces stop executing from raw query strings or raw ASTs.
- [ ] Extract a reusable typed CQ/query runner so example crates can exercise
  full query elaboration without depending on private `axiograph-cli` modules.
- [ ] Replace common string lookups with stable refs derived from the compiled
  schema/category IR.
- [ ] Make typed execution operate over compiled IR instead of stringly
  meta-plane indexes.
- [ ] Upgrade query/proposal diagnostics from strings and counts to typed repair
  objects shared by query IR, proposal authoring, and draft `.axi` review.
- [ ] Deepen meta-aware query trust from "in-scope ontology surface" to
  "actually used semantic surface".
- [ ] Bring typed-authoring response contracts to viz review flows and other
  non-LLM authoring entrypoints.
- [ ] Treat competency-question gap discovery as a first-class compiler service.

## Compiler And Ontology Services

- [ ] Turn the compiler/typechecker into ontology-engineering services:
  structural classification, typed completion, hole filling, ambiguity repair,
  elaboration output, semantic diff, migration preview, and CQ regression hooks.
- [ ] Extend trust contracts across REPL query output, LLM/tool-loop output,
  migration preview, proposal review, and authoring surfaces.
- [ ] Make type-driven ontology usefulness explicit in user tooling:
  schema-first exploration, olog-fragment synthesis, candidate constraints,
  path equations, rewrite-rule discovery, and previewable `.axi` deltas.
- [ ] Make olog authoring a review bundle over the canonical IR: stable ids,
  relation boxes, aspects, path equations, CQs, provenance, trust metadata, and
  typed refinement handles.
- [ ] Add schema-aware, position-aware completion for REPL and AxQL from the
  compiled IR.
- [ ] Show inferred types, ambiguity resolution, rewrite/normalization
  explanation, and suggested fixes as default UX, not debug-only output.

## Semantic VCS And World Models

- [ ] Evolve the accepted-plane snapshot store into a first-class semantic VCS,
  not just `HEAD` plus logs.
- [ ] Add refs beyond `HEAD`: `refs/heads/main`, `refs/heads/review/*`,
  `refs/heads/evidence/*`, `refs/heads/wm/*`, and `refs/tags/*`.
- [ ] Add semantic commit objects with parentage, author, timestamp, message,
  policy/reconciliation metadata, accepted modules, evidence overlays,
  certificates, validation refs, and optional world-model runs.
- [ ] Add semantic diffs for schema, theory, instance, context/world,
  certificates, rules, CQs, trust, coverage, and implementation obligations.
- [ ] Define merge as reconciliation with explicit conflict sets, decisions,
  residual obligations, and optional certificates.
- [ ] Track lifecycle states on facts, modules, proposals, reviews,
  certificates, runs, and projection manifests.
- [ ] Add semantic VCS CLI verbs for branch, checkout, status, diff, log, merge,
  tag, promote, supersede, and retract.
- [ ] Make world-model branches and review branches first-class lifecycle
  surfaces.
- [ ] Persist proposal/review bundles under `sem/validations/` with stable
  proposal-set and run anchors.
- [ ] Require CQ-gated review/merge transitions before accepted-plane promotion.
- [ ] Link committed world-model runs to semantic commit ids, refs, evaluations,
  planner outcomes, and promotion tags.

## Interop And Backends

- [ ] Lower RDF, OWL, SHACL, SPARQL, and property-graph adapters through the
  same canonical schema/category IR.
- [ ] Add RDF-star or statement-level metadata support as fact/evidence objects.
- [ ] Add OWL/RDFS mapping into reviewable `.axi` schema, theory, and instance
  material with explicit open-world semantics.
- [ ] Compile a SPARQL SELECT subset into typed query IR as an untrusted boundary
  dialect.
- [ ] Add SHACL validation as an ingestion/promotion gate that emits typed
  validation reports and optional `.axi` constraint proposals.
- [ ] Add a certifiable validated-import path for a restricted adapter subset.
- [ ] Treat property-graph compatibility as a projection layer, with n-ary
  relations emitted as fact nodes or relationship entities.
- [ ] Keep TypeDB and TerminusDB Docker-backed smoke tests in repo.
- [ ] Generate backend pushdown plans directly from compiled IR plus capability
  profiles, including native read-only query dialects, RDF/SHACL interfaces
  where real, lifting contracts, and trust caveats.

## Storage, Verification, And Hardening

- [ ] Converge the live `.axpd` format and the sectioned verified `.axpd` story,
  or explicitly scope `verified.rs` as non-production.
- [ ] Add end-to-end tests over actual production `.axpd` checkpoint bytes, not
  only exported anchors or synthetic examples.
- [ ] Keep accepted-plane snapshots plus PathDB/WAL as the real storage
  backbone until `axiograph-storage` placeholder behavior is removed.
- [ ] Prove runtime witness invariants with Verus where tractable.
- [ ] Fuzz untrusted surfaces: PathDB bytes, certificate JSON, `.axi` parsing,
  and adapter/plugin boundaries.
- [ ] Add Miri, Kani, Loom/Shuttle, or Aeneas selectively for small critical
  kernels when they provide concrete value.
- [ ] Keep expanding `make verify-semantics` as the must-pass Rust+Lean
  semantics suite.

## Visualization, Exploration, And Quality

- [ ] Add schema-first exploration modes: object/subtype/relation trees, theory
  panels, constraints, rewrite rules, contexts/worlds, provenance, and typed
  program views.
- [ ] Wire network-analysis results into viz overlays.
- [ ] Add snapshot diff and graph set operations over `.axpd` and accepted-plane
  snapshots.
- [ ] Add a competency-question runner for examples and CI.
- [ ] Keep examples pedagogical: each nontrivial example should document the
  typed, CQ, coverage, VCS, backend, or agent surface it exercises.
- [ ] Add migration preview UX for renamed objects/arrows, dropped information,
  CQ/query rewrites, certificate availability, and review checkpoints.
- [ ] Expand quality checks for unused or near-duplicate symbols, ambiguous
  n-ary field naming, closure checks, and rewrite-rule lint.
- [ ] Continue organizing docs into Diataxis and add start-here tutorial flows
  for `.axi` authoring, accepted-plane promotion, PathDB, AxQL, certificates,
  Lean verification, and ontology-engineering loops.
