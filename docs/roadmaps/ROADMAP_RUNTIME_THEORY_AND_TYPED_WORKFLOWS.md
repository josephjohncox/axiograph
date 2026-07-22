# Runtime Theory And Typed Workflow Execution Plan

This roadmap is the current execution plan for turning Axiograph's first-pass
typed surfaces into one operational ontology-engineering runtime. It is not a
new semantic source of truth. The source of truth remains accepted canonical
`.axi` plus canonical `KernelSnapshotIr`, `SchemaPresentationIr`,
`TypedTheoryIr`, and `InstanceModelIr`.

Greenfield rule: do not preserve stale query, export, report, or carry-forward
surfaces unless they protect accepted anchors, byte-format/debug parity, Lean
verifier continuity, or a documented operational contract.

## Target State

A useful typed ontology engine should let users and agents ask:

- what objects, relations, roles, obligations, equations, rewrites, contexts,
  worlds, slices, CQs, and implementation surfaces are in scope;
- which claims are accepted, review-only, evidence-backed, runtime-checked, or
  certifiable;
- which query, migration, reconciliation, merge, backend projection, or codegen
  action is well typed under the current anchors;
- which obligations are closed only under declared finite/evidence/world
  assumptions; and
- which resolver handle or next command repairs the gap.

The implementation target is one shared spine:

```text
exact canonical .axi bytes + import closure + accepted snapshot handle
  -> CanonicalCompiler
  -> CompiledKernelSnapshot
  -> derived authoring/query/CQ/migration/reconciliation/backend/codegen reports
  -> optional Lean certificate for the supported fragment
```

## Execution Tranches

### 1. Runtime Theory Depth

Make `TheoryIr` and `RuntimeTheoryCheckReportV1` fully addressable runtime
objects for ontology engineering.

Required capabilities:

- stable refs for constraints, path equations, opaque equations, rewrite rules,
  theory subjects, context/world axes, and transport items;
- obligation-to-subject and subject-to-obligation navigation;
- endpoint-safe path and rewrite admissibility diagnostics;
- explicit context/time/evidence preservation checks;
- residual obligations for unsupported higher-order/dependent fragments;
- closure claims scoped to `finite_fragment`, `evidence_weighted`, or
  `global_indexed` assumptions; and
- no claim of full HoTT, univalence, topological closure, or global ontology
  completeness from Rust runtime reports.

Acceptance gates:

- invalid path endpoints produce typed diagnostics;
- rewrites with undeclared variables or dropped context/time axes block or
  become explicit review obligations;
- opaque equations remain addressable but non-certifiable;
- transport plans classify preserved, transported, missing object image, missing
  arrow image, and out-of-fragment obligations; and
- closure reports list assumptions, blockers, residual obligations, and
  non-claims.

### 2. Compiled IR As Universal Surface

Make `SchemaPresentationIr`, `TypedTheoryIr`, `InstanceModelIr`, and canonical
`KernelRefV2` citations the shared currency for semantics-bearing runtime
features.

Required capabilities:

- `RuntimeSemanticIndex` exposes one read-only citation surface under
  `CompiledKernelSnapshot`, covering canonical schema objects, relation objects,
  roles, generators, theory obligations, instances, and facts without defining
  alternate category or instance-functor images;
- prepared typed queries cite compiled IR ids and inferred types;
- CQ reports cite prepared-query metadata instead of raw query strings only;
- typed authoring and olog review emit IR-level refs and refinement handles;
- migration preview and semantic rebase consume theory transport objects;
- reconciliation and merge plans cite theory obligations and subjects;
- backend projection plans are compiled from IR plus capability profiles; and
- codegen/coverage overlays bind to IR refs rather than embedding DDD/fDDD
  concepts inside domain `.axi`.

Acceptance gates:

- query, CQ, migration, reconciliation, backend, and authoring reports can cite
  the same derived `RuntimeIrRef` handles under one compiled-snapshot anchor;
- one prepared-query/report family is usable from CLI, REPL, server, MCP, and CQ
  paths;
- query/cert paths reject non-canonical semantic inputs;
- every semantics-bearing report includes anchor, trust, coverage, and non-claim
  fields; and
- string-only lookups are confined to boundary/advisory surfaces.

### 3. Semantic VCS Completion

Promote semantic VCS from persisted commits plus first merge plans into the
lifecycle backbone.

Required capabilities:

- central validation for `main`, `review/*`, `evidence/*`, `evidence/proposals/*`, and tags;
- typed AxiStore catalog refs with no filesystem `HEAD`;
- state/delta commits with compact CQ/trust/coverage/theory summaries;
- dry-run merge/rebase plans that materialize resolver steps and blockers;
- reconciliation objects with explicit decisions and residual obligations;
- lifecycle transitions for proposed, reviewed, accepted, certified,
  superseded, and retracted artifacts; and
- fail-closed gates for required CQ/trust/theory blockers.

Acceptance gates:

- invalid ref transitions are rejected centrally;
- direct `evidence/proposals/* -> main` mutation is rejected;
- unresolved required resolver steps block materialization;
- merge/rebase dry-runs persist enough typed state for review; and
- semantic commits cite stored previews rather than duplicating large reports.

### 4. Typed Query And Certification Closure

Unify prepared typed query handles across user and agent surfaces.

Required capabilities:

- `CompiledFiniteQuery` is the sole execution/certifiability currency;
- typed answers preserve accepted anchors through validation and certification;
- CQs and behavior cases can carry prepared-query trust summaries;
- Lean-facing witnesses cite canonical `.axi` anchors and IR ids; and
- superseded query-result/export families stay out of public semantics.

Acceptance gates:

- CLI, REPL, server, and MCP use the same prepared-query metadata;
- certified answers cannot lose their accepted anchor;
- CQ regressions distinguish unsupported query fragments from semantic failures;
- certifiability metadata is persisted where review needs it; and
- PathDB snapshot export/import surfaces remain deleted.

### 5. Software Authoring Tooling

Keep domain representation separate from engineering overlays while making the
tooling useful for DDD/fDDD/BDD/codegen/coverage.

Required capabilities:

- pure domain `.axi` examples;
- `ToolingOverlayBundleV1` for fDDD maps, implementation surfaces, coverage
  policy, and codegen plans;
- advisory definition and coverage queries;
- enforced continuous software coverage gates;
- codegen previews for Rust, TypeScript, Python, Go, and other configured
  languages when the overlay requests them; and
- MCP/LSP surfaces that expose typed reports and next actions without mutating
  accepted ontology state.

Acceptance gates:

- examples run through validate, theory check, definition query, overlay check,
  advisory coverage query, behavior-case report, enforced coverage, codegen plan,
  and continuous check;
- enforced coverage cannot be satisfied by weak/advisory probes;
- missing code/test refs are reported as implementation obligations; and
- tooling concepts do not leak into ordinary domain `.axi` examples.

### 6. Embedding And Evidence Sidecars

Treat embeddings as anchored evidence/index sidecars.

Required capabilities:

- `EmbeddingSidecarManifestV1` with accepted ref, module digest, model, source,
  target ids, dimensionality, and trust caveats;
- `EmbeddingEvidenceOverlayV1` for advisory relationship evidence such as
  `similar_to`, `mentions`, `supports`, `implements`, `violates`, and
  `subtype_candidate`;
- embedding-derived candidates lower into proposals, preview objects, or typed
  refinement handles, never accepted facts directly; and
- vector/backend reads are liftable through trust contracts.

Acceptance gates:

- manifests and overlays serialize deterministically;
- evidence overlays state that they are advisory and non-certifying;
- proposals derived from embeddings cite their sidecar manifest; and
- semantic previews show embedding evidence as evidence-plane input.

### 7. Backend Projection Plans

Keep graph databases useful through native query interfaces while preserving
Axiograph semantic authority.

Required capabilities:

- TypeDB as the primary typed backend target for relation/role/n-ary/subtype
  projection;
- TerminusDB as the RDF/VCS-shaped secondary target for named graph,
  context/world, and branch/history projection;
- `BackendCapabilityProfileV1` and projection manifests generated from compiled
  IR;
- native-readable lower-tier views with explicit lifting contracts; and
- no backend-native mutation path that bypasses Axiograph review/promotion.

Acceptance gates:

- projection plans list preserved interfaces, semantic losses, native query
  caveats, and reconciliation boundaries;
- TypeDB/TerminusDB plan tests cover relation-object and context mapping;
- container readback tests remain optional but documented; and
- backend query pushdown is framed as optimization, not semantic authority.

### 8. Production Hardening

Turn proof/workbench behavior into durable operational guarantees.

Required capabilities:

- production `.axpd` is authenticated SQLite under AxiStore;
- canonical fact logs and deterministic digests are testable;
- stable semantic ids are used across accepted modules, facts, queries, and
  certificates;
- certified-only mode has clear failure behavior;
- fuzz/property tests cover untrusted inputs; and
- optional Miri/Kani/Loom/Verus lanes are targeted at small critical invariants.

Acceptance gates:

- deterministic golden tests for digests and canonical reports;
- obsolete PathDB export/import and binary formats remain deleted;
- canonical `.axi` remains the public semantic input path;
- production-format tests cover deterministic SQLite images, receipts, and
  verified hydration; and
- docs distinguish runtime soundness, completeness under assumptions, and
  non-claims.

## Integration Order

1. Land runtime theory report and graph hardening first.
2. Rebase prepared queries, CQ reports, and typed authoring on the hardened IR
   handles.
3. Wire the same theory/query summaries into semantic merge/rebase and
   reconciliation blockers.
4. Make software-authoring coverage consume the shared summaries.
5. Add embedding evidence as advisory preview input.
6. Generate backend projection plans from the same compiled IR.
7. Add production hardening tests after the public surfaces settle.

## Verification Set

Run the smallest set relevant to a tranche, then the combined gate before
claiming closure:

```bash
PATH=/opt/homebrew/bin:$PATH cargo fmt --manifest-path rust/Cargo.toml --check
PATH=/opt/homebrew/bin:$PATH cargo check --manifest-path rust/Cargo.toml -p axiograph-pathdb -p axiograph-cli -p axiograph-tooling-overlays -p axiograph-software-authoring
PATH=/opt/homebrew/bin:$PATH cargo test --manifest-path rust/Cargo.toml -p axiograph-pathdb runtime_theory -- --nocapture
PATH=/opt/homebrew/bin:$PATH cargo test --manifest-path rust/Cargo.toml -p axiograph-cli semantic_tools -- --nocapture
PATH=/opt/homebrew/bin:$PATH cargo test --manifest-path rust/Cargo.toml -p axiograph-cli examples_e2e -- --nocapture
git diff --check
```

Container backend tests stay opt-in:

```bash
cd rust
AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 cargo test --test backend_container_tests -- --ignored --nocapture
```
