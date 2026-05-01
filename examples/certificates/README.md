# Certificate Fixtures

These JSON files are low-level Lean/Rust certificate fixtures. They are not the
recommended first teaching path for ontology authoring.

Use them when the feature being tested is the certificate checker itself:

- `normalize_path_v2*.json` exercises path normalization and groupoid laws.
- `path_equiv_v2.json` exercises path-equivalence witnesses.
- `rewrite_derivation_v2.json` exercises replayable rewrite steps.
- `resolution_v2.json` exercises deterministic evidence/conflict decisions.
- `delta_f_v1.json` exercises the current migration recompute fixture.

For user-facing flows, prefer canonical `.axi` modules plus typed reports from
`examples/README.md`. These fixtures are intentionally small and mechanical so
Lean checker failures are easy to diagnose.
