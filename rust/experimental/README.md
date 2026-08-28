# Experimental Rust Area

This directory is for design notes or disabled implementation sketches that are
not part of the supported Rust architecture.

Do not keep non-compiling Cargo crates here. If an importer or backend adapter is
not maintained against the current workspace, keep the design in docs and delete
the stale implementation. Reintroduce it as an active crate only after it is
rewritten against the current canonical spine:

```text
canonical .axi -> compiled IR -> typed reports -> optional Lean verifier
```

To promote experimental work into `rust/crates/`, first make it compile in the
workspace, add focused tests, and ensure it lowers into canonical proposals,
tooling overlays, or typed runtime reports rather than creating a parallel
semantic authority.
