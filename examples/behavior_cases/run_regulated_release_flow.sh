#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

AXI="${REPO_ROOT}/examples/industrial/RegulatedProductionLine.axi"
BEHAVIOR_CASE="${REPO_ROOT}/examples/behavior_cases/regulated_ship_release.json"
OVERLAY="${REPO_ROOT}/examples/behavior_cases/regulated_ship_release_overlay.json"
OUT_DIR="${1:-${REPO_ROOT}/build/examples/regulated_release}"

mkdir -p "${OUT_DIR}"

if [[ -n "${AXIOGRAPH_BIN:-}" ]]; then
  AXIOGRAPH_CMD=("${AXIOGRAPH_BIN}")
else
  AXIOGRAPH_CMD=(cargo run --manifest-path "${REPO_ROOT}/rust/Cargo.toml" -p axiograph-cli --)
fi

run() {
  printf '\n+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run "${AXIOGRAPH_CMD[@]}" check validate "${AXI}"

run "${AXIOGRAPH_CMD[@]}" check theory "${AXI}" \
  --closure-tier finite_fragment \
  --json \
  --out "${OUT_DIR}/theory_check.json"

run "${AXIOGRAPH_CMD[@]}" discover overlay-check "${AXI}" \
  --overlay "${OVERLAY}" \
  --out "${OUT_DIR}/overlay_validation.json"

run "${AXIOGRAPH_CMD[@]}" discover behavior-case "${AXI}" \
  --request "${BEHAVIOR_CASE}" \
  --cq-file "${SCRIPT_DIR}/regulated_ship_release.cq" \
  --overlay "${OVERLAY}" \
  --out "${OUT_DIR}/behavior_case_report.json"

printf '\nRegulated-release flow outputs written under %s\n' "${OUT_DIR}"
