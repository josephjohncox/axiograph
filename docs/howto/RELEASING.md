# Release Axiograph

This procedure publishes native bundles, checksums, a multi-architecture
container, and a GitHub release from one reviewed commit.

## Version Format

Axiograph uses Cargo-compatible calendar versions:

```text
YYYYMMDD.0.N
```

- `YYYYMMDD` is the UTC release date and must be a real Gregorian date.
- The middle component is reserved and must be `0`.
- `N` is the zero-based release sequence for that date, without leading zeros;
  the supported range is 0 through 999999.
- The Git tag is `v` followed by the exact version.

For example, the first and second releases on 29 August 2026 are
`20260829.0.0` and `20260829.0.1`, with tags `v20260829.0.0` and
`v20260829.0.1`.

Conventional `YYYY.MM.DD` is not used because zero-padded SemVer numeric
components are invalid in Cargo manifests. The chosen form sorts by date,
remains valid Cargo/SemVer syntax, and provides an explicit same-day sequence.

A published version is permanently consumed. Do not delete and reuse a version,
move its tag, or replace its assets.

## Prerequisites

Before changing the version:

1. Confirm the canonical repository and release destination.
2. Use the current UTC date.
3. Inspect Git tags, GitHub releases, and GHCR tags for that date.
4. Choose `N` as one greater than the highest observed sequence. Use `0` only
   when no release or partial publication exists for that date.
5. Work in a clean, isolated checkout. The release gate rejects dirty source.

A failed or partial publication still consumes its version if any versioned
artifact became public.

## Prepare The Candidate

Set `[workspace.package].version` in `rust/Cargo.toml`. All first-party crates
inherit this value. Update the two exact lockfiles without upgrading unrelated
dependencies:

```bash
cargo update --manifest-path rust/Cargo.toml -p axiograph-cli
cargo update --manifest-path rust/fuzz/Cargo.toml -p axiograph-cli
```

Set `appVersion` in `deploy/helm/axiograph/Chart.yaml` to the same value. Update
the exact `v<version>` image in
`deploy/k8s/axiograph-db-statefulset.yaml`. The Helm chart derives its default
image tag from `appVersion`; leave the chart's own `version` independent.

Validate the version and locked graphs:

```bash
python3 scripts/release_version.py workspace --repo-root .
cargo metadata --manifest-path rust/Cargo.toml \
  --locked --format-version 1 --no-deps >/dev/null
cargo metadata --manifest-path rust/fuzz/Cargo.toml \
  --locked --format-version 1 --no-deps >/dev/null
```

The release workflow derives its version from `rust/Cargo.toml`; it does not
maintain another version constant.

## Verify The Candidate

Commit the complete candidate before running the clean-source gate. Then run:

```bash
python3 -m unittest discover -s scripts/tests -p 'test_*.py' -v
make verify-release-packaging
make rehearse-release-publication
PATH="$(dirname "$(rustup which --toolchain 1.98.0 rustc)"):$PATH" \
  make release-gate
git status --short
git diff --check
```

The worktree must remain clean. Push the candidate branch and require its normal
pull-request checks before merging.

## Run The Hosted Failure Rehearsal

Use a disposable tag based on the final candidate version:

```bash
VERSION="$(python3 scripts/release_version.py workspace --repo-root .)"
CANDIDATE_SHA="$(git rev-parse HEAD)"
REHEARSAL_TAG="v${VERSION}-rehearsal-fail-1"

git tag "$REHEARSAL_TAG" "$CANDIDATE_SHA"
git push origin "$REHEARSAL_TAG"
gh workflow run Release --ref "$REHEARSAL_TAG" \
  -f inject_verify_failure=true
```

Rehearsal tags are excluded from automatic tag-triggered releases. The manual
run must fail in `verify` at the requested injection point. Confirm that every
build and publish job was skipped and that neither a GitHub release nor a GHCR
version tag was created.

Record the workflow URL, then remove the disposable tag:

```bash
git push origin ":refs/tags/${REHEARSAL_TAG}"
git tag -d "$REHEARSAL_TAG"
```

A rehearsal is evidence about job ordering and credential behavior. It is not a
successful release and does not establish platform support.

## Publish

Immediately before tagging, reconfirm that the exact Git tag, GitHub release,
and GHCR tag do not exist. Confirm that the reviewed merge commit is the commit
that passed the release gate.

Create and push one annotated tag:

```bash
VERSION="$(python3 scripts/release_version.py workspace --repo-root .)"
RELEASE_SHA="$(git rev-parse HEAD)"
python3 scripts/release_version.py validate-tag "v${VERSION}" "$VERSION"
git tag -a "v${VERSION}" "$RELEASE_SHA" -m "Axiograph ${VERSION}"
git push origin "v${VERSION}"
```

The tag workflow must complete in this order:

1. exact clean-checkout release gate;
2. deterministic native bundles on all declared hosted runners;
3. audited per-architecture container images and multi-architecture manifest;
4. revalidation and publication of GitHub release assets.

The release action generates release notes from the repository history.

## Record Publication Evidence

Retain evidence that:

- the tag and release point to the tested commit;
- both native archives and both checksum files exist;
- each checksum verifies;
- archive manifests contain the exact source commit and CalVer;
- packaged CLIs report `axiograph <version>`;
- the anchored checker corpus passes from every package;
- GHCR exposes the declared amd64 and arm64 container architectures;
- release notes were generated;
- each supported native platform has exact hosted-runner evidence.

Local rehearsal cannot prove GitHub/GHCR transactionality or hosted-platform
support.

## Handle A Partial Failure

If publication fails after any versioned artifact becomes public:

1. mark the version consumed;
2. preserve the failed workflow and provider evidence;
3. remove partial mutable aliases only when appropriate;
4. fix the failure on a new commit;
5. allocate the next same-day sequence, or the next date's `.0` release;
6. rerun the complete process.

Never force-move the production tag or reuse the failed version.
