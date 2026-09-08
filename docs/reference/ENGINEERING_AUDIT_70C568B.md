# Engineering audit at 70c568b

**Scope:** usability, correctness, theory, dependent types, ontology authoring,
search, RAG, APIs, and code quality.

This is the baseline assessment of commit `70c568b` on `main`. It records observed
behavior before the remediation work. It is not a claim about later revisions.
The [implementation roadmap](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md) tracks
changes, acceptance checks, and remaining work.

Confidence is **high** for inspected Rust/Lean behavior and reproduced checks.
Confidence is **moderate** for interactive UX and live backend behavior.
Retrieval quality and production capacity remain **unknown** without benchmarks.

## Conclusion

Axiograph is a typed ontology workbench with proof-carrying support for selected
finite fragments. It is not a general dependently typed ontology engine.

Its strongest parts are exact canonical identities, finite query certification,
resource controls, and separation of evidence from accepted authority. Its main
gaps are frontend quality, authoring ergonomics, response size, public library
APIs, retrieval evaluation, and formal coverage beyond the supported fragments.

## Maturity assessment

These scores are engineering judgments, not benchmark measurements.

| Area | Score / 10 | Baseline assessment |
| --- | --- | --- |
| Correctness and trust discipline | 8.5 | Strong, scoped, and tested |
| Theoretical grounding | 8 | Real category/type-theoretic structures, with broader design targets |
| End-to-end dependent types | 5 | Finite indexed/refinement effects, not general DTT |
| Core CLI usability | 6.5 | Organized but source-oriented and artifact-heavy |
| Ontology-engineer UX | 5 | Useful components, not a complete workbench |
| Typed query and structural search | 8 | Typed elaboration and selected exact finite certification |
| Text and semantic retrieval | 4.5 | Basic algorithms without relevance benchmarks |
| RAG/LLM trust architecture | 8 | Explicit provenance and authority separation |
| RAG answer/retrieval maturity | 5 | Grounding discipline exceeds measured retrieval capability |
| API design | 6 | Useful contracts, weak public library surface and response granularity |
| Rust code quality | 6.5 | Clean gates, but large concentration of responsibilities |
| Frontend quality | 3.5 | Bundle builds while typechecking fails |
| Documentation | 8 | Broad coverage and honest claims, with status drift |

## Verification evidence

| Check | Observed result |
| --- | --- |
| `make verify-canonical-spine` | Passed, approximately 21 seconds |
| `make verify-semantics` | Passed, approximately 248 seconds |
| Rust results aggregated within the semantics run | 382 passed, 0 failed |
| `cd rust && cargo fmt --all -- --check` | Passed |
| `cd rust && cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cd frontend/viz && npm ci --ignore-scripts` | Passed, audit reported zero vulnerabilities |
| `cd frontend/viz && npm run build` | Passed |
| `cd frontend/viz && npx tsc --noEmit` | Failed: TS2362 and TS2363 at `src/core/context.ts:30` |
| Checked local Markdown links | 91 checked, no missing targets in that checked set |
| Git status after the audit | Clean |

Timings describe these runs, not performance guarantees. The aggregate Rust
count is not a claim of 382 distinct tests across the whole repository.

The audit did not run the complete release gate, live TypeDB/TerminusDB
containers, fuzzing, Miri, Kani, or real model-provider requests. Passing focused
gates does not replace those checks or establish production readiness.

## A. Correctness and trust

### A1. Exact accepted meaning

Exact accepted `.axi` bytes and their ordered import closure define reviewable
meaning. The canonical compiler derives stable identities and one immutable
`CompiledKernelSnapshot`. PathDB row identifiers are execution identifiers, not
semantic authority.

Relevant sources:

- `rust/crates/axiograph-kernel/src/package.rs`
- [Kernel IR](KERNEL_IR.md)
- [Rust lifecycle types](RUST_LIFECYCLE_TYPES.md)

### A2. Independent Lean checking

The trusted checker is the import closure of
`lean/Axiograph/VerifyMain.lean`, not the entire Lean tree. The baseline first-party
corpus contains 34 files, approximately 12,914 lines, 92 theorem declarations,
and 353 definitions. The scan found no first-party `axiom`, `sorry`, or `unsafe`.
That scan does not describe all transitive dependencies or the full trusted base.
The inspected trusted import closure spans 14 first-party files.

The checks include adversarial certificate rejection, identity substitution,
malformed paths, and changed presentations, not only successful examples.

### A3. Exact finite query completeness

`query_result_v4` is the strongest query result inspected. The checker parses
canonical source, reconstructs a finite index, validates the supported fragment,
and compares supplied rows with its bounded finite denotation. It rejects both
missing and extra rows and rejects truncated answers. Revision, prepared-query,
answer, certificate, and verifier identities constrain the claim.

See `lean/Axiograph/Certificate/Check.lean`, especially
`finiteQueryDenotationV4`, `ensureExactFiniteCompletenessV4_sound`, and
`verifyQueryResultProofV4Anchored`.

This does not prove source-language lowering correctness, unrestricted query
completeness, evidence exhaustiveness, or open-world ontology closure.

### A4. Operational boundaries

The [security contract](SECURITY_BOUNDARIES.md) bounds bytes, nesting, import
closures, query work, subprocesses, network access, directory traversal, and
persistence. Tests exercise rejection behavior. Important controls include:

- no-follow file handling and bounded same-handle reads;
- private temporary files and atomic publication;
- subprocess timeout, output, descendant, and concurrency limits;
- redirect validation, DNS peer pinning, and loopback-only local Ollama access;
- bounded HTTP/MCP/LSP connections, queues, frames, and documents;
- receipt-bound immutable SQLite images and authenticated PathDB hydration.

SQLite is the only durable `.axpd` format. These Rust controls are operational
safety evidence. They do not make Rust a proved semantic kernel.

### A5. Formal coverage gaps

`category_kernel_v3` reconstructs a finite category presentation from exact
source. It checks equations, contextual congruence, formal inverse cancellation,
and generator-reachability saturation. Its wire paths do not yet have an
acceptance-to-denotation preservation theorem. See
[Trusted kernel](TRUSTED_KERNEL.md), including its certificate coverage matrix.

Other limits remain explicit:

- Rust compiler and runtime checks are not proofs of the entire Rust program.
- Some theorem-bearing Lean modules, including semantic VCS checks, remain
  outside the `VerifyMain` import closure.
- Merge accounting, persistence, projections, and generated code do not become
  proved because an adjacent query carries a Lean receipt.
- User axioms are modeling commitments. Certificate checking does not establish
  their truth in the world.

## B. Theoretical grounding

### B1. Implemented structures

The mathematical content is substantive:

- finite category presentations and relation objects with projection arrows;
- endpoint-indexed paths and typed composition;
- equations and contextual congruence;
- mathlib-backed free-groupoid denotation and path laws;
- finite interpretations and dependent role witnesses;
- finite refinements and context-indexed values;
- proof-carrying context transports;
- typed holes, checked path selections, and saturation explanations.

`lean/Axiograph/Theory/Finite.lean` defines `Path`, `GroupoidPath`,
`FiniteInterpretation`, `RoleWitness`, `RefinedValue`, `ContextFamily`,
`ContextValue`, `ContextTransport`, `TypedPathHole`, and `CheckedPathSelection`.

The regulated-shipment source exposes a real finite dependency:

```axi
relation DispatchReview(ctx: Context @context, contained_batch: indexed(relation(ShipmentContainsBatch); ctx) @data, reviewer: refined(QualityReviewer; enum(QA_Lee|QA_Mora)) @data, decision: DispatchDecision @data)
```

The compiler checks earlier-role scope, matching carriers, and role kinds.

The [applied theory reference](../research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md)
uses appropriate sources on ologs, functorial data migration, categories with
families, HoTT, institutions, RDF semantics, and SHACL. Citation suitability
alone is not evidence that each mathematical framework is implemented.

### B2. Design targets, not shipped semantics

Axiograph does not implement general dependent type theory with arbitrary
`Pi`/`Sigma` formation, dependent eliminators, universe-polymorphic ontology
programs, or metavariable/unification-based dependent elaboration. It does not
provide arbitrary proposition-valued refinements, univalence, higher inductive
types, general rewrite termination/confluence, or general topos/sheaf semantics.

Categories-with-families language is a design discipline, not a complete CwF
model. Contexts are finite indexed structures. The HoTT contribution is best
called free-groupoid/path-equivalence semantics inspired by HoTT.

## C. End-to-end dependent-type support

| Stage | Baseline support and limit |
| --- | --- |
| `.axi` authoring | Indexed and finite-refined roles |
| Compilation | Canonical typed IR, stable identifiers, dependency checks |
| Runtime validation | Formation, roles, carriers, paths, selected refinements and constraints |
| Querying | Typed elaboration and restricted exact finite certification |
| Context transport | Finite structures, witnesses, and reports |
| Evolution and migration | Typed obligations and partial finite coverage |
| Lean checking | Selected category, query, and path claims |
| JSON/MCP/HTTP | Runtime fields and references, not static dependent host-language types |
| UI | Dynamic JSON and loosely typed state |

Rust branding, typestate, checked builders, and snapshot-scoped handles provide
useful dependent-type effects. They do not turn Rust into a dependently typed
language. Universal lifecycle and dependency preservation across all surfaces
remains unfinished.

## D. Usability and ontology-engineer UX

### D1. Useful foundations

`examples/Family.axi` presents readable objects, relations, contexts, temporal
roles, keys, and constraints without requiring Lean knowledge. Competency
questions connect natural-language goals to executable expectations.

The CLI groups ingestion, checking, certification, tooling, authoring, database
operations, MCP, discovery, and REPL access. The regulated-shipment workflow
executes validation, CQs, certificates, evolution, merge, persistence,
projections, grounding, and a generated test. Generated tests remain test seams,
not proofs of application logic or regulatory compliance.

### D2. Oversized reports

A software-authoring run produced approximately 426 KB of JSON. Its validation
section was approximately 271 KB. Stable references, CQs, and the prepared query
added approximately 61 KB, 57 KB, and 28 KB respectively.

The regulated-shipment candidate report was **1,018,751 bytes** on disk:
`build/regulated-shipment.Pl3Crb/candidate_authoring.json`. Major sections measured
with compact `JSON.stringify` were:

| Section | Bytes |
| --- | ---: |
| Validation | 309,587 |
| Competency questions | 105,102 |
| Stable runtime references | 82,963 |
| Prepared query | 59,131 |
| Evolution previews | 57,448 |
| Query explanation | 13,895 |

The on-disk file is pretty-printed. Section counts use compact serialization and
therefore do not sum to the on-disk size. Build paths are disposable evidence,
not the durable source of this assessment.

Default megabyte-scale MCP reports waste context and obscure next actions.
Clients need detail levels, explicit section selection, bounded pages, and
anchor-bound follow-up handles. Summaries must retain failures, trust class,
counts, truncation state, and residual obligations.

### D3. Diagnostic precision

A module with `Company` declared and `Compny` referenced failed correctly:

```text
Error: schema `Broken.Broken` relation `WorksFor` role `employer` references unknown object `Compny`
```

The CLI supplied no line, column, source excerpt, or candidate correction. The
LSP commonly emits one-character ranges at column zero. Source-precise errors
and concrete repairs should come from shared compiler services.

### D4. Minimal LSP

The authoring LSP advertises full-document synchronization, diagnostics, a
single generic code action, and one execute-command entrypoint. It does not
advertise completion, hover, definitions/references, symbols, semantic tokens,
rename, or formatting. The generic quick fix inspects the workspace rather than
applying a position-specific repair.

See `rust/crates/axiograph-cli/src/authoring_workspace.rs`,
`lsp_initialize_result`, `handle_lsp_request`, and `authoring_diagnostic_to_lsp`.

### D5. Incomplete workbench

Graph, draft, query, proposal, and review components exist. A complete typed olog
edit-to-diff-to-review-to-merge-to-promotion workbench does not. CQ-gated evolution
and lifecycle distinctions are not yet universal across mutation surfaces.

## E. Search

### E1. Typed structural querying

AxQL and query IR offer typed atoms, variable inference, schema-aware elaboration,
contexts, path/regular-path queries, explanations, bounded execution, prepared
identities, and selected exact finite certification. This is stronger than the
text retrieval subsystem.

### E2. Basic relevance retrieval

`rust/crates/axiograph-pathdb/src/text_index.rs` implements token-to-entity bitmap
indexes and any/all-token matching. This is not mature relevance ranking.

`tool_semantic_search` in `rust/crates/axiograph-cli/src/llm.rs` scans token-hash
vectors and optional model embeddings, sorts scores, and combines scores with
`max`. The inspected path is exhaustive vector scanning, not ANN retrieval.
A comment describing deterministic ANN retrieval is stale. The model score field
also uses the provider-specific name `similarity_ollama` when OpenAI can supply
that score.

The audit found no implemented HNSW index, substantive BM25 ranker, learned
reranker, calibrated/rank-based fusion, or labeled relevance benchmark suite.
No nDCG, meaningful MRR, Recall@k, or Precision@k suite was found. Absence in the
inspected/searchable sources is not a benchmark result.

## F. RAG and model grounding

### F1. Authority separation

Evidence grounding and accepted-derived grounding have separate types.
Accepted grounding requires a receipt-checked materialization. Its provenance
binds repository, snapshot, tree, module closure, kernel, materialization, query,
and selection identities. It does not certify the model answer.

Embeddings remain advisory sidecars. Similarity cannot authorize subtype,
equality, or fact promotion. Model output must pass typed validation, review,
CQs, reconciliation, and promotion. Tool loops prefer structured query IR to
unconstrained query text.

See `rust/crates/axiograph-llm-sync/src/grounding.rs`,
[Embeddings and evidence](EMBEDDINGS_AND_EVIDENCE.md), and
[LLM protocol](LLM_REPL_PLUGIN.md).

### F2. Quality gaps

Accepted grounding uses bounded keyword/substring retrieval and graph adjacency.
Its provenance is stronger than its relevance selection. Missing evaluation and
selection features include:

- labeled retrieval corpora and lexical/vector/graph ablations;
- citation support, citation recall, and answer coverage;
- contradiction-aware and diversity-aware selection;
- calibrated or rank-based fusion;
- large-corpus ANN with receipt-bound index identity;
- claim decomposition and scoped support/entailment checks;
- compact evidence packets with explicit unsupported claims.

Retrieval is not entailment. Adding a score or citation cannot enlarge the
trusted ontology or certify downstream prose.

## G. APIs

### G1. Good contracts

Versioned requests/reports, stable refs, trust contracts, bounded I/O, and
machine-readable repairs support agent integration. One authoring service serves
CLI, LSP, MCP, and HTTP. Its adapters remain read-only. Accepted writes require
AxiStore authority and the applicable gates.

### G2. Weak public library boundary

`rust/crates/axiograph-cli/src/lib.rs` exports only `proposal_adapter_boundary`
and `repl_command`. Reusable query, CQ, workspace, typed-authoring, evolution,
and LLM behavior remains inside the binary crate.

External Rust clients cannot conveniently embed those services. The
[architecture guide](RUST_ARCHITECTURE_CLEANUP.md) already calls for thin CLI
adapters over reusable libraries.

### G3. Weak transport granularity and schema

The authoring MCP exposes one broad workspace tool. Its output schema is a
permissive object with `additionalProperties: true`, despite typed Rust reports.
The HTTP surface centers on `POST /authoring`. The audit found no OpenAPI
contract, generated client SDK, or dedicated authoring client library.

Semantic MCP, authoring MCP, database HTTP, authoring HTTP, LSP, REPL, certificate
CLI, and software-authoring tools are not yet a coherent product API. Improve
shared service boundaries rather than introduce another semantic implementation.

## H. Code and documentation quality

### H1. Rust discipline and concentration

The baseline has 692 tracked files, including 199 Rust files, 34 Lean files, and
33 TypeScript/TSX files. There are approximately 181,219 Rust lines across 18
active crates. These counts exclude installed dependencies and build artifacts.

Formatting and Clippy pass. Security utilities centralize bounded I/O. Typed IDs,
checked constructors, and lifecycle wrappers are widespread. Crate roles and
cleanup direction have durable references.

However, **28 Rust files exceed 2,000 lines**. Large examples include:

| File under `rust/crates/` | Lines |
| --- | ---: |
| `axiograph-cli/src/llm.rs` | 9,794 |
| `axiograph-cli/src/axql.rs` | 9,367 |
| `axiograph-cli/src/main.rs` | 8,101 |
| `axiograph-cli/src/repl.rs` | 5,299 |
| `axiograph-kernel/src/package.rs` | 4,993 |
| `axiograph-cli/src/query_ir.rs` | 4,724 |
| `axiograph-pathdb/src/kernel_ir.rs` | 4,073 |
| `axiograph-cli/src/typed_authoring.rs` | 3,978 |
| `axiograph-cli/src/evolution_preview.rs` | 3,892 |
| `axiograph-cli/src/authoring_workspace.rs` | 3,003 |

These files concentrate responsibilities and hinder review, reuse, and change
isolation. Split by service responsibility, not arbitrary line counts.

### H2. Frontend quality

The frontend bundle builds despite TypeScript errors. `strict` is disabled and
the package has no test script. The frontend release gate audits dependencies
and builds with Vite but does not run TypeScript checking.

The concrete failure comes from sorting an untyped `Set` with `(a, b) => a - b`
in `frontend/viz/src/core/context.ts`. Fix the data contract, not just a cast.

Static diagnostics also flag loose types and `innerHTML` assignments. Inspected
call sites generally use escaping or static markup. No exploitable XSS was
established. Treat scanner results as review leads, not confirmed vulnerabilities.
Use DOM construction or a maintained sanitizer and add hostile-input tests.

### H3. Documentation quality and drift

The 120 Markdown files cover architecture, trust, theory, workflows, and tests.
The current README correctly states the product boundary. Some deeper design
and roadmap status entries are stale.

For example, `TYPE_THEORY_DESIGN.md` describes category IR serialization into
`VerifyMain` as open, although `category_kernel_v3` implements finite formation
and replay. Correct that entry without claiming a denotation theorem or full
instance-interpretation coverage. Keep baseline audits separate from living
reference contracts and roadmap completion evidence.

## Remediation priorities

The [engineering-quality roadmap](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md)
contains every action from this assessment, with stable IDs and acceptance checks.
Priority order:

1. Repair frontend gates and reduce oversized authoring responses.
2. Extract public authoring/query/CQ services and typed transport schemas.
3. Add source-precise diagnostics, useful LSP operations, and integrated review UX.
4. Measure retrieval before selecting rankers, ANN backends, and rerankers.
5. Extend finite formal coverage without changing existing non-claims prematurely.
6. Split large modules and reconcile documentation with checked behavior.

Do not broaden the product claim to general DTT, HoTT, or ontology closure.
Make the supported finite semantics usable and verifiable first.
