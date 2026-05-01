# Agent Context

This document holds the durable context that used to live directly in
`AGENTS.md`. Keep `AGENTS.md` short and update this document when the current
technical reality changes.

## Current State

- The repo builds Rust plus Lean from the top-level `Makefile`.
- Rust is the runtime, compiler, ingestion, query, storage, and tool surface.
- Lean is the trusted checker for the currently supported certificate fragment.
- Canonical accepted `.axi` modules are the meaning plane.
- PathDB and `.axpd` snapshots are derived execution/query substrates.
- Accepted-plane snapshots plus PathDB WAL are the live storage backbone.
- The TypeScript viz frontend lives in `frontend/viz/`; server/tooling expects
  built assets from `frontend/viz/dist`.

## Protocol And Core Infrastructure

Prefer maintained, widely used Rust crates for protocol and core
infrastructure unless Axiograph semantics require custom logic. Custom framing,
dispatch, parsers, or runtimes should stay narrow, typed, and justified by the
ontology/workbench layer rather than by generic protocol plumbing.

Current protocol choices:

- MCP stdio servers use `rmcp` for host lifecycle, transport, framing, tool
  listing, and tool calls. Local JSON-RPC helpers are test harnesses, not the
  production server contract.
- LSP/editor surfaces use `lsp-server` for stdio transport/framing and
  `lsp-types` for protocol capability and request/response types. Axiograph
  code owns diagnostics, commands, and typed authoring reports.
- The DB HTTP server uses `hyper` with `http-body-util` for HTTP serving and
  body handling. Axiograph-specific code should stay at route dispatch,
  request validation, typed reports, and snapshot/query semantics.

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
- conservative `.axi` parsing and checking gates used by the verifier.

HoTT, topos, presheaf, sheaf, and univalence-related material outside that
verifier boundary is design/spec support until wired into the checker. Keep
user-facing claims scoped to the currently checked fragment.

## Rust Type-Theory Framing

Rust provides dependent-type effects and runtime guardrails, not a trusted
dependently typed kernel.

Current useful Rust surfaces include:

- stable semantic anchor newtypes such as `AxiDigest`, `AcceptedSnapshotId`,
  `PathdbSnapshotId`, `ProposalDigest`, `WorldModelRunId`, `SchemaId`,
  `TheoryId`, `ContextId`, and `StableFactId`;
- lifecycle wrappers such as `Module<Validated>` and `Module<Reviewed>`;
- checked builders and importer entrypoints that require typed lifecycle state;
- compiled IR support in `kernel_ir.rs`, including the first
  `SchemaCategoryIr` and `InstanceFunctorIr` runtime category/functor slice;
- runtime theory checking through `RuntimeTheoryCheckReportV1` with explicit
  finite/evidence/global-indexed closure tiers and scoped completeness/non-claim
  reporting;
- typed query elaboration, trust contracts, typed holes, and refinement handles;
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
intended kernel is accepted `.axi` plus one compiled schema/category IR.

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

Backend adapters should consume capability profiles and emit typed pushdown
plans from `CompiledSchemaIr`. Pushdown is an optimization and interoperability
surface, not semantic authority.

## Semantic VCS And Lifecycle

The accepted-plane code already has first slices of:

- `sem/commits`,
- `sem/refs`,
- `sem/validations`,
- `sem/world_model_runs`,
- persisted reconciliation previews,
- compact gate/trust/rule/coverage summaries.

The remaining direction is to make semantic VCS the default lifecycle backbone:
refs, branches, tags, ancestry, typed semantic diffs, reconciliation decisions,
CQ-gated review, world-model lineage, supersession, and retraction.

## Runtime Usefulness Bar

The Rust runtime checker should be useful before Lean certification is available.
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

## AI And World Models

AI, LLM, and world-model outputs remain evidence-plane artifacts until reviewed.
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
serves a concrete trust, verifier, live-byte, accepted-plane, or operational
contract.

Current cleanup pressure:

- keep `query_result_v3` as the active query certificate family,
- keep derived snapshot exports out of semantic anchoring, query/certificate
  authority, promotion, teaching, and interchange,
- collapse workflow-specific review wrappers into shared preview/trust/report
  families,
- remove outdated examples and docs when stronger typed surfaces replace them,
- keep domain harnesses, such as the industrial engineering example, outside
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
