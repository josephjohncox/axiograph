#!/usr/bin/env python3
"""Execute requirement-bound evidence for the no-unsafe regression rows.

The validator checks a finite operational evidence contract. It binds selected
unittest methods, stimuli, assertion sites, required execution levels, and
hosted receipts. It does not prove arbitrary Python semantics.
"""

from __future__ import annotations

import argparse
import ast
import contextlib
import functools
import hashlib
import inspect
import io
import json
import os
import re
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
if str(REPOSITORY_ROOT) not in sys.path:
    sys.path.insert(0, str(REPOSITORY_ROOT))

CASE_SCHEMA = "axiograph-no-unsafe-normative-cases-v5"
EVIDENCE_SCHEMA = "axiograph-no-unsafe-normative-evidence-v5"
OUTCOME_SCHEMA = "axiograph-no-unsafe-selector-outcome-v5"
CASE_MAP_REL = "scripts/no_unsafe_row_cases_v5.json"
NORMATIVE_MATRIX_REL = "build/engineering-quality/roadmap-wave-01-six-finding-correction/persist-terminal-handoff-20260909T130000Z/pins/REGRESSION_MATRIX.md"
NORMATIVE_MATRIX_SHA256 = "af9b894ee7eef8284a048327bee98cd3e0a41e62d70dcfaf9caea58ae7cad1f0"
# Filled only after the complete matrix-bound map is generated and reviewed.
EXPECTED_CASE_MAP_SHA256 = "83fded5dec7dd7e487424565f165f5eae121dbea32a7971b9b1a8f347db4442e"
OPERATIONAL_SCOPE = "Closed matrix-bound finite operational evidence; not proof of arbitrary Python semantics or transcript authenticity."
TEST_SOURCE_REL = "scripts/tests/test_no_unsafe_policy.py"
RUNNER_SOURCE_REL = "scripts/run_no_unsafe_row_evidence.py"
ALLOWED_RUNNER_LEVELS = {"CLI", "Make", "direct", "direct-real", "smoke", "static", "inspection"}
SAFE_ENVIRONMENT_NAMES = (
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "USER", "LOGNAME",
    "CARGO_HOME", "RUSTUP_HOME", "KANI_HOME", "LANG", "LC_ALL",
)
EXPECTED_LEVELS = {
    "BASE-01": "CLI", "BASE-02": "CLI+Make",
    "ROOT-01": "CLI", "ROOT-02": "CLI", "ROOT-03": "direct-real", "ROOT-04": "direct+inspection", "ROOT-05": "CLI smoke",
    "TYPE-01": "direct-real", "TYPE-02": "direct-real", "TYPE-03": "direct", "TYPE-04": "CLI", "TYPE-05": "CLI", "TYPE-06": "direct-real", "TYPE-07": "direct+inspection", "TYPE-08": "direct",
    "OWN-01": "CLI", "OWN-02": "CLI", "OWN-03": "CLI", "OWN-04": "CLI",
    "GIT-01": "CLI", "GIT-02": "CLI", "GIT-03": "CLI", "GIT-04": "CLI", "GIT-05": "CLI", "GIT-06": "CLI", "GIT-07": "direct+real", "GIT-08": "direct+CLI",
    "CARGO-01": "CLI", "CARGO-02": "direct+real", "CARGO-03": "direct-real", "CARGO-04": "CLI", "CARGO-05": "CLI smoke",
    "SOURCE-UTF8-01": "CLI", "SOURCE-UTF8-02": "CLI",
    "CHECK-01": "CLI", "CHECK-02": "CLI", "CHECK-03": "CLI", "CHECK-04": "CLI+direct-real", "CHECK-05": "direct", "CHECK-06": "direct", "CHECK-07": "direct", "CHECK-08": "direct-real", "CHECK-09": "direct-real", "CHECK-10": "direct+CLI", "CHECK-11": "direct", "CHECK-12": "static+CLI", "CHECK-13": "CLI smoke",
    "REGEN-00": "CLI+direct-real", "REGEN-01": "CLI", "REGEN-02": "CLI", "REGEN-03": "CLI", "REGEN-04": "CLI", "REGEN-05": "direct-real", "REGEN-06": "direct-real", "REGEN-07": "direct-real", "REGEN-08": "direct-real", "REGEN-09": "direct", "REGEN-10": "CLI", "REGEN-11": "CLI smoke", "REGEN-12": "CLI", "REGEN-13": "CLI", "REGEN-14": "direct+CLI",
    "ARCHIVE-01": "CLI", "ARCHIVE-02": "direct+real", "ARCHIVE-03": "direct", "ARCHIVE-04": "direct", "ARCHIVE-05": "direct", "ARCHIVE-06": "direct-real", "ARCHIVE-07": "direct", "ARCHIVE-08": "direct", "ARCHIVE-09": "direct", "ARCHIVE-10": "direct", "ARCHIVE-11": "direct", "ARCHIVE-12": "CLI smoke",
    "IO-01": "direct", "IO-02": "direct", "IO-03": "direct", "IO-04": "direct-real", "IO-05": "direct", "IO-06": "CLI", "IO-10": "direct-real",
}
REQUIRED_ATOMIC_LEVELS = {
    "CLI": {"CLI"}, "CLI+Make": {"CLI", "Make"}, "direct-real": {"direct-real"},
    "direct+real": {"direct", "direct-real"}, "direct": {"direct"}, "CLI smoke": {"smoke"},
    "direct+inspection": {"direct", "inspection"}, "direct+CLI": {"direct", "CLI"},
    "CLI+direct-real": {"CLI", "direct-real"}, "static+CLI": {"static", "CLI"},
}
HOSTED_PROOF_EXPECTATIONS = {
    "hosted-run-34669241286-attempt-1": {
        "source": "build/engineering-quality/hosted-capability-ci-import-fix/persist-success-20260912T031727Z/immutable-handoff/inputs/history/observe-hosted-0-complete/files/FINAL_RECEIPT.json.txt",
        "sha256": "9b8fa7fc7524c759183e9c06b038e06652fb346b97121d240d775f40a4fa9c3e",
        "run_id": 34669241286,
        "attempt": 1,
        "commit": "3824a36c266b8819b79fea391eeb4dc17f70e49e",
        "jobs": {"amd64": 103487307180, "arm64": 103487307334},
    }
}


class CaseMapFailure(ValueError):
    """A closed requirement map or evidence package failed validation."""


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


@functools.cache
def _normative_rows() -> dict[str, dict[str, str]]:
    path = REPOSITORY_ROOT / NORMATIVE_MATRIX_REL
    raw = path.read_bytes()
    if _sha256(raw) != NORMATIVE_MATRIX_SHA256:
        raise CaseMapFailure("normative matrix source hash mismatch")
    rows: dict[str, dict[str, str]] = {}
    for line in raw.decode("utf-8").splitlines():
        match = re.fullmatch(r"\| ([A-Z0-9-]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \|", line)
        if match is None or match.group(1) in {"ID", "---"}:
            continue
        row_id, level, stimulus, evidence = (part.strip() for part in match.groups())
        if row_id in rows:
            raise CaseMapFailure("duplicate normative matrix row")
        rows[row_id] = {
            "id": row_id,
            "required_level": level,
            "stimulus": stimulus,
            "required_evidence": evidence,
        }
    if set(rows) != set(EXPECTED_LEVELS):
        raise CaseMapFailure("normative matrix row set mismatch")
    for row_id, expected_level in EXPECTED_LEVELS.items():
        if rows[row_id]["required_level"] != expected_level:
            raise CaseMapFailure("normative matrix level mismatch")
    return rows


def _canonical_case_map(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def _safe_repr(value: object) -> str:
    rendered = repr(value)
    return rendered if len(rendered) <= 512 else rendered[:509] + "..."


_CAUSE_TOKEN = re.compile(r"(?<![A-Z0-9_])(E_[A-Z0-9_]+)(?![A-Z0-9_*])")
_NORMATIVE_CAUSE_TOKEN = re.compile(
    r"(?<![A-Z0-9_])(E_[A-Z0-9_]+(?:\*)?)(?![A-Z0-9_*])"
)


def _argument_causes(checks: list[dict[str, object]]) -> set[str]:
    return {
        cause
        for check in checks
        for cause in _CAUSE_TOKEN.findall(str(check["value"]))
    }


def _normative_cause_patterns(row: dict[str, object]) -> set[str]:
    return set(
        _NORMATIVE_CAUSE_TOKEN.findall(
            f"{row['stimulus']} {row['required_evidence']}"
        )
    )


def _cause_matches_pattern(cause: str, pattern: str) -> bool:
    return cause.startswith(pattern[:-1]) if pattern.endswith("*") else cause == pattern


def _subtest_params(test: unittest.case.TestCase) -> dict[str, str]:
    subtest = getattr(test, "_subtest", None)
    if subtest is None:
        return {}
    return {str(key): _safe_repr(value) for key, value in subtest.params.items()}  # type: ignore[attr-defined]


def _one_test(selector: str) -> unittest.case.TestCase:
    suite = unittest.defaultTestLoader.loadTestsFromName(selector)
    pending: list[unittest.TestSuite | unittest.case.TestCase] = [suite]
    tests: list[unittest.case.TestCase] = []
    while pending:
        item = pending.pop()
        if isinstance(item, unittest.TestSuite):
            pending.extend(item)
        else:
            tests.append(item)
    if len(tests) != 1 or tests[0].__class__.__name__ == "_FailedTest":
        raise CaseMapFailure("unknown selected method")
    return tests[0]


@functools.cache
def _method_binding(selector: str) -> dict[str, object]:
    test = _one_test(selector)
    method_name = test._testMethodName  # type: ignore[attr-defined]
    method = inspect.unwrap(getattr(test.__class__, method_name))
    source_lines, first_line = inspect.getsourcelines(method)
    source_file = Path(inspect.getsourcefile(method) or "").resolve()
    try:
        source_relative = source_file.relative_to(REPOSITORY_ROOT).as_posix()
    except ValueError as error:
        raise CaseMapFailure("selected method is outside repository") from error
    source = "".join(source_lines).encode("utf-8")
    return {
        "selector": selector,
        "method": method_name,
        "source_file": source_relative,
        "source_first_line": first_line,
        "source_lines": len(source_lines),
        "source_sha256": _sha256(source),
    }


def _invocation_id(
    binding: dict[str, object],
    level: str,
    official_archive: bool,
    purpose: str,
) -> str:
    canonical = json.dumps(
        {
            "binding": binding,
            "level": level,
            "official_archive": official_archive,
            "purpose": purpose,
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return "invocation-" + _sha256(canonical)[:24]


def _validate_hosted_proof_definition(proof: object) -> dict[str, Any]:
    if not isinstance(proof, dict) or set(proof) != {
        "id", "source", "sha256", "run_id", "attempt", "commit", "jobs",
    }:
        raise CaseMapFailure("hosted proof shape")
    proof_id = proof["id"]
    expected = HOSTED_PROOF_EXPECTATIONS.get(proof_id)
    if expected is None or any(proof[key] != expected[key] for key in expected):
        raise CaseMapFailure("unknown or altered hosted proof")
    return proof


def _load_hosted_proof(proof: dict[str, Any]) -> dict[str, Any]:
    path = REPOSITORY_ROOT / proof["source"]
    raw = path.read_bytes()
    if _sha256(raw) != proof["sha256"]:
        raise CaseMapFailure("hosted proof source hash mismatch")
    receipt = json.loads(raw)
    if not (
        receipt.get("status") == "pass"
        and receipt.get("findings") == []
        and receipt.get("run", {}).get("id") == proof["run_id"]
        and receipt.get("run", {}).get("attempt") == proof["attempt"]
        and receipt.get("run", {}).get("head_sha") == proof["commit"]
        and receipt.get("jobs", {}).get("amd64", {}).get("id") == proof["jobs"]["amd64"]
        and receipt.get("jobs", {}).get("arm64", {}).get("id") == proof["jobs"]["arm64"]
        and "sole exact leaf E_FS_NONREGULAR" in receipt["jobs"]["amd64"]["scanner"]
        and "sole exact leaf E_FS_NONREGULAR" in receipt["jobs"]["arm64"]["scanner"]
        and "sole exact leaf E_REGEN_CONFINEMENT and Errno 18" in receipt["jobs"]["amd64"]["generator"]
        and "sole exact leaf E_REGEN_CONFINEMENT and Errno 18" in receipt["jobs"]["arm64"]["generator"]
        and "unmounted-recorded" in receipt["jobs"]["amd64"]["cleanup"]
        and "removed-recorded" in receipt["jobs"]["amd64"]["cleanup"]
        and "unmounted-recorded" in receipt["jobs"]["arm64"]["cleanup"]
        and "removed-recorded" in receipt["jobs"]["arm64"]["cleanup"]
    ):
        raise CaseMapFailure("hosted receipt identity mismatch")
    return {
        "id": proof["id"],
        "source": proof["source"],
        "source_sha256": proof["sha256"],
        "run_id": proof["run_id"],
        "attempt": proof["attempt"],
        "commit": proof["commit"],
        "jobs": proof["jobs"],
        "receipt_status": receipt["status"],
        "receipt_findings": receipt["findings"],
        "amd64": receipt["jobs"]["amd64"],
        "arm64": receipt["jobs"]["arm64"],
        "local_privileged_execution": False,
    }


def _validate_stimulus(stimulus: object) -> None:
    if not isinstance(stimulus, dict) or set(stimulus) != {"kind", "params"}:
        raise CaseMapFailure("case stimulus shape")
    if stimulus["kind"] not in {"test", "subtest"}:
        raise CaseMapFailure("case stimulus value")
    if not isinstance(stimulus["params"], dict) or not all(isinstance(key, str) and isinstance(value, str) for key, value in stimulus["params"].items()):
        raise CaseMapFailure("case stimulus params")
    if stimulus["kind"] == "test" and stimulus["params"]:
        raise CaseMapFailure("test stimulus cannot have params")


@functools.cache
def _source_tree(source_file: str, source_sha256: str) -> ast.AST | None:
    source_raw = (REPOSITORY_ROOT / source_file).read_bytes()
    if source_sha256 != _sha256(source_raw):
        return None
    return ast.parse(source_raw)


@functools.cache
def _assertion_site_is_valid(
    method: str,
    test_function: str,
    source_file: str,
    source_sha256: str,
    source_line: int,
) -> bool:
    tree = _source_tree(source_file, source_sha256)
    if tree is None:
        return False
    containing_functions = [
        node
        for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and node.name == test_function
        and node.lineno <= source_line <= (node.end_lineno or node.lineno)
    ]
    assertion_calls = [
        node
        for node in ast.walk(tree)
        if isinstance(node, ast.Call)
        and node.lineno == source_line
        and isinstance(node.func, ast.Attribute)
        and node.func.attr == method
    ]
    return len(containing_functions) == 1 and bool(assertion_calls)


def _validate_assertion_spec(value: object) -> None:
    if not isinstance(value, dict) or set(value) != {
        "method", "test_function", "source_file", "source_sha256", "source_line",
        "subtest_params", "site_occurrence", "argument_equals", "normative_cause",
    }:
        raise CaseMapFailure("assertion predicate shape")
    if (
        value["method"] not in ASSERTION_METHODS
        or not isinstance(value["test_function"], str)
        or not value["test_function"]
        or value["source_file"] != TEST_SOURCE_REL
        or not isinstance(value["source_sha256"], str)
        or len(value["source_sha256"]) != 64
        or not isinstance(value["source_line"], int)
        or value["source_line"] <= 0
        or not isinstance(value["site_occurrence"], int)
        or isinstance(value["site_occurrence"], bool)
        or value["site_occurrence"] <= 0
    ):
        raise CaseMapFailure("assertion predicate method, source, or occurrence")
    if not _assertion_site_is_valid(
        value["method"],
        value["test_function"],
        value["source_file"],
        value["source_sha256"],
        value["source_line"],
    ):
        raise CaseMapFailure("assertion predicate is not the named source assertion site")
    if not isinstance(value["subtest_params"], dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in value["subtest_params"].items()):
        raise CaseMapFailure("assertion predicate subtest")
    checks = value["argument_equals"]
    if not isinstance(checks, list) or not checks:
        raise CaseMapFailure("assertion predicate arguments")
    positions: set[int] = set()
    for check in checks:
        if (
            not isinstance(check, dict)
            or set(check) != {"index", "value"}
            or not isinstance(check["index"], int)
            or isinstance(check["index"], bool)
            or check["index"] < 0
            or check["index"] in positions
            or not isinstance(check["value"], str)
            or not check["value"]
        ):
            raise CaseMapFailure("assertion predicate exact argument")
        positions.add(check["index"])
    cause = value["normative_cause"]
    if cause is not None and (
        not isinstance(cause, str)
        or _CAUSE_TOKEN.fullmatch(cause) is None
        or cause not in _argument_causes(checks)
    ):
        raise CaseMapFailure("assertion predicate source cause binding")


def _validate_normative_cause(
    row: dict[str, object],
    assertion: dict[str, object],
) -> None:
    cause = assertion["normative_cause"]
    if cause is None:
        return
    patterns = _normative_cause_patterns(row)
    if not any(_cause_matches_pattern(cause, pattern) for pattern in patterns):
        raise CaseMapFailure("assertion predicate normative cause binding")


def validate_case_map(
    value: object,
    *,
    enforce_identity: bool = True,
) -> list[dict[str, Any]]:
    if not isinstance(value, dict) or set(value) != {
        "schema", "operational_scope", "normative_matrix", "hosted_proofs", "rows",
    }:
        raise CaseMapFailure("top-level shape")
    if value["schema"] != CASE_SCHEMA or value["operational_scope"] != OPERATIONAL_SCOPE:
        raise CaseMapFailure("schema or operational scope")
    canonical = _canonical_case_map(value)
    if enforce_identity and (
        EXPECTED_CASE_MAP_SHA256 == "PENDING"
        or _sha256(canonical) != EXPECTED_CASE_MAP_SHA256
    ):
        raise CaseMapFailure("case map is not the fixed reviewed normative contract")
    matrix = value["normative_matrix"]
    if matrix != {
        "path": NORMATIVE_MATRIX_REL,
        "sha256": NORMATIVE_MATRIX_SHA256,
    }:
        raise CaseMapFailure("normative matrix identity")
    normative = _normative_rows()
    if not isinstance(value["hosted_proofs"], list):
        raise CaseMapFailure("hosted proofs array")
    proofs: dict[str, dict[str, Any]] = {}
    for proof_value in value["hosted_proofs"]:
        proof = _validate_hosted_proof_definition(proof_value)
        if proof["id"] in proofs:
            raise CaseMapFailure("duplicate hosted proof")
        proofs[proof["id"]] = proof
    if set(proofs) != set(HOSTED_PROOF_EXPECTATIONS):
        raise CaseMapFailure("missing hosted proof")
    if not isinstance(value["rows"], list):
        raise CaseMapFailure("rows array")
    rows: dict[str, dict[str, Any]] = {}
    case_ids: set[str] = set()
    predicate_ids: set[str] = set()
    claimed_observations: set[tuple[object, ...]] = set()
    for row in value["rows"]:
        if not isinstance(row, dict) or set(row) != {
            "id", "required_level", "stimulus", "required_evidence", "cases",
        }:
            raise CaseMapFailure("row shape")
        row_id = row["id"]
        if not isinstance(row_id, str) or row_id in rows or row_id not in normative:
            raise CaseMapFailure("unknown or duplicate row")
        if any(row[key] != normative[row_id][key] for key in (
            "id", "required_level", "stimulus", "required_evidence",
        )):
            raise CaseMapFailure("row differs from pinned normative matrix")
        if not isinstance(row["cases"], list) or not row["cases"]:
            raise CaseMapFailure("unexecuted row")
        levels: set[str] = set()
        hosted_count = 0
        for case in row["cases"]:
            if not isinstance(case, dict) or set(case) != {
                "id", "kind", "level", "stimulus", "invocation", "hosted", "predicates",
            }:
                raise CaseMapFailure("case shape")
            case_id = case["id"]
            if not isinstance(case_id, str) or not case_id.startswith(row_id.lower() + "-") or case_id in case_ids:
                raise CaseMapFailure("case id")
            case_ids.add(case_id)
            if case["level"] not in ALLOWED_RUNNER_LEVELS:
                raise CaseMapFailure("case level")
            levels.add(case["level"])
            _validate_stimulus(case["stimulus"])
            if case["kind"] == "hosted":
                hosted_count += 1
                if case["invocation"] is not None or case["hosted"] not in proofs or case["predicates"]:
                    raise CaseMapFailure("hosted case authority")
                continue
            if case["kind"] != "execution" or case["hosted"] is not None:
                raise CaseMapFailure("case kind")
            invocation = case["invocation"]
            if not isinstance(invocation, dict) or set(invocation) != {
                "id", "selector", "level", "official_archive", "purpose", "method",
            }:
                raise CaseMapFailure("invocation shape")
            if (
                invocation["level"] != case["level"]
                or not isinstance(invocation["official_archive"], bool)
                or invocation["purpose"] != row_id
            ):
                raise CaseMapFailure("invocation level or purpose")
            binding = _method_binding(invocation["selector"])
            if invocation["method"] != binding or invocation["id"] != _invocation_id(
                binding,
                invocation["level"],
                invocation["official_archive"],
                invocation["purpose"],
            ):
                raise CaseMapFailure("wrong selected method or source hash")
            if not isinstance(case["predicates"], list) or not case["predicates"]:
                raise CaseMapFailure("missing requirement predicates")
            for predicate in case["predicates"]:
                if not isinstance(predicate, dict) or set(predicate) != {"id", "assertion"}:
                    raise CaseMapFailure("predicate shape")
                predicate_id = predicate["id"]
                if not isinstance(predicate_id, str) or not predicate_id.startswith(case_id + "-") or predicate_id in predicate_ids:
                    raise CaseMapFailure("predicate id")
                predicate_ids.add(predicate_id)
                _validate_assertion_spec(predicate["assertion"])
                assertion = predicate["assertion"]
                _validate_normative_cause(row, assertion)
                observation_identity = (
                    invocation["id"],
                    assertion["method"],
                    assertion["test_function"],
                    assertion["source_file"],
                    assertion["source_sha256"],
                    assertion["source_line"],
                    json.dumps(assertion["subtest_params"], sort_keys=True),
                    assertion["site_occurrence"],
                )
                if observation_identity in claimed_observations:
                    raise CaseMapFailure("assertion observation occurrence is reused")
                claimed_observations.add(observation_identity)
                expected_params = case["stimulus"]["params"] if case["stimulus"]["kind"] == "subtest" else {}
                if predicate["assertion"]["subtest_params"] != expected_params:
                    raise CaseMapFailure("assertion is not bound to the exact case stimulus")
        execution_cases = [case for case in row["cases"] if case["kind"] == "execution"]
        if row_id == "TYPE-05":
            local_kinds = {
                case["stimulus"]["params"].get("kind")
                for case in execution_cases
                if case["invocation"]["selector"].endswith(
                    "test_candidate_fifo_and_socket_are_nonregular_without_data_open"
                )
                and case["level"] == "CLI"
            }
            if hosted_count != 1 or local_kinds != {"'fifo'", "'socket'"}:
                raise CaseMapFailure("TYPE-05 requires local FIFO/socket CLI cases and hosted device proof")
        elif row_id == "CHECK-03":
            local_parents = {
                case["stimulus"]["params"].get("check_parent")
                for case in execution_cases
                if case["invocation"]["selector"].endswith(
                    "test_generator_cli_confinement_and_identity_failures"
                )
                and case["level"] == "CLI"
            }
            if hosted_count != 1 or local_parents != {"'link'", "'non_directory'", "'escape'"}:
                raise CaseMapFailure("CHECK-03 requires local link/non-directory/escape CLI cases and hosted mount proof")
        elif hosted_count:
            raise CaseMapFailure("unexpected hosted row")
        if not REQUIRED_ATOMIC_LEVELS[row["required_level"]].issubset(levels):
            raise CaseMapFailure("wrong-level evidence")
        rows[row_id] = row
    if set(rows) != set(normative):
        raise CaseMapFailure("absent normative row")
    return [rows[row_id] for row_id in EXPECTED_LEVELS]


class VisibleResult(unittest.TestResult):
    def __init__(self) -> None:
        super().__init__()
        self.outcomes: list[dict[str, Any]] = []

    @staticmethod
    def _name(test: unittest.case.TestCase) -> str:
        return test.id()

    def addSuccess(self, test: unittest.case.TestCase) -> None:
        super().addSuccess(test)
        self.outcomes.append({"test": self._name(test), "kind": "test", "status": "pass"})

    def addSkip(self, test: unittest.case.TestCase, reason: str) -> None:
        super().addSkip(test, reason)
        self.outcomes.append({"test": self._name(test), "kind": "test", "status": "skip", "reason": reason})

    def addFailure(self, test: unittest.case.TestCase, err: tuple[type[BaseException], BaseException, object]) -> None:
        super().addFailure(test, err)
        self.outcomes.append({"test": self._name(test), "kind": "test", "status": "fail", "detail": self._exc_info_to_string(err, test)})

    def addError(self, test: unittest.case.TestCase, err: tuple[type[BaseException], BaseException, object]) -> None:
        super().addError(test, err)
        self.outcomes.append({"test": self._name(test), "kind": "test", "status": "error", "detail": self._exc_info_to_string(err, test)})

    def addSubTest(self, test: unittest.case.TestCase, subtest: unittest.case.TestCase, err: tuple[type[BaseException], BaseException, object] | None) -> None:
        super().addSubTest(test, subtest, err)
        outcome = {
            "test": self._name(test),
            "kind": "subtest",
            "params": {str(key): _safe_repr(value) for key, value in subtest.params.items()},  # type: ignore[attr-defined]
            "status": "pass" if err is None else "fail",
        }
        if err is not None:
            outcome["detail"] = self._exc_info_to_string(err, test)
        self.outcomes.append(outcome)


ASSERTION_METHODS = (
    "assertEqual", "assertNotEqual", "assertTrue", "assertFalse", "assertIs",
    "assertIsNot", "assertIsNone", "assertIsNotNone", "assertIn", "assertNotIn",
    "assertGreater", "assertGreaterEqual", "assertLess", "assertLessEqual",
    "assertRegex", "assertNotRegex", "assertRaises", "assertRaisesRegex",
)


def run_child(selector: str, invocation_id: str) -> int:
    binding = _method_binding(selector)
    suite = unittest.defaultTestLoader.loadTestsFromName(selector)
    result = VisibleResult()
    stdout = io.StringIO()
    stderr = io.StringIO()
    assertions: list[dict[str, Any]] = []
    site_occurrences: dict[tuple[str, str, int, tuple[tuple[str, str], ...]], int] = {}
    originals: dict[str, Any] = {}
    method_name = str(binding["method"])
    test_source_sha256 = _sha256((REPOSITORY_ROOT / TEST_SOURCE_REL).read_bytes())
    for name in ASSERTION_METHODS:
        original = getattr(unittest.TestCase, name)
        originals[name] = original

        def observed(self: unittest.TestCase, *args: object, _name: str = name, _original: Any = original, **kwargs: object) -> Any:
            caller = sys._getframe(1)
            subtest_params = _subtest_params(self)
            site_key = (
                _name,
                caller.f_code.co_name,
                caller.f_lineno,
                tuple(sorted(subtest_params.items())),
            )
            site_occurrences[site_key] = site_occurrences.get(site_key, 0) + 1
            event = {
                "occurrence": len(assertions) + 1,
                "site_occurrence": site_occurrences[site_key],
                "method": _name,
                "test_function": caller.f_code.co_name,
                "source_file": str(Path(caller.f_code.co_filename).resolve().relative_to(REPOSITORY_ROOT)),
                "source_sha256": test_source_sha256,
                "source_line": caller.f_lineno,
                "arguments": [_safe_repr(argument) for argument in args],
                "keywords": {key: _safe_repr(value) for key, value in sorted(kwargs.items())},
                "subtest_params": subtest_params,
            }
            try:
                returned = _original(self, *args, **kwargs)
            except BaseException:
                event["passed"] = False
                assertions.append(event)
                raise
            event["passed"] = True
            assertions.append(event)
            return returned

        setattr(unittest.TestCase, name, observed)
    try:
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            suite.run(result)
    finally:
        for name, original in originals.items():
            setattr(unittest.TestCase, name, original)
    payload = {
        "schema": OUTCOME_SCHEMA,
        "invocation_id": invocation_id,
        "selector": selector,
        "method": binding,
        "tests_run": result.testsRun,
        "successful": result.wasSuccessful(),
        "failures": len(result.failures),
        "errors": len(result.errors),
        "skips": len(result.skipped),
        "outcomes": result.outcomes,
        "assertions": assertions,
        "captured_stdout": stdout.getvalue(),
        "captured_stderr": stderr.getvalue(),
    }
    print(json.dumps(payload, sort_keys=True))
    return 0 if result.wasSuccessful() and not result.skipped and result.testsRun > 0 and assertions and method_name else 1


def _assertion_matches(spec: dict[str, Any], observed: object) -> bool:
    if not isinstance(observed, dict) or set(observed) != {
        "occurrence", "site_occurrence", "method", "test_function", "source_file",
        "source_sha256", "source_line", "arguments", "keywords", "subtest_params", "passed",
    }:
        return False
    if observed["passed"] is not True:
        return False
    if any(
        observed[key] != spec[key]
        for key in (
            "site_occurrence", "method", "test_function", "source_file", "source_sha256",
            "source_line", "subtest_params",
        )
    ):
        return False
    if not isinstance(observed["arguments"], list) or not isinstance(observed["keywords"], dict):
        return False
    arguments = observed["arguments"]
    return all(
        check["index"] < len(arguments)
        and arguments[check["index"]] == check["value"]
        for check in spec["argument_equals"]
    )


def _validate_outcome_assertions(outcome: dict[str, Any]) -> None:
    expected_shape = {
        "schema", "invocation_id", "selector", "method", "tests_run", "successful",
        "failures", "errors", "skips", "outcomes", "assertions", "captured_stdout",
        "captured_stderr",
    }
    if set(outcome) != expected_shape or not isinstance(outcome["assertions"], list):
        raise CaseMapFailure("execution outcome shape")
    if not isinstance(outcome["outcomes"], list):
        raise CaseMapFailure("execution test outcomes shape")
    for index, observed in enumerate(outcome["assertions"], 1):
        if not isinstance(observed, dict) or set(observed) != {
            "occurrence", "site_occurrence", "method", "test_function", "source_file",
            "source_sha256", "source_line", "arguments", "keywords", "subtest_params", "passed",
        }:
            raise CaseMapFailure("assertion observation shape")
        if observed["occurrence"] != index:
            raise CaseMapFailure("assertion observation occurrence sequence")
        if (
            observed["method"] not in ASSERTION_METHODS
            or observed["source_file"] != TEST_SOURCE_REL
            or not isinstance(observed["arguments"], list)
            or not all(isinstance(argument, str) for argument in observed["arguments"])
            or not isinstance(observed["keywords"], dict)
            or not all(
                isinstance(key, str) and isinstance(value, str)
                for key, value in observed["keywords"].items()
            )
            or not isinstance(observed["subtest_params"], dict)
            or not all(
                isinstance(key, str) and isinstance(value, str)
                for key, value in observed["subtest_params"].items()
            )
            or not isinstance(observed["passed"], bool)
            or not _assertion_site_is_valid(
                observed["method"],
                observed["test_function"],
                observed["source_file"],
                observed["source_sha256"],
                observed["source_line"],
            )
        ):
            raise CaseMapFailure("assertion observation source binding")


def _match_case_predicates(
    case: dict[str, Any],
    outcome: dict[str, Any],
    invocation_id: str,
    consumed: set[tuple[str, int]],
    *,
    context: str,
) -> list[dict[str, object]]:
    matched: list[dict[str, object]] = []
    for predicate in case["predicates"]:
        matches = [
            item
            for item in outcome["assertions"]
            if _assertion_matches(predicate["assertion"], item)
        ]
        if len(matches) != 1:
            raise CaseMapFailure(
                "missing, ambiguous, or unrelated requirement assertion: "
                f"{context} predicate={predicate['id']} matches={len(matches)} "
                f"invocation={invocation_id}"
            )
        occurrence = matches[0]["occurrence"]
        identity = (invocation_id, occurrence)
        if identity in consumed:
            raise CaseMapFailure(
                "assertion observation occurrence reused: "
                f"{context} predicate={predicate['id']} occurrence={occurrence}"
            )
        consumed.add(identity)
        matched.append({
            "id": predicate["id"],
            "occurrence": occurrence,
            "site_occurrence": matches[0]["site_occurrence"],
        })
    return matched


def _stimulus_observed(stimulus: dict[str, Any], outcome: dict[str, Any]) -> bool:
    if stimulus["kind"] == "test":
        return any(item.get("kind") == "test" and item.get("status") == "pass" for item in outcome["outcomes"])
    return any(
        item.get("kind") == "subtest"
        and item.get("status") == "pass"
        and item.get("params") == stimulus["params"]
        for item in outcome["outcomes"]
    )


def _expected_argv(selector: str, invocation_id: str) -> list[str]:
    return [
        sys.executable,
        str(Path(__file__).resolve()),
        "--child-selector",
        selector,
        "--invocation-id",
        invocation_id,
    ]


def _validate_invocation(execution: object, expected: dict[str, Any], *, require_satisfied: bool) -> dict[str, Any]:
    if not isinstance(execution, dict) or set(execution) != {
        "id", "selector", "level", "official_archive", "purpose", "method", "argv",
        "cwd", "allowlisted_environment", "exit", "stdout", "stderr", "outcome",
    }:
        raise CaseMapFailure("execution shape")
    for key in ("id", "selector", "level", "official_archive", "purpose", "method"):
        if execution[key] != expected[key]:
            raise CaseMapFailure("wrong method, source hash, level, purpose, or invocation")
    if execution["argv"] != _expected_argv(expected["selector"], expected["id"]) or execution["cwd"] != str(REPOSITORY_ROOT):
        raise CaseMapFailure("wrong actual invocation")
    environment = execution["allowlisted_environment"]
    if not isinstance(environment, dict) or set(environment) - (set(SAFE_ENVIRONMENT_NAMES) | {"AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE"}):
        raise CaseMapFailure("execution environment")
    if expected["official_archive"]:
        if environment.get("AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE") != "1":
            raise CaseMapFailure("official archive environment missing")
    elif "AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE" in environment:
        raise CaseMapFailure("unexpected official archive environment")
    outcome = execution["outcome"]
    if not isinstance(outcome, dict) or outcome.get("schema") != OUTCOME_SCHEMA:
        raise CaseMapFailure("execution outcome shape")
    if outcome.get("invocation_id") != expected["id"] or outcome.get("selector") != expected["selector"] or outcome.get("method") != expected["method"]:
        raise CaseMapFailure("execution outcome binding")
    _validate_outcome_assertions(outcome)
    if execution["stdout"] != json.dumps(outcome, sort_keys=True) + "\n":
        raise CaseMapFailure("execution stdout is not the recorded outcome")
    if require_satisfied and not (
        execution["exit"] == 0
        and outcome.get("successful") is True
        and outcome.get("tests_run", 0) == 1
        and outcome.get("skips") == 0
        and outcome.get("failures") == 0
        and outcome.get("errors") == 0
        and outcome.get("assertions")
    ):
        raise CaseMapFailure("invocation did not pass")
    return outcome


def validate_executed_evidence(value: object, case_rows: list[dict[str, Any]], *, require_satisfied: bool = True) -> None:
    if not isinstance(value, dict) or set(value) != {
        "schema", "operational_scope", "case_map", "verifier", "cwd", "row_count",
        "case_count", "invocation_count", "all_cases_executed", "all_requirements_satisfied",
        "rows", "invocations", "hosted_proofs",
    }:
        raise CaseMapFailure("evidence top-level shape")
    if value["schema"] != EVIDENCE_SCHEMA or value["operational_scope"] != OPERATIONAL_SCOPE:
        raise CaseMapFailure("evidence schema")
    if value["cwd"] != str(REPOSITORY_ROOT) or not isinstance(value["case_map"], dict) or not isinstance(value["verifier"], dict):
        raise CaseMapFailure("evidence source binding")
    for source, relative in ((value["case_map"], CASE_MAP_REL), (value["verifier"], RUNNER_SOURCE_REL)):
        if set(source) != {"path", "bytes", "sha256"} or source["path"] != relative:
            raise CaseMapFailure("evidence source identity shape")
        raw = (REPOSITORY_ROOT / relative).read_bytes()
        if source["bytes"] != len(raw) or source["sha256"] != _sha256(raw):
            raise CaseMapFailure("evidence source identity mismatch")
    expected_invocations: dict[str, dict[str, Any]] = {}
    expected_cases: dict[str, tuple[str, dict[str, Any]]] = {}
    for row in case_rows:
        for case in row["cases"]:
            expected_cases[case["id"]] = (row["id"], case)
            if case["kind"] == "execution":
                invocation = case["invocation"]
                prior = expected_invocations.setdefault(invocation["id"], invocation)
                if prior != invocation:
                    raise CaseMapFailure("invocation id collision")
    if not isinstance(value["invocations"], list) or not isinstance(value["hosted_proofs"], list) or not isinstance(value["rows"], list):
        raise CaseMapFailure("evidence arrays")
    execution_by_id: dict[str, dict[str, Any]] = {}
    for execution in value["invocations"]:
        if not isinstance(execution, dict) or execution.get("id") not in expected_invocations or execution["id"] in execution_by_id:
            raise CaseMapFailure("missing, unknown, or duplicate invocation")
        _validate_invocation(execution, expected_invocations[execution["id"]], require_satisfied=require_satisfied)
        execution_by_id[execution["id"]] = execution
    if set(execution_by_id) != set(expected_invocations):
        raise CaseMapFailure("missing or unexecuted invocation")
    hosted_by_id = {}
    for hosted in value["hosted_proofs"]:
        if not isinstance(hosted, dict) or hosted.get("id") not in HOSTED_PROOF_EXPECTATIONS or hosted["id"] in hosted_by_id:
            raise CaseMapFailure("unknown hosted proof evidence")
        expected = _load_hosted_proof({"id": hosted["id"], **HOSTED_PROOF_EXPECTATIONS[hosted["id"]]})
        if hosted != expected:
            raise CaseMapFailure("hosted proof evidence mismatch")
        hosted_by_id[hosted["id"]] = hosted
    if set(hosted_by_id) != set(HOSTED_PROOF_EXPECTATIONS):
        raise CaseMapFailure("missing hosted proof evidence")

    observed_rows: dict[str, dict[str, Any]] = {}
    observed_case_ids: set[str] = set()
    consumed_observations: set[tuple[str, int]] = set()
    for row_result in value["rows"]:
        if not isinstance(row_result, dict) or set(row_result) != {
            "id", "required_level", "stimulus", "required_evidence", "status", "cases",
        }:
            raise CaseMapFailure("evidence row shape")
        row_id = row_result["id"]
        expected_row = next((row for row in case_rows if row["id"] == row_id), None)
        if expected_row is None or row_id in observed_rows:
            raise CaseMapFailure("unknown or duplicate evidence row")
        for key in ("required_level", "stimulus", "required_evidence"):
            if row_result[key] != expected_row[key]:
                raise CaseMapFailure("evidence row contract mismatch")
        if not isinstance(row_result["cases"], list) or not row_result["cases"]:
            raise CaseMapFailure("missing evidence cases")
        row_satisfied = True
        levels: set[str] = set()
        for result in row_result["cases"]:
            if not isinstance(result, dict) or set(result) != {
                "id", "kind", "level", "invocation_id", "hosted_proof_id", "stimulus",
                "matched_predicates", "executed", "satisfied",
            }:
                raise CaseMapFailure("case result shape")
            case_id = result["id"]
            if case_id in observed_case_ids or case_id not in expected_cases or expected_cases[case_id][0] != row_id:
                raise CaseMapFailure("missing, unknown, or duplicate case")
            observed_case_ids.add(case_id)
            case = expected_cases[case_id][1]
            if any(result[key] != case[key] for key in ("id", "kind", "level", "stimulus")):
                raise CaseMapFailure("wrong case stimulus or level")
            levels.add(result["level"])
            if case["kind"] == "hosted":
                satisfied = case["hosted"] in hosted_by_id
                if result["invocation_id"] is not None or result["hosted_proof_id"] != case["hosted"] or result["matched_predicates"] != []:
                    raise CaseMapFailure("hosted case result binding")
            else:
                invocation = case["invocation"]
                if result["invocation_id"] != invocation["id"] or result["hosted_proof_id"] is not None:
                    raise CaseMapFailure("case invocation reference")
                outcome = execution_by_id[invocation["id"]]["outcome"]
                stimulus_seen = _stimulus_observed(case["stimulus"], outcome)
                matched = _match_case_predicates(
                    case,
                    outcome,
                    invocation["id"],
                    consumed_observations,
                    context=f"row={row_id} case={case_id}",
                )
                if result["matched_predicates"] != matched:
                    raise CaseMapFailure("case predicate record differs from actual execution")
                satisfied = stimulus_seen and bool(matched) and execution_by_id[invocation["id"]]["exit"] == 0
            if result["executed"] is not True or result["satisfied"] is not satisfied:
                raise CaseMapFailure("unexecuted or false case claim")
            row_satisfied = row_satisfied and satisfied
        if {result["id"] for result in row_result["cases"]} != {case["id"] for case in expected_row["cases"]}:
            raise CaseMapFailure("missing row case")
        row_satisfied = row_satisfied and REQUIRED_ATOMIC_LEVELS[row_result["required_level"]].issubset(levels)
        expected_status = "satisfied" if row_satisfied else "failed"
        if row_result["status"] != expected_status or (require_satisfied and not row_satisfied):
            raise CaseMapFailure("requirement status mismatch")
        observed_rows[row_id] = row_result
    if set(observed_rows) != set(EXPECTED_LEVELS) or observed_case_ids != set(expected_cases):
        raise CaseMapFailure("missing evidence row or case")
    if value["row_count"] != len(case_rows) or value["case_count"] != len(expected_cases) or value["invocation_count"] != len(expected_invocations):
        raise CaseMapFailure("evidence count summary mismatch")
    expected_all_executed = all(case["executed"] for row in value["rows"] for case in row["cases"])
    expected_all_satisfied = all(row["status"] == "satisfied" for row in value["rows"])
    if value["all_cases_executed"] is not expected_all_executed or value["all_requirements_satisfied"] is not expected_all_satisfied:
        raise CaseMapFailure("evidence summary mismatch")
    if require_satisfied and not expected_all_satisfied:
        raise CaseMapFailure("requirements are not all satisfied")


def _source_identity(relative: str) -> dict[str, object]:
    raw = (REPOSITORY_ROOT / relative).read_bytes()
    return {"path": relative, "bytes": len(raw), "sha256": _sha256(raw)}


def execute(case_map: Path, output: Path) -> dict[str, Any]:
    case_map = case_map.resolve()
    if case_map != REPOSITORY_ROOT / CASE_MAP_REL:
        raise CaseMapFailure("case map must use the fixed reviewed path")
    case_value = json.loads(case_map.read_bytes())
    rows = validate_case_map(case_value)
    invocations: dict[str, dict[str, Any]] = {}
    for row in rows:
        for case in row["cases"]:
            if case["kind"] == "execution":
                invocations.setdefault(case["invocation"]["id"], case["invocation"])
    executions: dict[str, dict[str, Any]] = {}
    for invocation_id, invocation in invocations.items():
        argv = _expected_argv(invocation["selector"], invocation_id)
        environment = {name: os.environ[name] for name in SAFE_ENVIRONMENT_NAMES if name in os.environ}
        if invocation["official_archive"]:
            environment["AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE"] = "1"
        completed = subprocess.run(argv, cwd=REPOSITORY_ROOT, env=environment, input=b"", capture_output=True, timeout=600, check=False)
        try:
            outcome = json.loads(completed.stdout)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            outcome = {"parse_error": str(error)}
        executions[invocation_id] = {
            "id": invocation_id,
            "selector": invocation["selector"],
            "level": invocation["level"],
            "official_archive": invocation["official_archive"],
            "purpose": invocation["purpose"],
            "method": invocation["method"],
            "argv": argv,
            "cwd": str(REPOSITORY_ROOT),
            "allowlisted_environment": environment,
            "exit": completed.returncode,
            "stdout": completed.stdout.decode("utf-8", errors="replace"),
            "stderr": completed.stderr.decode("utf-8", errors="replace"),
            "outcome": outcome,
        }
    hosted = [_load_hosted_proof(proof) for proof in case_value["hosted_proofs"]]
    hosted_ids = {proof["id"] for proof in hosted}
    row_results = []
    consumed_observations: set[tuple[str, int]] = set()
    for row in rows:
        case_results = []
        levels: set[str] = set()
        for case in row["cases"]:
            levels.add(case["level"])
            if case["kind"] == "hosted":
                satisfied = case["hosted"] in hosted_ids
                case_results.append({
                    "id": case["id"], "kind": case["kind"], "level": case["level"],
                    "invocation_id": None, "hosted_proof_id": case["hosted"], "stimulus": case["stimulus"],
                    "matched_predicates": [], "executed": True, "satisfied": satisfied,
                })
                continue
            invocation = case["invocation"]
            execution = executions[invocation["id"]]
            outcome = execution["outcome"]
            try:
                matched = _match_case_predicates(
                    case,
                    outcome,
                    invocation["id"],
                    consumed_observations,
                    context=f"row={row['id']} case={case['id']}",
                )
            except CaseMapFailure:
                matched = []
            satisfied = (
                execution["exit"] == 0
                and outcome.get("successful") is True
                and _stimulus_observed(case["stimulus"], outcome)
                and len(matched) == len(case["predicates"])
            )
            case_results.append({
                "id": case["id"], "kind": case["kind"], "level": case["level"],
                "invocation_id": invocation["id"], "hosted_proof_id": None, "stimulus": case["stimulus"],
                "matched_predicates": matched, "executed": True, "satisfied": satisfied,
            })
        row_satisfied = all(case["satisfied"] for case in case_results) and REQUIRED_ATOMIC_LEVELS[row["required_level"]].issubset(levels)
        row_results.append({
            "id": row["id"], "required_level": row["required_level"], "stimulus": row["stimulus"],
            "required_evidence": row["required_evidence"],
            "status": "satisfied" if row_satisfied else "failed", "cases": case_results,
        })
    payload = {
        "schema": EVIDENCE_SCHEMA,
        "operational_scope": case_value["operational_scope"],
        "case_map": _source_identity(case_map.relative_to(REPOSITORY_ROOT).as_posix()),
        "verifier": _source_identity(RUNNER_SOURCE_REL),
        "cwd": str(REPOSITORY_ROOT),
        "row_count": len(row_results),
        "case_count": sum(len(row["cases"]) for row in row_results),
        "invocation_count": len(executions),
        "all_cases_executed": all(case["executed"] for row in row_results for case in row["cases"]),
        "all_requirements_satisfied": all(row["status"] == "satisfied" for row in row_results),
        "rows": row_results,
        "invocations": list(executions.values()),
        "hosted_proofs": hosted,
    }
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0)
    descriptor = os.open(output, flags, 0o600)
    try:
        data = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()
        view = memoryview(data)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise OSError("evidence write made no progress")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    validate_executed_evidence(payload, rows, require_satisfied=False)
    return payload


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case-map", default=str(REPOSITORY_ROOT / CASE_MAP_REL))
    parser.add_argument("--output")
    parser.add_argument("--child-selector")
    parser.add_argument("--invocation-id")
    args = parser.parse_args()
    if args.child_selector:
        if args.output or not args.invocation_id:
            parser.error("child mode requires an invocation id and no output")
        return run_child(args.child_selector, args.invocation_id)
    if not args.output or args.invocation_id:
        parser.error("parent mode requires output and no invocation id")
    payload = execute(Path(args.case_map), Path(args.output))
    print(json.dumps({
        "schema": payload["schema"],
        "row_count": payload["row_count"],
        "case_count": payload["case_count"],
        "invocation_count": payload["invocation_count"],
        "all_cases_executed": payload["all_cases_executed"],
        "all_requirements_satisfied": payload["all_requirements_satisfied"],
    }, sort_keys=True))
    return 0 if payload["all_cases_executed"] and payload["all_requirements_satisfied"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
