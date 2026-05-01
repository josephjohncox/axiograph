#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
source "${SCRIPT_DIR}/example_registry.sh"

AXI="${REPO_ROOT}/examples/software_authoring/OrderFulfillmentDomain.axi"
OVERLAY="${REPO_ROOT}/examples/software_authoring/order_fulfillment_tooling_overlay.json"
BEHAVIOR_CASE="${REPO_ROOT}/examples/software_authoring/order_fulfillment_behavior_case.json"
CQ_FILE="${REPO_ROOT}/examples/software_authoring/order_fulfillment.cq"
OUT_DIR="${1:-${REPO_ROOT}/build/examples/software_authoring}"
DEFINITION_OUT_DIR="${OUT_DIR}/definitions"
GENERATED_TESTS_DIR="${OUT_DIR}/generated-tests"
COVERAGE_QUERY_ARGS=()
while IFS= read -r arg; do
  COVERAGE_QUERY_ARGS+=("${arg}")
done < <(software_authoring_coverage_query_args order_fulfillment)

mkdir -p "${OUT_DIR}" "${DEFINITION_OUT_DIR}" "${GENERATED_TESTS_DIR}"

if [[ -n "${AXIOGRAPH_BIN:-}" ]]; then
  AXIOGRAPH_CMD=("${AXIOGRAPH_BIN}")
else
  AXIOGRAPH_CMD=(cargo run --manifest-path "${REPO_ROOT}/rust/Cargo.toml" -p axiograph-cli --)
fi

if [[ -n "${AXIOGRAPH_SOFTWARE_AUTHORING_EXAMPLE_BIN:-}" ]]; then
  AUTHORING_EXAMPLE_CMD=("${AXIOGRAPH_SOFTWARE_AUTHORING_EXAMPLE_BIN}")
else
  AUTHORING_EXAMPLE_CMD=(cargo run --manifest-path "${REPO_ROOT}/rust/Cargo.toml" -p axiograph-example-software-authoring --bin axiograph-software-authoring-example --)
fi

run() {
  printf '\n+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run_expected_report_gate() {
  local out_file="$1"
  shift
  printf '\n+'
  printf ' %q' "$@"
  printf '\n'
  set +e
  "$@"
  local status=$?
  set -e
  if [[ ! -s "${out_file}" ]]; then
    printf 'expected fail-closed gate report was not written: %s\n' "${out_file}" >&2
    if [[ "${status}" -eq 0 ]]; then
      return 1
    fi
    return "${status}"
  fi
  if [[ "${status}" -eq 0 ]]; then
    printf 'strict gate passed; no fail-closed coverage issue was detected\n'
  else
    printf 'strict gate failed as an expected teaching artifact (exit %s); report: %s\n' "${status}" "${out_file}"
  fi
}

printf 'Flow: validate .axi -> runtime theory -> CQ authoring -> definitions -> overlay -> coverage -> behavior -> codegen -> continuous gates\n'

run "${AXIOGRAPH_CMD[@]}" check validate "${AXI}"

run "${AXIOGRAPH_CMD[@]}" check theory "${AXI}" \
  --closure-tier finite_fragment \
  --json \
  --out "${OUT_DIR}/theory_check.json"

run "${AXIOGRAPH_CMD[@]}" authoring tool-specs \
  --out "${OUT_DIR}/tool_specs.json"

run "${AXIOGRAPH_CMD[@]}" authoring lsp-capabilities \
  --out "${OUT_DIR}/lsp_capabilities.json"

run "${AXIOGRAPH_CMD[@]}" authoring integration-manifest \
  --out "${OUT_DIR}/integration_manifest.json"

run "${AXIOGRAPH_CMD[@]}" authoring competency-questions \
  --axi "${AXI}" \
  --cq "${CQ_FILE}" \
  --out "${OUT_DIR}/competency_questions_authoring.json"

run "${SCRIPT_DIR}/run_definition_queries.sh" "${DEFINITION_OUT_DIR}"

run "${AXIOGRAPH_CMD[@]}" discover overlay-check "${AXI}" \
  --overlay "${OVERLAY}" \
  --out "${OUT_DIR}/overlay_validation.json"

run "${AXIOGRAPH_CMD[@]}" discover coverage-query "${AXI}" \
  --overlay "${OVERLAY}" \
  "${COVERAGE_QUERY_ARGS[@]}" \
  --out "${OUT_DIR}/coverage_query.json"

run "${AXIOGRAPH_CMD[@]}" discover behavior-case "${AXI}" \
  --request "${BEHAVIOR_CASE}" \
  --cq-file "${CQ_FILE}" \
  --overlay "${OVERLAY}" \
  --out "${OUT_DIR}/behavior_case_report.json"

run "${AXIOGRAPH_CMD[@]}" check software-coverage "${AXI}" \
  --behavior-case "${BEHAVIOR_CASE}" \
  --cq-file "${CQ_FILE}" \
  --overlay "${OVERLAY}" \
  --repo-root "${REPO_ROOT}" \
  --out "${OUT_DIR}/software_coverage.json"

run "${AXIOGRAPH_CMD[@]}" authoring codegen-plan \
  --overlay "${OVERLAY}" \
  --out "${OUT_DIR}/codegen_plan.json"

run "${AXIOGRAPH_CMD[@]}" authoring continuous-check \
  --behavior-report "${OUT_DIR}/behavior_case_report.json" \
  --repo-root "${REPO_ROOT}" \
  --out "${OUT_DIR}/continuous_coverage.json"

run_expected_report_gate "${OUT_DIR}/enforced_continuous_coverage.json" \
  "${AXIOGRAPH_CMD[@]}" authoring continuous-check \
  --behavior-report "${OUT_DIR}/behavior_case_report.json" \
  --repo-root "${REPO_ROOT}" \
  --strict-coverage \
  --out "${OUT_DIR}/enforced_continuous_coverage.json"

run_expected_report_gate "${OUT_DIR}/ci_continuous_coverage.json" \
  "${AXIOGRAPH_CMD[@]}" authoring continuous-check \
  --behavior-report "${OUT_DIR}/behavior_case_report.json" \
  --repo-root "${REPO_ROOT}" \
  --strict-coverage \
  --require-code-refs \
  --require-runtime-theory \
  --out "${OUT_DIR}/ci_continuous_coverage.json"

run "${AUTHORING_EXAMPLE_CMD[@]}" continuous-check \
  --behavior-report "${OUT_DIR}/behavior_case_report.json" \
  --repo-root "${REPO_ROOT}" \
  --out "${OUT_DIR}/example_crate_continuous_coverage.json"

printf '\n+ materialize skeletons > %q\n' "${OUT_DIR}/materialize_skeletons.json"
"${AXIOGRAPH_CMD[@]}" authoring materialize-skeletons \
  --behavior-report "${OUT_DIR}/behavior_case_report.json" \
  --out-dir "${GENERATED_TESTS_DIR}" \
  --overwrite \
  --out "${OUT_DIR}/materialize_skeletons.json"

printf '\nAuthoring-flow outputs written under %s\n' "${OUT_DIR}"
printf 'Nested AuthoringFlowReportV1 payloads are embedded at *.json authoring_flow fields\n'
printf 'Generated test skeleton previews written under %s\n' "${GENERATED_TESTS_DIR}"
