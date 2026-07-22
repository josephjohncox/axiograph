# Rust Architecture Cleanup

**Diataxis:** Reference  
**Audience:** Rust contributors and coding agents

This page is the current factoring guide for reducing Rust-side cognitive load
without dropping active functionality.

## Architectural Constraint

The public semantic spine is:

```text
exact-byte canonical .axi import closure + accepted snapshot handle
  -> axiograph_kernel::CanonicalCompiler
  -> CompiledKernelSnapshot
  -> KernelSnapshotIr + SchemaPresentationIr + InstanceModelIr
  -> derived RuntimeModuleIndex / RuntimeSemanticIndex / RuntimeIrRef
  -> typed runtime reports and optional Lean verifier
```

Rust should make this spine easy to use. Rust should not create parallel
semantic authorities.

## Current Review Result

The latest Rust review used separate crate-boundary, runtime-theory, CLI/MCP/LSP,
software-authoring, error/report, dependency, functional-programming, and
production-hardening passes. The result is not that the system should become
smaller by deleting capability. The result is that feature-bearing code should
collapse around fewer typed surfaces.

Fixed in the current cleanup tranche:

- Runtime-theory wire enums now parse and display through their owning types, so
  CLI parsing is not a parallel string table.
- Evidence-thresholded theory closure no longer turns blockers or residual
  obligations into successful review-only judgments.
- `StringInterner` allocation is serialized so concurrent duplicate intern calls
  preserve the string/id bijection.
- Typed fact edge confidence now rejects non-finite and out-of-range values
  instead of silently clamping.
- Semantic merge selectors no longer use substring matching for typed refs.
- REPL interactive tokenization now uses the same tokenizer as scripted
  execution paths.
- MCP/tool-loop schemas for tooling overlays, coverage queries, weak probes, and
  definition queries are generated from typed DTOs instead of hand-written
  partial JSON objects.
- Overlay coverage reports and continuous repo coverage reports now have
  distinct report versions. Planned codegen languages no longer count as present
  generated coverage.
- `authoring run` gates on overlay validation, overlay coverage, and continuous
  repo coverage. Strict mode remains usable for advisory development; CI mode is
  the profile that requires concrete code refs and runtime-theory inputs.
- `chunks.json` now means typed `EvidenceChunkBundleV1`, not a bare JSON array.
  The bundle is explicitly evidence-plane and cannot be confused with canonical
  `.axi` truth. Its Rust type also has a generated JSON Schema so tool/server
  boundaries can advertise the actual contract.
- Overlay validation reports carry typed derived `RuntimeIrRef` handles instead
  of opaque JSON values. Those refs cite runtime indexes; they do not replace
  the canonical compiled-snapshot handle.
- Overlay software coverage consumes a typed `BehaviorCaseCoverageViewV1`
  boundary view instead of walking arbitrary behavior-report JSON pointers.
  Continuous coverage no longer fabricates resolved ontology-ref counts when it
  has no compiled kernel input; enforced mode fails closed on those unresolved
  refs.
- Software-authoring continuous coverage now parses
  `BehaviorCaseAuthoringReportV1` once at the file/MCP/LSP boundary and derives
  codegen, code-ref, competency, typed-ref, and runtime-theory checks from that
  typed view.
- Software-authoring LSP command names now have one constant source inside the
  crate, so capability metadata, code actions, human output, and command
  execution do not drift independently.
- Software-authoring MCP inputs now use typed overlay and query DTOs
  (`ToolingOverlayBundleV1`, `DefinitionQueryV1`, `CoverageQueryV1`) at the
  protocol boundary instead of accepting generic `Value` blobs for those
  fields. Text convenience remains only as `overlay_text`.
- Example JSON artifacts now prefer explicit top-level contract versions
  (`behavior_case_check_request_v1`, `coverage_query_v1`,
  `definition_query_bundle_v1`, backend/embedding plan versions) rather than
  anonymous untyped wire snippets.
- `axiograph-storage` has no persistence authority. It stages typed evidence in
  process memory; accepted promotion and authenticated `.axpd` publication go
  through `axiograph-store`, and process-local PathDB indexes are rebuilt after
  receipt and anchor verification.

The remaining architectural debt is concentrated in a few places:

- `axiograph-cli` still contains too much reusable semantic logic. Continue
  moving pure query, report, authoring, coverage, and VCS behavior into library
  crates.
- `axiograph-pathdb` is still several responsibilities in one crate. Keep the
  module boundaries crisp until extraction is worth the churn.
- Theory checking is improving, but `TheoryIr` still needs fully addressable
  variables, dependent contexts, transports, residuals, and admissibility
  records everywhere reports are consumed.
- Query, CQ, server, MCP, LSP, behavior, and certification paths still need to
  converge harder around one prepared-query and certificate-policy lifecycle.
- Software-authoring should collapse MCP, LSP, and CLI dispatch onto one typed
  `AuthoringAction`/`AuthoringService` registry. The current constants remove
  name drift, but tool execution logic still appears in multiple protocol
  adapters.
- Evidence, ingest, and LLM DTOs still duplicate fact/provenance concepts. The
  target is one evidence-overlay vocabulary with typed refinement handles.
- Storage now has a hard line between deterministic semantic artifacts and
  evidence/cache state. AxiStore owns accepted state and derived `.axpd`
  receipts; generic evidence staging remains non-authoritative and process-local.

## Crate Roles

| Crate | Role | Cleanup Direction |
| --- | --- | --- |
| `axiograph-dsl` | Canonical `.axi` parsing and ASTs. | Keep as syntax boundary; do not add runtime storage or query semantics here. |
| `axiograph-kernel` | Exact-byte canonical compiler, immutable accepted snapshot handle, canonical schema and finite-instance IR. | This is the only meaning compiler. Keep constructors and validation centralized here. |
| `axiograph-store` | SQLite accepted-state, immutable-object, audit, ref/tag, reconciliation, and semantic-lineage authority. | Keep one generation-CAS transaction and objects-first publication path; do not add file-pointer or direct-copy authorities. |
| `axiograph-pathdb` | Derived runtime graph/index engine, query, runtime theory, certificates. | Consume compiled snapshots; never reconstruct canonical meaning or mint accepted handles. |
| `axiograph-cli` | CLI/server/MCP/LSP orchestration. | Keep orchestration thin; move reusable logic into libraries. |
| `axiograph-tooling-overlays` | DDD/fDDD overlays, weak queries, coverage inputs. | Keep `.axi` pure by putting tooling metadata here. |
| `axiograph-software-authoring` | Shared authoring/codegen/coverage engine. | Prefer this over CLI-only implementations. |
| `axiograph-storage` | Process-local runtime evidence staging. | Keep it non-durable and non-authoritative; publication belongs to AxiStore. |
| `axiograph-llm-sync` | LLM extraction, grounding, evidence/reconciliation inputs. | Keep outputs evidence-plane until review/promotion. |
| ingest crates | Boundary importers. | Lower into canonical proposals or overlays; avoid direct accepted-state mutation. Do not keep non-compiling importer crates in the active workspace. |
| example crates | Pedagogical application packages. | Consume public library surfaces only; do not become private kernels. |

## Simplification Rules

- String lookup belongs at boundary selectors. Canonical consumers should use
  `CompiledKernelSnapshot` and its typed ids; derived reports may use
  `RuntimeIrRef` without claiming semantic authority.
- JSON is a wire format, not an internal model. Parse JSON once at CLI, MCP,
  server, or file boundaries into typed DTOs; keep report builders typed.
- Query, CQ, behavior, MCP, LSP, and server paths should share prepared-query
  and certificate-policy code instead of carrying local variants.
- Runtime-theory reports should use one report family for CLI, server, MCP,
  semantic VCS, migration, reconciliation, and authoring.
- Tooling overlays should own DDD/fDDD, BDD, implementation surfaces, codegen,
  and coverage policy. Domain `.axi` should model domain facts only.
- Evidence/embedding/LLM outputs should produce overlays, proposals, or
  refinement handles. They should not write accepted ontology meaning directly.
- Backend adapters should consume compiled IR and capability profiles. Native
  writes are drift unless re-imported through Axiograph review.

## Preferred Libraries

Use maintained crates when they lower complexity without weakening semantics:

- `clap` for CLI shape and help.
- `serde`, `serde_json`, `ciborium`, and `schemars` for typed wire/report
  payloads and schemas.
- `thiserror` for library errors and `anyhow` at orchestration boundaries.
- `tokio` for async server/tooling surfaces.
- `rmcp` for MCP server surfaces.
- `lsp-server` and `lsp-types` for LSP instead of custom JSON-RPC protocol
  types.
- `tracing` for structured diagnostics.
- `proptest` for parser/report/projection invariants.
- `assert_cmd` for CLI contracts once command tests are migrated away from
  hand-rolled process assertions.
- `insta` for stable JSON/report golden snapshots where diffs are useful.
- `wiremock` for HTTP-backed LLM/proposal-adapter tests.
- `config` for layered env/file/CLI configuration if provider/server config
  continues to grow.
- `axum` plus `tower-http` if the DB server becomes a supported API surface
  rather than a narrow internal tool.
- `typedb-driver` for live TypeDB integration instead of hand-rolled clients.
- `oxigraph` for local RDF/SPARQL execution or validation support when useful.

Do not replace domain semantics with generic crates. Use libraries for protocol,
serialization, parsing support, concurrency, diagnostics, and testing; keep
ontology typing, admissibility, anchors, and trust contracts in Axiograph code.

## Dependency Governance

- There is no first-party PathDB persistence codec. `.axpd` is one authenticated,
  bounded SQLite format under AxiStore. Old bincode/sectioned readers, custom WAL,
  checkpoint fallback, and their dependencies are deleted; old bytes are
  intentionally unsupported and must be rebuilt from accepted inputs.
- Keep `rmcp`, `lsp-server`, and `lsp-types`; the cleanup target is duplicated
  protocol glue, not those crates.
- Upgrade parser/client crates only behind focused regression tests. `sqlparser`
  in particular should be upgraded deliberately because AST churn is expected.
- Workspace audits should cover every active crate. Non-compiling design crates
  do not belong under `rust/crates/`; keep the design in docs until there is a
  maintained workspace crate with tests.

## Current High-Value Refactors

1. Make `axiograph-cli` a thin shell. Command handlers should own Clap wiring,
   filesystem/env/process boundaries, stdout/stderr, and exit codes. Pure
   semantic logic should move to libraries.
2. Collapse duplicated fact/schema/evidence DTOs. `StorableFact`,
   `StructuredFact`, ingest-layer extracted facts, and schema-context variants
   should converge behind one shared evidence/model crate.
3. Choose one LLM/provider stack. Keep provider IO behind one feature-gated
   adapter surface and make CLI, MCP, and tests consume the same library.
4. Split or facade `axiograph-pathdb` by responsibility as it stabilizes:
   storage engine, compiled IR, runtime theory, query, and certificates should
   have clear module/crate boundaries.
5. Centralize report DTOs and builders. `SemanticClaimV1`,
   `EvolutionPreviewV1`, authoring reports, coverage reports, and semantic VCS
   reports should share refs, anchors, trust classes, and diagnostics.
6. Promote importers such as STEP/IGES only when they compile against current
   DSL/IR types and have focused tests. Delete stale non-workspace Cargo
   manifests instead of carrying them as adapters.
7. Keep shared validation for `RuntimeIrRef` resolution so strict derived
   reports cannot be promoted with unresolved internal refs, while retaining
   the canonical `CompiledKernelSnapshot` anchor.
8. Centralize query certificate policy across CLI, REPL, server, MCP, CQ, and
   behavior-case paths.
9. Split `axiograph-cli` orchestration from reusable authoring/coverage/query
   library functions where command handlers are doing domain work.
10. Make `TheoryIr` items fully addressable: equations, rewrites, obligations,
   subjects, variables, dependent contexts, transports, residuals.
11. Keep derived PathDB snapshot export/import deleted; exact canonical `.axi`
    bytes are the only accepted semantic input.
12. Convert remaining evidence-storage language from “unified source of truth”
   to “runtime evidence/cache materialization.”
13. Keep examples in `examples/catalog.json`; any non-catalog example should
   either be cataloged with a clear teaching purpose or removed.

## Review Checklist

Before adding a Rust surface, answer:

- Does it consume or emit canonical `.axi`, compiled IR, typed refs, typed
  reports, or explicit evidence overlays?
- Is mutation authority still Axiograph semantic VCS/review?
- Is weak/advisory output impossible to confuse with accepted truth?
- Can the same library code serve CLI, MCP, LSP, tests, and examples?
- Is this custom code required by Axiograph semantics, or should a maintained
  crate handle it?
- Does the test name describe current behavior rather than superseded behavior?

The target is not minimal code; it is lower cognitive load under the real
constraints of typed ontology engineering.
