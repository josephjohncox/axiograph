# Verification Anchors

These files are verification fixtures, not teaching examples.

- `rewrite_rules_anchor_v1.axi` anchors rewrite/path certificate checks.
- `query_result_v4_exact.axi` and `query_result_v4_exact.json` are the durable
  positive exact-finite query fixture.
- `reject/query_result_v4_{extra_row,missing_row,duplicate_row,truncated}.json`
  exercise exact-denotation and truncation rejection with mutation-consistent
  selected-answer digests where applicable.
- `reject/query_result_v4_forged_witness.json` preserves the selected answer but
  forges a type witness; the verifier-bridge adversarial suite also mutates a
  canonical path `axi_fact_id` and requires a rejected receipt.
- `reject/query_result_v4_{altered_prepared_digest,altered_answer_digest}.json`
  exercise cryptographic binding rejection.
- The query boundary has no V3 reader or compatibility fixture.

This directory is deliberately outside `examples/` so the public examples stay
focused on authoring, querying, semantic VCS, software coverage, and backend
projection flows. Do not use these fixtures as ontology-authoring inputs.
