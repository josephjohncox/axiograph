# Knowledge Generation And Learning

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph treats learning systems, embeddings, LLM extraction, and proposal-adapter
outputs as evidence-plane machinery. They can suggest ontology changes and
implementation work, but they do not define accepted meaning.

## Core Principle

Learning output must flow through the same typed ontology workflow as human
authoring:

```text
source material
  -> evidence sidecar / extraction report / definition query
  -> typed proposal or refinement handle
  -> EvolutionPreviewV1 / reconciliation
  -> CQ, trust, coverage, and runtime-theory gates
  -> accepted .axi + compiled IR only after review/promotion
```

The canonical meaning layer remains accepted `.axi` plus compiled semantic IR.
PathDB, embeddings, RAG stores, and graph backends are execution or evidence
substrates.

## Useful Learning Surfaces

Learning is useful when it improves one of these workflows:

- discovering candidate domain objects, relation objects, roles, and contexts,
- finding missing business rules or invariants,
- mapping code/docs/process descriptions to implementation surfaces,
- suggesting subtype/type-family refinements,
- proposing CQs and behavior cases,
- identifying semantic drift between code and ontology,
- ranking review work without bypassing typed validation.

## Evidence Sidecars

Embedding and extraction outputs should be recorded as sidecars:

- `EmbeddingSidecarManifestV1` anchors vector runs to model identity, source
  digests, accepted refs, and target ids,
- `EmbeddingEvidenceOverlayV1` carries vector-free relationship candidates,
  caveats, and suggested typed refinement handles,
- definition-query reports carry weak matches, ambiguity notes, suggested AxQL,
  and follow-up actions.

No score mutates accepted ontology state. Promotion requires typed review,
semantic VCS, and configured gates.

## Human Learning And Guardrails

In domains like machining, chemical plants, finance, or ERP workflows, learning
systems should help people understand why a rule matters:

- show the accepted rule and its anchor,
- show supporting CQs and evidence,
- distinguish accepted, review, evidence, and unknown claims,
- surface implementation coverage gaps,
- suggest next ontology/code/test/migration actions.

This is stronger than retrieving a relevant document chunk. The system should
answer: what is accepted, what is weak, what is missing, and what needs review.

## Software And Business Co-Evolution

Learning over wikis, code, documents, tickets, traces, and business processes
should produce typed deltas:

- ontology deltas for missing concepts and rules,
- overlay deltas for implementation surfaces and coverage edges,
- behavior cases for business flows,
- CQ additions for important questions,
- merge/rebase previews when bounded contexts diverge,
- codegen or test skeleton plans when implementation is missing.

Tools consume the ontology; they should not inject tooling concepts into the
domain `.axi` unless those concepts are real domain facts.

## Runtime Trust

Rust can provide operational checks:

- typed refs,
- runtime-theory closure under declared tiers,
- coverage reports,
- weak/advisory definition queries,
- semantic VCS previews,
- evidence-to-preview wiring.

Lean strengthens selected finite claims when a certificate fragment is
implemented. Everything else remains runtime-checked, advisory, or review-only
with explicit caveats.

## Teaching Examples

Use these examples to exercise the learning workflow:

- `examples/software_authoring/` for weak definition queries, overlays,
  behavior cases, codegen planning, and continuous software coverage.
- `examples/semantic_merge/` for typed co-evolution across bounded contexts.
- `examples/llm_sync/` for evidence-overlay guidance.
- `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` for sidecar contracts.
