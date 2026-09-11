# Released Baseline v20260908.0.0

**Diataxis:** Reference
**Audience:** contributors

The parent accepted release `v20260908.0.0` as the engineering-quality baseline.
Use this release as the start point for later roadmap work.

## Identity

- Release: <https://github.com/josephjohncox/axiograph/releases/tag/v20260908.0.0>
- Commit: `a2d9c80f8e5acc1a1ef6b106f9cf97bb2c0c30df`
- Tree: `084782076e71ca0e938436d29deed31592b30d28`
- Annotated tag object: `030420be7231f953a287a9251f27c3b28d9a253d`
- Release workflow: <https://github.com/josephjohncox/axiograph/actions/runs/34203736810>
- Merged release pull request: <https://github.com/josephjohncox/axiograph/pull/19>

The local parent receipt is
`build/engineering-quality/release-roadmap/parent-release-acceptance-20260908-a2d9c80f.json`.
Its SHA-256 is
`e020fad39e2e60881f3fbfd37cb94b64fdb5cd80aa37ab2e627aa536e3c786f5`.

## Accepted Checks

The parent receipt records these independent checks:

- The audit rehashed 303 regular files.
- All 27 frozen release source files matched Git.
- The live annotated tag resolved to the commit and tree above.
- All seven release workflow jobs completed successfully.
- All four public release assets matched the rehashed audit copies.
- The release gate used Rust 1.98.0, Node.js 26.8.1, and npm 11.19.0.
- It used cargo-audit 0.22.2 and nightly-2026-09-01 with Miri and `rust-src`.
- It used cargo-fuzz 0.13.2 and cargo-kani 0.67.0.
- The Kani PathDB proof checked ten nonempty properties with no failures.

The independent audit manifest pin is
`1c070a159aced8b79af7283b6cf1ee886d7346d8ae53a8459616187288ccf4eb`.
The independent report pin is
`14c9803156a8a787e0710d6eb4e6aeec990616744210243d9ca2eb8c454cff80`.

The four public asset SHA-256 digests are:

| Asset | SHA-256 |
| --- | --- |
| `axiograph-aarch64-apple-darwin.tar.gz` | `da885ceb02655e3abf38886440b87272a3201c7bc3c7587f9de6c6a490e7de77` |
| `axiograph-aarch64-apple-darwin.tar.gz.sha256` | `67826b1346dc630bcdfad6e1c0ed0ebe38cd4e67c817bd00c4505b706bb3cb51` |
| `axiograph-x86_64-unknown-linux-gnu.tar.gz` | `f14f8c860dc22264693b86ac1b7cbb348bb65d9a62a3a2caf87df0c858f1c11e` |
| `axiograph-x86_64-unknown-linux-gnu.tar.gz.sha256` | `cbc7cda8218932faf2e126272847b87bb435f3f483b9337aeaa7765f76d65395` |

The public container reference is
`ghcr.io/josephjohncox/axiograph:v20260908.0.0`.
Its unique OCI index digest is
`sha256:e9787984044f25a1b8e15acb9219a9fe28162c2c8ba5e4b17fbbe4e9224c5df0`.
The index contains `linux/amd64` and `linux/arm64` runtime manifests.

## Scope

This acceptance closes only the EQ-19 complete pinned release-gate item.
It does not close other engineering-quality requirements.
It does not authorize another release, tag move, deployment, or publication.

Release authentication is operational evidence.
It is not an Axiograph semantic certificate, query proof, or ontology-completeness claim.
Earlier failed and partial release records remain historical evidence.
