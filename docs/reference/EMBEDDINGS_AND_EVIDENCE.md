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
| `EmbeddingSidecarManifestV1` | accepted ref/module digest, optional PathDB snapshot id, compiled-IR digest, model identity, target ids, text digests, trust caveats | audit metadata for retrieval/index sidecars |
| `EmbeddingEvidenceOverlayV1` | candidate matches, similarity observations, citations, proposal links, model/run provenance | weak/evidence-plane |
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
is retrieval support, keep it in evidence sidecars. The Rust manifest/overlay
types are intentionally vector-free except for `EmbeddingsFileV1`: overlays
name target ids and observations, then cite proposals or refinement handles.

## Required Anchors

Every embedding sidecar must carry enough identity to make retrieval results
auditable:

- accepted `.axi` digest or semantic ref when available,
- PathDB snapshot id when resolved against a materialized graph,
- embedding backend and model name,
- embedding model version, digest, or deployment id when available,
- dimension and normalization policy,
- target kind (`doc_chunks`, `entities`, or future typed target),
- stable item key (`chunk_id`, `(type, name)`, or stable semantic ref),
- source text digest,
- creation timestamp,
- optional prompt/truncation/chunking metadata.

The current Rust sidecar contract is split across:

- `EmbeddingsFileV1`: vector payloads plus stable target keys;
- `EmbeddingSidecarManifestV1`: accepted ref, accepted module digest
  (`accepted_axi_anchor.axi_digest`), optional PathDB snapshot id, compiled-IR
  digest, embedding file digest, source model identity/version/digest, target
  ids, source text digests, normalization, and trust caveats;
- `EmbeddingEvidenceOverlayV1`: vector-free similarity observations and
  candidate relationship evidence that points back to a manifest sidecar and
  carries suggested runtime refinement handles for review.

`EmbeddingSidecarManifestV1` validation requires source model identity, nonzero
dimensions, target ids, source text digests, accepted anchor material, and
trust caveats. `EmbeddingEvidenceOverlayV1` validation requires every candidate
relationship to be `advisory_only=true` and to include at least one valid
runtime refinement handle. Those handles are review/apply surfaces, not
promotion authority.

## Rust Builder Slice

`rust/crates/axiograph-cli/src/embeddings.rs` now exposes a small, CLI-ready
slice without making embedding scores authoritative:

- `build_embedding_sidecar_manifest_v1` converts an `EmbeddingsFileV1` into an
  `EmbeddingSidecarManifestV1`.
- `embedding_file_digest_v1` computes a stable FNV digest over the vector
  sidecar payload, including vector bits and sorted metadata.
- `discover_embedding_evidence_overlay_v1` performs deterministic pairwise
  cosine discovery over tiny vector sidecars and emits an
  `EmbeddingEvidenceOverlayV1`.

The manifest builder requires each item to have a `text_digest`, rejects
duplicate target keys, derives stable target ids, carries accepted and optional
PathDB anchors, and copies model version/digest/deployment identity from the
builder input or embedding-file metadata. If model identity is absent,
validation fails rather than inventing provenance.

The relationship discovery function is intentionally simple and exact:

- it uses pairwise cosine, not an ANN backend;
- it sorts candidates by score, then target ids for deterministic tie breaking;
- it emits `similar_to` relationships by default;
- every relationship is `advisory_only=true`;
- no typed relation or proposal ref is invented.

This is suitable for tests, demos, and small evidence sidecars. Larger stores
should use a vector backend for retrieval, then lower selected candidates into
the same overlay shape.

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
   `same_as_candidate`, `mentions`, `supports`, `implements`, `violates`,
   `subtype_candidate`, `relation_candidate`, `axiom_candidate`, or `unknown`.
4. Emit typed evidence/proposal objects with provenance.
5. Preview the impact through runtime type checking, CQ gates, coverage, and
   trust deltas.
6. Promote only reviewed, well-typed deltas.

When the system cannot resolve a candidate to typed IR refs, it should emit a
typed hole/refinement candidate rather than inventing a generic entity or
relation.

The Rust relationship vocabulary for overlays is:

- `similar_to`: retrieval/similarity observation surfaced as weak evidence;
- `supports`: source evidence appears to support a claim/proposal;
- `mentions`: source text mentions an accepted or candidate typed object;
- `implements`: code/documentation appears to implement a capability or rule;
- `violates`: evidence appears to conflict with a constraint or requirement;
- `subtype_candidate`: candidate subtype/specialization relation;
- `same_as_candidate`: possible entity identity merge;
- `relation_candidate`: candidate domain relation;
- `axiom_candidate`: candidate typed axiom/rewrite/equation;
- `contradicts`: source evidence appears to challenge a claim/proposal;
- `unknown`: unresolved advisory evidence that still needs classification.

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

## Roadmap Hooks

Active implementation work is tracked in
`docs/roadmaps/ROADMAP_RUNTIME_THEORY_AND_TYPED_WORKFLOWS.md` and
`docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md`. The stable contract for
this reference page is:

- embedding query/relationship tools are weak/advisory;
- embedding-derived candidates must return typed refinement handles or proposal
  preview inputs;
- `EvolutionPreviewV1` must show CQ/trust/coverage impact before review; and
- no embedding score can mutate canonical `.axi`.
