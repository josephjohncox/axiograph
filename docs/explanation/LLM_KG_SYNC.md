# LLM Evidence And Ontology Proposal Loop

**Diataxis:** Explanation
**Audience:** contributors and agent-tool authors

Axiograph uses LLMs as untrusted evidence, discovery, and planning components.
They may propose facts, candidate relations, definition matches, schema/theory
deltas, or software-authoring follow-ups. They do not directly mutate accepted
ontology state.

The current loop is:

```text
documents / code / conversations / embeddings / business traces
  -> evidence chunks and sidecars
  -> advisory definition and coverage probes
  -> typed proposals or refinement handles
  -> EvolutionPreviewV1 / SemanticMergePlanV1 / authoring reports
  -> CQ, trust, coverage, and runtime-theory gates
  -> semantic VCS review and promotion
```

Accepted meaning remains canonical `.axi` plus compiled IR. Model output is
evidence-plane material until the normal review path accepts it.

## Tooling Surfaces

Use existing typed services rather than inventing one-off message protocols:

| Need | Surface |
| --- | --- |
| Query/exploration from an agent host | `axiograph mcp`, `axql_explore`, `axql_elaborate`, `axql_run` |
| Human REPL exploration | `ask`, `q --typecheck`, `q --elaborate`, `llm ask` |
| Weak definition grounding | `discover define`, `semantic_definition_query` |
| CQ/BDD/DDD authoring | `.cq`, `semantic_competency_questions`, behavior-case tools |
| Software coverage and codegen planning | authoring overlay tools and read-only authoring MCP |
| Evidence from embeddings | `EmbeddingSidecarManifestV1`, `EmbeddingEvidenceOverlayV1` |
| Ontology/code deltas | proposal import, typed refinement handles, evolution previews |
| Promotion | semantic VCS review, CQ/trust/coverage/runtime-theory gates |

Command plugins and JSON payloads are adapter boundaries. MCP uses `rmcp`, LSP
uses `lsp-server`/`lsp-types`, HTTP uses typed server endpoints, and CLI reports
use versioned Rust structs serialized at the boundary.

## What The Model May Produce

The model may produce weak candidates:

- a possible definition for a process, function, business rule, object, relation,
  invariant, policy, or implementation surface;
- a candidate CQ or AxQL lowering to check;
- a candidate ontology proposal with evidence links;
- a candidate code/test/migration follow-up;
- a candidate embedding/evidence relationship with score semantics;
- a natural-language summary of typed results.

Every candidate must carry enough grounding to be useful:

- source text, chunk, file, or fact digest;
- accepted anchor or explicit evidence-plane anchor;
- candidate ontology refs or unresolved typed holes;
- caveats and ambiguity notes;
- suggested refinement handles or next tool calls.

Scores are evidence metadata. They are not acceptance, truth, or correctness.

## Strong And Weak Boundaries

Weak outputs can help users discover structure, but cannot satisfy gates:

- `semantic_definition_query` can suggest “this looks like the shipment
  eligibility rule.”
- `semantic_weak_coverage_probe` can suggest “this implementation surface seems
  related.”
- embedding relationship discovery can rank likely semantic neighbors.

Strong outputs require typed Axiograph checks:

- canonical `.axi` input and accepted anchors;
- compiled IR refs rather than labels alone;
- runtime theory report with declared closure tier and non-claims;
- behavior/CQ/coverage reports under explicit policy;
- optional Lean certificate for a supported fragment;
- semantic VCS state transition through review and promotion.

Do not collapse this distinction into one confidence number. Reports should
surface trust class, anchors, coverage mode, unresolved refs, residual
obligations, and next actions.

## Proposal Lifecycle

LLM- or evidence-derived deltas should follow this lifecycle:

1. Extract or discover evidence.
2. Ground it to canonical `.axi` refs where possible.
3. Emit weak candidates, typed holes, or `RuntimeRefinementHandleV2` records.
4. Build an `EvolutionPreviewV1` or authoring report.
5. Run CQ, trust, coverage, runtime-theory, and merge/rebase checks.
6. Review conflicts and residual obligations.
7. Promote only accepted, typed deltas through semantic VCS.

If the ontology cannot type the proposal, the system should emit a refinement
handle or residual obligation. It should not synthesize a silent fallback entity
or promote an untyped relation.

## Grounding Example

A conversation says:

```text
Titanium should stay below 50 m/min in this milling cell because the tooling
work-hardens the part above that range.
```

A useful weak candidate is not “add this fact.” It is:

- candidate kind: business/process rule;
- source: chunk digest and conversation span;
- likely ontology refs: `Material`, `ToolingCell`, candidate speed-limit
  relation or typed hole;
- evidence policy: advisory;
- suggested CQ: “does every titanium milling cell enforce a configured speed
  ceiling?”;
- suggested overlay follow-up: map the HMI or PLC surface that enforces the
  speed ceiling;
- residual obligation: model the unit/time/process context if missing.

Promotion happens only after the domain model has the right types, the rule is
reviewed, the CQ is executable, and the relevant coverage/tooling policy passes.

## Design Constraints

- Canonical `.axi` is the meaning plane; PathDB and backend graphs are
  execution/projection substrates.
- RDF/SHACL, TypeDB, TerminusDB, embeddings, and LLM outputs are boundary
  layers or evidence overlays unless lowered through the canonical IR.
- Model-assisted authoring must produce typed handles and reports that agents
  can act on, not prose-only recommendations.
- MCP/LSP/API integrations should expose the same typed report families as CLI
  flows.
- Accepted mutation always goes through Axiograph review and semantic VCS.
- `UnifiedStorage` is a bounded process-local evidence queue. Policy-exempt
  changes may materialize into its derived PathDB view; constraints,
  low-confidence evidence, and schema extensions stay pending until an explicit
  approval or rejection. `SyncManager` preserves the exact fact-to-change
  mapping and does not report a storage-deferred fact as integrated. Neither
  transition promotes accepted ontology state.
