#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${1:-$root/build/examples/regulated_shipment}"
axiograph="${AXIOGRAPH_BIN:-$root/rust/target/debug/axiograph}"
verifier="${AXIOGRAPH_VERIFY_BIN:-$root/lean/.lake/build/bin/axiograph_verify}"
baseline="$root/examples/regulated_shipment/RegulatedShipmentBaseline.axi"
candidate="$root/examples/regulated_shipment/RegulatedShipment.axi"
cq="$root/examples/regulated_shipment/regulated_shipment.cq"
overlay="$root/examples/regulated_shipment/regulated_shipment_tooling_overlay.json"
behavior="$root/examples/regulated_shipment/regulated_shipment_behavior_case.json"
query="$root/examples/regulated_shipment/release_certificate_query.json"

mkdir -p "$out/certificates" "$out/generated"

if [ -z "${AXIOGRAPH_BIN:-}" ]; then
	cargo build --manifest-path "$root/rust/Cargo.toml" \
		-p axiograph-cli -p axiograph-example-regulated-shipment --locked
elif [ ! -x "$axiograph" ]; then
	echo "error: AXIOGRAPH_BIN is not executable: $axiograph" >&2
	exit 1
fi
if [ -z "${AXIOGRAPH_VERIFY_BIN:-}" ]; then
	(cd "$root/lean" && lake build axiograph_verify)
elif [ ! -x "$verifier" ]; then
	echo "error: AXIOGRAPH_VERIFY_BIN is not executable: $verifier" >&2
	exit 1
fi
verifier_sha256="$(shasum -a 256 "$verifier" | awk '{print $1}')"

"$axiograph" check validate "$baseline"
"$axiograph" check validate "$candidate"
"$axiograph" check theory "$baseline" --closure-tier finite_fragment --json \
	--out "$out/baseline_theory.json"
"$axiograph" check theory "$candidate" --closure-tier finite_fragment --json \
	--out "$out/candidate_theory.json"

"$axiograph" authoring workspace --workspace "$root" \
	--request "$root/examples/regulated_shipment/authoring_baseline_request.json" \
	--out "$out/baseline_authoring.json"
"$axiograph" authoring workspace --workspace "$root" \
	--request "$root/examples/regulated_shipment/authoring_request.json" \
	--out "$out/candidate_authoring.json"

for phase in baseline candidate; do
	if [ "$phase" = baseline ]; then
		axi="$baseline"
	else
		axi="$candidate"
	fi
	"$axiograph" cert typecheck "$axi" \
		--out "$out/certificates/${phase}_typecheck.json"
	"$axiograph" cert constraints "$axi" \
		--out "$out/certificates/${phase}_constraints.json"
	cargo run --quiet --manifest-path "$root/rust/Cargo.toml" \
		-p axiograph-kernel --example emit_category_kernel_certificate --locked -- \
		"$axi" RegulatedShipment \
		>"$out/certificates/${phase}_category_kernel.json"
	{
		"$verifier" "$axi" "$out/certificates/${phase}_typecheck.json"
		"$verifier" "$axi" "$out/certificates/${phase}_constraints.json"
		"$verifier" "$axi" "$out/certificates/${phase}_category_kernel.json"
	} >"$out/certificates/${phase}_verification_receipt.txt"
	"$axiograph" check finite-query "$axi" \
		--query "$query" \
		--verify-bin "$verifier" \
		--verify-sha256 "$verifier_sha256" \
		--verify-build-id axiograph-verify-main-v3 \
		--out "$out/certificates/${phase}_query_verification.json"
done

for tamper in saturation presentation congruence; do
	cargo run --quiet --manifest-path "$root/rust/Cargo.toml" \
		-p axiograph-kernel --example emit_category_kernel_certificate --locked -- \
		"$candidate" RegulatedShipment "--tamper-${tamper}" \
		>"$out/certificates/candidate_category_kernel_${tamper}_tampered.json"
	if "$verifier" "$candidate" \
		"$out/certificates/candidate_category_kernel_${tamper}_tampered.json" \
		>"$out/certificates/candidate_category_kernel_${tamper}_tampered.stdout" \
		2>"$out/certificates/candidate_category_kernel_${tamper}_tampered.stderr"; then
		echo "error: tampered category-kernel ${tamper} certificate unexpectedly verified" >&2
		exit 1
	fi
done

"$axiograph" discover overlay-check "$candidate" --overlay "$overlay" \
	--out "$out/overlay_validation.json"
"$axiograph" discover behavior-case "$candidate" --request "$behavior" \
	--overlay "$overlay" --cq-file "$cq" --out "$out/behavior_case.json"
"$axiograph" check software-coverage "$candidate" --behavior-case "$behavior" \
	--overlay "$overlay" --cq-file "$cq" --out "$out/software_coverage.json"
"$axiograph" authoring materialize-skeletons \
	--behavior-report "$out/behavior_case.json" \
	--out-dir "$out/generated" --language rust \
	--out "$out/generated_code.json"

rust_test="$out/generated/tests/behavior_cases/regulated_shipment_release_dispatch.rs"
rustc --edition=2021 --test "$rust_test" -o "$out/generated/regulated_shipment_test"
"$out/generated/regulated_shipment_test"

"$axiograph" tools projection emit "$candidate" --backend typedb \
	--search-root "$root/examples/regulated_shipment" \
	--out "$out/typedb_projection.json" \
	--artifact-out "$out/regulated_shipment.tql"
"$axiograph" tools projection emit "$candidate" --backend pathdb \
	--search-root "$root/examples/regulated_shipment" \
	--out "$out/pathdb_projection.json" \
	--artifact-out "$out/regulated_shipment.pathdb.json"

cargo run --quiet --manifest-path "$root/rust/Cargo.toml" \
	-p axiograph-example-regulated-shipment -- \
	--baseline-axi "$baseline" \
	--candidate-axi "$candidate" \
	--baseline-authoring-report "$out/baseline_authoring.json" \
	--candidate-authoring-report "$out/candidate_authoring.json" \
	--baseline-theory-report "$out/baseline_theory.json" \
	--candidate-theory-report "$out/candidate_theory.json" \
	--baseline-verification-receipt "$out/certificates/baseline_verification_receipt.txt" \
	--candidate-verification-receipt "$out/certificates/candidate_verification_receipt.txt" \
	--baseline-query-verification "$out/certificates/baseline_query_verification.json" \
	--candidate-query-verification "$out/certificates/candidate_query_verification.json" \
	--verify-bin "$verifier" \
	--verify-sha256 "$verifier_sha256" \
	--verify-build-id axiograph-verify-main-v3 \
	--verify-timeout-secs 30 \
	--store-dir "$out/store" \
	--out "$out/usefulness_report.json"

for bad in \
	"$root/fixtures/adversarial/regulated_shipment/BadReviewer.axi" \
	"$root/fixtures/adversarial/regulated_shipment/BadPathEquation.axi"; do
	if "$axiograph" check validate "$bad" >"$out/$(basename "$bad").rejection.log" 2>&1; then
		echo "error: adversarial fixture unexpectedly validated: $bad" >&2
		exit 1
	fi
done

(
	cd "$root/lean"
	lake exe axiograph_finite_theory_tests
)
(
	cd "$root/rust"
	python3 "$root/scripts/run_required_query_tests.py" --package axiograph-query \
		--filter regulated_shipment_exact_path_query_is_complete_and_missing_rows_reject
	cargo test -p axiograph-example-regulated-shipment -- --nocapture
)

printf 'regulated-shipment workflow passed; report: %s\n' "$out/usefulness_report.json"
