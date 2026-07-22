#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

source "${SCRIPT_DIR}/example_registry.sh"

SUITE_FILE="${1:-${REPO_ROOT}/examples/software_authoring/software_authoring_examples.json}"
OUT_ROOT="${2:-${REPO_ROOT}/build/examples/software_authoring/codegen_examples}"
HOST_CONTRACTS_DIR="${OUT_ROOT}/host_contracts"

# The JSON registry is still the typed tool input; this shell runner uses the
# registry so examples do not depend on external JSON-filter helper tools.
if [[ ! -f "${SUITE_FILE}" ]]; then
	printf 'suite catalog does not exist: %s\n' "${SUITE_FILE}" >&2
	exit 1
fi

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

RAN_EXAMPLES=0

run() {
	printf '\n+'
	printf ' %q' "$@"
	printf '\n'
	"$@"
}

run_expect_report() {
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
		printf 'expected report was not written: %s\n' "${out_file}" >&2
		if [[ "${status}" -eq 0 ]]; then
			return 1
		fi
		return "${status}"
	fi
	if [[ "${status}" -ne 0 ]]; then
		printf 'command exited %s after writing fail-closed report %s\n' "${status}" "${out_file}" >&2
	fi
	return 0
}

abspath() {
	local path="$1"
	if [[ "${path}" = /* ]]; then
		printf '%s\n' "${path}"
	else
		printf '%s\n' "${REPO_ROOT}/${path}"
	fi
}

run_example() {
	local id="$1"
	local title axi overlay behavior_case cq_file authoring_request out_dir generated_dir
	local -a coverage_query_args

	title="$(software_authoring_example_field "${id}" title)"

	if [[ -n "${EXAMPLE_ID:-}" && "${EXAMPLE_ID}" != "${id}" ]]; then
		return
	fi
	RAN_EXAMPLES=$((RAN_EXAMPLES + 1))

	axi="$(abspath "$(software_authoring_example_field "${id}" axi)")"
	overlay="$(abspath "$(software_authoring_example_field "${id}" overlay)")"
	behavior_case="$(abspath "$(software_authoring_example_field "${id}" behavior_case)")"
	cq_file="$(abspath "$(software_authoring_example_field "${id}" cq_file)")"
	if [[ "${id}" == "order_fulfillment" ]]; then
		authoring_request="${SCRIPT_DIR}/authoring_workspace_request.json"
	else
		authoring_request="${SCRIPT_DIR}/${id}_authoring_workspace_request.json"
	fi
	coverage_query_args=()
	while IFS= read -r arg; do
		coverage_query_args+=("${arg}")
	done < <(software_authoring_coverage_query_args "${id}")
	out_dir="${OUT_ROOT}/${id}"
	generated_dir="${out_dir}/generated-tests"

	mkdir -p "${out_dir}" "${generated_dir}"

	printf '\n=== %s: %s ===\n' "${id}" "${title}"
	printf 'Flow: unified typed workspace authoring -> definitions -> overlay -> coverage -> behavior -> codegen -> continuous gates\n'

	run "${AXIOGRAPH_CMD[@]}" check validate "${axi}"
	run "${AXIOGRAPH_CMD[@]}" check theory "${axi}" \
		--closure-tier finite_fragment \
		--json \
		--out "${out_dir}/theory_check.json"
	run "${AXIOGRAPH_CMD[@]}" authoring workspace \
		--workspace "${REPO_ROOT}" \
		--request "${authoring_request}" \
		--out "${out_dir}/authoring_workspace_report.json"
	run "${SCRIPT_DIR}/run_definition_queries.sh" \
		"${out_dir}/definitions" \
		"${id}" \
		"${axi}" \
		"${overlay}"
	run "${AXIOGRAPH_CMD[@]}" discover overlay-check "${axi}" \
		--overlay "${overlay}" \
		--out "${out_dir}/overlay_validation.json"
	run "${AXIOGRAPH_CMD[@]}" discover coverage-query "${axi}" \
		--overlay "${overlay}" \
		"${coverage_query_args[@]}" \
		--out "${out_dir}/coverage_query.json"
	run "${AXIOGRAPH_CMD[@]}" discover behavior-case "${axi}" \
		--request "${behavior_case}" \
		--cq-file "${cq_file}" \
		--overlay "${overlay}" \
		--out "${out_dir}/behavior_case_report.json"
	run "${AXIOGRAPH_CMD[@]}" check software-coverage "${axi}" \
		--behavior-case "${behavior_case}" \
		--cq-file "${cq_file}" \
		--overlay "${overlay}" \
		--repo-root "${REPO_ROOT}" \
		--out "${out_dir}/software_coverage.json"
	run "${AXIOGRAPH_CMD[@]}" authoring codegen-plan \
		--overlay "${overlay}" \
		--out "${out_dir}/codegen_plan.json"
	run "${AXIOGRAPH_CMD[@]}" authoring continuous-check \
		--behavior-report "${out_dir}/behavior_case_report.json" \
		--repo-root "${REPO_ROOT}" \
		--out "${out_dir}/continuous_coverage.json"
	run_expect_report "${out_dir}/enforced_continuous_coverage.json" \
		"${AXIOGRAPH_CMD[@]}" authoring continuous-check \
		--behavior-report "${out_dir}/behavior_case_report.json" \
		--repo-root "${REPO_ROOT}" \
		--strict-coverage \
		--out "${out_dir}/enforced_continuous_coverage.json"
	run_expect_report "${out_dir}/ci_continuous_coverage.json" \
		"${AXIOGRAPH_CMD[@]}" authoring continuous-check \
		--behavior-report "${out_dir}/behavior_case_report.json" \
		--repo-root "${REPO_ROOT}" \
		--strict-coverage \
		--require-code-refs \
		--require-runtime-theory \
		--out "${out_dir}/ci_continuous_coverage.json"
	run "${AUTHORING_EXAMPLE_CMD[@]}" continuous-check \
		--behavior-report "${out_dir}/behavior_case_report.json" \
		--repo-root "${REPO_ROOT}" \
		--out "${out_dir}/example_crate_continuous_coverage.json"

	printf '\n+ materialize skeletons for %s > %s\n' "${id}" "${out_dir}/materialize_skeletons.json"
	"${AXIOGRAPH_CMD[@]}" authoring materialize-skeletons \
		--behavior-report "${out_dir}/behavior_case_report.json" \
		--out-dir "${generated_dir}" \
		--overwrite \
		--out "${out_dir}/materialize_skeletons.json"
}

mkdir -p "${HOST_CONTRACTS_DIR}"

run "${AXIOGRAPH_CMD[@]}" authoring tool-specs \
	--out "${HOST_CONTRACTS_DIR}/tool_specs.json"
run "${AXIOGRAPH_CMD[@]}" authoring lsp-capabilities \
	--out "${HOST_CONTRACTS_DIR}/lsp_capabilities.json"
run "${AXIOGRAPH_CMD[@]}" authoring integration-manifest \
	--out "${HOST_CONTRACTS_DIR}/integration_manifest.json"

while IFS= read -r id; do
	run_example "${id}"
done < <(software_authoring_example_ids)

if [[ "${RAN_EXAMPLES}" -eq 0 ]]; then
	printf 'no software-authoring examples matched EXAMPLE_ID=%s in %s\n' "${EXAMPLE_ID:-}" "${SUITE_FILE}" >&2
	exit 1
fi

printf '\nCodegen example outputs written under %s\n' "${OUT_ROOT}"
printf 'Host integration contracts written under %s\n' "${HOST_CONTRACTS_DIR}"
