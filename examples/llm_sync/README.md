# LLM Evidence Overlay Examples

This directory documents LLM/agent examples that use the current Axiograph
evidence model:

1. Gather source text, code, documents, or conversations as evidence.
2. Produce advisory extraction, embedding, or definition-query reports.
3. Emit typed proposal/refinement handles against canonical `.axi` and compiled
   IR refs.
4. Promote only through Axiograph review, CQ/trust gates, reconciliation, and
   semantic VCS.

LLM output is never accepted ontology truth by itself. PathDB is not a write
authority for ontology meaning. Examples in this directory should be read-only
planning or evidence-overlay flows unless they explicitly call the semantic VCS
promotion commands.

Start with a local evidence source document:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest doc examples/ingest_sources/machining_conversation.txt \
  --out build/examples/llm_sync/machining_proposals.json \
  --machining \
  --chunks build/examples/llm_sync/machining_chunks.json \
  --facts build/examples/llm_sync/machining_facts.json
```

Then ask an advisory definition question against the current learning ontology:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover define examples/learning/MachinistLearning.axi \
  --prompt "define the titanium chatter mitigation rule" \
  --kind-hint business_rule \
  --include-queries \
  --out build/examples/llm_sync/chatter_definition_query.json
```

For embedding sidecars and evidence overlays, see
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`. This directory also includes
vector-free advisory fixtures for the embedding/evidence schemas:

- `embedding_sidecar_manifest_example.json`
- `embedding_evidence_overlay_example.json`

Those fixtures are documentation/test inputs. They are not accepted ontology
state and do not satisfy promotion gates without normal typed review.
