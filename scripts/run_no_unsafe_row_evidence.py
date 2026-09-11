#!/usr/bin/env python3
"""Execute and validate the no-unsafe policy's normative regression rows.

This is a test-evidence runner, not scanner or generator runtime authority. It
executes real unittest selectors and records individual successful subtests,
failures, skips, commands, exits, and required-level coverage.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
if str(REPOSITORY_ROOT) not in sys.path:
    sys.path.insert(0, str(REPOSITORY_ROOT))

SCHEMA = "axiograph-no-unsafe-executed-row-cases-v1"
EVIDENCE_SCHEMA = "axiograph-no-unsafe-executed-row-evidence-v2"
CASE_MAP_REL = "scripts/no_unsafe_row_cases_v1.json"
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
    "direct+real": {"direct-real"}, "direct": {"direct"}, "CLI smoke": {"smoke"},
    "direct+inspection": {"direct", "inspection"}, "direct+CLI": {"direct", "CLI"},
    "CLI+direct-real": {"CLI", "direct-real"}, "static+CLI": {"static", "CLI"},
}


class CaseMapFailure(ValueError):
    """A closed case map failed before any row execution."""


def _assertion_is_observed(value: object, row_id: str) -> bool:
    return (
        isinstance(value, dict)
        and set(value) == {
            "row_id", "method", "test_function", "source_file", "source_line",
            "arguments", "keywords", "passed",
        }
        and value["row_id"] == row_id
        and value["method"] in ASSERTION_METHODS
        and isinstance(value["test_function"], str)
        and bool(value["test_function"])
        and value["source_file"] == "scripts/tests/test_no_unsafe_policy.py"
        and isinstance(value["source_line"], int)
        and value["source_line"] > 0
        and isinstance(value["arguments"], list)
        and isinstance(value["keywords"], dict)
        and value["passed"] is True
    )


def _selector_is_one_test(selector: str) -> bool:
    suite = unittest.defaultTestLoader.loadTestsFromName(selector)
    pending: list[unittest.TestSuite | unittest.case.TestCase] = [suite]
    tests: list[unittest.case.TestCase] = []
    while pending:
        item = pending.pop()
        if isinstance(item, unittest.TestSuite):
            pending.extend(item)
        else:
            tests.append(item)
    return len(tests) == 1 and tests[0].__class__.__name__ != "_FailedTest"


def validate_case_map(value: object) -> list[dict[str, Any]]:
    if not isinstance(value, dict) or set(value) != {"schema", "rows"}:
        raise CaseMapFailure("top-level shape")
    if value["schema"] != SCHEMA or not isinstance(value["rows"], list):
        raise CaseMapFailure("schema or rows")
    rows: dict[str, dict[str, Any]] = {}
    for row in value["rows"]:
        if not isinstance(row, dict) or set(row) != {"id", "required_level", "stimulus", "required_evidence", "runners"}:
            raise CaseMapFailure("row shape")
        row_id = row["id"]
        if not isinstance(row_id, str) or row_id in rows or row_id not in EXPECTED_LEVELS:
            raise CaseMapFailure("unknown or duplicate row")
        if row["required_level"] != EXPECTED_LEVELS[row_id]:
            raise CaseMapFailure("wrong required level")
        if not isinstance(row["stimulus"], str) or not row["stimulus"]:
            raise CaseMapFailure("missing stimulus")
        if not isinstance(row["required_evidence"], str) or not row["required_evidence"]:
            raise CaseMapFailure("missing required evidence")
        runners = row["runners"]
        if not isinstance(runners, list) or not runners:
            raise CaseMapFailure("unexecuted row")
        levels: set[str] = set()
        for runner in runners:
            if not isinstance(runner, dict) or set(runner) != {"selector", "level", "official_archive"}:
                raise CaseMapFailure("runner shape")
            selector = runner["selector"]
            level = runner["level"]
            if not isinstance(selector, str) or not selector.startswith("scripts.tests.test_no_unsafe_policy.") or not _selector_is_one_test(selector):
                raise CaseMapFailure("unknown runner")
            if level not in ALLOWED_RUNNER_LEVELS or not isinstance(runner["official_archive"], bool):
                raise CaseMapFailure("unknown runner level")
            levels.add(level)
        if not REQUIRED_ATOMIC_LEVELS[row["required_level"]].issubset(levels):
            raise CaseMapFailure("wrong-level evidence")
        rows[row_id] = row
    if set(rows) != set(EXPECTED_LEVELS):
        raise CaseMapFailure("absent normative row")
    return [rows[row_id] for row_id in EXPECTED_LEVELS]


def validate_executed_evidence(
    value: object,
    case_rows: list[dict[str, Any]],
    *,
    require_passed: bool = True,
) -> None:
    if not isinstance(value, dict):
        raise CaseMapFailure("evidence is not an object")
    required_top = {
        "schema", "case_map", "cwd", "row_count", "execution_count",
        "all_executed", "all_passed", "rows", "executions",
    }
    if set(value) != required_top or value["schema"] != EVIDENCE_SCHEMA:
        raise CaseMapFailure("evidence top-level shape")
    if not isinstance(value["rows"], list) or not isinstance(value["executions"], list):
        raise CaseMapFailure("evidence arrays")
    expected_by_id = {row["id"]: row for row in case_rows}
    expected_execution_keys = {
        (row["id"], runner["selector"], runner["level"], runner["official_archive"])
        for row in case_rows
        for runner in row["runners"]
    }
    execution_by_key: dict[tuple[str, str, str, bool], dict[str, Any]] = {}
    for execution in value["executions"]:
        if not isinstance(execution, dict) or set(execution) != {
            "row_id", "selector", "level", "official_archive", "argv", "cwd",
            "allowlisted_environment", "exit", "stdout", "stderr", "outcome",
        }:
            raise CaseMapFailure("execution shape")
        key = (
            execution["row_id"], execution["selector"], execution["level"],
            execution["official_archive"],
        )
        if key not in expected_execution_keys or key in execution_by_key:
            raise CaseMapFailure("unknown or duplicate execution")
        outcome = execution["outcome"]
        if not isinstance(outcome, dict) or not isinstance(outcome.get("assertions"), list):
            raise CaseMapFailure("execution outcome shape")
        if require_passed and not (
            execution["exit"] == 0
            and outcome.get("successful") is True
            and outcome.get("tests_run", 0) > 0
            and outcome.get("skips", 0) == 0
            and len(outcome["assertions"]) > 0
            and all(_assertion_is_observed(item, execution["row_id"]) for item in outcome["assertions"])
        ):
            raise CaseMapFailure("execution did not pass with observed assertions")
        execution_by_key[key] = execution
    if set(execution_by_key) != expected_execution_keys:
        raise CaseMapFailure("absent execution")
    observed_by_id: dict[str, dict[str, Any]] = {}
    for row in value["rows"]:
        if not isinstance(row, dict) or set(row) != {
            "id", "required_level", "stimulus", "required_evidence", "executed",
            "passed", "observations", "executions",
        }:
            raise CaseMapFailure("evidence row shape")
        row_id = row["id"]
        if row_id not in expected_by_id or row_id in observed_by_id:
            raise CaseMapFailure("unknown or duplicate evidence row")
        expected = expected_by_id[row_id]
        if any(row[key] != expected[key] for key in ("required_level", "stimulus", "required_evidence")):
            raise CaseMapFailure("evidence row contract mismatch")
        if row["executed"] is not True or not isinstance(row["executions"], list) or not row["executions"]:
            raise CaseMapFailure("unexecuted evidence row")
        configured = {(item["selector"], item["level"], item["official_archive"]) for item in expected["runners"]}
        levels: set[str] = set()
        expected_observations: list[dict[str, Any]] = []
        for reference in row["executions"]:
            if not isinstance(reference, dict) or set(reference) != {"selector", "level", "official_archive", "exit", "outcome"}:
                raise CaseMapFailure("evidence reference shape")
            configured_key = (reference["selector"], reference["level"], reference["official_archive"])
            key = (row_id, *configured_key)
            if configured_key not in configured or key not in execution_by_key:
                raise CaseMapFailure("unknown or wrong-level evidence reference")
            execution = execution_by_key[key]
            if reference["exit"] != execution["exit"] or reference["outcome"] != execution["outcome"]:
                raise CaseMapFailure("evidence reference differs from execution")
            assertions = execution["outcome"]["assertions"]
            invalid = [item for item in assertions if not _assertion_is_observed(item, row_id)]
            if not assertions or invalid:
                raise CaseMapFailure(
                    f"row {row_id} lacks passing observed assertions: "
                    f"exit={execution['exit']} successful={execution['outcome'].get('successful')} "
                    f"invalid={invalid[:1]}"
                )
            expected_observations.append({
                "selector": reference["selector"], "level": reference["level"],
                "official_archive": reference["official_archive"], "assertions": assertions,
            })
            levels.add(reference["level"])
        if row["observations"] != expected_observations:
            raise CaseMapFailure("row observations are not derived from execution")
        if not REQUIRED_ATOMIC_LEVELS[row["required_level"]].issubset(levels):
            raise CaseMapFailure("wrong-level executed evidence")
        if require_passed and row["passed"] is not True:
            raise CaseMapFailure("failed evidence row")
        observed_by_id[row_id] = row
    if set(observed_by_id) != set(expected_by_id) or value["row_count"] != len(expected_by_id):
        raise CaseMapFailure("absent evidence row")
    if value["all_executed"] is not all(row["executed"] for row in value["rows"]):
        raise CaseMapFailure("all_executed summary mismatch")
    if value["all_passed"] is not all(row["passed"] for row in value["rows"]):
        raise CaseMapFailure("all_passed summary mismatch")
    if value["execution_count"] != len(value["executions"]):
        raise CaseMapFailure("execution_count summary mismatch")
    if require_passed and value["all_passed"] is not True:
        raise CaseMapFailure("evidence is not all passing")


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
        params = {str(key): repr(value) for key, value in subtest.params.items()}  # type: ignore[attr-defined]
        outcome = {"test": self._name(test), "kind": "subtest", "params": params, "status": "pass" if err is None else "fail"}
        if err is not None:
            outcome["detail"] = self._exc_info_to_string(err, test)
        self.outcomes.append(outcome)


ASSERTION_METHODS = (
    "assertEqual", "assertNotEqual", "assertTrue", "assertFalse", "assertIs",
    "assertIsNot", "assertIsNone", "assertIsNotNone", "assertIn", "assertNotIn",
    "assertGreater", "assertGreaterEqual", "assertLess", "assertLessEqual",
    "assertRegex", "assertNotRegex", "assertRaises", "assertRaisesRegex",
)


def _safe_repr(value: object) -> str:
    rendered = repr(value)
    return rendered if len(rendered) <= 512 else rendered[:509] + "..."


def run_child(selector: str, row_id: str) -> int:
    suite = unittest.defaultTestLoader.loadTestsFromName(selector)
    result = VisibleResult()
    stdout = io.StringIO()
    stderr = io.StringIO()
    assertions: list[dict[str, Any]] = []
    originals: dict[str, Any] = {}
    for name in ASSERTION_METHODS:
        original = getattr(unittest.TestCase, name)
        originals[name] = original

        def observed(self: unittest.TestCase, *args: object, _name: str = name, _original: Any = original, **kwargs: object) -> Any:
            caller = sys._getframe(1)
            event = {
                "row_id": row_id, "method": _name,
                "test_function": caller.f_code.co_name,
                "source_file": str(Path(caller.f_code.co_filename).resolve().relative_to(REPOSITORY_ROOT)),
                "source_line": caller.f_lineno,
                "arguments": [_safe_repr(argument) for argument in args],
                "keywords": {key: _safe_repr(value) for key, value in sorted(kwargs.items())},
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
        "schema": "axiograph-no-unsafe-selector-outcome-v2",
        "row_id": row_id,
        "selector": selector,
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
    return 0 if result.wasSuccessful() and not result.skipped and result.testsRun > 0 and assertions else 1


def execute(case_map: Path, output: Path) -> dict[str, Any]:
    root = REPOSITORY_ROOT
    rows = validate_case_map(json.loads(case_map.read_bytes()))
    executions: dict[tuple[str, str, str, bool], dict[str, Any]] = {}
    for row in rows:
        for runner in row["runners"]:
            key = (row["id"], runner["selector"], runner["level"], runner["official_archive"])
            argv = [
                sys.executable, str(Path(__file__).resolve()),
                "--child-selector", runner["selector"], "--row-id", row["id"],
            ]
            environment = {
                name: os.environ[name]
                for name in SAFE_ENVIRONMENT_NAMES
                if name in os.environ
            }
            if runner["official_archive"]:
                environment["AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE"] = "1"
            completed = subprocess.run(argv, cwd=root, env=environment, input=b"", capture_output=True, timeout=600, check=False)
            try:
                outcome = json.loads(completed.stdout)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                outcome = {"parse_error": str(error), "stdout_sha256_unavailable": True}
            executions[key] = {
                "row_id": key[0], "selector": key[1], "level": key[2], "official_archive": key[3],
                "argv": argv, "cwd": str(root),
                "allowlisted_environment": environment,
                "exit": completed.returncode,
                "stdout": completed.stdout.decode("utf-8", errors="replace"),
                "stderr": completed.stderr.decode("utf-8", errors="replace"),
                "outcome": outcome,
            }
    evidence_rows: list[dict[str, Any]] = []
    for row in rows:
        refs=[]
        observations=[]
        for runner in row["runners"]:
            key=(row["id"],runner["selector"],runner["level"],runner["official_archive"])
            execution=executions[key]
            ref={"selector":key[1],"level":key[2],"official_archive":key[3],"exit":execution["exit"],"outcome":execution["outcome"]}
            refs.append(ref)
            observations.append({"selector":key[1],"level":key[2],"official_archive":key[3],"assertions":execution["outcome"].get("assertions",[])})
        passed=all(ref["exit"]==0 and ref["outcome"].get("successful") is True and ref["outcome"].get("tests_run",0)>0 and ref["outcome"].get("skips",0)==0 and ref["outcome"].get("assertions") and all(_assertion_is_observed(item, row["id"]) for item in ref["outcome"]["assertions"]) for ref in refs)
        evidence_rows.append({"id":row["id"],"required_level":row["required_level"],"stimulus":row["stimulus"],"required_evidence":row["required_evidence"],"executed":True,"passed":passed,"observations":observations,"executions":refs})
    payload={"schema":EVIDENCE_SCHEMA,"case_map":str(case_map),"cwd":str(root),"row_count":len(evidence_rows),"execution_count":len(executions),"all_executed":all(row["executed"] for row in evidence_rows),"all_passed":all(row["passed"] for row in evidence_rows),"rows":evidence_rows,"executions":list(executions.values())}
    validate_executed_evidence(payload, rows, require_passed=False)
    flags=os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,"O_CLOEXEC",0)
    descriptor=os.open(output,flags,0o600)
    try:
        data=(json.dumps(payload,indent=2,sort_keys=True)+"\n").encode()
        view=memoryview(data)
        while view:
            written=os.write(descriptor,view)
            if written<=0: raise OSError("evidence write made no progress")
            view=view[written:]
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    return payload


def main() -> int:
    parser=argparse.ArgumentParser()
    parser.add_argument("--case-map",default=str(REPOSITORY_ROOT/CASE_MAP_REL))
    parser.add_argument("--output")
    parser.add_argument("--child-selector")
    parser.add_argument("--row-id")
    args=parser.parse_args()
    if args.child_selector:
        if args.output or args.row_id not in EXPECTED_LEVELS: parser.error("child mode requires one known row and no output")
        return run_child(args.child_selector, args.row_id)
    if not args.output or args.row_id: parser.error("parent mode requires output and no row")
    payload=execute(Path(args.case_map),Path(args.output))
    print(json.dumps({"schema":payload["schema"],"row_count":payload["row_count"],"execution_count":payload["execution_count"],"all_executed":payload["all_executed"],"all_passed":payload["all_passed"]},sort_keys=True))
    return 0 if payload["all_executed"] and payload["all_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
