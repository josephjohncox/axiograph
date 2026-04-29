#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT_DIR="${OUT_DIR:-build/examples/semantic_merge}"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd -P)"
ACCEPTED_DIR="$OUT_DIR/accepted_plane"

case "$ACCEPTED_DIR" in
  "$ROOT"/build/*|/tmp/*|/private/tmp/*) ;;
  *)
    echo "Refusing to clean accepted-plane outside repo build/ or tmp: $ACCEPTED_DIR" >&2
    echo "Set OUT_DIR under build/ or /tmp for this example." >&2
    exit 2
    ;;
esac

rm -rf "$ACCEPTED_DIR"

if [[ -n "${AXIOGRAPH_BIN:-}" ]]; then
  AXIO=("$AXIOGRAPH_BIN")
else
  AXIO=(cargo run --manifest-path rust/Cargo.toml -q -p axiograph-cli --)
fi

json_field() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, key = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)
value = data
for part in key.split("."):
    value = value[part]
print(value)
PY
}

lean_path_arg() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '../%s\n' "$1" ;;
  esac
}

echo "== Validate canonical plant modules =="
"${AXIO[@]}" check validate examples/semantic_merge/PlantOperationsCore.axi
"${AXIO[@]}" check validate examples/semantic_merge/PlantProcurementCertification.axi
"${AXIO[@]}" check validate examples/semantic_merge/PlantSimulationSafety.axi

echo "== Check runtime theory surface for the base module =="
"${AXIO[@]}" check theory examples/semantic_merge/PlantOperationsCore.axi \
  --closure-tier finite_fragment \
  --json > "$OUT_DIR/base_theory_check.json"

echo "== Build semantic review branches =="
"${AXIO[@]}" db accept init --dir "$ACCEPTED_DIR"
"${AXIO[@]}" db accept promote examples/semantic_merge/PlantOperationsCore.axi \
  --dir "$ACCEPTED_DIR" \
  --message "accept plant operations core" > "$OUT_DIR/base_snapshot.txt"
"${AXIO[@]}" sem status --dir "$ACCEPTED_DIR" --json > "$OUT_DIR/status_base.json"
BASE_COMMIT="$(json_field "$OUT_DIR/status_base.json" sem_head_commit_id)"

"${AXIO[@]}" db accept promote examples/semantic_merge/PlantProcurementCertification.axi \
  --dir "$ACCEPTED_DIR" \
  --message "review procurement certification slice" > "$OUT_DIR/procurement_snapshot.txt"
"${AXIO[@]}" sem status --dir "$ACCEPTED_DIR" --json > "$OUT_DIR/status_procurement.json"
PROCUREMENT_COMMIT="$(json_field "$OUT_DIR/status_procurement.json" sem_head_commit_id)"
"${AXIO[@]}" sem branch --dir "$ACCEPTED_DIR" --family review procurement-certification \
  --commit "$PROCUREMENT_COMMIT" --json > "$OUT_DIR/ref_procurement.json"

"${AXIO[@]}" sem ref set --dir "$ACCEPTED_DIR" --ref heads/main \
  --commit "$BASE_COMMIT" --json > "$OUT_DIR/ref_main_base.json"
"${AXIO[@]}" sem checkout --dir "$ACCEPTED_DIR" heads/main --json > "$OUT_DIR/checkout_base.json"

"${AXIO[@]}" db accept promote examples/semantic_merge/PlantSimulationSafety.axi \
  --dir "$ACCEPTED_DIR" \
  --message "review simulation safety slice" > "$OUT_DIR/simulation_snapshot.txt"
"${AXIO[@]}" sem status --dir "$ACCEPTED_DIR" --json > "$OUT_DIR/status_simulation.json"
SIMULATION_COMMIT="$(json_field "$OUT_DIR/status_simulation.json" sem_head_commit_id)"
"${AXIO[@]}" sem branch --dir "$ACCEPTED_DIR" --family review simulation-safety \
  --commit "$SIMULATION_COMMIT" --json > "$OUT_DIR/ref_simulation.json"

echo "== Build dry-run semantic merge plans =="
"${AXIO[@]}" sem merge \
  --dir "$ACCEPTED_DIR" \
  --source heads/review/procurement-certification \
  --target heads/review/simulation-safety \
  --dry-run \
  --json > "$OUT_DIR/merge_plan.json"

"${AXIO[@]}" sem merge \
  --dir "$ACCEPTED_DIR" \
  --source heads/review/procurement-certification \
  --target heads/review/simulation-safety \
  --dry-run \
  --lean-json > "$OUT_DIR/merge_plan_lean.json"

"${AXIO[@]}" sem rebase \
  --dir "$ACCEPTED_DIR" \
  --source heads/review/simulation-safety \
  --onto heads/review/procurement-certification \
  --lean-json > "$OUT_DIR/rebase_plan_lean.json"

echo "== Check semantic VCS payloads in Lean =="
(
  cd lean
  lake build axiograph_semantic_vcs_check >/dev/null
  lake exe axiograph_semantic_vcs_check \
    ../examples/semantic_merge/plant_clean_merge_lean.json \
    ../examples/semantic_merge/plant_clean_rebase_lean.json \
    "$(lean_path_arg "$OUT_DIR/merge_plan_lean.json")"
)

python3 - "$OUT_DIR/merge_plan_lean.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as f:
    payload = json.load(f)
assert payload["version"] == "semantic_vcs_lean_merge_plan_v1", payload["version"]
assert payload["blockers"] == [], payload["blockers"]
assert payload["resolver_steps"] == [], payload["resolver_steps"]
assert payload["residual_obligations"] == [], payload["residual_obligations"]
assert payload["result"]["refs"], "generated Lean merge payload should cite result refs"
PY

python3 - "$OUT_DIR/rebase_plan_lean.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as f:
    payload = json.load(f)
assert payload["version"] == "semantic_vcs_lean_rebase_plan_v1", payload["version"]
assert payload["blockers"], "generated rebase payload should expose unresolved transport blockers"
assert payload["residual_obligations"], "generated rebase payload should expose residual obligations"
PY

expect_lean_reject() {
  local payload="$1"
  local label="$2"
  if (cd lean && lake exe axiograph_semantic_vcs_check "$payload"); then
    echo "error: $label unexpectedly passed Lean check" >&2
    exit 1
  else
    echo "ok: Lean rejected $label"
  fi
}

expect_lean_reject \
  ../examples/semantic_merge/plant_conflict_merge_lean.json \
  "conflicting merge fixture with required resolver step"

expect_lean_reject \
  ../examples/semantic_merge/plant_blocked_rebase_lean.json \
  "blocked rebase fixture with residual obligations"

expect_lean_reject \
  ../examples/semantic_merge/plant_failed_transport_no_blocker_lean.json \
  "required transport fixture without explicit blockers"

expect_lean_reject \
  "$(lean_path_arg "$OUT_DIR/rebase_plan_lean.json")" \
  "generated rebase plan with unresolved transports"

python3 - "$OUT_DIR/semantic_vcs_conformance_coverage.json" <<'PY'
import json
import sys

out = sys.argv[1]
report = {
    "version": "semantic_vcs_conformance_coverage_v1",
    "scope": "finite semantic VCS merge/rebase checker surface",
    "complete_for_claimed_surface": True,
    "coverage_basis": [
        "canonical .axi validation",
        "runtime theory finite_fragment check",
        "accepted-plane branch/review ref construction",
        "Rust-generated semantic merge Lean payload",
        "Lean executable semantic VCS checker"
    ],
    "cases": [
        {
            "case_id": "plant_modules_validate",
            "operation": "canonical_axi_validate",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "plant_core_theory_finite_fragment",
            "operation": "runtime_theory_check",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "accepted_plane_review_branch_split",
            "operation": "semantic_vcs_refs",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "clean_merge_fixture",
            "operation": "lean_merge_materialization",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "clean_rebase_fixture",
            "operation": "lean_rebase_transport",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "rust_generated_merge_payload",
            "operation": "rust_to_lean_merge_payload",
            "expected": "pass",
            "observed": "pass"
        },
        {
            "case_id": "conflicting_merge_fixture",
            "operation": "lean_merge_materialization",
            "expected": "reject",
            "observed": "reject"
        },
        {
            "case_id": "blocked_rebase_fixture",
            "operation": "lean_rebase_materialization",
            "expected": "reject",
            "observed": "reject"
        },
        {
            "case_id": "required_failed_transport_fixture",
            "operation": "lean_rebase_transport",
            "expected": "reject",
            "observed": "reject"
        },
        {
            "case_id": "rust_generated_blocked_rebase_payload",
            "operation": "rust_to_lean_rebase_payload",
            "expected": "reject",
            "observed": "reject"
        }
    ],
    "non_claims": [
        "not complete ontology closure",
        "not globally optimal semantic merge",
        "not full HoTT or univalence",
        "not a replacement for VerifyMain trusted certificate families",
        "not backend-native mutation authority"
    ]
}
with open(out, "w", encoding="utf-8") as f:
    json.dump(report, f, indent=2, sort_keys=True)
    f.write("\n")
PY

echo "wrote reports under $OUT_DIR"
