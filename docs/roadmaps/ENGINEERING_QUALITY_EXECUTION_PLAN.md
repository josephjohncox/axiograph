# Engineering Quality Execution Plan

**Status:** Pending parent review

This plan covers all requirements in EQ-01 through EQ-19.
It uses the frozen 1,729-line roadmap with SHA-256 `949db32a2c4f67767963ad9fab283376eeec202847cae5d1a4760f3a9aea14a8`.
The [requirements ledger](ENGINEERING_QUALITY_REQUIREMENTS_V1.json) preserves exact text, ranges, acceptance clauses, evidence, and remaining work.
The [unit DAG](ENGINEERING_QUALITY_UNITS_V1.json) contains the complete ordered implementation queue.

## Marker accounting

| State | Frozen original | Current |
| --- | ---: | ---: |
| Unchecked | 65 | 64 |
| Partial | 22 | 22 |
| Checked | 27 | 28 |
| Total | 114 | 114 |

The original had 87 incomplete occurrences.
Parent acceptance closed only the EQ-19 complete release-gate occurrence.
The catalog preserves all 87 originally incomplete occurrences and all accepted occurrences.
One preserved occurrence now records the authorized closure. The other 86 remain incomplete.
No other marker changes in this catalog.

## Wave 01 selection

These six units have no pending unit dependency.
The parent workflow must run them sequentially because the repository permits one source writer.

| Order | Unit | Areas | Scope |
| ---: | --- | --- | --- |
| 1 | `EQ-02-U01` | EQ-02 | Whole-frontend strict state migration |
| 2 | `EQ18-U01` | EQ-18 | Correct retrieval and score terminology |
| 3 | `EQ-03-U01` | EQ-03, EQ-05 | Nested validation/query/CQ drilldown and bounds |
| 4 | `AE-01` | EQ-06 | General canonical occurrence source map |
| 5 | `RG-U01` | EQ-10 | Versioned relevance corpus and metric kernel |
| 6 | `EQ16-U01` | EQ-16 | Canonical `.axi` AST and exact-byte contract |

## Complete ordered queue

A dependency can refer only to an earlier row.
The machine-readable DAG contains the full acceptance, validation, prerequisite, risk, and path details.

| Order | Unit | Areas | Dependencies | Risk | Title |
| ---: | --- | --- | --- | --- | --- |
| 1 | `EQ-02-U01` | EQ-02 | None | trust-boundary | Whole-frontend strict state migration |
| 2 | `EQ18-U01` | EQ-18 | None | trust-boundary | Correct retrieval and score terminology |
| 3 | `EQ-03-U01` | EQ-03, EQ-05 | None | trust-boundary | Nested validation/query/CQ drilldown and bounds |
| 4 | `AE-01` | EQ-06 | None | trust-boundary | General canonical occurrence source map |
| 5 | `RG-U01` | EQ-10 | None | ordinary | Versioned relevance corpus and metric kernel |
| 6 | `EQ16-U01` | EQ-16 | None | trust-boundary | Canonical `.axi` AST and exact-byte contract |
| 7 | `EQ-02-U02` | EQ-02 | `EQ-02-U01` | trust-boundary | Complete frontend JSON boundary validation |
| 8 | `AE-02` | EQ-06 | `AE-01` | ordinary | Bounded multi-diagnostic taxonomy and conservative suggestions |
| 9 | `RG-U02` | EQ-10, EQ-11 | `RG-U01` | trust-boundary | Production-path retrieval baseline and ablation report |
| 10 | `EQ16-U02` | EQ-16 | `EQ16-U01` | trust-boundary | Deterministic Rust/Lean differential parser runner |
| 11 | `EQ18-U02` | EQ-18 | `EQ18-U01` | trust-boundary | Reconcile roadmap status and claim taxonomy |
| 12 | `EQ-02-U03` | EQ-02 | `EQ-02-U01` | ordinary | Remaining rendering-sink data-flow closure |
| 13 | `RG-U03` | EQ-10, EQ-13 | `RG-U01`, `RG-U02` | trust-boundary | Citation and unsupported-answer evaluation contract |
| 14 | `EQ16-U03` | EQ-16 | `EQ16-U02` | trust-boundary | Import, dependent-role, scope, encoding, and limit corpus |
| 15 | `EQ18-U03` | EQ-18 | `EQ18-U02` | ordinary | Publish runnable tutorials for shipped compact, diagnostic, and embedding workflows |
| 16 | `EQ-03-U02` | EQ-03, EQ-05 | `EQ-03-U01` | trust-boundary | Nested evolution/olog/repair artifact drilldown |
| 17 | `AE-04` | EQ-06, EQ-07 | `AE-01` | trust-boundary | Versioned LSP document graph and unsaved import overlays |
| 18 | `RG-U04` | EQ-11 | `RG-U02` | ordinary | Maintained lexical ranker in the shared production service |
| 19 | `EQ16-U04` | EQ-16 | `EQ16-U02`, `EQ16-U03` | trust-boundary | Contract-driven drift removal without shared authority |
| 20 | `EQ17-U01` | EQ-17, EQ-19 | `EQ18-U01` | trust-boundary | Extract typed LLM provider and report contracts |
| 21 | `EQ17-U03` | EQ-17 | None | trust-boundary | Separate AxQL syntax and elaboration |
| 22 | `EQ-04-U01` | EQ-04 | `EQ-03-U02` | trust-boundary | Public workspace authoring service extraction |
| 23 | `RG-U05` | EQ-11 | `RG-U04` | trust-boundary | Typed hybrid rank fusion and request-scoped embedding reuse |
| 24 | `EQ14-U01` | EQ-14 | `EQ16-U01` | formal | Dependent retyping for category-kernel wire paths |
| 25 | `EQ17-U02` | EQ-17, EQ-19 | `EQ17-U01`, `RG-U02` | trust-boundary | Extract LLM tool execution and retrieval services |
| 26 | `EQ17-U04` | EQ-17 | `EQ17-U03` | trust-boundary | Separate query execution and certification lifecycle |
| 27 | `EQ-04-U03` | EQ-04 | `EQ-04-U01` | trust-boundary | Process-free full workspace embedding client |
| 28 | `RG-U06` | EQ-10, EQ-11 | `RG-U05` | ordinary | Post-ranking relevance regression and production workflow gate |
| 29 | `EQ14-U02` | EQ-14 | `EQ14-U01` | formal | Formal cancellation replay denotation theorem |
| 30 | `EQ-04-U02` | EQ-04, EQ-05 | `EQ-04-U01` | trust-boundary | Adapter migration to the public authoring owner |
| 31 | `EQ17-U05` | EQ-17, EQ-18 | `EQ17-U03`, `EQ17-U04`, `EQ-04-U02`, `EQ-04-U03` | trust-boundary | Extract reusable authoring dispatch and thin CLI/REPL |
| 32 | `AE-07` | EQ-07 | `AE-01`, `AE-04` | trust-boundary | Semantic tokens and exact-byte formatting previews |
| 33 | `RG-U07` | EQ-12 | `RG-U02`, `RG-U05` | trust-boundary | Maintained ANN backend with explicit exact oracle |
| 34 | `EQ14-U03` | EQ-14 | `EQ14-U01` | formal | Declared-equation congruence denotation theorem |
| 35 | `EQ17-U06` | EQ-17 | `EQ17-U03`, `EQ17-U05` | trust-boundary | Factor canonical package and runtime theory boundaries |
| 36 | `EQ-04-U04` | EQ-04 | `EQ-04-U01` | trust-boundary | Canonical identity-aware execution namespaces |
| 37 | `AE-05` | EQ-07 | `AE-01`, `AE-04`, `EQ-04-U04` | ordinary | Identity-aware LSP symbol and navigation index |
| 38 | `AE-06` | EQ-07 | `AE-02`, `AE-04`, `AE-05` | trust-boundary | Precondition-bound concrete LSP repairs |
| 39 | `AE-08` | EQ-07 | `AE-04`, `AE-05`, `AE-07` | trust-boundary | Rename with semantic and CQ impact preview |
| 40 | `RG-U08` | EQ-12 | `RG-U07` | trust-boundary | Snapshot-scoped ANN sidecar receipts and fail-closed opening |
| 41 | `EQ14-U04` | EQ-14 | `EQ14-U02`, `EQ14-U03`, `EQ16-U04` | trust-boundary | Actual `verifyV3` acceptance theorem and parity matrix |
| 42 | `EQ17-U07` | EQ-17 | `EQ17-U02`, `EQ17-U04`, `EQ17-U06` | trust-boundary | Facade PathDB runtime responsibilities and remove residual duplication |
| 43 | `EQ-05-U01` | EQ-05 | `EQ-03-U02`, `EQ-04-U01` | trust-boundary | Closed generated domain schemas |
| 44 | `AE-03` | EQ-06 | `AE-02`, `AE-04`, `EQ-04-U01`, `EQ-05-U01` | ordinary | Library-owned diagnostic parity across adapters |
| 45 | `EQ-05-U04` | EQ-05 | `EQ-04-U02`, `EQ-05-U01` | trust-boundary | Cross-surface authority and lifecycle discovery |
| 46 | `AE-09` | EQ-07 | `AE-03`, `AE-05`, `AE-06`, `AE-07`, `AE-08` | ordinary | Complete LSP capability and protocol conformance gate |
| 47 | `RG-U09` | EQ-12, EQ-10 | `RG-U01`, `RG-U07`, `RG-U08` | ordinary | ANN scale, memory, latency, build, and recall benchmark |
| 48 | `EQ15-U01` | EQ-15 | `EQ14-U04`, `EQ16-U04` | trust-boundary | Finite role and refinement checked claim |
| 49 | `EQ19-U01` | EQ-19 | `EQ17-U06` | trust-boundary | Exercise production backend projection and readback in isolated containers |
| 50 | `EQ-05-U02` | EQ-05 | `EQ-04-U02`, `EQ-05-U01` | trust-boundary | Complete mechanically checked HTTP API description |
| 51 | `AE-10` | EQ-09 | `AE-09` | trust-boundary | Uniform anchored CQ lifecycle gate |
| 52 | `RG-U10` | EQ-13, EQ-10 | `RG-U03`, `RG-U05` | trust-boundary | Compact typed evidence packets with diversity and contradiction selection |
| 53 | `EQ15-U02` | EQ-15 | `EQ14-U04`, `EQ16-U04` | trust-boundary | Finite context transport and migration-obligation certificate |
| 54 | `EQ-02-U04` | EQ-02, EQ-19 | `EQ-02-U02`, `EQ-02-U03`, `EQ-05-U04` | trust-boundary | Real-browser keyboard and accessibility workflow |
| 55 | `AE-11` | EQ-09 | `AE-10` | trust-boundary | Typed migration, mapping, and theory repair holes |
| 56 | `RG-U11` | EQ-13 | `RG-U10` | trust-boundary | Mechanically scoped claim decomposition and support decisions |
| 57 | `EQ15-U03` | EQ-15 | `EQ14-U04`, `EQ16-U04` | formal | Conditional semantic theorem for selected user rewrites |
| 58 | `EQ19-U03` | EQ-19 | `EQ17-U04`, `EQ17-U05`, `EQ17-U07` | ordinary | Establish production response and query capacity gates |
| 59 | `EQ-05-U03` | EQ-05 | `EQ-05-U02` | trust-boundary | Supported typed client SDK |
| 60 | `AE-12` | EQ-08 | `AE-09`, `AE-10`, `AE-11`, `EQ-04-U02`, `EQ-05-U03` | trust-boundary | Guided workbench session and typed presentation contract |
| 61 | `RG-U12` | EQ-13, EQ-10 | `RG-U06`, `RG-U10`, `RG-U11` | external-approval | Offline reranker evaluation before backend selection |
| 62 | `EQ15-U04` | EQ-15 | `EQ15-U01`, `EQ15-U02`, `EQ15-U03` | formal | Mechanically checked finite-fragment and trust-dependency matrix |
| 63 | `EQ-05-U05` | EQ-03, EQ-04, EQ-05 | `EQ-03-U02`, `EQ-04-U02`, `EQ-05-U04` | trust-boundary | Narrow identity-bound discovery and detail operations |
| 64 | `AE-13` | EQ-08, EQ-09 | `AE-10`, `AE-12` | trust-boundary | Stable identity lineage from workbench edit to accepted snapshot |
| 65 | `RG-U13` | EQ-10, EQ-11, EQ-12, EQ-13, EQ-18, EQ-19 | `RG-U09`, `RG-U12` | trust-boundary | Retrieval and grounding integrated acceptance evidence |
| 66 | `AE-14` | EQ-08, EQ-09 | `AE-10`, `AE-13` | external-approval | Explicit reviewed workbench promotion boundary |
| 67 | `EQ19-U05` | EQ-19 | `EQ17-U01`, `EQ17-U02` | external-approval | Run bounded live model-provider integrations after owner approval |
| 68 | `AE-15` | EQ-08 | `AE-12`, `AE-13`, `AE-14` | external-approval | Real browser ontology-engineer acceptance scenario |
| 69 | `EQ18-U04` | EQ-18 | `EQ-02-U04`, `RG-U13`, `AE-15` | trust-boundary | Complete relevance and workbench tutorials when features ship |
| 70 | `AE-16` | EQ-09 | `AE-10`, `AE-11`, `AE-13`, `AE-14`, `AE-15` | trust-boundary | Cross-surface lifecycle truth and strict-gate conformance |

## Duplicate mapping

The catalog keeps every panel object in the local evidence archive.
It removes duplicate execution slots with the following explicit mapping.

| Panel unit | Canonical unit | Disposition |
| --- | --- | --- |
| `EQ19-U00` | No execution unit | This object preserves completed release evidence. It has no pending implementation and must not consume a wave slot. |
| `EQ19-U02` | `EQ-02-U04` | Both objects require the same real-browser read-only keyboard, layout, accessibility, stale-response, and authority workflow. |
| `EQ19-U04` | `RG-U13` | Both objects integrate the same production retrieval evaluation, capacity, exact-oracle, evidence, and tutorial workflow. |

## Execution constraints

- Keep one source and Git writer in the checkout.
- Compare each unit with its complete tracked and new-file baseline.
- Review each coherent unit before wider changes.
- Keep all checklist markers unchanged until parent acceptance.
- Do not commit, push, move tags, release, publish, deploy, or mutate unrelated accepted stores.
- Run applicable gates on uncommitted work. Report the clean-source release gate as pending.
- Stop before an authority change that needs owner or trust-boundary approval.
- Report missing tools, services, credentials, or capacity as blocked or skipped. Never report them as passed.

## Scope limits

- The accepted release proves only the pinned released source and operational gates. It does not close broader product requirements.
- A bounded accepted subtask does not close its partial or unchecked parent requirement.
- Accepted `.axi` bytes plus compiled IR remain the meaning plane. SQLite and AxiStore receipts remain the only durable `.axpd` authority.
- Retrieval, model output, scores, generated prose, browser state, and backend readback remain evidence only.
- A full scan is not ANN. The exact oracle must be explicit and must not act as a hidden fallback.
- DOM spies do not satisfy browser, layout, keyboard, focus, or accessibility acceptance.
- Public wire metadata cannot mint accepted, certificate-emitted, Lean-verified, or receipt-backed state.
- Finite Lean work does not establish general DTT, HoTT, arbitrary confluence, open-world completeness, ontology closure, or reversible ontology data maps.
- This catalog changes no checklist marker, accepted store, Git ref, release, package, image, deployment, or external account.

## Local reconstruction evidence

The ignored `build/engineering-quality/roadmap-wave-01/catalog/` directory contains the five full panel objects.
It also contains frozen ledger and plan copies, coverage checks, navigation, and a hash manifest.
The versioned JSON files remain the durable source after local evidence cleanup.
