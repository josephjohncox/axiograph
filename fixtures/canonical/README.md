# Canonical Corpus

`corpus.json` is the selected canonical-syntax `.axi` set for parser,
semantics, and Rust/Lean parity work. It references teachable modules under
`examples/` and a small number of review-plane proposal modules for
syntax/parity coverage only. Review-plane entries are marked with
`trust_plane`; they are not accepted canonical examples. The corpus
intentionally excludes derived storage images and obsolete reverse exports.

The broader test suite still validates every `.axi` under `examples/`; storage
and verifier boundary fixtures live under `fixtures/`.
