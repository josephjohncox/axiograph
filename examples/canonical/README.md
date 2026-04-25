# Canonical Corpus

`corpus.json` is the selected canonical `.axi` set for parser, semantics, and
Rust/Lean parity work. It intentionally excludes derived `PathDBExportV1`
snapshots and other historical interchange artifacts.

The broader test suite still validates every `.axi` under `examples/` except
known debug snapshot anchors.
