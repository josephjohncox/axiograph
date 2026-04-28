# Embeddings And Evidence Sidecars

This is the source of truth for how embeddings, embedding models, vector
indexes, RAG retrieval, and embedding-derived relationships fit into Axiograph.

## Core Rule

Embeddings are **sidecar evidence/index artifacts**, not ontology meaning.

Accepted `.axi` plus compiled semantic IR remains the meaning plane. Embedding
vectors, ANN indexes, cosine scores, nearest-neighbor sets, reranker scores,
and model prompts do not become trusted facts by being stored beside a snapshot.

The right flow is:

1. Canonical `.axi` defines typed domain meaning.
2. PathDB materializes an execution snapshot.
3. Embedding sidecars attach to a snapshot/ref with model and text anchors.
4. Retrieval proposes weak matches, candidate relations, or candidate axioms.
5. Typed validation and review turn selected candidates into `.axi` deltas.
6. Promotion makes accepted ontology facts/rules, not the embedding score.

## Artifact Split

Use three separate layers.

| Layer | What It Contains | Authority |
| --- | --- | --- |
| `EmbeddingsFileV1` / vector sidecar | vectors, target keys, backend, model, dimensions, text digest, metadata | retrieval/index only |
| evidence overlay | candidate matches, similarity observations, citations, proposal links, model/run provenance | weak/evidence-plane |
| canonical `.axi` | reviewed domain objects, relations, constraints, rules, equations, instances | semantic meaning after promotion |

Do not put large vectors directly into canonical `.axi`. If a vector sidecar
needs a reviewable companion, create a small typed evidence overlay that names
the embedding run, model, source chunks/entities, similarity observations, and
candidate semantic relationships.

## Can Embeddings Have Semantic Relationships?

Yes, but only through typed lifting.

Raw vector facts:

- `chunk A is close to chunk B`,
- `entity X is near entity Y`,
- `query q retrieved item i`,
- `model m produced score s`,

are evidence observations. They can suggest semantic relationships such as:

- `DocChunk supports Claim`,
- `Service likely implements Capability`,
- `Function mentions BusinessRule`,
- `MaterialLot may violate CertificateRequirement`,
- `Entity A candidate-same-as Entity B`,
- `Relation R should exist between X and Y`,
- `Axiom candidate: path p rewrites to path q`.

Those suggestions must lower through typed proposal or authoring surfaces:

- `ProposalSet<Validated, A>` for candidate facts/relations,
- `TypedAuthoringDelta<S, A>` for candidate schema/theory edits,
- `RuntimeRefinementHandleV1` for typed repair/apply steps,
- `EvolutionPreviewV1` for CQ/trust/coverage impact,
- semantic VCS review/promotion for accepted state.

Embedding similarity is never itself a proof of semantic equivalence,
subtyping, rule validity, completeness, or ontology closure.

## Sidecar `.axi`

The phrase "sidecar `.axi`" should mean an evidence overlay module, not an
alternate ontology kernel.

Allowed:

- a canonical-looking `.axi` draft that represents typed evidence observations,
  candidate relationships, and provenance;
- a reviewed `.axi` delta generated from embedding-derived proposals;
- a small manifest module that names an embedding run and links it to
  `DocChunk`, `Entity`, `Proposal`, or `Evidence` objects.

Not allowed:

- vectors as accepted domain facts,
- cosine similarity as a trusted ontology relation,
- embedding-model output as direct canonical truth,
- a sidecar `.axi` that bypasses typed validation, CQ gates, trust contracts,
  or semantic VCS promotion.

If the relationship is domain meaning, promote it into canonical `.axi`. If it
is retrieval support, keep it in evidence sidecars.

## Required Anchors

Every embedding sidecar must carry enough identity to make retrieval results
auditable:

- accepted `.axi` digest or semantic ref when available,
- PathDB snapshot id when resolved against a materialized graph,
- embedding backend and model name,
- embedding model version, digest, or deployment id when available,
- dimension and normalization policy,
- target kind (`docchunks`, `entities`, or future typed target),
- stable item key (`chunk_id`, `(type, name)`, or stable semantic ref),
- source text digest,
- creation timestamp,
- optional prompt/truncation/chunking metadata.

The current Rust `EmbeddingsFileV1` already stores backend, model, dimensions,
target kind, item keys, vectors, text digests, and metadata. The next tightening
step is to add explicit accepted/PathDB/semantic-ref anchors and model-version
metadata to the persisted sidecar format.

## Retrieval Semantics

Retrieval has a weak claim shape:

```text
Gamma ; Snapshot ; Model ; TextDigest |- item retrieved_for query with score s
```

That judgment means "this model/index retrieved this item under this snapshot
and text identity." It does not mean the item is true, complete, semantically
equivalent, or accepted.

Agent-facing retrieval reports should always include:

- grounding refs,
- model/run metadata,
- matched ontology refs if any,
- score and retrieval method,
- caveats,
- suggested typed queries,
- suggested proposal or refinement handles.

## Relationship Lifting

Embedding-derived semantic relationships should use this pipeline:

1. Retrieve candidate chunks/entities.
2. Resolve candidates to compiled-IR refs where possible.
3. Classify the suggested relationship:
   `same_as`, `mentions`, `supports`, `implements`, `violates`, `subtype_candidate`,
   `relation_candidate`, `axiom_candidate`, or `unknown`.
4. Emit typed evidence/proposal objects with provenance.
5. Preview the impact through runtime type checking, CQ gates, coverage, and
   trust deltas.
6. Promote only reviewed, well-typed deltas.

When the system cannot resolve a candidate to typed IR refs, it should emit a
typed hole/refinement candidate rather than inventing a generic entity or
relation.

## Backend And Query Use

Vector backends, ANN indexes, and embedding model APIs are optimization and
retrieval substrates. They can be queried natively where useful, but semantic
mutation remains Axiograph-native.

Good native interfaces:

- vector search over `DocChunk` and entity text,
- hybrid keyword/vector retrieval,
- nearest-neighbor exploration,
- evidence discovery for agents.

Bad native interfaces:

- accepting ontology facts directly from vector score thresholds,
- using nearest-neighbor results as schema morphisms,
- treating embedding clusters as accepted type families without review.

## Trust Boundary

Embedding sidecars belong outside the Lean trusted kernel.

They can support:

- evidence discovery,
- candidate ontology creation,
- weak definition queries,
- software coverage exploration,
- agent planning,
- semantic search,
- world-model proposal generation.

They cannot support by themselves:

- certified query soundness,
- rewrite soundness,
- migration soundness,
- ontology completeness,
- global semantic equivalence,
- accepted business-rule correctness.

Those require runtime typed checks, review gates, and, for the strongest
fragments, Lean certificates.

## Implementation TODOs

- Add `EmbeddingSidecarManifestV1` with explicit accepted-ref, PathDB snapshot,
  semantic ref, compiled IR digest, model version/digest, dimension, target,
  normalization, and source text digest fields.
- Add `EmbeddingEvidenceOverlayV1` for typed similarity observations and
  candidate semantic relationships without vector payloads.
- Add `semantic_embedding_query` / `semantic_embedding_relationships` tool-loop
  surfaces that return weak candidates plus typed refinement handles.
- Add an `axiograph discover embedding-relationships` CLI command that emits
  proposal/evidence overlays, not accepted facts.
- Thread embedding-derived proposals into `EvolutionPreviewV1` so CQ/trust/
  coverage deltas are visible before review.
