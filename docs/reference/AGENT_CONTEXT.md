# Agent Context

This document holds the durable context that used to live directly in
`AGENTS.md`. Keep `AGENTS.md` short and update this document when the current
technical reality changes.

## Current State

- The repo builds Rust plus Lean from the top-level `Makefile`.
- Rust is the runtime, compiler, ingestion, query, storage, and tool surface.
- Lean is the trusted checker for the currently supported certificate fragment.
- Canonical accepted `.axi` modules are the meaning plane.
- PathDB and authenticated SQLite `.axpd` materializations are derived execution/query substrates.
- AxiStore is the sole accepted-state, semantic-lineage, receipt, and `.axpd`
  publication authority; there is no custom PathDB WAL.
- The TypeScript viz frontend lives in `frontend/viz/`; server/tooling expects
  built assets from `frontend/viz/dist`.

## Protocol And Core Infrastructure

Prefer maintained, widely used Rust crates for protocol and core
infrastructure unless Axiograph semantics require custom logic. Custom framing,
dispatch, parsers, or runtimes should stay narrow, typed, and justified by the
ontology/workbench layer rather than by generic protocol plumbing.

Current protocol choices:

- MCP stdio servers use `rmcp` for host lifecycle, transport, framing, tool
  listing, and tool calls. Local JSON-RPC helpers are test helpers, not the
  production server contract.
- LSP/editor surfaces use `lsp-server` for stdio transport/framing and
  `lsp-types` for protocol capability and request/response types. Axiograph
  code owns diagnostics, commands, and typed authoring reports.
- The DB HTTP server uses `hyper` with `http-body-util` for HTTP serving and
  body handling. Axiograph-specific code should stay at route dispatch,
  request validation, typed reports, and snapshot/query semantics.

## Runtime Security Boundary

`axiograph-security` owns the shared no-follow bounded file reader, atomic
bounded publisher, JSON-depth guard, and descendant-aware bounded process
runner. Public HTTP adapters disable proxies and redirects, pin validated
public DNS answers, and verify the connected peer. Ollama is the separate
loopback-only HTTP class. GitHub imports use exact HTTPS repository syntax,
DNS pinning, disabled hooks/submodules, neutralized ambient Git configuration,
and option-safe refs.

Incoming HTTP, MCP, and LSP surfaces have hard connection, worker, queue,
frame, document, and response limits. Immutable `.axpd` verification hashes and
SQLite-deserializes one byte image. These are runtime safety controls, not
semantic authority. Parent-directory confinement remains a caller policy; an
operator must not place mutation-authorized roots under an attacker-writable
parent. See `docs/reference/SECURITY_BOUNDARIES.md`.

## Trust Boundary

Axiograph is currently best described as a typed ontology workbench with
proof-carrying claims for selected high-value fragments.
It is not yet a full categorical, topos-theoretic, or dependently typed ontology
kernel.

The trusted boundary is the import closure of
`lean/Axiograph/VerifyMain.lean`. The strongest formalized pieces today are:

- typed path expressions and groupoid/rewrite semantics,
- certificate format and checking,
- fixed-point probability witnesses,
- anchored replay against canonical `.axi` contexts,
- conservative `.axi` parsing and checking gates used by the verifier, and
- `category_kernel_v3`: anchored reconstruction of the finite relation-object
  category presentation, including ordered projections, identities, typed
  composition, parallel equations, contextual congruence, exact signed
  normalization traces for both formal inverse laws of every generator, and
  exact bounded generator-saturation explanation replay. This category wire
  path is decision procedure plus replay; it has no acceptance-to-denotation
  theorem yet.

HoTT, topos, presheaf, sheaf, and univalence-related material outside that
verifier boundary is design/spec support until wired into the checker. Keep
user-facing claims scoped to the currently checked fragment.

## Rust Type-Theory Framing

Rust provides dependent-type effects and runtime guardrails, not a trusted
dependently typed kernel.

Current useful Rust surfaces include:

- stable semantic anchor newtypes such as `AxiDigest`, `AcceptedSnapshotId`,
  `MaterializationIdV2`, `ProposalDigest`, `ProposalAdapterRunId`, `SchemaId`,
  `TheoryId`, `ContextId`, and `StableFactId`;
- lifecycle wrappers such as `Module<Validated>` and `Module<Reviewed>`;
- checked builders and importer entrypoints that require typed lifecycle state;
- canonical `SchemaPresentationIr` plus its derived `category_formation`
  evidence, including typed and relation objects, ordered role projections,
  identities, explicit generators, parallel equations, contextual congruence,
  formal signed paths with deterministic cancellation traces, bounded
  saturation, and replayable explanations;
- canonical `InstanceModelIr` object-membership, role-indexed,
  finite-constraint, and context/world/temporal witnesses with checked
  lifecycle/residual state;
- derived `kernel_ir.rs` execution indexes whose `RuntimeSemanticIndex`
  contains read-only canonical `KernelRefV2` citations; the former duplicate
  `SchemaCategoryIr` and `InstanceFunctorIr` representations have been removed;
- runtime theory checking through `RuntimeTheoryCheckReportV1` with typed
  finite/evidence/global-indexed scope, coverage counts, transport summaries,
  residual ids, and structured non-claims instead of synthetic closure fields;
- typed query elaboration, trust contracts, query/olog/CQ/theory holes,
  refinement handles bound to exact theory obligations and subjects, and
  canonical `Authoring`/`Query`/`Merge` finite-theory gate receipts whose exact
  coverage spans category formation, refinements, saturation explanations,
  contexts, and identity transports;
- typed olog checks and shared refinement handles across query/authoring,
  migration preview, reconciliation review, and CQ repair.

The remaining Rust goal is to make these surfaces universal:

- lifecycle and anchor typestate on all major artifacts,
- stable refs instead of raw strings or raw ids in semantic workflows,
- first-class prepared/typechecked query handles,
- runtime-addressable theory obligations and transports,
- typed diagnostics and repair objects,
- semantic coverage and business-rule reports as structured services.

## Storage And Backend Framing

PathDB is a strong graph/query substrate, but it is not the ontology kernel. The
implemented Rust semantic package is exact accepted `.axi` bytes plus the
immutable `CompiledKernelSnapshot` produced by `CanonicalCompiler`. Derived
in-process `RuntimeModuleIndex` values retain that snapshot; serialization
strips the handle and cannot recreate authority. Lean still trusts only the
narrower verifier import closure.

`axiograph-store::AxiStore` is the sole accepted-state and semantic-lineage
persistence authority: application-identified and size-bounded SQLite
WAL/FULL/foreign-key/strict catalog, bounded immutable SHA-256 objects, one
generation-CAS `store_state`, and one contiguous checksum-linked audit/ref
transaction. File HEADs, JSONL accepted logs, separate semantic state files,
legacy imports, symlinked authority files, and direct-copy replication are not
authority paths.

Keep relation-as-object plus projection arrows canonical. Binary edges are a
projection, not the primary semantic object.

External graph systems are projection targets and execution substrates. They
may expose useful native read/query interfaces, but semantic mutation and
lifecycle transitions should flow through Axiograph.

Backend priority:

- TypeDB is the primary high-fidelity typed backend target.
- TerminusDB is the preferred RDF/VCS-shaped secondary target.
- Property-graph backends remain experimental unless capability profiles show
  enough semantic preservation.

`axiograph-projections` now consumes only `CompiledKernelSnapshot` and emits
capability-declared PathDB, TypeDB, TerminusDB, RDF/OWL, and property-graph
manifests with finite `KernelRefV2` records, semantic-loss reports, read-only
artifacts, and evidence-only readback. It does not accept `RuntimeSchemaIndex`
as a second projection authority. Remote query pushdown remains future adapter
optimization, not semantic authority. See
`docs/reference/BACKEND_PROJECTIONS.md`.

## Semantic VCS And Lifecycle

AxiStore already provides immutable semantic commits, catalog refs and tags,
audit lineage, accepted objects/trees/snapshots, reconciliation records, gate
attachments, and authenticated materialization receipts. Review/projection DTOs
in the CLI are filesystem-free and carry no persistence authority.

The remaining direction is to make AxiStore semantic VCS the default lifecycle backbone:
refs, branches, tags, ancestry, typed semantic diffs, reconciliation decisions,
CQ-gated review, predictive-proposal lineage, supersession, and retraction.

## Runtime Usefulness Bar

The Rust runtime checker should be useful before Lean certification is available.
`make verify-regulated-shipment` is the executable bar: one canonical scenario
must cross finite typed theory, exact query checking, explanation, evolution,
reviewed merge, restart, projection, and generated-test surfaces without
blurring their trust classes. Its accepted `query_result_v4` report is parsed
and bound into the reviewed candidate and merge trust gates; placeholder receipt
identities reject before protected main advances.

It should answer operational questions under explicit anchors:

- what applies here,
- what is strongly checked vs weakly grounded,
- what context/world/version is in scope,
- which rules and CQs are involved,
- what code/test/ontology action is missing,
- what changed across semantic refs,
- what can be repaired through a typed handle.

Prefer extending shared report families over creating one-off payloads:

- `EvolutionPreviewV1`,
- trust contracts,
- business-rule applicability reports,
- semantic coverage/drift reports,
- bounded-context and `BehaviorCaseV1` reports with `CaseReceiptV1` outputs,
  semantic slice selectors, and `ContextMapV1` merge/rebase bridge objects,
- typed authoring reports,
- agent-facing semantic reports.

## AI And Proposal Adapters

AI, LLM, and predictive-proposal outputs remain evidence-plane artifacts until reviewed.
They should carry typed run/proposal/snapshot anchors, proposal digests,
grounded evidence links, candidate schema/theory/olog deltas, and explicit
preview failures.

Embeddings and vector indexes are evidence/index sidecars, not ontology meaning.
Embedding-derived relationships should become typed evidence/proposal overlays
or refinement handles first; only reviewed, well-typed deltas can enter
canonical `.axi`. Use `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` as the
source of truth for this boundary.

LLM-assisted means typed plugin/API/tool-loop/MCP-skill-style integration
surfaces. It does not mean free-form authority over accepted ontology state.

## Greenfield Compatibility Policy

Backward compatibility is not a default goal. Keep compatibility only when it
serves a concrete trust, verifier, byte-format, accepted-plane, or operational
contract.

Current cleanup pressure:

- keep the accepted-anchor certified query path as the only default query
  certificate path,
- keep derived debug export artifacts out of semantic anchoring, query/certificate
  authority, promotion, teaching, and interchange,
- collapse workflow-specific review wrappers into shared preview/trust/report
  families,
- remove outdated examples and docs when stronger typed surfaces replace them,
- keep domain example crates, such as the industrial engineering example, outside
  `axiograph-cli` unless they become reusable ontology-engineering
  infrastructure.

## Harness Quality

`AGENTS.md` should stay short enough for agents to actually use. If important
context grows too large, move it here or into a dedicated reference, explanation,
how-to, tutorial, or roadmap document.

Manual checks after changing agent docs:

```bash
wc -l AGENTS.md
rg "\\- \\[[ x~]\\]" AGENTS.md
rg "AGENT_CONTEXT|ROADMAP_AGENT_BACKLOG" docs/README.md docs/reference/README.md docs/roadmaps/README.md
git diff --check
```
