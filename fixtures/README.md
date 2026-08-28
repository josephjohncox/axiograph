# Fixtures

This directory contains test and verifier fixtures that are intentionally not
part of the public example catalog.

- `canonical/` holds the selected parser/digest conformance corpus.
- `certificates/` holds low-level Lean/Rust certificate payloads.
- `verification/` holds narrow Rust/Lean/parser boundary fixtures.
- `adversarial/regulated_shipment/` holds intentionally invalid refinement and
  non-parallel-path modules exercised by the primary usefulness gate.

Use `examples/` for teachable ontology, authoring, query, coverage, merge, and
backend projection flows. Use `fixtures/` when a file exists only to pin a
checker boundary or regression test.
