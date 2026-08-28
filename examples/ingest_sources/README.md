# Ingest Source Documents

These files are source inputs for ingestion and evidence-plane tutorials. They
are not canonical ontology examples by themselves.

Use them with current ingestion commands that produce proposals, chunks,
evidence overlays, or review material:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest doc examples/ingest_sources/machining_conversation.txt \
  --out build/examples/ingest/machining_proposals.json \
  --chunks build/examples/ingest/machining_chunks.json
```

Promote generated material only through typed review and semantic VCS.
