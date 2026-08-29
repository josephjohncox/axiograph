# Development

This guide is for contributors and release operators. The root
[The repository README](https://github.com/josephjohncox/axiograph/blob/main/README.md)
describes the product, architecture, and primary user workflows.

## Toolchains

Axiograph pins the tools that decide whether a commit may be published:

- Rust 1.88.0 for the release gate. Newer Rust versions may be used during local
  development only when the exact 1.88.0 gate remains green.
- The Lean toolchain and mathlib revisions in `lean/lean-toolchain` and
  `lean/lake-manifest.json`.
- Node.js 24.19.0 from `.node-version` for the visualization frontend.
- `nightly-2026-07-23`, Miri, `rust-src`, cargo-fuzz 0.13.2,
  cargo-audit 0.22.2, and cargo-kani 0.67.0 for the complete release gate.
- Docker for container checks and optional backend projection/readback tests.
- mdBook 0.5.4 for the published documentation. `make book` downloads the
  official platform binary and verifies its pinned SHA-256 digest.

See [Testing](howto/TESTING.md) for the exact gate requirements and skip/failure
policy.

## Build From Source

Build the main Rust workspace:

```bash
cargo check --manifest-path rust/Cargo.toml \
  -p axiograph-cli \
  -p axiograph-pathdb
```

Build the trusted Lean checker boundary:

```bash
cd lean
lake build Axiograph.VerifyMain
```

Build the documentation book:

```bash
make book
```

## Formatting And Source Hygiene

Run the focused local checks before opening a pull request:

```bash
cargo fmt --manifest-path rust/Cargo.toml --check
python3 scripts/check_greenfield_surface.py
git diff --check
```

First-party Rust uses `unsafe_code = "forbid"`. Production libraries and
binaries must also pass the no-panic policy. Run the maintained gates rather
than substituting text searches:

```bash
make check-no-unsafe
make check-no-panics
```

## Choose A Verification Gate

| Gate | Purpose |
| --- | --- |
| `make verify-regulated-shipment` | Primary cross-layer usefulness and trust fixture |
| `make verify-canonical-spine` | Canonical runtime workflow regression |
| `make verify-semantics` | Broad Rust and Lean identity, certificate, lineage, merge, and storage checks |
| `make verify-lean-semantic-vcs` | External finite Semantic VCS conformance check |
| `make verify-release-packaging` | Deterministic archives, manifests, corruption rejection, and publication rehearsal |
| `make release-gate` | Exact publication decision |

The detailed suite map is in [Testing](howto/TESTING.md). Certificate work must
also follow [Formal verification](howto/FORMAL_VERIFICATION.md), the
[trusted-kernel boundary](reference/TRUSTED_KERNEL.md), and the
[certificate reference](reference/CERTIFICATES.md).

Only the import closure of `lean/Axiograph/VerifyMain.lean` is the trusted Lean
checker. Other Lean checks can provide useful conformance evidence without
expanding that boundary.

## Backend And Container Tests

Typed projection/readback tests that do not require containers live in the Rust
workspace. Docker-backed TypeDB and TerminusDB checks are available through:

```bash
make test-backend-containers
```

Backend state is derived from accepted `.axi` and compiled IR. Passing a native
backend test does not make that backend an ontology authority. See
[Backend projections](reference/BACKEND_PROJECTIONS.md).

## Documentation

`docs/SUMMARY.md` defines the published book order. Chapter files remain the
single documentation source; the website is generated output.

```bash
make book
```

The target validates the chapter graph, builds the static site, and rejects
broken rendered links or missing search and theme assets. Pull requests build
the same book. Pushes to `main` publish the result through the GitHub Pages
workflow.

## Release And Platform Policy

Release candidates build native bundles for:

- `x86_64-unknown-linux-gnu`;
- `aarch64-apple-darwin`;
- `x86_64-pc-windows-msvc`.

A platform is supported only for a release whose exact hosted-runner lane
records successful archive extraction, checksum, mode, CLI, and anchored
checker smokes. No support is inferred before that evidence exists.

Linux arm64 is a container candidate only. Native Linux arm64, macOS Intel,
Windows arm64, and unexecuted runner paths remain unsupported. The Linux bundle
uses the Ubuntu 24.04 glibc/OpenSSL 3 ABI baseline. The macOS arm64 deployment
target is 13.0.

Axiograph releases use the Cargo-compatible CalVer policy documented in
[Releasing](howto/RELEASING.md). The Rust workspace version is authoritative.
Workflows derive the CLI, archive, and container version from it. Helm
`appVersion` is synchronized explicitly and the release gate checks equality.

Run the complete publication decision from a clean checkout:

```bash
PATH="$(dirname "$(rustup which --toolchain 1.88.0 rustc)"):$PATH" \
  make release-gate
```

This gate decides whether exact source bytes may be published. It does not
expand the semantic scope of the Lean checker.

## Further References

- [Testing](howto/TESTING.md)
- [Releasing](howto/RELEASING.md)
- [Formal verification](howto/FORMAL_VERIFICATION.md)
- [Security boundaries](reference/SECURITY_BOUNDARIES.md)
- [Rust architecture](reference/RUST_ARCHITECTURE_CLEANUP.md)
- [Compiled kernel IR](reference/KERNEL_IR.md)
