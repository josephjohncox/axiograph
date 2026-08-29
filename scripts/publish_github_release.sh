#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${GITHUB_REF:?GITHUB_REF is required}"
: "${GITHUB_REF_NAME:?GITHUB_REF_NAME is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${RELEASE_FILES:?RELEASE_FILES is required}"

files=()
expected_assets=()
while IFS= read -r file; do
  if [[ -z "${file}" ]]; then
    continue
  fi
  [[ "${file}" != -* ]]
  [[ -f "${file}" ]]
  files+=("${file}")
  expected_assets+=("$(basename "${file}")")
done <<<"${RELEASE_FILES}"
[[ "${#files[@]}" -gt 0 ]]
[[ "${GITHUB_REF}" == "refs/tags/${GITHUB_REF_NAME}" ]]

python3 scripts/release_version.py validate-tag \
  "${GITHUB_REF_NAME}" \
  "$(python3 scripts/release_version.py workspace --repo-root .)" \
  >/dev/null

tag_path="repos/${GITHUB_REPOSITORY}/git/ref/tags/${GITHUB_REF_NAME}"
[[ "$(gh api "${tag_path}" --jq '.object.type')" == tag ]]
tag_object="$(gh api "${tag_path}" --jq '.object.sha')"
tag_target="repos/${GITHUB_REPOSITORY}/git/tags/${tag_object}"
[[ "$(gh api "${tag_target}" --jq '.object.type')" == commit ]]
[[ "$(gh api "${tag_target}" --jq '.object.sha')" == "${GITHUB_SHA}" ]]

if release_error="$(
  gh api \
    "repos/${GITHUB_REPOSITORY}/releases/tags/${GITHUB_REF_NAME}" \
    2>&1
)"; then
  echo "release already exists for ${GITHUB_REF_NAME}" >&2
  exit 1
elif ! grep -Fq '(HTTP 404)' <<<"${release_error}"; then
  echo "${release_error}" >&2
  echo "could not prove release tag availability" >&2
  exit 1
fi

gh release create "${GITHUB_REF_NAME}" \
  --repo "${GITHUB_REPOSITORY}" \
  --verify-tag \
  --draft \
  --generate-notes \
  --target "${GITHUB_SHA}" \
  --title "Axiograph ${GITHUB_REF_NAME#v}" \
  -- "${files[@]}"

export EXPECTED_ASSETS
EXPECTED_ASSETS="$(printf '%s\n' "${expected_assets[@]}")"
export RELEASE_JSON
RELEASE_JSON="$(
  gh release view "${GITHUB_REF_NAME}" \
    --repo "${GITHUB_REPOSITORY}" \
    --json assets,isDraft,tagName,targetCommitish
)"
python3 - <<'PY'
import json
import os

release = json.loads(os.environ["RELEASE_JSON"])
expected = sorted(os.environ["EXPECTED_ASSETS"].splitlines())
actual = sorted(asset["name"] for asset in release["assets"])
if release["tagName"] != os.environ["GITHUB_REF_NAME"]:
    raise SystemExit("draft release tag changed")
if release["targetCommitish"] != os.environ["GITHUB_SHA"]:
    raise SystemExit("draft release target changed")
if release["isDraft"] is not True:
    raise SystemExit("release became public before verification")
if actual != expected:
    raise SystemExit(
        f"draft release assets are not exact: expected {expected}, actual {actual}"
    )
PY

gh release edit "${GITHUB_REF_NAME}" \
  --repo "${GITHUB_REPOSITORY}" \
  --draft=false
