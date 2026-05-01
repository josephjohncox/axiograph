# Canonical Corpus

`corpus.json` is the selected canonical-syntax `.axi` set for parser,
semantics, and Rust/Lean parity work. It includes accepted-domain modules plus
review-plane proposal modules marked with `trust_plane`, and intentionally
excludes derived `PathDBExportV1` snapshots.

The broader test suite still validates every `.axi` under `examples/` except
known debug snapshot anchors.
