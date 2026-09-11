# Engineering quality implementation roadmap

This is the execution tracker for the
[baseline audit at 70c568b](../reference/ENGINEERING_AUDIT_70C568B.md).
It covers every finding and recommendation from that assessment. The
[agent backlog](ROADMAP_AGENT_BACKLOG.md) links here rather than duplicating its
status. Existing domain roadmaps still define broader product goals. The
[complete execution plan](ENGINEERING_QUALITY_EXECUTION_PLAN.md) maps each original
requirement and acceptance clause to the dependency-ordered implementation units.

## Execution rules

- `[ ]` means not implemented. `[~]` means partially implemented or under review.
- `[x]` requires code, regression tests, usable examples, and recorded checks.
- Record partial progress without closing the parent item.
- Preserve exact accepted bytes, anchors, resource limits, and fail-closed gates.
- Compact reports must retain failure, trust, coverage, and truncation information.
- Typed/runtime evidence must not become a Lean proof through relabeling.
- Use one writer per checkout. Review each coherent change before broader work.
- Do not replace implementation with a design document or a synthetic test.
- Keep test results, skipped checks, review findings, and next actions below.
- Do not publish, deploy, or change accepted repository state as part of this work.

## P0: product integrity and usable APIs

### EQ-01: frontend typecheck gate

- [x] Fix the arithmetic type errors in `frontend/viz/src/core/context.ts` through
  a real numeric context-ID contract, not `any`, suppression, or unsafe casting.
- [x] Add `npm run typecheck` and make the frontend verification gate run it.
- [x] Make production and debug build entrypoints reject type errors.
- [x] Add regression tests for numeric ordering, duplicates, empty contexts,
  context selection, and context badge behavior.

**Acceptance:** clean install, typecheck, tests, and production/debug builds pass.
An intentionally invalid TypeScript fixture fails the typecheck/build gate. The
Makefile frontend gate runs checks before bundle publication. Document any pinned
Node/npm mismatch separately from source failures.

### EQ-02: frontend contracts, safe rendering, and regression coverage

- [~] Replace loose graph/context/review state with explicit boundary types.
  - [x] Type the context module's numeric fact/context maps, labels, options, and
    DOM controls; test rejection of string context IDs at the typed interface.
- [~] Enable strict checking in migrated modules, then the whole frontend.
  - [x] Strict-check `core/context.ts` through `tsconfig.context.json` in the real
    typecheck and build gates. Whole-frontend migration remains open.
  - Bounded rendering slice (accepted; frozen review `440125f2`): strict-check DOM construction,
    review status and the read-only draft selector in the same gate.
- [~] Validate incoming JSON at its boundary instead of trusting type assertions.
  - [x] Strict-check read-only database capability/status/finite-query envelopes,
    bound streamed replies and reject stale results.
    Receipt internals, graph payloads and broader authoring schemas remain partial.
- [~] Replace risky dynamic HTML composition with DOM construction or a maintained
  sanitizer. Inspect actual data flow before calling a scanner hit an exploit.
  - [x] Keep context labels in `textContent`, construct option nodes, and clear
    controls with `replaceChildren`; no context-label exploit claimed at baseline.
  - Bounded rendering slice (accepted; frozen review `440125f2`): replace list/detail/draft/status
    HTML sinks and resets with explicit DOM nodes/text, retaining rich sections,
    controls and same-page numeric navigation; no sanitizer or HTML parser added.
- [~] Test graph rendering, draft review, query submission, promotion controls,
  malformed input, and hostile labels. Add keyboard/accessibility smoke checks.
  - [x] Exercise production context functions with hostile/unusual labels and
    malformed selected values; DOM write spies reject HTML sinks. Broader browser,
    JSON-boundary, review/query/promotion, and accessibility coverage remains open.
  - Bounded rendering slice (accepted; frozen review `440125f2`): production-module list/detail/review
    regressions and actual initialization-order action tests use mocked request
    boundaries, not a browser or accepted store. Whole wire validation remains open.

**Acceptance:** tests exercise production functions, not duplicate logic. No
suppression hides existing errors. Tests prove model labels cannot execute markup
and evidence-only UI state cannot authorize accepted mutation.

### EQ-03: compact, selected, and paginated authoring reports

- [x] Introduce `summary`, `standard`, and `full` detail levels with a compact
  default on interactive adapters and an explicit full artifact route.
- [x] Add typed section selection with rejection of unknown fields/sections.
- [~] Add bounded pagination for diagnostics, stable refs, repairs, and other
  large collections. Preserve total counts and visible omitted/truncated state.
  - [x] Bound entries in 18 typed sections, including diagnostics, refs, repairs,
    holes, runtime reports, and optional singleton artifacts; test canonical-wire
    page unions and explicit omission totals.
  - [ ] Further subdivide large nested validation/query/evolution artifacts if
    needed for item-level drilldown. Current pages bound entries, not item bytes;
    nested canonical artifact payloads remain indivisible and explicitly selected.
- [x] Support anchor-bound follow-up handles or cursors that reject stale,
  cross-workspace, changed-buffer, and changed-import-closure reuse.
- [x] Preserve overall failures, residual counts, trust class, source identity,
  promotion blockers, and next actions in every detail level.
- [x] Add human summaries and executable client examples.

**Acceptance:** measure full and compact responses using the software-authoring
and regulated-shipment fixtures. Establish a tested compact byte ceiling. Page
unions match full collections without duplicates or missing entries. Summary
requests do not promote hidden failures to success. Invalid limits fail closed.
Source, query, answer, and certificate identities do not change with display mode.

### EQ-04: reusable public authoring/query/CQ service

- [~] Extract reusable query, CQ, and authoring services from the CLI binary into
  public library surfaces with explicit options and reports.
  - [x] Move AxQL/IR preparation, execution, trust, bounded verifier bridge, shared
    refinement identities, CQ DTOs and deterministic generation/evaluation into
    `axiograph-query`; full workspace/olog/evolution extraction remains open.
- [~] Keep CLI, MCP, HTTP, and LSP as adapters over that service.
  - [x] Use the same library query/CQ implementation through existing adapters;
    preserve compact workspace responses and source/trust identities in parity tests.
- [~] Remove cycles, private-source inclusion, duplicated compiler/query logic,
  and process-launch workarounds from embedding clients.
  - [x] The new query crate has no CLI/LLM/HTTP/editor dependency and owns moved
    implementations, not source-inclusion shims or CLI process facades.
- [~] Add an external integration-test client that embeds the service without
  importing private CLI modules or executing the CLI.
  - [x] Add process-free query/CQ integration tests and runnable embedding example;
    full workspace authoring embedding remains open. The bounded package query
    projection now has a process-free positive import/query/CQ integration test.
- [~] Materialize canonical import closures as a package, including schema-only
  imports and dependent instances. The reusable derived query seam now supports
  these inputs and metadata-only packages, retaining canonical ownership and
  exact-source citations. Workspace/MCP/HTTP/CLI positives replace the former
  explicit unsupported-input regression; broad namespace support remains open.
  - [ ] Replace local-name execution namespaces with canonical identity-aware
    qualified resolution across metadata, runtime and AxQL consumers. Until then,
    unrepresentable collisions reject before DB construction; canonical source
    validity is reported separately from derived projection capability.

**Acceptance:** library and CLI produce equivalent source/query/CQ/trust results
for canonical and import-aware fixtures. Existing adapter, canonical-spine,
semantic, and rejection gates pass. Do not reclassify report-only helpers as the
complete reusable service.

### EQ-05: typed transport schemas and client API

- [~] Replace the authoring MCP permissive output object with a truthful typed
  output schema, including compact responses, errors, pages, and trust metadata.
  - [x] Close and validate compact envelope/aggregate/source/trust/promotion/page
    metadata, full top-level fields, and errors using an offline maintained JSON
    Schema validator on real adapter responses and malformed fixtures.
  - [ ] Expand opaque nested canonical artifact schemas; current output schema
    intentionally does not claim complete domain artifact or SDK coverage.
- [~] Add a generated or mechanically checked HTTP API description.
  - [x] Check the read-only database discovery descriptor and descriptive IR
    schema with real HTTP/schema/client regressions. This is not a complete
    OpenAPI/serde-equivalence or authoring HTTP schema.
- [~] Supply a typed client example or SDK using the actual shared wire contract.
  - [x] Exercise the typed read-only database client: finite IR JSON through the
    real compiler, strict result/trust validation and receipt-bound HTTP tests.
    Full SDK/domain schema coverage remains open.
- [ ] Make semantic/authoring MCP, database/authoring HTTP, LSP, REPL, and CLI
  discovery describe the same authority and lifecycle distinctions.
- [ ] Expose narrower discovery/detail operations where the broad workspace tool
  otherwise forces oversized or ambiguous requests.

**Acceptance:** schema validation accepts real successful and failed responses and
rejects malformed fixtures. Contract tests compare adapter behavior. No client can
mint accepted handles or certificates from deserialized report metadata.

## P1: ontology-authoring experience

### EQ-06: source-precise diagnostics

- [~] Preserve byte/source spans for canonical declarations and terms.
- [~] Report filename, line, column, source excerpt, code, and typed subject refs.
- [~] Add conservative typo suggestions, including `Company` versus `Compny`.
- [~] Share diagnostics between CLI, LSP, HTTP, and MCP.
- [~] Cover imported modules, CRLF, Unicode/UTF-16 position conversion, unsaved
  buffers, missing files, and multiple errors without guessing locations.
  - Accepted bounded slice (reviews `eea82c68`, `98150489`): unknown object/relation-object role carriers,
    including nested indexed/refined bases, with syntactic occurrence subjects
    (not checked refs), exact-image coordinates and bounded advisory suggestions.
    First-error behavior is preserved; other classes/accumulation remain open.

**Acceptance:** the audit typo fixture points to the wrong role type, not line
one or column zero. Span metadata does not alter exact-byte semantic identity.
Stale diagnostics clear after edits and document closure.

### EQ-07: useful LSP services

- [ ] Add position-aware schema/type/relation/role completion.
- [ ] Add hover with inferred types, source refs, and scoped trust information.
- [ ] Add definitions, references, and document symbols.
- [ ] Add concrete typed repair edits tied to source version and preconditions.
- [ ] Add semantic tokens and formatting with exact-byte change previews.
- [ ] Add rename with semantic impact preview and no silent accepted-state edit.
- [~] Support unsaved/new files and incremental updates within document limits.
  - Accepted bounded slice (reviews `eea82c68`, `98150489`): existing unsaved root buffers and precise
    role-carrier diagnostic projection, imported-owner routing and stale/close
    clearing. New files, unsaved import overlays and incremental edits remain open.

**Acceptance:** protocol tests exercise advertised capabilities, Unicode ranges,
unsaved imports, stale edit rejection, and actual editor request sequences.
Unsupported methods do not advertise capability. Code actions must not equate
inspection or proposal creation with accepted promotion.

### EQ-08: integrated olog review workbench

- [ ] Connect typed olog edits to canonical deltas and explicit provenance.
- [ ] Show inferred types, holes, repairs, CQs, residual obligations, and trust.
- [ ] Preserve stable refs through edit, semantic diff, review, merge, and promotion.
- [ ] Provide a guided ontology-engineer flow that does not require raw JSON.
- [ ] Keep an expert full-artifact view without making it the default.

**Acceptance:** an executable browser scenario edits a canonical example, sees a
failed CQ, applies a typed repair, reviews the diff, and requests gated promotion.
A missing receipt or unresolved required obligation blocks promotion.

### EQ-09: uniform CQ and lifecycle review

- [ ] Use CQ-gated evolution consistently for proposals, migration previews,
  semantic merge, review branches, and accepted promotion.
- [ ] Extend typed holes to migration authoring, implementation mappings, and
  theory obligations instead of returning prose-only repair advice.
- [ ] Preserve snapshot/schema/context/lifecycle identity in public APIs and UI.
- [ ] Keep generated tests and code coverage separate from proof of business logic.

**Acceptance:** cross-surface tests distinguish evidence, runtime validation,
finite checking, emitted certificates, and verified receipts. Unsupported scope
remains explicit and cannot pass a strict gate through an empty/default field.

## P1: measured retrieval and grounded answers

### EQ-10: reproducible relevance evaluation

- [ ] Add versioned, labeled domain corpora and queries using real Axiograph data.
- [ ] Implement tested Recall@k, Precision@k, MRR, and nDCG calculations.
- [ ] Evaluate the actual production retriever with lexical/vector/graph ablations.
- [ ] Record corpus/index/model identity, query limits, latency, and result counts.
- [ ] Add citation support, citation recall, and unsupported-answer measurements.

**Acceptance:** a deterministic offline command produces a bounded report.
Hand-calculated edge cases validate metrics. No-relevant-result cases, ties,
duplicates, and zero results are defined. Baselines are recorded before changing
retrieval. Small fixture scores are not production-quality claims.

### EQ-11: ranked lexical and hybrid retrieval

- [ ] Replace basic token matching as the sole relevance mechanism with a
  maintained lexical ranking implementation, such as BM25.
- [ ] Replace raw `max` fusion of incomparable token/model scores with explicit
  tested rank fusion or calibrated scoring.
- [ ] Use provider-neutral score names and record the actual retrieval method.
- [ ] Reuse query embeddings when entity and document targets share a model.
- [ ] Preserve deterministic ties, bounds, snapshot scope, and evidence-only status.

**Acceptance:** production retrieval uses the new path. EQ-10 compares quality,
latency, and regressions against the baseline. Include synonym, rare-term,
zero-overlap, duplicate, and mixed-score-scale cases. Do not call a full scan ANN.

### EQ-12: scalable vector sidecars

- [ ] Select a maintained ANN backend after checking available crates and tradeoffs.
- [ ] Bind index/model/dimension/text/corpus identities to snapshot-scoped receipts.
- [ ] Reject stale, corrupt, incompatible, or cross-snapshot indexes.
- [ ] Keep an exact oracle for ANN recall evaluation, not an undocumented fallback.
- [ ] Benchmark scale, memory, build time, latency, and Recall@k.

**Acceptance:** compare ANN with exhaustive search on the same vectors. Results
remain evidence only. Bounds and failure behavior are tested. No performance or
quality claim relies only on the tiny teaching fixtures.

### EQ-13: compact evidence and answer support

- [ ] Add diversity-aware and contradiction-aware evidence selection.
- [ ] Evaluate reranking before selecting a maintained model/backend.
- [ ] Build compact evidence packets with source locators, typed refs, exclusions,
  budget/truncation state, and explicit unsupported claims.
- [ ] Add claim decomposition and scoped support checks where mechanically valid.
- [ ] Keep model-generated prose untrusted even when its inputs have receipts.

**Acceptance:** adversarial contradictory, stale, missing, and irrelevant evidence
fixtures produce honest support/unknown states. Citation claims refer to exact
sources. EQ-10 measures tradeoffs. No automatic promotion follows similarity.

## P2: formal coverage and semantic preservation

### EQ-14: category certificate acceptance-to-denotation theorem

- [ ] Retype accepted wire paths into the endpoint-indexed finite path model.
- [ ] Prove denotation preservation for formal normalization and congruence replay
  under explicit assumptions for declared equations.
- [ ] Connect the theorem to the actual `category_kernel_v3` acceptance path in
  the `VerifyMain` import closure.
- [ ] Add negative Rust/Lean parity fixtures and update the exact claim matrix.

**Acceptance:** Lean builds without `sorry` or new first-party axioms. The theorem
mentions actual verifier acceptance, not an unrelated typed helper. Do not infer
that arbitrary ontology relations have reversible data interpretations.

### EQ-15: finite dependent and transport coverage

- [ ] Extend checked finite refinement and role-witness satisfaction.
- [ ] Extend context transport and migration-obligation certificates.
- [ ] Extend selected user-rewrite checking under declared modeling assumptions.
- [ ] State exact supported fragments, residuals, and trusted import dependencies.

**Acceptance:** each extension has a theorem or executable checked claim with
honest scope, emit/check parity, tamper rejection, and an end-to-end fixture.
General DTT, arbitrary confluence, and open-world completeness are not substitutes
for these finite deliverables and are not part of this remediation.

### EQ-16: Rust/Lean parser contract

- [ ] Specify the shared canonical AST and exact-byte anchor contract.
- [ ] Expand generated/differential positive and adversarial conformance cases.
- [ ] Cover imports, dependent roles, refinements, equations, scopes, and limits.
- [ ] Remove duplicated interpretation rules where a shared contract can replace
  them without trusting Rust output as Lean source meaning.

**Acceptance:** both parsers agree on the declared supported corpus and rejection
classes. Exact source bytes remain the verifier anchor. The Lean checker still
reconstructs meaning independently of untrusted serialized Rust IR.

## P2: maintainability and documentation

### EQ-17: split concentrated modules

- [ ] Split `llm.rs` into providers, tool execution, retrieval, and report contracts.
- [ ] Split AxQL/query IR into syntax, elaboration, execution, and certification.
- [~] Thin `main.rs` and `repl.rs` after public service extraction.
  - [x] Main/REPL now consume library-owned query/IR/trust/checker implementation;
    CQ file/LLM and olog application remain genuine adapters. Further command
    dispatch thinning and intra-AxQL/IR decomposition are not completed.
- [ ] Factor canonical package/theory and PathDB runtime responsibilities where
  dependency boundaries justify it.
- [ ] Reduce repeated provider/ranking/report code without hiding semantics behind
  generic frameworks or preserving stale paths.
  - Bounded maintenance: provider-only preparation/dispatch now compiles at its
    actual feature boundaries; predictive response parsing has one shared helper.
    This does not complete the broader LLM/provider decomposition.

**Acceptance:** tests preserve behavior and trust. Public APIs replace private
source imports. Each move has a responsibility and dependency rationale. Smaller
line counts alone do not establish better architecture.

### EQ-18: documentation and terminology consistency

- [x] Correct the stale category-IR/VerifyMain status in `TYPE_THEORY_DESIGN.md`.
- [ ] Correct ANN comments and provider-specific score descriptions.
  - EQ18-U01 now has a local implementation pending parent review. The V2 tool
    response separates token, embedding, and fusion methods, rejects legacy
    ambiguity, and keeps evidence-only authority. Parent acceptance, independent
    source/evidence review, and roadmap closure remain pending.
- [ ] Reconcile completed versus planned entries across the relevant roadmaps.
- [ ] Label design targets, baseline audits, runtime checks, and formal results.
- [ ] Add runnable tutorials for compact authoring, diagnostics, embedding clients,
  relevance evaluation, and the workbench as each feature ships.
- [ ] Keep all durable findings and implementation evidence discoverable in indexes.

**Acceptance:** local doc links and the book graph pass their checks. Examples use
real commands. Current references do not claim unsupported theorem coverage.
The historical audit remains identified as a baseline rather than silently
rewritten to make the initial findings disappear.

### EQ-19: broader operational validation

- [x] Run feature-matrix and full locked Rust tests after public API refactors.
  Default workspace tests and all seven CLI check configurations passed in final
  integration; the later strict-feature maintenance slice is recorded below.
- [x] Make supported minimal/provider feature configurations pass strict
  all-target Clippy through correct feature boundaries, without blanket warning
  suppression or enabling providers by default. Test disabled-provider behavior.
  The bounded nine-configuration maintenance gate and production-entrypoint
  regressions pass; independent review of this slice remains required.
- [ ] Run live backend projection/readback tests when containers are available.
- [ ] Run model-provider integration tests only with configured test credentials
  and explicit bounded requests. Do not expose credentials in reports.
- [x] Run the complete release gate when its pinned tools are available.
  Parent acceptance pins the released result in the
  [v20260908.0.0 baseline](../reference/RELEASE_BASELINE_V20260908.md).
- [~] Add realistic usability, response-size, query, and retrieval capacity checks.
  - [x] Recheck real localhost compact client full/page parity and fixture payload
    ceilings, plus process-free query/CQ embedding, after service extraction.
  - [ ] Browser workflows and production-scale query/retrieval capacity remain open.

**Acceptance:** evidence identifies the tested source state and command. Missing
tools or external services are reported as blocked/skipped, never passed. Tests
do not mutate unrelated accepted stores or publish artifacts.

## Execution order and dependencies

1. EQ-01, focused EQ-02 regressions, and EQ-18 factual corrections.
2. EQ-03, then EQ-04 and EQ-05 with adapter parity checks.
3. EQ-06 and EQ-07 before the larger EQ-08/EQ-09 UI workflow.
4. EQ-10 before selecting EQ-11/EQ-12/EQ-13 retrieval replacements.
5. EQ-14 and EQ-16 before expanding EQ-15 claims.
6. EQ-17 alongside service extraction. EQ-19 after each affected integration slice.

Each stage can land a smaller verified subtask. Keep the unchecked requirements
visible and continue safe next steps. Escalate product/trust-boundary changes,
unavailable dependencies, infrastructure failures, and unresolved review findings.

## Validation record

### Baseline

Source: `70c568b`, clean `main`. See the audit for commands and observations.
The typecheck failure, report-size measurements, and source anchors are baseline
evidence, not current completion claims.

### Parent-accepted released baseline

The parent accepted [v20260908.0.0](../reference/RELEASE_BASELINE_V20260908.md)
at commit `a2d9c80f8e5acc1a1ef6b106f9cf97bb2c0c30df` and tree
`084782076e71ca0e938436d29deed31592b30d28`.
This acceptance closes only the EQ-19 complete pinned release-gate item.
It does not close the broader roadmap or replace earlier historical evidence.

### Remediation

No implementation item was complete when this tracker was created. Entries below
record implemented slices; they do not retroactively alter the baseline audit.

#### Frontend context/gates and category status (EQ-01, bounded EQ-02, EQ-18)

Source: working tree on `main`, base `70c568bc6e28bd446da9bb60426ed62035d17af2`.

- Numeric context maps/options now have explicit types and the context module is
  strict-checked. Empty context sets disable the filter; reinitialization restores
  enabled state when populated. Missing/disabled filters clear stale badges and
  do not provide a context name. Labels are always DOM text, not HTML.
- `npm run typecheck` checks existing `src` plus the migrated strict module;
  both build entrypoints run it before Vite. `make verify-viz` installs/audits,
  typechecks/tests, then builds debug and production (production output last).
- Added six production-function context tests and an isolated negative gate test:
  actual scripts reject injected invalid numeric and string-ID map assignments
  with TS2322, leaving a bundle sentinel untouched. No browser emulator or parallel
  implementation. Runnable commands and exact coverage are in `howto/TESTING.md`.
- Executed on **Node 26.1.0 / npm 11.13.0**, not the pinned 26.8.1 / 11.19.0:
  `cd frontend/viz && npm ci --ignore-scripts` passed (0 vulnerabilities);
  `node --test tests/context.test.mjs` passed (6 tests); `npm run typecheck`,
  `npm test` (7 tests), `npm run build:debug`, and `npm run build` passed, including
  a repeat after clean install. Production: 35 modules, 107.74 kB JS; debug:
  35 modules, 169.18 kB JS plus source map. These are build sizes, not performance
  or browser-usability measurements.
- `make -n verify-viz` passed recipe inspection only. **Full `make verify-viz`
  not run: exact toolchain unavailable; no pin override or release claim.**
- Corrected category serialization/dispatch status while retaining the missing
  category acceptance-to-denotation theorem (EQ-14). Registered audit/roadmap and
  their linked backlog/protocol chapters in the book graph. `python3
  scripts/check_book.py` initially rejected unregistered roadmap links; after
  registration it passed (50 unique chapters). A local-link check using that
  script's resolver passed for 57 Markdown file targets across the four changed
  docs (heading anchors not checked). `git diff --check` passed and
  `git diff --cached --name-only` was empty. Full rendered `make book` not run.
- Detailed logs: ignored `build/engineering-quality/{context-tests,frontend-tests,
  typecheck,npm-ci,build-debug,build-production,verify-viz-dry-run,book-check,doc-links}.log`.
- Independent frontend review passed with no actionable findings (review run
  `40554375-382b-4d1e-bbc3-ed56acdc7c71`). Exact pinned-gate acceptance is still
  pending. Broader frontend boundary validation, existing `@ts-nocheck` modules,
  whole-frontend strictness, rendering/review/query/promotion browser workflows,
  and keyboard/accessibility tests remain open under EQ-02.
  No accepted-state or Lean runtime changes were made.

#### Compact authoring checkpoint and timeout recovery

The compact-authoring worker `2d9209e8-daa7-45ce-8065-5e3a36120537` timed out
at 1,800,000 ms. Workflow `28a9e7e6-9ed7-4e22-96ec-15b70e012998` stopped before
compact review, public-service extraction, and final integration validation.
This is an incomplete implementation checkpoint, not acceptance.

- Parent confirmed the child process is terminal. Checkout remains `main` at
  base `70c568bc6e28bd446da9bb60426ed62035d17af2`, with no staged files, new
  branches, or additional worktrees. All source and documentation changes remain.
- Parent captured the tracked and new-file diff before retry at
  `/tmp/axiograph-quality-recovery-8z7z9z_0/partial.diff` with `state.json` beside
  it. Temporary recovery artifacts do not replace the versioned source/tests.
- Existing logs confirm 23 focused authoring/adapter/CLI tests and seven
  software-authoring E2Es passed. Explicit Clippy completed. Earlier format and
  import-failure logs remain historical failures, not current clean results.
- Real HTTP client logs show full/page parity for 97 software and 135 shipment
  stable refs. Minified full/summary sizes were 424,354/9,071 and 643,655/9,200
  bytes respectively. Both summaries were below the tested 12 KB ceiling.
- The schema-only import defect is now an explicit EQ-04 subtask. Compact output
  must preserve its failed validation state, not hide it through fixture choice.
- No completed canonical-spine result was present in the checkpoint logs.
  Compact-authoring review and all-stage acceptance remain pending.
- Recovery must use the same native subagent protocol. Finish the bounded
  compact handoff and validation before starting a fresh independent review.
  Do not repeat finished frontend work or mark later roadmap items complete.

#### Compact authoring checked slice (EQ-03, bounded EQ-05)

Source: working tree on `main`, base `70c568bc6e28bd446da9bb60426ed62035d17af2`.
This continues the checkpoint above without rewriting its timeout or review status.

- CLI, MCP, HTTP, and LSP execute-command results now share typed presentation:
  default `summary`, selected/paged `standard`, explicit canonical `full`.
  Machine request fixtures explicitly select full; source, prepared-query,
  certificate metadata, promotion blockers, and full serialization stay unchanged.
  LSP diagnostic notifications still use the internal full report.
- Compact summaries retain aggregate failures, finite coverage, runtime/dependent
  residual totals, CQ status, exact source/closure, trust/non-claims, promotion
  gates/blockers, and next actions. Every section shows totals/omissions; summary
  cursors bind the first detail request as well as subsequent pages.
- Cursors are stateless, domain/version-separated SHA-256 consistency checks,
  **not authentication or authority**. They bind workspace, exact candidate and
  baseline closures, CQ bytes, semantic request, section, and canonical ordering.
  Display detail/page size may vary. Malformed, oversized, overflow, stale,
  changed-offset-with-invalid-checksum, cross-section/workspace, and out-of-range
  tokens fail; recomputed valid read-only selectors cannot authorize promotion.
- Source/read/compile bounds remain enforced on every follow-up. Failed import
  compilation issues no cursors. The schema-only import failure is regression
  tested and remains an unchecked EQ-04 materialization defect, not a pagination
  success claim. No accepted store or Lean trust changes were made.
- Actual fixture sizes use the same minified UTF-8 `serde_json::to_vec` method:
  software authoring **424,354 full / 9,071 summary bytes**; regulated shipment
  **643,655 / 9,200 bytes**. Regression requires summary <12,000 bytes and >90%
  reduction for both. This is a fixture payload bound, not a universal workspace,
  computation, memory, or indivisible-artifact byte bound.
- Added 11 projection tests plus a CLI presentation-switch test. Independent full
  wire fields are the pagination oracle; tests cover invalid limits/sections,
  stale buffers/imports/baseline/CQ/workspaces/requests, cursor integrity checks,
  full byte parity, hidden compile failures, review-only residual counts, and
  real MCP/HTTP/LSP payload parity. Dev-only `jsonschema` 0.33 validates real
  success/failure/schema fixtures offline; no custom validator or suppression.
- Recovery reran `cargo fmt --manifest-path rust/Cargo.toml --all -- --check`
  (passed) and `cargo test --locked --offline --manifest-path rust/Cargo.toml
  -p axiograph-cli --bin axiograph authoring_workspace -- --nocapture`
  (**23 passed**). `make verify-canonical-spine` then **completed successfully**,
  including Rust runtime/query/merge/software-authoring/embedding/projection
  regressions, Lean theory/build checks, semantic-VCS fixtures, and diff checks.
- Before recovery, the focused software-authoring E2E command passed **7 tests**;
  explicit CLI/test Clippy with `-D warnings` passed. The real localhost HTTP
  client `compact_authoring_client.py --check-full` fetched **97 software / 135
  shipment stable refs**, with exact page/full and source/trust/promotion parity.
  The server was terminated after validation. Commands/examples and partial
  schema scope are documented in `reference/SOFTWARE_AUTHORING_TOOLS.md`.
- Final recovery checks also passed: `cargo clippy --locked --offline
  --manifest-path rust/Cargo.toml -p axiograph-cli --bin axiograph --tests --
  -D warnings`; `python3 scripts/check_book.py` (**50 unique chapters**);
  changed-doc local Markdown file-link resolution (2 targets, no heading-anchor
  claim); four changed request/host JSON fixtures and Python client syntax;
  actual CLI summary/selected/cursor commands and generated capability/manifest
  contracts. `git diff --check` passed; `git diff --cached --name-only` was empty.
  Review source is captured in `build/engineering-quality/compact-review.diff`,
  with `compact-review-manifest.json` documenting tracked/new-file provenance.
- Earlier SHA2 formatting, LSP test expectation, formatting, and schema-only
  fixture failures were corrected or recorded as unsupported input; no historical
  failed log is relabeled as a pass. Historical deferred lint advisories were not
  accepted as current evidence; explicit Clippy is the evidence.
- Logs and review artifacts are in ignored `build/engineering-quality/compact-*`.
  Independent compact review is still pending. EQ-03 nested artifact drilldown,
  EQ-04 public service/package materialization, and unrelated EQ-05 OpenAPI,
  typed SDK, nested domain schemas, and cross-product discovery remain open.

#### Compact review compatibility fix (EQ-03)

- Independent compact review found one P1 blocker: the regulated-shipment baseline
  request still selected the compact default, unlike the candidate request. The
  workflow's strict canonical evidence consumer rejected the resulting `detail`
  field. Added explicit `presentation.detail=full` to the baseline fixture; no
  consumer schema, trust gate, or interactive default was relaxed.
- Added `workflow_cli_authoring_artifacts_satisfy_canonical_consumer_contract` in
  the regulated-shipment consumer's tests. It runs the current CLI with both
  actual workflow requests (no detail override), writes temporary artifacts, and
  uses the production bounded parser, strict report type, and authoring validator
  with each exact source revision. It neither forges receipts nor mutates a store.
- Red/green evidence: before the fixture fix the new regression failed with
  `parse baseline authoring report: ... unknown field detail`; after the fix all
  **10 regulated-shipment tests passed**, including both real CLI artifacts and
  existing fail-closed/restart tests. The test builds the CLI through offline,
  locked Cargo, so it requires the CLI's cached dependencies/toolchain.
- Focused package Clippy (`--all-targets -- -D warnings`) passed. The initial
  format check found two new-test formatting differences; these were corrected.
  Final workspace format check, package Clippy, all 10 package tests, and
  `git diff --check` passed; `git diff --cached --name-only` was empty.
  Logs and stage diffs are recorded in ignored
  `build/engineering-quality/compact-fixes-*`, with the combined incremental
  review patch at `build/engineering-quality/compact-review-fixes.diff` and its
  manifest at `build/engineering-quality/compact-fixes-manifest.json`.
- Full shipment shell workflow and broad canonical-spine gate were not rerun in
  this narrow fix stage. The earlier canonical-spine result remains historical
  evidence. Independent re-review of the compatibility fix remains required;
  nested pagination, public-service extraction, and other open items are unchanged.

#### Public query/CQ library slice (EQ-04, partial EQ-17)

- Added `axiograph-query` with moved AxQL/IR/trust/checker implementations,
  deterministic CQ generation/evaluation and DTOs, and shared refinement
  identities. Existing CLI/MCP/HTTP/LSP paths use that owner. CLI retains CQ
  file/LLM translation and olog application; no process facade, source-inclusion
  shim, duplicate query semantics, or CLI dependency was introduced.
- Added four external, process-free embedding tests, a runnable `embedded_query`
  example, and separate CLI generation/workspace query/CQ/trust parity test.
  The example produced `rows=1 cq=3/3`, certificate available but not emitted,
  completeness not claimed. Initial test assumptions were corrected to the
  existing contract: unknown CQ types reject evaluation; unlowered CQs report
  residuals; same-DB mutations affecting answers reject at certificate replay,
  not merely because any unrelated entity was inserted.
- Public receipt extraction required a real authority boundary: the private wire
  DTO now feeds an opaque Serialize-only `VerifierReceiptV2`. No unchecked
  constructor or Deserialize can mint it. Wire fields remain unchanged.
  Trusted host/operator checker approval remains required. Real-checker tests
  exercise checked rejection with aligned identities and independent source,
  query, answer, and certificate mismatch rejection at the production lifecycle
  transition; the compile-fail deserialization test is additional evidence.
- Focused locked/offline validation: **111 query library unit tests, four external
  embedding tests, one compile-fail doctest, one CLI parity test, 23 authoring
  workspace/compact/adapter tests, eight MCP tests, four SQLish tests, and 14
  predictive/CQ parser tests passed**. Migrated SQLish/MCP tests now consume
  public query/security APIs instead of library-private test helpers.
- Workspace formatting and default-feature query+CLI all-target Clippy with
  `-D warnings` passed. No-default CLI `cargo check` passed. An additional strict
  no-default all-target Clippy run **failed with 40 disabled-provider/config
  warnings promoted to errors**, including unreachable/unused LLM paths and
  unused web/REPL helpers. This feature-configuration cleanup remains open;
  default Clippy is not evidence of a clean no-default lint matrix. Separately,
  strict **`axiograph-query --no-default-features --all-targets` Clippy passed**.
  No warning suppression or unrelated provider rewrite was added.
  Failed command: `cargo clippy --manifest-path rust/Cargo.toml --locked --offline
  -p axiograph-cli --no-default-features --all-targets -- -D warnings`.
  Diagnostic locations in that run: `llm.rs:708,773,7016,8433-8435`;
  `main.rs:2023-2028,2051,4535-4544,4735,4742,4765,4829,4940,4975,5038,
  5207-5213,5334,5346,5365,5428,5542-5546,6761`;
  `predictive_proposals.rs:991,1623`; `web.rs:706,762,876`; `repl.rs:27`.
  These are current failed-run locations, not a claim of a baseline lint run.
  The stage diff attributes `main.rs` changes to four library imports; provider
  function bodies were not rewritten. Full diagnostics remain in
  `public-service-no-default-clippy.log` and the review manifest.
- Full workspace authoring extraction and schema-only import materialization
  remain open. The existing `authoring_workspace_schema_only_import_failure_stays_explicit`
  regression passed; no positive dependent-instance support is claimed.
  `RUST_ARCHITECTURE_CLEANUP.md` records the exact remaining CLI dependency graph
  and why receipt-bound hydration is not an authoring substitute. Intra-AxQL/IR
  decomposition, broad suites and independent review remain later stages.
- Stage diffs (including NEW files), move-aware review aid, manifest and bounded
  command logs are under ignored `build/engineering-quality/public-service-*`.
  `public-service-review.diff` and `public-service-review-manifest.json` are the
  primary review artifacts. No files were staged and no commits/store mutations
  were performed. Final `git diff --check` passed. Review remains required.

#### Independent review history and final bounded integration (EQ-19)

Source: dirty `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`.
These results cover the implemented frontend-context, compact-authoring and public
query/CQ slices, not global engineering-quality completion or release approval.

- Native independent review history (parent-confirmed structured results):
  frontend `40554375-382b-4d1e-bbc3-ed56acdc7c71` **passed** under original
  workflow `28a9e7e6-9ed7-4e22-96ec-15b70e012998`, with the pinned gate open.
  Under recovery workflow `6edb2135-609c-43f5-9840-b8112b90b28e`, initial compact
  review `db1548ab-75fa-4fe4-bf4e-3b01db0dbadb` requested **fixes_needed** for
  the sole P1 missing explicit full baseline artifact; targeted fix review
  `2b8f122d-0388-4797-aa7d-c3c36337e27d` **passed**, without certifying broader
  recovery. Public-service review `eaf6ba8f-718e-4651-a4ec-8ef6e6d0e20f`
  **passed/no findings** from source and recorded logs, not independent reruns.
  Aggregate review `53e576dc-8d76-4435-b668-bd2c443dff62` returned
  **fixes_needed**: four named query-test selections still targeted the CLI after
  extraction and passed vacuously. Corrective evidence is recorded below;
  aggregate acceptance and the correction still require independent re-review.
- Initial full locked/offline workspace tests failed because the unchanged
  `analyze_network_and_quality_regression` unconditionally passed `--cpu-profile`
  when the optional profiling feature was disabled. A focused rerun reproduced
  `unexpected argument '--cpu-profile'`; `--no-fail-fast` confirmed this was the
  only failing target. The flag, test and feature definition were present at HEAD.
  With explicit parent approval, the integration-only source edit guards those
  two test arguments with `cfg(feature = "profiling")`. No assertion, CLI feature
  gate, production code or trust consumer changed; the test was not skipped.
- Red/green: the actual network/quality regression now passes both default and
  `--features profiling` configurations. A fresh `cargo test --locked --offline
  --manifest-path rust/Cargo.toml --workspace` **passed: 956 tests, 0 failures,
  3 ignored**, across 91 test/doctest groups. Ignored tests remain the two existing
  large-scale PathDB tests and the opt-in Docker backend test. No live provider
  calls or unrelated accepted-store mutation were requested.
- `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` and `cargo clippy
  --locked --offline --manifest-path rust/Cargo.toml --workspace --all-targets --
  -D warnings` **passed**, including final reruns after the test edit.
  `make check-cli-feature-matrix` **passed** no-default and each of
  `repl-rustyline`, `llm-ollama`, `llm-openai`, `llm-anthropic`, `profiling`, and
  `proposal-adapter-http`. These are compile checks, not a strict lint matrix;
  disabled-provider warnings and the prior no-default Clippy failure remain open.
- Initial `make verify-canonical-spine`, `make verify-semantics`, and explicit
  `make verify-regulated-shipment` exited **0**, but aggregate review found their
  named query subgates **vacuous**, not accepted: obsolete CLI selectors executed
  zero prepared-query, checker-rejection, prepared-AST-golden and shipment-query
  tests. Original `integration-canonical-spine.log:387–429`,
  `integration-semantics.log:615–659,696–733`, and
  `integration-regulated-shipment.log:45–87` are retained, not replaced by green
  reruns. Other shipment checks did validate real full baseline/candidate
  artifacts, finite-query receipts, temporary typed merge/materialization,
  restart and adversarial rejection. The independent 956-test workspace pass
  (including 111 query unit tests) is valid separate evidence, not a repair of
  these recipes. Earlier pre-extraction compact canonical-spine evidence and
  finite Lean scope are unchanged. Corrected gate evidence follows below.
- On available **Node 26.1.0 / npm 11.13.0**, `npm run typecheck`, `npm test`
  (**7 passed**), `npm run build:debug`, and `npm run build` **passed**. Debug JS
  remains 169.18 kB plus source map; production JS 107.74 kB (35 modules each).
  Actual `make verify-viz` **blocked/failed at check-node-toolchain**: required
  Node 26.8.1, found 26.1.0. Required npm is 11.19.0, found 11.13.0 (the recipe
  stops before its npm assertion). No pin overrides; the install/audit/build
  portion of this exact gate did not run. Earlier clean-install evidence remains
  historical; no new release or browser-usability acceptance is claimed.
- Actual localhost `compact_authoring_client.py --check-full` again retrieved
  **97 software / 135 shipment refs** in canonical full/page order. Raw minified
  HTTP UTF-8 full/summary bytes remain **424,354/9,071** and **643,655/9,200**;
  both summaries are <12,000 bytes and >90% smaller. Source/trust/promotion/ok
  parity passed; the read-only server was terminated. Focused authoring suite
  **23 passed**, including all 18 section unions, full wire parity, hidden
  schema-only import failure, real adapter schema and byte-budget regressions.
  Public `embedded_query` ran process-free: `rows=1 cq=3/3`, certificate available
  but not emitted, completeness not claimed.
- `python3 scripts/check_book.py` **passed (50 unique chapters)**. Changed-doc
  local Markdown file targets were checked with its resolver; this does not check
  heading anchors or rendered site links. Full `make book` and release gate,
  live providers/backends, broad browser flows and capacity tests were not run.
- Logs and aggregate tracked/new-file patches are in ignored
  `build/engineering-quality/integration-*`. `integration-review.diff` plus
  `integration-review-manifest.json` capture the aggregate source, new files,
  hashes, commands and residual risks for shell-less review. The separate
  `integration-test-fix.diff` isolates the newly approved test-only change.
  `git diff --check` passed; no files staged, commits, branches or pushes made.
- Still open: exact pinned frontend/release gates; CLI no-default strict lint
  cleanup; full authoring service and schema-only import materialization; nested
  artifact byte/drilldown bounds and complete transport schemas/SDK; broader
  frontend validation/accessibility; realistic retrieval/capacity evaluation;
  all other unchecked roadmap requirements. Do not convert these integration
  passes or previous bounded reviews into global completion.

#### Non-vacuous named query gates (aggregate P1 correction, EQ-19)

- Fixed only the four obsolete selections identified by aggregate review
  `53e576dc-8d76-4435-b668-bd2c443dff62`: the three Make recipes and the shipment
  workflow now select `axiograph-query --lib` with their original substring
  filters. No Rust production code, test assertions, accepted-store authority or
  inherited query extraction changed. The V4 recipe also fails if Lean is absent
  instead of skipping its required check.
- Added `scripts/run_required_query_tests.py`, reusing the existing bounded
  subprocess runner (600 seconds, 8 MiB per stream). A successful command must
  report one nonempty successful libtest summary and the same number of matching
  named passes. Missing/blank selectors, zero tests, all ignored tests, missing or
  inconsistent output, subprocess failures, timeout and output overflow reject.
  This is protection for these named gates, not a generic test framework or a
  claim that every repository recipe is non-vacuous. Usage and current selectors
  are documented in `docs/howto/TESTING.md`.
- Red evidence: the real obsolete `axiograph-cli` / `verifier_bridge::tests`
  library selection still makes Cargo exit 0 with **0 passed, 4 filtered out**;
  the guard now exits **1**. A nonexistent query-library filter similarly yields
  **0 passed, 111 filtered out**, guard exit **1**. The first evidence driver
  incorrectly expected Cargo's no-library status 101; the CLI does have a small
  library. The corrected assertion and fresh red rerun confirm guard exit 1;
  no source behavior was changed to accommodate the mistaken expectation.
- Green evidence from final actual logs (not inferred from banners): standalone
  rejection gate **13 passed**; prepared-query selection **6 passed**; V4
  prepared-AST-golden gate **1 passed**. `make verify-canonical-spine` passed
  **142 Rust tests, 0 failed, 0 ignored** across 63 groups, including the six
  prepared-query tests. `make verify-semantics` passed **383 Rust tests, 0 failed,
  2 existing ignored** across 61 groups; its repaired subgates executed **13
  rejection + 1 golden + 1 shipment-query** passes. Explicit
  `make verify-regulated-shipment` passed **12 Rust tests, 0 failed, 0 ignored**
  across five groups, including the **one** exact-query/missing-row rejection
  test and **10** workflow consumer tests. Counts are per invocation and overlap;
  Lean checks also completed but are not counted as Rust tests.
- All **7** focused helper tests passed, including ignored/empty selections,
  wrong/absent filters, status propagation and bounded-process failures. Initial
  Python format and shebang/executable lint failures were fixed; final Ruff
  format/lint, workspace Rust formatting, shipment shell syntax, book validation
  (**50 chapters**), changed-doc local Markdown file-target checks and
  `git diff --check` passed. Full rendered `make book` was not run.
- Original vacuous `integration-*` logs are preserved; the three command
  summaries in `integration-review-manifest.json` now explicitly annotate their
  invalid named-subgate claims. Parent-confirmed prior bounded review context
  remains in `parent-review-verdicts.json` and the correction manifest. Review
  artifacts under ignored `build/engineering-quality/` are
  `gate-fix-review.diff` (incremental against stage start, including NEW helper
  and tests), `gate-fix-review-manifest.json`, and `gate-fix-final-*.log`.
  No staging, commits, branches, pushes, deployments, publication or unrelated
  store mutation occurred; inherited dirty files outside this correction remain
  intact. Independent review `a01874e3-38bc-4546-914f-0b7a418b7b7e`
  passed with no findings. It checked all four guarded call sites, zero-match
  rejection, real test counts, checker build prerequisites, unchanged golden
  and missing-row assertions, and the corrected historical evidence.
- Parent accepts this bounded aggregate P1 correction. The earlier frontend,
  compact-response and query/CQ slices retain their recorded review scope and
  limitations; this does not close their unfinished parent work areas. Parent
  also reran the seven helper regressions and `git diff --check`; both passed.
  Checkout remains unstaged `main` at base `70c568b`. The next correctness task
  is package-aware authoring materialization for schema-only imports and
  dependent instances, not a new claim of accepted-store or Lean authority.
- Still open: strict no-default CLI Clippy (the prior 40-diagnostic failure);
  exact pinned Node/npm frontend and release gates; schema-only import
  materialization; full authoring service extraction; global/nested artifact
  byte budgets and drilldown bounds; complete schemas/SDK; and all other
  undemonstrated engineering-quality requirements. Neither these repaired gates
  nor the separate 956-test workspace result complete the global roadmap.

#### Bounded canonical-package query projection (EQ-04, pending independent review)

- Replaced authoring's per-module PathDB loop with the reusable, derived-only
  `derive_package_query_index` seam. It checks exact closure membership/revisions,
  orders by canonical closure, installs schema/theory metadata before data, and
  resolves imported instance/schema ownership through canonical IDs. Metadata-only
  packages and schema-only imports with dependent instances now work. No source
  concatenation, receipt creation, bare-image hydration, or store authority change.
- Corrected the flattened runtime-index owner used for imported fact IDs. Existing
  runtime fact-ID spelling is retained; canonical fact citations are returned
  separately. Imported Base instance provenance and fact IDs match standalone Base.
- Investigation exposed a pre-existing relation-valued-role defect: the old importer
  created same-named placeholder objects instead of linking actual facts. The
  approved bounded correction factors the shared builder into allocation and
  linking phases. Canonical typed fact references select existing nodes, including
  forward references; dangling/wrong-owner references reject. Repeated local fact
  labels in separate instances remain isolated. Regulated-shipment behavior and
  compact/full budgets are restored, not bypassed with an unsupported fixture.
- The approved scope deliberately rejects unrepresentable execution namespaces
  before DB construction. Repeated object labels under distinct schema names are
  supported with qualified selection. Repeated theory/instance labels can be
  canonically valid but reject in the derived adapter; duplicate visible schema
  labels already reject in the compiler. Reserved/virtual/qualification namespaces,
  relation/generator or role-edge collisions, relation-object generator endpoints,
  and canonical facts collapsing to one runtime identity fail closed. Full and
  compact diagnostics distinguish projection capability from canonical validity,
  preserve exact anchors, and block promotion. General identity-aware namespace
  support and full public workspace extraction remain unchecked above.
- Production regressions cover PathDB package ownership/citations, metadata-only
  lowering, imported constraints, forward fact references, multiple imports,
  scoped duplicate object/relation names, subtype membership, object generators,
  collision rejection, wrong/missing/duplicate/reordered sources, invalid/cyclic
  imports, workspace symlink/path escapes, unsaved root bytes, candidate/baseline
  import cursor invalidation, named query/CQ execution, CLI subprocess behavior,
  MCP/HTTP payload parity, and process-free query embedding. No mock compiler or
  alternate AST semantics implementation was added.
- Checks: **9** focused package tests; combined PathDB/query suites **324 passed,
  0 failed, 2 existing ignored** across 27 groups (including **111** query unit,
  **1** new package embedding, **4** existing embedding tests); workspace authoring
  **28 passed**; actual CLI package test **1 passed**. Counts overlap across runs.
  `make verify-canonical-spine`: **142 passed, 0 failed/ignored** across 67 groups,
  including the **6** required prepared-query tests. Actual guarded checker gates:
  **13** rejection tests, **1** V4 golden, **1** shipment exact-query/missing-row
  rejection. The gates retain their non-vacuous selection guard and Lean scope.
- Workspace formatting and strict all-target Clippy for PathDB/query/CLI pass.
  Initial test-fixture/compiler assumptions, a wrong CLI library test selector
  (**zero tests**, not accepted as evidence), and a test closure large-error Clippy
  finding were corrected; no lint or assertion suppression was added. Real
  software/shipment full/summary sizes remain **424,354/9,071** and
  **643,655/9,200** bytes, with >90% reduction and <12,000-byte summaries.
- Incremental source/new-file diff, manifest/context inventories, exact commands,
  and source/log hashes are under ignored `build/engineering-quality/imports-*`.
  Inherited dirty work is preserved. No staged files, commits, branches, pushes,
  deployments, accepted-state publication, or unrelated store mutation. Independent
  review remains required; this does not close EQ-04 or broaden checker claims.
- Not rerun/claimed here: pinned Node/npm frontend gate, release/deploy gates,
  no-default CLI strict lint cleanup, full workspace suite, full rendered book,
  broad browser/live-backend/capacity checks, and remaining unchecked roadmap work.

#### Package projection review correction (EQ-04, bounded P1)

- Independent import review found that preflight accepted cross-schema repeated
  labels involving explicit generators, while named AxQL qualifies relations only.
  The importer could emit `Shared.Parent` but query resolution selected `Parent`,
  silently losing answers; qualified generator queries rejected outright. Prior
  relation/relation regression evidence did not establish mixed-arrow support.
- Preflight now rejects any repeated execution label containing an explicit
  generator with `UnsupportedQueryProjection`, before DB construction. Supported
  relation/relation qualification and unique object-generator lowering are unchanged.
  The software-authoring reference now states this narrower capability contract.
  No canonical compilation, exact-byte identity, resolver, or checker changes.
- Added a process-free external regression for relation/generator in either owner
  order and generator/generator collisions, each metadata-only and populated,
  with both source-slice orders. All fixtures first compile canonically. Red run
  failed because the mixed-arrow projection succeeded; after the correction the
  regression passes. Existing positive named relation/relation queries still pass.
  Extended full/compact authoring rejection tests to both generator collision
  classes: canonical-valid flags and source/trust identity remain visible,
  promotion remains blocked, and source files remain byte-identical.
- Validation: locked/offline PathDB + query suites **325 passed, 0 failed, 2 existing
  ignored** across 27 groups; authoring workspace suite **28 passed**; actual CLI
  package adapter **1 passed**. Strict all-target PathDB/query/CLI Clippy and final
  workspace formatting passed. Initial format-only differences were corrected,
  not suppressed. Logs are `build/engineering-quality/imports-fixes-*.log`.
- Incremental patch `imports-fixes-review.diff` and
  `imports-fixes-review-manifest.json` under that ignored directory include stage
  snapshots/hashes for inherited NEW files as well as tracked edits. No staging,
  commits, branches, publication, or unrelated store mutations. This is a narrow
  review correction, not broad EQ-04 completion; independent re-review is required.
  Broad canonical/semantic gates, full workspace tests, live backends, rendered
  book, pinned Node/npm and release gates were not rerun for this correction.

#### Import-materialization integration validation (EQ-19, bounded EQ-04)

Source: dirty `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`, after
both import projection slices above. This stage found no attributable integration
defect and changed no production source or regression assertions. Earlier failed
and vacuous logs remain historical evidence, not relabeled current passes.

- Full `cargo test --locked --offline --manifest-path rust/Cargo.toml --workspace`
  passed **973 tests, 0 failed, 3 existing ignored**, across 93 test/doctest groups.
  Ignored tests are the two existing large-scale PathDB tests and opt-in Docker
  backend smoke. Tests use temporary stores/workspaces; no unrelated accepted
  state was changed. This is not live backend/provider or capacity validation.
- Default workspace strict all-target Clippy (`--workspace --all-targets --
  -D warnings`) and workspace `cargo fmt --all -- --check` passed. Cargo commands
  use `--manifest-path rust/Cargo.toml`; Clippy was locked/offline.
  `make check-cli-feature-matrix` passed all seven no-default/single-feature
  compile configurations. Its disabled-provider warnings remain visible: this
  does not repair or certify the prior strict no-default CLI Clippy failure.
- Fresh `make verify-canonical-spine` passed **142 Rust tests, 0 failed/ignored**
  across 67 groups, including the guarded **6 prepared-query** passes.
  `make verify-semantics` passed **392 Rust tests, 0 failed, 2 existing ignored**
  across 63 groups, including guarded **13 checker-rejection + 1 prepared-AST
  golden + 1 shipment exact-query/missing-row** passes. Explicit
  `make verify-regulated-shipment` passed **12 Rust tests, 0 failed/ignored**
  across five groups, including the guarded shipment query and 10 workflow
  consumer tests. Lean checks completed; these counts count Rust only and
  overlap across invocations. The seven guard-helper regressions also passed.
- Focused authoring/adapter/compact tests passed **28**, external process-free
  package embedding tests **2**, and the real CLI package adapter test **1**.
  These include positive schema-only imports and imported instances, qualified
  named query/CQ execution, explicit collision rejection, exact-source citations,
  all 18 section unions and source/trust/promotion parity. They are overlapping
  focused evidence, not tests additional to the workspace total.
- The actual localhost compact client with `--check-full` again fetched
  **97 software / 135 shipment refs** in canonical page/full order. Raw minified
  HTTP full/summary sizes are **424,354/9,071** and **643,655/9,200 bytes**;
  both summaries are <12,000 bytes and >90% smaller. Unit payload regressions
  independently reproduced these sizes. This remains a fixture ceiling, not a
  global response, memory, computation, or nested-artifact byte bound.
- A fresh temporary schema-only `Base` / imported `Family` instance example
  passed real HTTP authoring, prepared-query and CQ checks (**1/1 satisfied**).
  Its **11 refs** matched full/pages; HTTP full equaled the actual CLI full
  report. Summary/full source, trust, promotion and `ok` matched, unsaved root
  bytes were used without changing files, and protected-main eligibility stayed
  false. Its full/summary sizes were **56,744/8,849 bytes** (no >90% reduction
  claim for this small example). Both read-only servers were terminated and the
  temporary workspace removed. No accepted receipt or promotion was minted.
- `python3 scripts/check_book.py` and `git diff --check` passed; the index remains
  empty. Current tool versions are Rust/Cargo **1.98.0**, Node **26.1.0**, npm
  **11.13.0**. Exact Node **26.8.1** / npm **11.19.0** remain unavailable: no
  frontend gate rerun, pin bypass, full rendered book or release claim here.
- Current logs, actual HTTP reports, test counts, and the executable stage-only
  HTTP evidence driver are in ignored `build/engineering-quality/imports-integration-*`.
  `imports-integration-review.diff` combines the import slice and correction
  against the saved pre-import dirty tree, including NEW files;
  `imports-integration-only.diff` isolates this validation record against stage
  start. `imports-integration-review-manifest.json` identifies both baselines,
  source/log hashes, unchanged Cargo manifests/lockfile, exact commands and
  non-vacuous selections for shell-less independent review. Final review
  `43746c71-848f-4199-8d57-dcb43c5eddfb` passed with no findings. It checked
  import projection, identity/authority preservation, actual guarded test
  execution and bounded acceptance claims. Parent accepts this restricted
  integration slice, including the separately reviewed generator-collision
  correction; general namespaces and full authoring extraction stay open.
  Parent rechecked `git diff --check` and the empty staging index. No staging,
  commit, branch, push or publication occurred.
- Still open: general identity-aware execution namespaces (bounded unsupported
  cases reject), full public authoring extraction, strict no-default CLI lint,
  exact pinned frontend/release gates, global/nested byte budgets and drilldown,
  full transport schemas/SDK, browser/accessibility, live backends/providers,
  realistic capacity/retrieval evaluation, and all undemonstrated roadmap items.
  The former schema-only import defect is resolved only for the documented
  bounded projection; these passes do not complete EQ-04 or widen Lean scope.

#### Strict minimal/provider feature boundaries (EQ-19, bounded EQ-17)

Source: dirty `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`, after
accepted restricted import integration. Earlier 40-diagnostic minimal Clippy
failure and 973-test integration results remain historical, not rewritten.

- Provider-only helpers, imports, endpoint/model bindings, tool-loop arguments,
  embedding timeout preparation and pinned HTTP methods now compile with their
  actual consumers. Predictive dispatch returns directly for disabled/mock/command
  cases; enabled providers share the existing content parse/normalization path.
  Draft discovery reuses its already validated base draft when no suggestions
  exist. Simple REPL return and feature-scoped completion construction are fixed.
  No provider defaults, supported flags, enabled-provider requests/security checks,
  exact-byte anchors, receipt/promotion authority or accepted stores changed.
  No warning suppressions or dummy argument uses were added.
- Added opt-in `make lint-cli-feature-matrix`: **nine** locked/offline strict
  all-target Clippy invocations for query + CLI, covering default, no-default,
  each of `repl-rustyline`, `llm-ollama`, `llm-openai`, `llm-anthropic`, `profiling`,
  `proposal-adapter-http` alone, and all-features. All pass. This is not exhaustive
  feature-powerset testing; the original compile-only matrix and release-gate
  prerequisites are unchanged.
- Added actual CLI subprocess regressions for unavailable Ollama/OpenAI/Anthropic
  through draft, augmentation and predictive-plugin entrypoints. They require
  explicit unavailable-feature errors, untouched output sentinels/no trace output,
  and no connection to a bound loopback canary. Child environments are cleared;
  no real credentials or providers are used. Positive no-provider discovery,
  exclusive selection, mock evidence output and exact-source tamper rejection
  remain exercised even when every provider is compiled.
- The new test target passed in all nine configurations: **2 default, 5 minimal,
  5 rustyline, 4 Ollama, 4 OpenAI, 4 Anthropic, 5 profiling, 5 HTTP adapter,
  2 all-features** (36 passes across invocations, overlapping tests). Minimal
  CLI `--bin axiograph --lib` passed **254 tests**. Fresh locked/offline default
  workspace tests passed **975 tests, 0 failed, 3 existing ignored**, across
  **94** test/doctest groups, including query embedding, compact/import/receipt
  regressions and existing LLM parser/endpoint and web peer-pinning tests. The ignored tests
  remain two existing large-scale PathDB cases and opt-in Docker backend smoke.
- Fresh default workspace strict all-target Clippy, workspace formatting,
  `python3 scripts/check_book.py` (**50 chapters**) and `git diff --check` passed.
  Exact commands are in `howto/TESTING.md` and the stage manifest. Early stage
  checks exposed two newly scoped imports, rustyline-only unused mutability and
  an initially too-narrow embedding-timeout cfg (OpenAI also consumes it); all
  were corrected and the complete matrix rerun. Failed logs remain recorded.
- Stage-start tracked/new-file snapshot, incremental patch (including NEW test),
  source range map, exact command/status/count records and logs are under ignored
  `build/engineering-quality/features-*`; primary review artifacts are
  `features-review.diff` and `features-review-manifest.json`. All inherited dirty
  work is preserved, Cargo manifests/lockfile are unchanged, and nothing is staged.
  Independent review remains required; this is not global EQ-17/EQ-19 completion.
- Not rerun/claimed here: canonical/semantic/shipment Make gates, live providers
  or backends, pinned frontend/release gates, rendered book, HTTP compact-client
  measurements, browser/accessibility or capacity evaluations. Their prior bounded
  evidence and every other unchecked roadmap requirement retain their scope.

#### Feature-boundary final integration (EQ-19, bounded validation)

Source: dirty `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`, after
the strict minimal/provider slice above. No attributable integration defect was
found: this stage changes only this validation record, not production code, test
assertions, feature defaults, exact-byte/trust/promotion contracts or accepted state.

- Fresh locked/offline workspace tests passed **975 tests, 0 failed, 3 existing
  ignored**, across **94** test/doctest groups. Ignored cases remain the two
  large-scale PathDB tests and opt-in Docker backend smoke. Temporary test stores
  and shipment run directories are isolated from unrelated accepted stores.
- Default workspace all-target strict Clippy and `make lint-cli-feature-matrix`
  passed. The latter executed **nine** successful locked/offline strict all-target
  query/CLI configurations: default, no-default, six individual features and
  all-features. No suppressions or feature-default changes; this is not exhaustive
  feature-powerset coverage. Workspace formatting also passed.
- Actual `feature_boundaries_cli` subprocess tests passed in all nine configurations:
  **2/5/5/4/4/4/5/5/2**, in the matrix order documented above (**36 overlapping
  passes**). Disabled-provider entrypoints reject without canary connections or
  output mutation. Focused existing local tests passed **8 security**, **9 web**
  (including peer pinning/private-target/redirect rejection) and **25 LLM**
  parser/endpoint/tool tests. No live provider request or credential use occurred.
- Fresh `make verify-canonical-spine` passed **142 Rust tests, 0 failed/ignored**
  across **71** groups, including **6 guarded named prepared-query passes**.
  `make verify-semantics` passed **392 Rust tests, 0 failed, 2 existing ignored**
  across **65** groups, including **13 guarded checker-rejection + 1 prepared-AST
  golden + 1 shipment exact-query/missing-row** passes. Explicit
  `make verify-regulated-shipment` passed **12 Rust tests, 0 failed/ignored**
  across **5** groups, including the **1 guarded shipment query** and **10 workflow
  consumer tests**. Lean checks completed; counts are Rust-only and overlap across
  invocations. Extra empty filtered groups from the new CLI test target do not
  replace the nonempty named gate checks. All **7 guard-helper regressions** passed.
- Focused compact/import/adapter authoring tests passed **28**, process-free
  package embedding **2**, and actual CLI package integration **1**. The authoring
  byte-budget test again measured software **424,354 full / 9,071 summary bytes**
  and shipment **643,655 / 9,200**, preserving <12,000-byte fixture summaries and
  >90% reduction. Page unions, hidden failures, exact sources, import collisions,
  and source/trust/promotion parity remain tested. No fresh HTTP compact-client
  measurement or universal/nested-artifact byte bound is claimed.
- Book graph validation passed (**50 chapters**); final formatting and
  `git diff --check` passed and the index is empty. The recorded exact Node
  **26.8.1** / npm **11.19.0** gate remains open; no pin bypass, frontend/release
  gate, rendered book, live backend/provider, browser or capacity run is claimed.
- Evidence is in ignored `build/engineering-quality/features-integration-*`:
  `features-integration-review.diff` includes the entire feature slice against
  its saved pre-feature dirty tree, including the NEW subprocess test;
  `features-integration-only.diff` isolates this record against integration start.
  `features-integration-review-manifest.json` records source/log hashes, both
  baselines, exact command/status/counts, actual named gate passes and source/log
  navigation for shell-less review. All inherited work remains byte-identical
  except this appended roadmap record. No files staged, commits, branches,
  pushes, publication or unrelated accepted-store mutations. Final independent
  review `62c05be6-504e-4f0f-83a7-9825ecbd42b3` passed with no findings;
  source review `c32803b9-535f-48e2-b088-125c8f128b2e` also passed. Parent accepts
  this bounded feature-boundary slice: strict minimal/provider configurations
  now pass without changing feature defaults or suppressing diagnostics.
  Parent rechecked the unstaged `main` state and `git diff --check`.
- Still open: general identity-aware namespaces, full public authoring extraction,
  global/nested byte budgets and drilldown, full transport schemas/SDK, exact
  frontend/release gates, browser/accessibility, formal coverage, realistic
  retrieval/capacity evaluation and every other unchecked roadmap requirement.
  These checks close neither the broader provider decomposition nor the roadmap.

#### Source-diagnostics launch recovery

- Workflow `8b896e3a-f884-4597-9632-97826ce88aa6` failed JavaScript parsing
  before any child started: `Unexpected token (6:1094)`. It produced no source
  diagnostics implementation or validation evidence.
- Parent confirmed zero child launches, no staged files, and the existing single
  `main` worktree at base `70c568b`. The inherited 44 tracked changes and 29 new
  files were preserved in a recovery diff/state snapshot under
  `/tmp/axiograph-diagnostics-launch-recovery-99c7dnld` before retry.
- The corrected statement-body script uses quoted paragraph arrays and is saved
  at ignored `build/engineering-quality/source-diagnostics.workflow.txt`.
  Native offline workflow validation returned `ok: true`, with no errors.
  Retry uses the same native subagent protocol; no execution-mode fallback or
  product/source workaround was used. EQ-06/EQ-07 acceptance remains open.

#### Parser-owned role-carrier diagnostics (EQ-06, necessary EQ-07 projection)

- Implemented the approved bounded seam for unknown object/relation-object role
  carriers, including nested indexed/refined bases. The actual parser emits a
  nonserialized occurrence-indexed source map; the actual compiler wraps only
  those failures with structured module/schema/relation/role context. Display,
  fail-fast order, AST/IR serialization and exact-byte semantic identities are
  preserved. Other errors remain explicitly unlocated; no second recognizer,
  rendered-message token search, grammar expansion or auto-repair was added.
- The bounded import resolver retains the selected filename with the same opened
  source image. Full/compact authoring diagnostics now expose optional exact
  filename/module/revision, half-open UTF-8 bytes, one-based scalar coordinates,
  zero-based LSP UTF-16 coordinates, a clipped 240-scalar excerpt and conservative
  namespace-specific declared-name suggestion. Syntactic subjects are not checked
  KernelRefs. Invalid sources retain failed validation and promotion blockers;
  failed closure compilation still disables cursor issuance/replay.
- LSP routes precise errors to their source URI and clears prior owner
  publications on edits/close. Unlocated errors carry an empty placeholder range
  and explicit `data.sourceLocated=false`. Disk-backed imports differing from
  open buffers do not receive stale coordinates; unsaved import overlays and new
  unsaved files remain unsupported. HTTP/MCP/LSP share the extended diagnostic
  schema, including full and selected compact diagnostics. The strict shipment
  consumer already uses JSON diagnostic values, so it required no wire relaxation.
- Regression coverage includes real parser/compiler occurrences, repeated target
  names/comments/refinement data, schema/import ownership, unsaved roots, LF/CRLF,
  Unicode whitespace, nested relation-object carriers, advisory ties/bounds,
  clipped excerpts, invalid/mismatched spans, baseline failures, ambiguous imports,
  explicit unsupported fallbacks, stale imported buffers and close clearing.
  Astral UTF-16 conversion is tested in the shared coordinate primitive; astral
  role identifiers were not invented to expand the canonical grammar.
- Focused final locked/offline DSL+kernel tests: **72 passed** across 10 groups;
  authoring/compact/adapter suite: **35 passed**; diagnostic-specific selection:
  **9 passed**; real CLI runnable error example: **1 passed**; strict shipment
  consumer suite: **10 passed**. Counts overlap. `make verify-w02-compiler` passed
  **47 Rust tests**, plus **11 Rust/Lean parse/typecheck cases, 0 failures**.
  `make verify-canonical-spine` passed **144 Rust tests**, 0 failed/ignored, across
  75 groups, including the **6 guarded prepared-query tests**. Valid source-map
  removal produces identical AST, source revision and serialized IR in a direct
  compiler regression; existing checker/golden gates remain unchanged.
- A final approved error-presentation correction removes redundant identical
  source-chain links only for the two wrapped semantic leaf classes. Structured
  causes remain directly inspectable; original Display and outer operation/file
  contexts remain intact. Compiler and real input-layer tests assert single-message
  pretty chains and typed cause access; unrelated errors remain unwrapped with
  their original anyhow types. Parent's bounded exact-symbol inventory found no
  additional production variant/downcast consumer outside the changed seams.
  Affected suites, strict lints, both compiler/spine gates and live adapters were
  rerun after this correction; the earlier logs are retained.
- Real temporary-workspace CLI and localhost HTTP full/summary/standard reports
  matched source diagnostics, trust and fail-closed promotion. Real stdio LSP
  protocol exchanges verified imported CRLF token ranges, unsaved-root correction
  and close clearing. Source files remained byte-identical, servers terminated,
  and temporary fixtures were removed. The runnable public example is
  `bash examples/software_authoring/run_source_diagnostics.sh`; transport exit 0
  means report delivery, not successful validation (`ok=false`).
- Final workspace formatting, strict all-target DSL/kernel/CLI Clippy, and strict
  no-default query/CLI Clippy passed. Early logs preserve a missing LSP test-state
  initializer, two invalid presentation test assumptions, a test-module ordering
  lint, and a canonical-spine formatting failure; each was corrected and its
  actual check rerun, not suppressed or relabeled. Review artifacts (including
  NEW files, source ranges, hashes, commands and logs) are under ignored
  `build/engineering-quality/diagnostics-review.diff` and
  `diagnostics-review-manifest.json`. Inherited dirty work is preserved, the
  index is empty, and independent review remains required.
- EQ-06/EQ-07 remain partial: other semantic/parser classes, accumulation, broader
  edit/overlay workflows and all other unchecked items remain open. No full
  workspace integration rerun, full nine-way feature matrix, exact pinned
  Node/npm gate, release/deploy, provider/backend, browser or capacity validation
  is claimed. The prior 975/0/3 workspace result remains historical; the exact
  Node/npm gate is still blocked, with no override.

#### Diagnostic review corrections (EQ-06/EQ-07, three bounded P1 fixes)

- Independent diagnostics review blocked the preceding slice on quadratic
  multiline source mapping, equivalent file URIs bypassing open-buffer checks,
  and resource-rejected editor images leaving disk/cached coordinates eligible.
  Earlier passes did not cover these cases and remain historical evidence.
- Replaced each carrier's restarted segment scan with a monotonic cursor.
  For R carriers and S segments, the cursor advances at most S times, with at
  most S+R loop probes and R containment checks: O(S+R), not O(S*R). No parser
  limits, accepted bytes, AST/IR identity or checker wire output changed.
  The production parser regression checks every exact span at 25k/50k/100k
  multiline roles, plus ordinary-parse AST parity. The largest input is
  1,788,938 bytes, inside existing 4 MiB/200,000-line limits. Debug source-map
  timings were 95,891/175,670/355,511 microseconds; ordinary parses were
  86,960/174,423/350,968 microseconds. These supplement the algorithmic bound,
  not a flaky timing assertion or a broad capacity claim.
- LSP cached images now retain the workspace-validated canonical file identity
  and exact image digest alongside each client URI. Initial routing and later
  invalidation use that identity, so `%42ase.axi` and `Base.axi` cannot bypass
  revision checks. A unique matching image publishes to its client URI; multiple
  open aliases remain unlocated even when their bytes agree. Imports are still
  disk-backed; no unsaved-import overlay or canonical compiler change was added.
- Any byte/count-rejected document update removes its stale cached image and
  sets one sticky session flag. Existing and future precise publications become
  explicitly unlocated until session restart, while compiler failures remain
  visible. The resource diagnostic states the restart requirement. This bounded
  conservative fallback does not accumulate rejected-URI tombstones or silently
  restore certainty after a close. Existing count/aggregate byte maxima remain.
- Four production framed-LSP regressions first failed against the old behavior
  (**2 existing passed / 4 new failed**). They now cover equivalent-URI importer
  revalidation, client URI routing/alias ambiguity and recovery, document-65
  rejection, and aggregate-byte-rejected edits followed by importer revalidation
  and stale-code-action rejection. Repeated rejected URIs do not grow cached
  documents/publications; disk fixtures remain byte-identical. One intermediate
  green attempt exposed an incorrect test expectation that invalidation also
  recompiles another alias's old report; explicitly revalidating that alias fixed
  the test, without relaxing the unlocated assertions. Failed logs are retained.
- Final locked/offline DSL+kernel suites passed **73 tests** across 10 groups;
  authoring/compact/adapter suite passed **39**, including the four new LSP tests.
  The dedicated parser run passed **2** tests. Strict all-target DSL/kernel/CLI
  Clippy and strict no-default query/CLI Clippy passed. `make verify-w02-compiler`
  passed **47 Rust tests + 11 Rust/Lean cases**; `make verify-canonical-spine`
  passed **144 Rust tests** across 75 groups, including **6 guarded prepared-query
  passes**. Counts overlap; all final runs have zero failures/ignored tests.
  Full/compact fixture bytes remain software **424,354/9,071**, shipment
  **643,655/9,200**, with unchanged fail-closed validation/promotion tests.
- Incremental tracked/NEW-file patch and manifest, baseline snapshots, source/log
  hashes, line navigation, exact commands/statuses and retained red logs are in
  ignored `build/engineering-quality/diagnostics-fixes-review.diff` and
  `diagnostics-fixes-review-manifest.json` (logs use `diagnostics-fixes-*`).
  Inherited dirty work is preserved; no staging, commits, branches, provider calls,
  publication or accepted-store mutation occurred. Independent re-review remains
  required. The broad roadmap remains partial; no full workspace integration,
  nine-way feature matrix, fresh live adapter/browser/capacity run, exact pinned
  Node/npm gate, rendered book, release or deployment is claimed here. The exact
  frontend toolchain gate remains blocked, without an override.

#### Diagnostics integration (EQ-06/EQ-07, bounded EQ-19)

Source: unstaged `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`.
This validates the source-diagnostic slice and its three review corrections above;
it does not rewrite earlier failures or close the broad roadmap. No attributable
integration defect was found, so this stage changes only validation documentation.

- Fresh `cargo test --locked --offline --manifest-path rust/Cargo.toml --workspace`
  passed **993 tests, 0 failed, 3 ignored across 96 groups**. Existing ignored
  coverage remains the two large-scale PathDB tests and opt-in Docker backend test.
  This supersedes the historical 975/0/3 result only for this tested source state.
- Default strict workspace all-target Clippy, `make lint-cli-feature-matrix`
  (**all nine strict configurations**) and workspace formatting passed. No lint
  suppression, feature-default changes or relaxed assertion was needed.
- `make verify-w02-compiler` passed **47 Rust tests in four groups and 11 Rust/Lean
  parse/typecheck cases, 0 failures**. `make verify-canonical-spine` passed **144
  Rust tests in 75 groups**; `make verify-semantics` passed **394 Rust tests,
  0 failed, 2 existing ignored in 67 groups**; explicit
  `make verify-regulated-shipment` passed **12 Rust tests in five groups**.
  Lean checks completed; these counts count Rust libtest summaries only and
  overlap across invocations. Shipment stores are fresh temporary build fixtures.
- Non-vacuous guarded selectors are explicit in the fresh logs: spine line 417
  reports **6 prepared-query** passes; semantics lines 664/712/1065 report
  **13 checker rejection / 1 prepared-AST golden / 1 shipment-query** passes;
  explicit shipment line 54 reports **1 shipment-query** pass. All seven helper
  regressions passed. A real nonexistent query selector made Cargo report zero
  passes and the guard exit **1**, as required; this expected rejection is not
  a successful named semantic test.
- A focused authoring/compact/adapter rerun passed **39 tests**, including URI
  aliases, count/aggregate-byte rejection, stale-coordinate clearing, unchanged
  fail-closed promotion, schema parity and compact fixture budgets. The full
  workspace also ran the real CLI error example, parser occurrence/scaling tests,
  and `parser_source_map_is_not_semantic_identity_or_ir_wire`. Exact canonical
  source, import order, semantic IDs/IR and existing checker wire/goldens remain
  unchanged; no compiler or runtime source was edited during integration.
- Fresh real CLI and localhost HTTP full/summary/selected-standard requests used
  an unsaved importing root and a CRLF `Base.axi` containing `Company`/`Compny`.
  CLI/HTTP reports agreed on filename, exact `Compny` bytes, advisory `Company`,
  one-based line **4**, scalar columns **32–38**, and zero-based UTF-16 line **3**,
  characters **31–37**. Failed validation, trust, promotion blockers and disabled
  follow-up cursors agreed across details. Actual stdio LSP exchanges verified
  those imported ranges, correction clearing, the same exact range on an unsaved
  root typo, and close clearing. Both source files remained byte-identical;
  processes were stopped and the temporary workspace removed.
- The real localhost compact client fetched canonical full/page unions of
  **97 software / 135 shipment refs** with source/trust/promotion/ok parity.
  Minified UTF-8 full/summary sizes remained **424,354/9,071** and
  **643,655/9,200** bytes: each summary is <12,000 bytes and >90% smaller.
  This is a fixture budget, not a global nested-artifact or capacity claim.
- The exact frontend toolchain prerequisite was rechecked, not overridden:
  `make check-node-toolchain` exited **2** (required Node **26.8.1**, found
  **26.1.0**). Available npm is **11.13.0**, not required **11.19.0**; the recipe
  stops before its npm assertion. Full `verify-viz`/release gates remain blocked
  and were not rerun. No provider requests, credentials, live backends, publication,
  deployments or unrelated accepted stores were used.
- `python3 scripts/check_book.py` passed (**50 unique chapters**); the two
  integration docs resolved **5 local Markdown file targets**. Heading anchors
  and rendered links were not checked. `git diff --check` passed; full rendered
  `make book` was not run.
- Fresh logs use ignored `build/engineering-quality/diagnostics-integration-*`.
  `diagnostics-integration-review.diff` includes the combined diagnostics slice
  against its pre-diagnostics snapshot, including NEW files; its manifest records
  exact commands/statuses/counts, hashes and source/log line navigation. The
  separate `diagnostics-integration-stage.diff` isolates this documentation-only
  stage against its own start snapshot. All inherited dirty work, manifests,
  lockfile and canonical fixtures are preserved. The index remains empty.
- Final independent review `98150489-f2a0-4c07-b095-b8fd55e609d6` passed with
  no findings, after re-review `eea82c68-d316-4ef2-8c00-4a96cd106f58` approved
  the three P1 corrections. Parent accepts this bounded diagnostic integration
  and rechecked `git diff --check` and the unstaged `main` state.
- Parent session diagnostics still report seven baseline frontend findings in
  `render/list.ts`, `render/detail.ts`, `core/draft.ts` and `core/status.ts`:
  six HTML sinks and one potentially unhandled parse. These remain EQ-02 review
  work, not regressions attributed to diagnostics or proven exploits. No global
  clean-scanner claim is made.
- EQ-06/EQ-07 remain partial: other diagnostic classes, accumulation, new unsaved files,
  unsaved import overlays, incremental edits and broader editor features stay
  open. Full workspace service extraction, frontend/browser/accessibility work,
  nested budgets/schema/SDK, retrieval/capacity evaluation, broader formal claims,
  pinned frontend/release and every other unchecked roadmap item remain open.

#### Bounded frontend rendering and draft-selection repair (EQ-02, pending review)

Source: unstaged `main`, HEAD `70c568bc6e28bd446da9bb60426ed62035d17af2`.
Earlier scanner findings were review leads, not established exploits. This slice
changes frontend rendering/selection and necessary caller wiring only; Rust/Lean,
canonical bytes, IR/IDs, checker behavior, receipts and accepted authority are unchanged.

- List, detail, draft and review-status paths now construct DOM nodes/text and
  use `replaceChildren` for resets. A small typed DOM builder replaces HTML
  templates without a new dependency, framework, sanitizer or parser. Grouping,
  virtual slices, highlights, shift-click, tabs, tables, rich detail sections,
  draft selection/filtering, evidence buttons and validation chips remain visible.
  Links use fixed `href="#"`; numeric IDs drive local selection or an encoded
  same-page `focus_id` query parameter. ID zero works; malformed IDs cannot
  become navigation. Source locators and hostile attributes remain text.
- The former draft JSON round trip was a copy, not raw-input parsing. Neither
  original consumer mutated its result. The replacement copies changed envelope/
  array containers and shares read-only retained records, preserving metadata and
  legitimate nullable chunk fields. Selection-relevant malformed arrays, records,
  IDs, duplicate IDs and stale selections reject explicitly before requests/output
  clearing. This is not complete frontend wire validation or a backend substitute.
- Inspection confirmed separate pre-existing runtime wiring defects: the selector
  was private while add actions called an undefined identifier; draft registered
  add handlers before their initialization; output/tab/evidence callbacks and the
  review .axi control were undeclared in their consumers. Parent approved bounded
  repairs through the same shared `appCtx`, including late-bound evidence lookup.
  The direct tool-loop prefill caller now belongs to the draft API, initialized
  before LLM captures it. Missing callbacks and selection failures give actionable
  status text without requests/output mutation. No automatic commit or promotion
  was added; backend authority and validation are unchanged.
- Parent provenance triage confirmed 20 empty catches already existed at HEAD.
  Eighteen storage/preferences/history catches remain explicitly deferred UX work.
  The two obsolete add-action status catches could fall through to admin POSTs.
  Source inspection of `db_server.rs:233–277` confirmed the actual V2 server
  routes only health/status/query, with no role or mutation endpoints. With parent
  approval, removed both legacy admin POST branches and role/token authorization
  assumptions. Buttons now say **CLI only**; every V2, legacy role-only, missing,
  malformed, non-2xx, rejected or offline status blocks with canonical check/
  authoring and AxiStore CLI guidance, preserving local drafts and output.
- Added **12 production-module regressions** using the locked esbuild loader and
  small DOM operation spies, plus extended the existing genuine injected-error
  gate with read-only selector and DOM-value type errors. Tests cover rich/hostile
  output, malformed selection/DB link boundaries, empty/reset/reinitialization,
  virtual scrolling callbacks, tabs/selection, tool-loop prefill, real module
  initialization order, deny-all commit/promote actions, malformed/unavailable
  status, missing handlers, and encoded read-only DocChunk lookup. The V2-shaped
  mock has an explicitly non-authoritative receipt: it is not a genuine server
  response or wire-conformance test. Context tests are reused unchanged. DOM
  spies are not browser/layout/accessibility evidence. Existing proposal/draft/
  DocChunk routes still do not match the current read-only backend; their mocked
  callback tests are not live successful workflows. Broader UI/backend contract
  migration remains open, without restoring obsolete endpoints.
- On available **Node 26.1.0/npm 11.13.0**, clean `npm ci --ignore-scripts`,
  `npm audit --audit-level=moderate --ignore-scripts` (**0 vulnerabilities**),
  `npm run typecheck`, `npm test` (**19 passed**), `npm run build:debug`, then
  `npm run build` all passed, including final post-format/post-contract reruns.
  Both builds execute actual typechecks. Production is **37 modules / 102,382 JS
  bytes**, debug **163.44 kB** plus source map;
  these are bundle sizes, not performance or browser-usability results.
- Locked/offline Rust visualization tests passed **3**, and the existing real
  source-level query-certificate policy integration passed **1**. Actual CLI
  export of `examples/Family.axi` produced **22 nodes / 86 edges**, copying and
  inlining the exact final production asset into a temporary offline bundle;
  input bytes stayed unchanged and output was removed. Assets are runtime-read,
  not compile-time embedded. The runnable review scenario and offline inspection
  commands are in `docs/howto/TESTING.md`.
- Early test-harness errors (absent-global mocking and teardown timeout) were
  corrected by explicit fixture-global installation/restoration; no production
  workaround or test suppression was added. Timed-out fixture child processes
  were terminated. An export-driver assertion initially expected compact graph
  JSON; the corrected check parses the real embedded JSON and verifies counts.
  Historical logs retain these failures; final green runs do not relabel them.
  The earlier 18-test run and mock master-role success were pre-contract-correction
  evidence, not validation of the real backend. Final `contract-final-*` logs
  supersede those frontend source/build results with deny-all mutation coverage.
- Final parent read-only primary LSP checks reported **9 clean modules,
  0 diagnostics**, with no unsupported/unavailable/failed/inconclusive outcomes.
  Parent independently matched all **12 final tested-source hashes**. Its cached
  frontend lens view showed no error issues across **31 dispatched files**; this
  is cache scope, not a globally clean fresh scan or retraction of prior catches.
  The original HTML-sink/JSON-copy findings no longer appeared in the earlier
  fresh scan.
  No repository frontend lint/format command or standalone formatter exists in
  this environment; no dependency/script was invented. The auxiliary scan is not
  claimed globally clean; its 18 preference/cache findings are historical scoped
  evidence, not a current whole-source count (see frozen recovery below).
  Native gitleaks timed out
  at 120 seconds and is explicitly **inconclusive**, not a clean secrets result.
  Book graph validation (**50 chapters**) and `git diff --check` passed.
- Stage-start tracked/new-file preservation hashes, complete incremental diff,
  command/status records, source navigation and logs are under ignored
  `build/engineering-quality/frontend-rendering-*`; review entry points are
  `frontend-rendering-review.diff` and `frontend-rendering-review-manifest.json`.
  The index is empty; no commits, branches, publication, deployments, credentials,
  live providers or unrelated accepted stores were used. Independent review is
  required. Exact pinned Node **26.8.1**/npm **11.19.0** frontend/release gates
  remain unavailable and were not bypassed or claimed. Full browser/keyboard/
  screen-reader/layout tests, whole-frontend strictness, legacy `@ts-nocheck`
  removal, complete JSON-boundary validation and every broader EQ-02 item remain
  open. This bounded repair does not complete the engineering-quality roadmap.

#### Rendering review corrections (EQ-02, pending re-review)

- Independent read-only review requested three narrow corrections: the generated
  overlay continuation shadowed app context with a string and threw before
  highlighting; legacy LLM auto-commit handling claimed unsupported mutation;
  and two automatically formatted files no longer matched tested hashes.
  The preceding 19-test/`contract-final-*` results and parent hash comparison
  describe an earlier checkpoint, **not final review-byte validation**. The
  original logs/manifests remain historical; no failed evidence is relabeled.
- The actual LLM ask callback now uses the captured clear-highlights function
  after prefill. Auto-commit is unchecked and disabled regardless of persisted
  preferences. Removed mutation request fields, the admin-token header and
  commit-success/reload handling. Generated overlays receive evidence-only
  Review/canonical-check/authoring/AxiStore CLI guidance; query-certificate
  policy fields and local prefill/highlighting are preserved.
- Added two actual `llmAskBtn` regressions: successful generated-overlay prefill
  continues through clear/highlight/rerender without an error; all four legacy
  enabled preference spellings and commit/receipt-looking mock data cannot
  authorize mutation claims, request signaling or navigation. The second test
  also checks certificate-policy absence and `emit`/`verify`/`require_verified`
  precedence. These are mocked callback tests, not live endpoint or browser
  coverage; existing broader frontend/backend route drift remains open.
- Fresh available-tool checks passed on **Node 26.1.0/npm 11.13.0**:
  both TypeScript configurations, **14 rendering tests**, **21 total npm tests**,
  debug then production builds (**37 modules; 101,565 production JS bytes**),
  **3 Rust visualization tests** and **1 query-certificate policy integration**.
  A subsequent real CLI export copied and inlined the exact production bytes
  with **22 nodes/86 edges**, unchanged Family input and temporary output cleanup.
  All **43 frontend input files** were hashed before and after these checks;
  the final fix manifest rechecks them again against review bytes. The actual
  pinned-tool prerequisite still exits **2** (Node mismatch); no pin override
  or pinned frontend/release pass is claimed.
- Review artifacts: `build/engineering-quality/frontend-rendering-fixes-review.diff`
  and `frontend-rendering-fixes-review-manifest.json` contain the complete
  original rendering-stage incremental patch (including new files), the fix-only
  delta, both preservation baselines, final hashes, source navigation and exact
  command/status/log references. Historical review artifacts are not overwritten.
  No Rust/Lean, dependency, canonical source or accepted-state changes were made.
  Re-review remains required; broader EQ-02 stays partial.

#### Frozen rendering evidence recovery (EQ-02, review still required)

- Native automatic formatting invalidated final-byte evidence **twice** after
  validation. The second independent rereview
  (`a8cb4949-437d-481b-8ed7-3e65dade1391`) found **no additional behavioral issue**;
  only test-file drift remained. Of 43 frontend inputs, 42 matched the preceding
  fix manifest. `rendering.test.mjs` changed from SHA-256
  `7edce0bbdffb369e42602cbdf425424c1e446b3aeb6153fb83a8231a47ddbd4e` to
  `8623682cf541c386b864780ab5c8c2585dc8240680b74ae03f40ed5f8c5d527e`
  (1,051 physical lines). `llm.ts` remained 527 lines and SHA-256
  `ada7ded55de227aa96939c097fe2a505a8c4c93d4406b3d2cff75bfb9919232c`.
  Both earlier final-byte claims are historical, not current validation;
  original logs, review findings and manifests are preserved unchanged.
- Recovery changed **documentation/evidence only**, preserving reviewed source,
  tests, canonical bytes, IR/IDs, receipts and inherited dirty work. After needed
  actual source reads and parent confirmation that no further active native
  checks were required, fresh available-tool validation passed: clean
  `npm ci --ignore-scripts`, audit with scripts disabled (**0 vulnerabilities**),
  both TypeScript configurations, **21 frontend tests** (14 rendering, 6 context,
  1 real injected-error gate), debug then production builds (**37 modules;
  101,565 production JS bytes**), **3 Rust visualization tests** and **1
  query-certificate policy test**. The actual CLI copied/inlined exact production
  bytes into a temporary Family export (**22 nodes/86 edges**), preserving its
  input and removing temporary output. All 43 frontend hashes matched before/
  after every command; production was the last build. Exact Node 26.8.1/npm
  11.19.0 pins remain unmet by Node 26.1.0/npm 11.13.0; the actual prerequisite
  failed with exit 2, without a bypass or release claim.
- Current lexical enumeration across the 43-input manifest's `frontend/viz/src`
  scope finds **24 empty catch sites**, recorded with paths/lines in
  `rendering-frozen-empty-catches.json`. This is not comparable to the historical
  auxiliary scan's scoped 18 findings or a new analyzer pass. Existing storage/
  history/preference persistence and error-reporting gaps remain deferred.
  Historical primary LSP results (9 modules, then 3 correction-scope files,
  zero diagnostics) and the cached 31-file lens remain supplemental scoped
  checkpoints, not fresh current-byte/global-clean evidence. Gitleaks' 120-second
  timeout remains **inconclusive**, not a clean secrets scan.
- New evidence under `build/engineering-quality/rendering-frozen-*` includes
  exact commands/timestamps/log hashes, pre/post input hashes, preservation and
  navigation records, a complete original-stage combined diff including new
  files, and a recovery-only documentation delta. After all checks and final
  documentation, `rendering-frozen/source/` holds byte-exact `.txt` copies with
  explicit original mappings, hashes, byte counts and physical line counts.
  The entry points are `rendering-frozen-manifest.json`,
  `rendering-frozen-source-index.json`, `rendering-frozen-combined.diff` and
  `rendering-frozen-verify.py`. Review the frozen copies; the read-only
  standard-library verifier checks originals/copies without source-tool reads,
  formatting, repairs or expected-hash replacement. Frozen review and final
  parent verification remain required; **no acceptance is claimed**.
- No live provider, credential, accepted-store mutation, staging, commit,
  branch, publication or deployment was used. No frontend/backend redesign was
  attempted: the real router still exposes health/status/query only. Browser
  commit/promote and LLM auto-commit remain unavailable; local Review/highlighting
  and query-certificate policy remain intact. Other draft/proposal/LLM/evidence
  routes remain backend drift; mocks do not establish live support. Browser,
  layout, keyboard, accessibility, exploitable-XSS conclusions, full strictness,
  JSON-boundary migration, pin/release and broader EQ-02 gates remain open.

#### Parent acceptance of the bounded rendering slice

- Frozen-source review `440125f2-a6dd-42b7-a07d-7439e1665d02` passed without
  findings. Parent independently verified the reviewed manifest, source-index
  and verifier hashes, then ran the read-only verifier successfully: **43
  frontend inputs, 67 exact original/copy pairs, 726 repository originals and
  109 evidence originals matched**. `git diff --check` passed; `main` remained
  at `70c568b` with an empty staging index.
- Parent accepts the bounded DOM rendering, selection/caller wiring, truthful
  read-only controls and LLM continuation corrections, supported by the fresh
  **21-test** frontend run, builds and export checks above. This does not close
  EQ-02, restore unsupported backend routes, establish browser/accessibility
  coverage or satisfy the pinned frontend/release gate.
- This append-only acceptance record postdates the frozen documentation copy.
  Frozen evidence remains unchanged; the verifier result describes the state
  immediately before this record. Frontend input bytes are rechecked separately
  after the documentation append. No production/test source is changed by
  parent acceptance.

#### Read-only database client (EQ-02/EQ-05, pending review)

- Adds a shared, mechanically checked read-only discovery profile, descriptive IR
  schema and fixed optional `/viz` startup cache over an authenticated immutable
  image. Existing finite compilation/execution, receipt opening and HTTP resource
  guards are unchanged; no mutation/LLM/proposal/evidence routes are restored.
- Browser finite queries send only `{query: QueryIrV1}`. The strict boundary checks
  status/results/trust/non-claims and streamed bytes/shape; unknown/malformed and
  superseded replies cannot apply partial state. Full output remains inspectable,
  truncation is conspicuous, and graph highlighting is unavailable without an
  atomic image binding. Receipt metadata is opaque, not client-minted authority,
  HTTP authentication or a query certificate. The query schema describes a profile,
  not exact serde/semantic equivalence; the explicit object-branch defect is fixed
  and paired serde-only acceptance cases are documented/tested.
- Removed obsolete remote requests from server discovery, query, LLM, predictive
  proposals, draft generation, automatic describe and path certification. Disabled
  controls and programmatic callbacks explain unavailability. Local graph/draft
  inspection and direct prefill/selection callback coverage remain; older tests
  that mocked unsupported successful endpoints now assert zero remote requests.
  Historical rendering records above are unchanged, not retroactively relabeled.
- UI preparation preflights borrowed repeated string references/attribute counts
  before owned extraction, caps graph/label growth and streamed JSON/assets/inlined
  output, and caches only a fixed same-origin page. Missing assets or UI-budget
  rejection leaves health/status/query usable; tampered materializations still
  reject before listener publication. No filesystem serving, CORS, sanitizer,
  dependency, accepted-byte/IR/identity or authority redesign was added.
- The runnable [temporary-store workflow](../howto/TESTING.md#read-only-database-client-workflow)
  uses production AxiStore publication/opening and the production TS client against
  real processes. Synthetic logical/support blobs are explicitly not checker
  evidence; receipt-bound image opening, nonempty exact/approximate/truncated
  queries, malformed rejection and available/unavailable HTTP pages are exercised.
  This is not browser/layout/accessibility or a full release/roadmap gate.
  The production TS/bundle workflow is an explicitly invoked Rust example gate;
  default Rust e2e tests use controlled temporary assets, contain substantive HTTP
  assertions, and do not depend on Node/npm or ambient dist. Shared fixture setup
  lives under tests/support and exposes no production API.
- Checkpoint failures are retained under `build/engineering-quality/read-only-client-logs`:
  an initial Rust PathBuf borrow error and outdated frontend test expectations
  were corrected. A full-workspace run then found the old source-level test that
  required nonexistent LLM certificate requests; it now asserts the supported
  read-only boundary, while canonical policy checks on supported adapters remain.
  Native formatter drift after checkpoints required fresh final
  validation; automatic hasOwn rewriting conflicted with ES2020 and was replaced
  by set-based own-key validation without suppressions or toolchain changes.
  Exact Node 26.8.1/npm 11.19.0 pins remain unmet by available 26.1.0/11.13.0.
  Historical gitleaks timeout remains inconclusive; no global diagnostics-clean,
  whole-graph boundary, browser or pinned release claim is made.
- New stage evidence: `read-only-client-review.diff`,
  `read-only-client-review-manifest.json`, frozen `.txt` source index and read-only
  verifier under `build/engineering-quality`. Earlier `rendering-frozen-*` evidence
  is preserved. Independent review and parent live-hash verification are required;
  no acceptance or broader EQ completion is claimed. No staging, commits, branches,
  publication, deployment, credentials or live providers were used; store changes
  were confined to authorized fresh temporary test fixtures.

#### Read-only client timeout recovery checkpoint

- Workflow `70269270-473f-4f60-8cac-38997b0e4eb5` stopped before review when worker
  `e2c19a3a-182f-4caa-a7ca-042a229ebbae` timed out after **3,600,000 ms**.
  The native harness confirmed terminal process state. Implementation and
  checkpoint logs remain; the final review diff, manifest and freeze named above
  had **not been emitted**. No final-byte or independent-review pass is claimed.
- Parent captured the partial diff, exact dirty-file copies and repository hashes
  in `/tmp/axiograph-readonly-client-recovery-x9q9783b/`. The capture covers 732
  physical source files and four deleted tracked paths, including all 44 untracked
  source files. All captured hashes matched on recheck. The single unstaged
  `main` worktree remains at `70c568b`; the inherited changes are preserved.
- Recovery must use the same native subagent protocol. It must inspect the
  interrupted final-check driver, preserve prior failures and formatter history,
  finish validation, and produce exact frozen copies before independent review
  and parent verification. The later formatter notice for `db_server.rs` is
  another reason to validate current bytes rather than reuse checkpoint claims.

#### Read-only client recovered validation and freeze (review required)

- Same-protocol recovery preserved the original interrupted driver, all earlier
  logs, original-stage snapshots and `rendering-frozen-*` evidence. All **726**
  original-stage source snapshot hashes matched; the recovery-start workspace
  capture records **736 declared paths / 732 physical files / 4 deletions** and
  hashes **4,482 prior evidence files**. The parent's timeout and formatter
  checkpoint history above remains intact. No external transcript read or
  permission change was attempted during recovery.
- Narrow recovery changes correct stale DB server/Viz tutorial/test/trust
  instructions to the actual fixed routes, QueryIrV1 JSON, runtime asset setup,
  optional UI failure behavior and receipt/query/HTTP-auth distinction. Added two
  UI regressions for exact count boundaries, repeated long attribute references,
  and morphism-label/context fanout that passes borrowed preflight but rejects at
  the bounded JSON writer. These supplement per-string/aggregate/attribute/asset
  N/N+1 tests; SQLite file size and interned string uniqueness are not allocation
  arguments, and a 4 MiB payload is not a peak-RAM claim. No production API,
  execution, Serde or authority change was added by recovery.
- Fresh `read-only-client-recovery-final-01` checks passed on available **Node
  26.1.0/npm 11.13.0**, Rust/Cargo **1.98.0**: clean `npm ci --ignore-scripts`,
  `npm audit --audit-level=moderate --ignore-scripts` (**0 vulnerabilities**),
  both TypeScript configurations, **29 frontend tests**, debug then production
  builds (**38 modules / 91,425 JS bytes**), locked/offline full workspace tests
  (**999 passed / 0 failed / 3 existing ignored / 96 groups**), default strict
  **workspace all-target** Clippy, no-default strict **CLI/query all-target**
  Clippy, and formatting. Focused nonempty selections passed **7 viz**, **3 DB**,
  **6 genuine receipt-bound HTTP**, **1 unsupported-browser-boundary**, and **1
  certificate-policy documentation** tests. Counts overlap; the pre-lane **993**
  result remains historical and separate. The prior expanded frontend checkpoint
  actually recorded **28 passed / 1 failed**, not a final all-tests pass; that
  stale empty-draft wording assertion was already repaired before recovery.
- The explicit production TypeScript/client + bundle example separately passed
  all **three cases** (UI available, UI-budget-unavailable, missing asset). Default
  Rust tests retain substantive HTTP assertions with controlled temporary assets
  and a child PATH excluding frontend tools; they do not require Node/npm/esbuild
  or ambient dist. Controlled assets are transport/template evidence, not the
  production frontend. Fixture support/logical blobs are synthetic, not genuine
  checker receipts or an acceptance-to-denotation proof; materialization receipts
  come through production AxiStore publication/opening in fresh temporary stores.
- Fresh CLI Family export via `tools viz examples/Family.axi --format html --all
  --typed-overlay` copied and inlined the exact final production asset with SHA-256
  `63d59683208d439c4e9322a2155ec7d4e6bbf198baba02fa4e09660198d5835e`, **91,425 JS
  bytes / 146,703 HTML bytes / 22 nodes / 86 edges**. Family bytes were unchanged
  and temporary output removed. The old wrong `db viz --help` exit 2 and old
  101,565-byte rendering result remain historical. Assets are runtime reads, not
  embedded by Rust compilation. Book graph passed **50 chapters**, diff check and
  empty index passed. The actual pin prerequisite still failed with exit **2**;
  no pin override or implied release pass is permitted.
- All **736 input hashes** matched before and after every first-run command. This
  intentional documentation append is followed by the same full fresh command
  list in `read-only-client-recovery-final-02`, preserving the first run rather
  than changing its expectations. Freeze emission requires that final run to
  match final inputs. `read-only-client-recovery-review.diff` compares the entire
  original-stage increment to `read-only-client-start/sources/<path>.txt`,
  including new files; `read-only-client-recovery-only.diff` is additional.
  The manifest, source index, exact `.txt` copies (including unchanged helpers and
  contracts), command/log/preservation records, read-only path-restricted verifier
  and workspace handoff all use `read-only-client-recovery-*` names. Reviewers
  must use frozen copies, not live source tools; any later drift blocks acceptance.
- Independent review and parent verification remain required: **no acceptance is
  claimed**. No staged files, commits, branches, pushes, deployment, publication,
  credentials, live providers or unrelated accepted-store mutations occurred.
  The nine-configuration and semantic/canonical final integration is not claimed
  here. No browser/layout/accessibility, global diagnostic-clean, full JSON/graph
  validation, whole EQ-02/EQ-05 closure or pinned release claim is made. The
  historical gitleaks timeout remains inconclusive.

#### Read-only client final integration (independent final review required)

- Parent confirmed independent source reviewer `fda33754-1f64-42ba-9fd8-44c121896093`
  passed without findings; native workflow `8eb03c14-9e9f-4d51-b6ac-e3f1ce1be1ab`
  advanced through its pass-only branch to integration. This authorizes checks,
  not parent acceptance or release. Integration preserved all reviewed production
  and test bytes, with only this roadmap and Testing evidence documentation
  updated. DB Server and Viz Explorer instructions were inspected and remain
  consistent with the fixed routes and supported workflow; no further edit was
  needed. No new feature, dependency, execution/Serde or authority change occurred.
- Fresh locked/offline full workspace tests passed **999 / 0 failed / 3 existing
  ignored / 96 groups**; strict default workspace all-target Clippy and the
  existing **nine-configuration** strict query/CLI matrix passed. Matrix scope is
  default, minimal, each of `repl-rustyline`, `llm-ollama`, `llm-openai`,
  `llm-anthropic`, `profiling`, `proposal-adapter-http`, and all-features; this is
  lint coverage, not all feature subsets or live-provider execution.
- Existing canonical-spine, semantics, shipment and compiler/parity gates passed
  **144**, **395 (2 existing ignored)**, **12**, and **47 Rust tests / 11 Rust-Lean
  W02 cases**, respectively. Counts overlap and remain separate from historical
  pre-lane **993** evidence. The required query-library guard reported actual
  **6 prepared-query**, **13 verifier-bridge**, **1 prepared-AST golden**, and
  **1 shipment-query** passes after checker build prerequisites. All **7** guard
  regressions passed, including empty/ignored/malformed selection rejection.
  Focused package/source-diagnostic/public-query parity passed **3** tests and
  software-authoring passed **7**. Source schema changes did not require further
  compiler, query or authoring corrections in these gates.
- Fresh `npm ci --ignore-scripts`, script-disabled audit (**0 vulnerabilities**),
  both typechecks and **29 frontend tests** passed; debug then production built
  **38 modules / 91,425 JS bytes**. Explicit production TS/client/bundle HTTP
  coverage again passed all **three** available/budget-unavailable/missing-asset
  cases, separately from default Rust tests. Focused **7 viz / 3 DB / 6 HTTP / 1
  unsupported-browser-boundary / 1 certificate-policy-docs** passes overlap the
  workspace. Default HTTP tests keep controlled assets and child PATH without
  frontend tools; they do not establish production frontend behavior. Synthetic
  logical/support blobs remain non-checker fixtures; only materialization image
  receipts use genuine repository-bound publication/opening in temporary stores.
- A fresh CLI exported actual `examples/Family.axi` using `tools viz --format html
  --all --typed-overlay`, copying/inlining production SHA-256
  `63d59683208d439c4e9322a2155ec7d4e6bbf198baba02fa4e09660198d5835e`: **91,425 JS
  bytes / 146,703 HTML bytes / 22 nodes / 86 edges**. Exact Family input stayed
  unchanged and temporary output was removed. Runtime assets are not embedded by
  Rust compilation. Formatting, **50-chapter** book graph, diff and empty-index
  checks passed. Available Rust/Cargo are **1.98.0**, Lean **4.33.1**; available
  Node **26.1.0** and npm **11.13.0** still fail required **26.8.1/11.19.0**.
  The real pin gate failed with exit **2**, not a skipped or overridden success.
- New `read-only-client-integration-final-01/` preserves the initial 30-command
  integration run with per-command before/after source hash equality, bounded
  process-tree execution and exact log hashes. This intentional documentation
  update is followed by the same complete fresh list under `final-02/`; final
  freeze requires **736 declared paths / 732 physical files / 4 deletions** to
  match before and after every command. **7,541 previous engineering-quality
  evidence files** were hashed at integration start and remain unchanged,
  including the interrupted driver, original/recovery snapshots and all earlier
  frozen artifacts. No external transcript read or permission change was needed.
- Review entry point: `build/engineering-quality/read-only-client-integration-handoff.md`.
  The new complete original-stage `review.diff` compares against
  `read-only-client-start/sources/<path>.txt` and includes new files;
  integration-only `only.diff` is additional, not a substitute. The manifest,
  separate source/hash indexes, command/log/preservation maps, exact `.txt`
  source/helper/contract copies, navigation ranges and read-only confined verifier
  all use new integration names. Review frozen text only; never invoke live
  TS/MJS/Rust source tools after validation. Any drift blocks acceptance; expected
  hashes are not replaced to conceal changes.
- Final independent review and parent verification remain required; **no
  acceptance or release is claimed**. No staging, commits, branches, push,
  deployment, publication, credentials, live providers or unrelated accepted-store
  mutation occurred. HTTP remains execution-only; image receipts are not HTTP
  auth, verified query proofs or ontology closure. No browser/layout/accessibility,
  global diagnostic-clean, full graph/JSON migration or whole EQ-02/EQ-05 closure
  is implied. Historical formatter/timeout/failure checkpoints are preserved,
  and historical gitleaks timeout remains inconclusive.

#### Parent acceptance of the bounded read-only client

- Independent source review `fda33754-1f64-42ba-9fd8-44c121896093` and final
  integration review `9132e896-8601-49e1-b297-41f8f03d23a6` passed without findings.
  Parent checked the reviewed manifest, source index, verifier and handoff hashes,
  inspected the read-only verifier, and ran it independently with exit **0**.
- The check matched **736 declared originals**, **732 original/frozen/tested
  triples**, **7,541 preserved evidence files**, **3,074 new artifacts** and
  **4 runtime assets**. The declared source inventory also matched Git's complete
  tracked/untracked inventory, including four deletions. Whitespace and
  empty-index checks passed; the single `main` worktree remains at `70c568b`.
- Parent accepts the bounded read-only discovery/client/UI slice, supported by
  **999 Rust passes**, **29 frontend passes**, the nine strict feature checks and
  the real HTTP/client/export checks above. Only the three matching checklist
  subtasks are closed. Broad EQ-02/EQ-05, graph-image binding, browser/accessibility,
  complete schemas/SDK and pinned release acceptance remain open.
- This acceptance record and three checklist updates postdate the frozen roadmap
  copy. They change no production/test source or frozen evidence. Post-record
  verification checks all other declared inputs, copies, evidence and runtime
  bytes unchanged; the original verifier result refers to the pre-record state.
- The parent's cache-only diagnostic check reported no errors, with remaining
  frontend style/complexity warnings and hints. Two stale missing-roadmap-link
  warnings were marked false-positive after checking the exact target exists.
  This was not an active project-wide scan or a clean secrets-scan claim.
