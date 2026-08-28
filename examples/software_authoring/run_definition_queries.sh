#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

source "${SCRIPT_DIR}/example_registry.sh"

OUT_DIR="${1:-${REPO_ROOT}/build/examples/software_authoring/definitions}"
EXAMPLE_ID="$(software_authoring_resolve_example_id "${2:-order_fulfillment}")"
AXI="${3:-${REPO_ROOT}/$(software_authoring_example_field "${EXAMPLE_ID}" axi)}"
OVERLAY="${4:-${REPO_ROOT}/$(software_authoring_example_field "${EXAMPLE_ID}" overlay)}"

mkdir -p "${OUT_DIR}"

if [[ -n "${AXIOGRAPH_BIN:-}" ]]; then
  AXIOGRAPH_CMD=("${AXIOGRAPH_BIN}")
else
  AXIOGRAPH_CMD=(cargo run --manifest-path "${REPO_ROOT}/rust/Cargo.toml" -p axiograph-cli --)
fi

slugify() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/_/g; s/^_+//; s/_+$//'
}

printf 'Running bundled definition queries for %s\n' "${EXAMPLE_ID}"

while IFS='|' read -r slug prompt kind context include_queries max_matches; do
  if [[ -z "${slug}" ]]; then
    slug="$(slugify "${prompt}")"
  fi

  args=(discover define "${AXI}" --overlay "${OVERLAY}" --prompt "${prompt}")
  if [[ -n "${kind}" ]]; then
    args+=(--kind-hint "${kind}")
  fi
  if [[ -n "${context}" ]]; then
    args+=(--context-hint "${context}")
  fi
  if [[ "${include_queries}" == "true" ]]; then
    args+=(--include-queries)
  fi
  if [[ -n "${max_matches}" ]]; then
    args+=(--max-matches "${max_matches}")
  fi

  out="${OUT_DIR}/${slug}.json"
  printf '\n+ discover define: %s\n' "${prompt}"
  "${AXIOGRAPH_CMD[@]}" "${args[@]}" --out "${out}"
done < <(software_authoring_definition_query_rows "${EXAMPLE_ID}")

printf '\nDefinition-query reports written under %s\n' "${OUT_DIR}"
