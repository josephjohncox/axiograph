#!/usr/bin/env python3
"""Read-only verifier for closed no-unsafe normative evidence schema v5."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import sys
from collections.abc import Callable
from pathlib import Path

_BOOTSTRAP_ROOT = Path(__file__).resolve().parents[1]
if str(_BOOTSTRAP_ROOT) not in sys.path:
    sys.path.insert(0, str(_BOOTSTRAP_ROOT))

from scripts.run_no_unsafe_row_evidence import (
    CASE_MAP_REL,
    EVIDENCE_SCHEMA,
    REPOSITORY_ROOT,
    CaseMapFailure,
    validate_case_map,
    validate_executed_evidence,
)


def _expect_reject(name: str, operation: Callable[[], object], rejected: list[str]) -> None:
    try:
        operation()
    except CaseMapFailure:
        rejected.append(name)
    else:
        raise CaseMapFailure(f"negative accepted: {name}")


def _sync_execution_stdout(execution: dict[str, object]) -> None:
    execution["stdout"] = json.dumps(execution["outcome"], sort_keys=True) + "\n"


def _first_bound_case(
    case_value: dict[str, object],
    evidence: dict[str, object],
) -> tuple[dict[str, object], dict[str, object], dict[str, object], dict[str, object]]:
    case_row = next(
        row
        for row in case_value["rows"]  # type: ignore[index,union-attr]
        if any(
            case["kind"] == "execution" and case["predicates"]
            for case in row["cases"]
        )
    )
    case = next(
        case
        for case in case_row["cases"]
        if case["kind"] == "execution" and case["predicates"]
    )
    evidence_row = next(
        row for row in evidence["rows"] if row["id"] == case_row["id"]  # type: ignore[index,union-attr]
    )
    result = next(item for item in evidence_row["cases"] if item["id"] == case["id"])
    return case_row, case, evidence_row, result


def _wrong_cause_mutation(
    case_value: dict[str, object],
) -> tuple[dict[str, object], str]:
    """Make a map-valid mutation to another cause named by the same row."""
    for row in case_value["rows"]:  # type: ignore[index,union-attr]
        causes = {
            predicate["assertion"]["normative_cause"]
            for case in row["cases"]
            if case["kind"] == "execution"
            for predicate in case["predicates"]
            if predicate["assertion"]["normative_cause"] is not None
        }
        if len(causes) < 2:
            continue
        for case in row["cases"]:
            if case["kind"] != "execution":
                continue
            for predicate in case["predicates"]:
                assertion = predicate["assertion"]
                original = assertion["normative_cause"]
                if original is None:
                    continue
                replacement = next(cause for cause in sorted(causes) if cause != original)
                mutated = copy.deepcopy(case_value)
                mutated_case = next(
                    candidate
                    for mutated_row in mutated["rows"]  # type: ignore[index,union-attr]
                    for candidate in mutated_row["cases"]
                    if candidate["id"] == case["id"]
                )
                mutated_assertion = next(
                    candidate["assertion"]
                    for candidate in mutated_case["predicates"]
                    if candidate["id"] == predicate["id"]
                )
                check = next(
                    item
                    for item in mutated_assertion["argument_equals"]
                    if original in item["value"]
                )
                check["value"] = check["value"].replace(original, replacement)
                mutated_assertion["normative_cause"] = replacement
                return mutated, case["id"]
    raise CaseMapFailure("case map lacks two normative causes in one row")


def _negative_checks(
    case_value: dict[str, object],
    evidence: dict[str, object],
    case_rows: list[dict[str, object]],
) -> list[str]:
    """Require structural and semantic mutations of executed evidence to reject."""
    rejected: list[str] = []
    map_mutations: dict[str, dict[str, object]] = {}

    missing_row = copy.deepcopy(case_value)
    missing_row["rows"].pop()  # type: ignore[index,union-attr]
    map_mutations["map-missing-row"] = missing_row
    wrong_level = copy.deepcopy(case_value)
    wrong_level["rows"][0]["cases"][0]["level"] = "direct"  # type: ignore[index]
    map_mutations["map-wrong-level"] = wrong_level
    wrong_stimulus = copy.deepcopy(case_value)
    wrong_stimulus["rows"][0]["stimulus"] = "unrelated"  # type: ignore[index]
    map_mutations["map-wrong-stimulus"] = wrong_stimulus
    wrong_method = copy.deepcopy(case_value)
    method_case = next(
        case
        for row in wrong_method["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution"
    )
    method_case["invocation"]["method"]["source_sha256"] = "0" * 64
    map_mutations["map-wrong-method"] = wrong_method
    wrong_window = copy.deepcopy(case_value)
    window = next(
        case
        for row in wrong_window["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution" and case["stimulus"]["kind"] == "subtest"
    )
    window["stimulus"]["params"] = {"window": "unrelated"}
    map_mutations["map-wrong-window"] = wrong_window
    unrelated = copy.deepcopy(case_value)
    unrelated_predicate = next(
        predicate
        for row in unrelated["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution"
        for predicate in case["predicates"]
    )
    unrelated_predicate["assertion"]["source_line"] = 1
    map_mutations["map-unrelated-assertion"] = unrelated
    missing_case = copy.deepcopy(case_value)
    missing_case["rows"][0]["cases"] = []  # type: ignore[index]
    map_mutations["map-missing-case"] = missing_case
    wrong_matrix = copy.deepcopy(case_value)
    wrong_matrix["normative_matrix"]["sha256"] = "0" * 64  # type: ignore[index]
    map_mutations["map-wrong-normative-contract"] = wrong_matrix
    duplicate_reuse = copy.deepcopy(case_value)
    reuse_case = next(
        case
        for row in duplicate_reuse["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution" and len(case["predicates"]) >= 2
    )
    reuse_case["predicates"][1]["assertion"] = copy.deepcopy(
        reuse_case["predicates"][0]["assertion"]
    )
    map_mutations["map-duplicate-observation-reuse"] = duplicate_reuse
    wrong_normative_cause = copy.deepcopy(case_value)
    normative_assertion = next(
        predicate["assertion"]
        for row in wrong_normative_cause["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution"
        for predicate in case["predicates"]
        if predicate["assertion"]["normative_cause"] is not None
    )
    original_cause = normative_assertion["normative_cause"]
    normative_check = next(
        item
        for item in normative_assertion["argument_equals"]
        if original_cause in item["value"]
    )
    normative_check["value"] = normative_check["value"].replace(
        original_cause,
        "E_FAKE",
    )
    normative_assertion["normative_cause"] = "E_FAKE"
    map_mutations["map-wrong-normative-cause"] = wrong_normative_cause
    for name, malformed in map_mutations.items():
        _expect_reject(
            name,
            lambda malformed=malformed: validate_case_map(
                malformed,
                enforce_identity=False,
            ),
            rejected,
        )

    _, bound_case, _, bound_result = _first_bound_case(case_value, evidence)
    wrong_cause, wrong_cause_case_id = _wrong_cause_mutation(case_value)
    wrong_cause_rows = validate_case_map(wrong_cause, enforce_identity=False)
    wrong_cause_result = next(
        case
        for row in evidence["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["id"] == wrong_cause_case_id
    )
    if not wrong_cause_result["matched_predicates"]:
        raise CaseMapFailure("wrong-cause mutation selected an unbound case")
    _expect_reject(
        "semantic-wrong-cause-identity-disabled",
        lambda: validate_executed_evidence(evidence, wrong_cause_rows),
        rejected,
    )
    wrong_occurrence = copy.deepcopy(case_value)
    wrong_occurrence_case = next(
        case
        for row in wrong_occurrence["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["id"] == bound_case["id"]
    )
    wrong_occurrence_case["predicates"][0]["assertion"]["site_occurrence"] += 1_000_000
    wrong_occurrence_rows = validate_case_map(wrong_occurrence, enforce_identity=False)
    _expect_reject(
        "semantic-wrong-occurrence-identity-disabled",
        lambda: validate_executed_evidence(evidence, wrong_occurrence_rows),
        rejected,
    )

    evidence_mutations: dict[str, dict[str, object]] = {}
    missing_row_evidence = copy.deepcopy(evidence)
    missing_row_evidence["rows"].pop()  # type: ignore[index,union-attr]
    evidence_mutations["evidence-missing-row"] = missing_row_evidence
    missing_case_evidence = copy.deepcopy(evidence)
    missing_case_evidence["rows"][0]["cases"].pop()  # type: ignore[index]
    evidence_mutations["evidence-missing-case"] = missing_case_evidence
    unknown_case = copy.deepcopy(evidence)
    unknown_case["rows"][0]["cases"][0]["id"] = "unknown-case"  # type: ignore[index]
    evidence_mutations["evidence-unknown-case"] = unknown_case
    unexecuted = copy.deepcopy(evidence)
    unexecuted["rows"][0]["cases"][0]["executed"] = False  # type: ignore[index]
    evidence_mutations["evidence-unexecuted-case"] = unexecuted
    missing_invocation = copy.deepcopy(evidence)
    missing_invocation["invocations"].pop()  # type: ignore[index,union-attr]
    evidence_mutations["evidence-missing-invocation"] = missing_invocation
    invocation_method = copy.deepcopy(evidence)
    invocation_method["invocations"][0]["method"]["source_sha256"] = "0" * 64  # type: ignore[index]
    evidence_mutations["evidence-wrong-method"] = invocation_method

    row_id = next(
        row["id"]
        for row in evidence["rows"]  # type: ignore[index,union-attr]
        if any(case["id"] == bound_result["id"] for case in row["cases"])
    )
    for name, mutate in (
        ("evidence-wrong-level", lambda case: case.__setitem__("level", "inspection")),
        (
            "evidence-wrong-stimulus",
            lambda case: case["stimulus"].__setitem__("params", {"wrong": "stimulus"}),
        ),
        (
            "evidence-wrong-occurrence",
            lambda case: case["matched_predicates"][0].__setitem__(
                "occurrence",
                case["matched_predicates"][0]["occurrence"] + 1_000_000,
            ),
        ),
    ):
        malformed = copy.deepcopy(evidence)
        selected = next(
            case
            for row in malformed["rows"]  # type: ignore[index,union-attr]
            if row["id"] == row_id
            for case in row["cases"]
            if case["id"] == bound_result["id"]
        )
        mutate(selected)
        evidence_mutations[name] = malformed

    subtest_result = next(
        case
        for row in evidence["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["kind"] == "execution" and case["stimulus"]["kind"] == "subtest"
    )
    wrong_window_evidence = copy.deepcopy(evidence)
    selected_window = next(
        case
        for row in wrong_window_evidence["rows"]  # type: ignore[index,union-attr]
        for case in row["cases"]
        if case["id"] == subtest_result["id"]
    )
    selected_window["stimulus"]["params"] = {"window": "unrelated"}
    evidence_mutations["evidence-wrong-window"] = wrong_window_evidence

    invocation_id = bound_case["invocation"]["id"]
    predicate = bound_case["predicates"][0]["assertion"]
    argument_check = predicate["argument_equals"][0]
    occurrence = bound_result["matched_predicates"][0]["occurrence"]
    for name, replacement in (
        ("evidence-wrong-cause", "'E_FAKE'"),
        ("evidence-fabricated-outcome", "'fabricated'"),
    ):
        malformed = copy.deepcopy(evidence)
        execution = next(
            item for item in malformed["invocations"] if item["id"] == invocation_id  # type: ignore[index,union-attr]
        )
        execution["outcome"]["assertions"][occurrence - 1]["arguments"][argument_check["index"]] = replacement
        _sync_execution_stdout(execution)
        evidence_mutations[name] = malformed
    unrelated_evidence = copy.deepcopy(evidence)
    unrelated_execution = next(
        item
        for item in unrelated_evidence["invocations"]  # type: ignore[index,union-attr]
        if item["id"] == invocation_id
    )
    unrelated_execution["outcome"]["assertions"][occurrence - 1]["source_line"] = 1
    _sync_execution_stdout(unrelated_execution)
    evidence_mutations["evidence-unrelated-assertion"] = unrelated_evidence

    copied = copy.deepcopy(evidence)
    source_execution = next(
        item
        for item in copied["invocations"]  # type: ignore[index,union-attr]
        if item["id"] != copied["invocations"][0]["id"]  # type: ignore[index]
    )
    copied["invocations"][0]["outcome"] = copy.deepcopy(source_execution["outcome"])  # type: ignore[index]
    _sync_execution_stdout(copied["invocations"][0])  # type: ignore[index]
    evidence_mutations["evidence-copied-outcome"] = copied

    for name, malformed in evidence_mutations.items():
        _expect_reject(
            name,
            lambda malformed=malformed: validate_executed_evidence(
                malformed,
                case_rows,
            ),
            rejected,
        )
    return rejected


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case-map", default=str(REPOSITORY_ROOT / CASE_MAP_REL))
    parser.add_argument("--evidence", required=True)
    parser.add_argument("--negative-checks", action="store_true")
    args = parser.parse_args()
    case_path = Path(args.case_map).resolve()
    if case_path != REPOSITORY_ROOT / CASE_MAP_REL:
        parser.error("case map must use the fixed reviewed path")
    evidence_path = Path(args.evidence).resolve()
    case_raw = case_path.read_bytes()
    evidence_raw = evidence_path.read_bytes()
    case_value = json.loads(case_raw)
    case_rows = validate_case_map(case_value)
    evidence = json.loads(evidence_raw)
    validate_executed_evidence(evidence, case_rows)
    rejected = _negative_checks(case_value, evidence, case_rows) if args.negative_checks else []
    print(
        json.dumps(
            {
                "schema": "axiograph-no-unsafe-normative-evidence-verification-v5",
                "evidence_schema": EVIDENCE_SCHEMA,
                "case_map_sha256": hashlib.sha256(case_raw).hexdigest(),
                "evidence_sha256": hashlib.sha256(evidence_raw).hexdigest(),
                "row_count": evidence["row_count"],
                "case_count": evidence["case_count"],
                "invocation_count": evidence["invocation_count"],
                "all_cases_executed": evidence["all_cases_executed"],
                "all_requirements_satisfied": evidence["all_requirements_satisfied"],
                "operational_scope": evidence["operational_scope"],
                "negative_checks": rejected,
                "negative_check_count": len(rejected),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
