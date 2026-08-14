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
- `docs/roadmaps/ROADMAP_TOOLING_OVERLAY_SEPARATION.md`

## Highest-Priority Work

- [x] Route meaning compilation through one exact-byte canonical compiler and
  immutable compiled-snapshot handle. `CanonicalCompiler` implements
  `KernelSnapshotIr`, `SchemaPresentationIr`, and validated finite
  `InstanceModelIr` in `axiograph-kernel`, with ordered import-closure/package
  identities, relation objects, projection generators, subtype coherence,
  theories, equations, executable finite saturation, object-membership and
  role-indexed witnesses, finite-constraint witnesses, dependent
  contexts/worlds/temporal scopes, checked lifecycle state, and adversarial
  Rust/Lean formation parity. In-process `RuntimeModuleIndex` values retain the licensing
  `CompiledKernelSnapshot`; serialized runtime indexes lose that handle and
  remain citations only.
- [ ] Extend the trusted Lean checker from current certificate and formation
  parity to the complete canonical `KernelSnapshotIr` finite-model fragment.
  The anchored `category_kernel_v3` slice reconstructs the finite category
  presentation from exact `.axi`, checks ordered projections, identities,
  composition, parallel equations and contextual congruence, replays both
  formal inverse-law normalization traces for every generator, and replays
  complete bounded generator saturation. Full instance interpretations, formal
  groupoid equation lowering, refinements, and transports remain outside that
  certificate.
- [~] Make Rust theory checking first-class and useful: current slice adds
  `RuntimeTheoryCheckReportV1`, closure tiers (`finite_fragment`,
  `evidence_weighted`, `global_indexed`), `axiograph check theory`,
  `axiograph discover theory-check`, `/semantic/theory-check`, and
  `semantic_theory_check`. Module summaries now use typed scope, coverage,
  transport, residual, and non-claim fields rather than synthetic closure
  claims. Canonical finite-theory replay uses one shared
  `Authoring`/`Query`/`Merge` gate receipt with exact category/refinement/
  explanation/context/identity-transport coverage; AxiStore stores and
  reproduces the merge receipt during candidate recompilation. Remaining work
  is to feed the richer runtime-theory report into every behavior-case, context,
  migration, and semantic coverage gate by default.
- [~] Make typed authoring and ontology exploration return one preview contract.
  W09 adds `authoring_workspace_request_v1` and
  `authoring_workspace_report_v1` as the sole CLI/LSP/MCP/HTTP ontology-
  authoring service: exact workspace import-closure anchors, compiled stable
  refs, diagnostics, query/olog/CQ holes and shared repairs, prepared-query
  explanations, finite kernel payload diffs, typed olog evolution previews,
  runtime-theory validation, directed next actions, and fail-closed promotion
  review. Remaining work is a canonical `.axi` delta renderer with explicit
  evidence/context provenance and the separate trusted workflow that turns a
  reviewed report plus VerifyMain receipt into an AxiStore `PromotionPlan`.
- [~] Keep typed query and certification first-class across REPL, server, and
  tooling through `query_ir_v1`, prepared query handles, structured
  diagnostics, anchor-aware answers, trust contracts, and soundness-scoped
  certificates. `axiograph check finite-query` now provides a bound
  file-oriented route, and the regulated-shipment merge stores its exact
  accepted answer receipt as the trust gate; remaining query surfaces must use
  the same lifecycle distinction.
- [~] Prefer maintained Rust crates over custom protocol/core infrastructure
  where Axiograph semantics do not require custom logic. Current choices:
  `rmcp` for MCP stdio servers, `lsp-server`/`lsp-types` for LSP/editor
  surfaces, and `hyper`/`http-body-util` for the DB HTTP server.
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
- [~] Make semantic VCS the reviewable unit of ontology change. W04 adds the
  transactional `AxiStore` authority for accepted state, immutable objects,
  refs/branches/tags, audit lineage, typed deltas, reconciliation, and pinned
  ancestry. Remaining work is CLI/service cutover plus supersession/retraction
  workflows over the new store.
- [~] Make semantic merge/rebase operate through typed slices and a finite
  runtime merge lattice. The current slice adds `SemanticSliceManifestV1`,
  `SemanticMergeLatticeV1`, `SemanticMergePlanV1`, conservative auto-join
  analysis, resolver-step extraction, and read-only MCP/tool surfaces. Pure
  builders enrich supplied commit/ref payloads from `RuntimeModuleIndex` with
  canonical schema/generator, theory, instance, and fact refs. Durable slice
  and plan artifacts must be immutable AxiStore attachments; the former
  `sem/slices/`
  and broad CLI command family were removed.
- [ ] Make the typed lifecycle useful for co-evolving real engineering systems:
  simulators, optimizers, PLC logic, HMI views, reports, ERP/MRP surfaces, SOPs,
  code, tests, and deployment artifacts.
- [ ] Refactor DDD/fDDD, BDD, implementation-surface, coverage, and codegen
  concepts out of ordinary domain `.axi` examples and into typed overlay
  manifests. Canonical `.axi` should model domain meaning; tools should use the
  ontology through stable compiled IR refs. Track the detailed plan in
  `docs/roadmaps/ROADMAP_TOOLING_OVERLAY_SEPARATION.md`.
- [ ] Keep AI/proposal-adapter outputs evidence-plane but richly typed: run anchors,
  proposal-set digests, grounded evidence links, candidate schema/theory/olog
  deltas, and explicit preview failures.
- [ ] Treat embeddings as evidence/index sidecars: add anchored
  `EmbeddingSidecarManifestV1`, typed relationship-evidence overlays, and
  tool/CLI surfaces that lift vector matches into proposals or refinement
  handles instead of accepted facts.
- [x] Keep verification language pinned to accepted `.axi` anchors and the
  actual checked production `.axpd` path. W05 converges production storage on
  authenticated SQLite with explicit AxiStore receipts and bounded verification.
- [x] Finish the greenfield example cleanup by rewriting removed-export REPL
  scripts and `repl_scripts_canonical_regression` so examples foreground canonical
  `.axi`, typed reports, certificates, behavior cases, and semantic previews
  instead of requiring every script to emit `*_export_v1.axi`.
  - Continued cleanup: schema-discovery inputs no longer teach synthetic
    `Entity` fallback; generated drafts use concrete observed types or
    `TypeHole_*` review obligations, and the proto-theory example uses a
    meaningful `ApiArtifact` umbrella type.
  - Guardrail: canonical-only input paths reject obsolete derived snapshot
    modules, removed REPL export commands fail, and teaching scripts do not emit
    reverse-exported ontology snapshots.

## Current Slices To Build On

- [x] Typed anchors, lifecycle wrappers, and the initial `kernel_ir.rs`
  compiled semantics slice exists in Rust.
- [x] Canonical `SchemaPresentationIr` is the sole relation-as-object category
  presentation, and `RuntimeSemanticIndex` exposes only read-only
  `KernelRefV2` citations. The duplicate runtime `SchemaCategoryIr` /
  `InstanceFunctorIr` representations were removed.
- [x] JSON `BehaviorCaseV1` exists as a BDD/DDD/coding-agent wrapper over
  bounded-context reports, CQ coverage, trust contracts, `CaseReceiptV1`, and
  Rust/TypeScript test skeleton previews.
- [x] Bounded contexts, behavior cases, and DDD/fDDD context maps now emit
  `SemanticSliceSelectorV1` payloads, so context reports, BDD specs, and
  context maps can feed semantic merge/rebase planning without a parallel DDD
  merge system.
- [x] Query trust contracts and query-result certification expose a first
  soundness-scoped trust surface, including separate certifiability, emission,
  and exact-answer-bound receipt status in the regulated-shipment gate.
- [x] CQ-gated proposal preview exists for evidence-plane overlays.
- [x] AxiStore owns semantic commits, refs, immutable attachments, audit
  lineage, accepted closures, and authenticated materialization receipts.
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
- [x] CLI-built semantic slice manifests are enriched from canonical compiled
  snapshots through the derived `RuntimeModuleIndex`, so semantic VCS slicing
  is grounded in canonical object, generator, theory, instance, and fact
  citations rather than commit metadata only.
- [x] Industrial engineering example runtime split out of `axiograph-cli` into
  `rust/crates/axiograph-example-industrial`, with example docs and a
  standalone teaching binary over canonical `.axi`.
- [x] Examples now have a teaching catalog, directory guides, BehaviorCaseV1
  examples, and a `fixtures/canonical/corpus.json` conformance corpus so agents
  can route examples by feature instead of by historical script names.
- [~] First software-authoring continuous coverage crate exists, but its
  current example still over-embeds DDD/tooling concepts in `.axi`; the next
  slice is the overlay separation refactor.

## Lean And Certificates

- [x] Add `lean/Axiograph/SemanticVCS.lean` and
  `docs/reference/LEAN_THEORY_EVALUATION.md` so semantic merge/lattice/rebase
  preservation has a finite Lean conformance slice and a precise status matrix.
- [ ] Port remaining knowledge-graph transport, quotient, equivalence, and
  migration transport model into Lean.
- [x] Finish endpoint-indexed identity/composition/inverse paths, unit/inverse/
  associativity/congruence laws, mandatory normalization traces, and
  Rust/Lean rejection parity using mathlib free-groupoid denotation.
- [ ] Extend the proved finite path fragment into broader functoriality and
  migration-transport theorems without treating rounded confidence as path
  equality.
- [ ] Port remaining probability and reconciliation verification modules as
  needed for the trusted checker.
- [ ] Converge Rust and Lean parsers to a shared `Axiograph.ModuleAST` anchored
  to `fixtures/canonical/corpus.json`.
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
  `ProposalAdapterRun<A>`, and `CertifiedAnswer<A>`.
- [x] Lift module kind into the type surface: distinguish exact accepted `.axi`
  modules from derived materialization receipts and runtime handles.
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
- [ ] Make DDD/fDDD/BDD/codegen/coverage tooling consume ontology refs instead
  of requiring those engineering-method concepts to be represented inside the
  business ontology.
- [ ] Make olog authoring a review bundle over the canonical IR: stable ids,
  relation boxes, aspects, path equations, CQs, provenance, trust metadata, and
  typed refinement handles.
- [ ] Add schema-aware, position-aware completion for REPL and AxQL from the
  compiled IR.
- [ ] Show inferred types, ambiguity resolution, rewrite/normalization
  explanation, and suggested fixes as default UX, not debug-only output.

## Semantic VCS And Proposal Adapters

- [x] Replace file-pointer and JSONL accepted authority with SQLite-backed
  `AxiStore`, immutable exact-byte objects, one audit chain, and one
  generation-CAS singleton state.
- [x] Add protected main, review/evidence branches, immutable tags, and a
  cryptographic digest of the complete ref map in the same catalog transaction.
- [x] Add semantic commit objects binding repository, ordered parentage,
  tree/snapshot/build manifest, full typed reindex delta, gates, attachments,
  provenance, lifecycle events, and exact merge reconciliation.
- [ ] Add semantic diffs for schema, theory, instance, context/world,
  certificates, rules, CQs, trust, coverage, and implementation obligations.
- [ ] Define merge as reconciliation with explicit conflict sets, decisions,
  residual obligations, and optional certificates.
- [ ] Track lifecycle states on facts, modules, proposals, reviews,
  certificates, runs, and projection manifests.
- [ ] Expose narrow CLI/server adapters over typed AxiStore operations only where
  an operational workflow requires them; do not restore filesystem semantic VCS,
  compatibility readers, or dual writes.
- [ ] Make proposal-adapter branches and review branches first-class lifecycle
  surfaces.
- [ ] Persist proposal/review bundles as immutable AxiStore attachments with
  stable proposal-set and run anchors.
- [ ] Require CQ-gated review/merge transitions before accepted-plane promotion.
- [ ] Link committed proposal-adapter runs to semantic commit ids, refs, evaluations,
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
- [x] Treat property-graph support as an experimental projection layer, with
  relation objects and n-ary facts emitted as explicit nodes rather than
  canonical binary edges.
- [x] Keep TypeDB and TerminusDB Docker-backed API smoke tests in repo; typed
  projection/readback semantics are covered separately without containers.
- [x] Generate capability-declared PathDB, TypeDB, TerminusDB, RDF/OWL, and
  property-graph projections directly from `CompiledKernelSnapshot`, with
  finite `KernelRefV2` coverage, semantic-loss reports, native read-only
  artifacts, evidence-only readback, and Axiograph-only mutation authority.

## Storage, Verification, And Hardening

- [x] Converge `.axpd` on one SQLite format under AxiStore. The executable gate
  builds a deterministic image, authenticates exact/logical digests and semantic
  anchors, opens read-only under limits, and hydrates PathDB only after receipt
  verification.
- [x] Add adversarial tests over production SQLite bytes: insertion-order
  determinism, semantic-row changes, N/N+1 limits, corruption, truncation,
  substitution, old-format rejection, recovery, fault injection, deletion, and
  bounded arbitrary bytes.
- [x] Remove PathDB/WAL and `axiograph-storage` persistence authority. AxiStore
  owns accepted objects, refs, audit, and materialization receipts;
  `axiograph-storage` is process-local evidence staging only.
- [ ] Prove runtime witness invariants with Verus where tractable.
- [x] Fuzz initial untrusted surfaces with named executable targets.
  Checked-corpus targets cover authenticated `.axpd` bytes before PathDB
  hydration, Certificate V2/V3 JSON, Rust `.axi` parsing, production CLI/REPL
  command parsing, and the shared predictive-proposal command/HTTP adapter
  response boundary through `make verify-fuzz`.
- [ ] Add Miri, Kani, Loom/Shuttle, or Aeneas selectively for small critical
  kernels when they provide concrete value. Each lane needs a target that either
  runs the suite or explicitly reports that the optional tool is unavailable;
  silent no-ops do not count.
- [ ] Keep expanding `make verify-semantics` as the must-pass Rust+Lean
  semantics suite.

## Visualization, Exploration, And Quality

- [ ] Add schema-first exploration modes: object/subtype/relation trees, theory
  panels, constraints, rewrite rules, contexts/worlds, provenance, and typed
  program views.
- [ ] Wire network-analysis results into viz overlays.
- [ ] Add semantic snapshot diff over accepted trees and optional execution diff
  over verified materialization receipts.
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
