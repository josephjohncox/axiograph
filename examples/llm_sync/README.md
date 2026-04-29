# LLM Evidence Overlay Examples

This directory is reserved for LLM/agent examples that use the current
Axiograph evidence model:

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

Start with:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --prompt "define the shipment eligibility business rule" \
  --kind-hint business_rule \
  --include-queries
```

For embedding sidecars and evidence overlays, see
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.
