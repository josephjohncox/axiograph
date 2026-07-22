#!/usr/bin/env python3
"""Run the release checker against one hash-pinned positive/adversarial corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath

try:
    from .bounded_io import (
        BoundedIoError,
        read_regular_bounded,
        validate_json_nesting,
    )
    from .bounded_subprocess import BoundedProcessError, run_bounded
except ImportError:
    from bounded_io import BoundedIoError, read_regular_bounded, validate_json_nesting
    from bounded_subprocess import BoundedProcessError, run_bounded

HEX64 = re.compile(r"[0-9a-f]{64}")
MAX_FIXTURE_BYTES = 4 * 1024 * 1024
MAX_CHECKER_BYTES = 256 * 1024 * 1024
EXPECTED_SCHEMA = "axiograph-release-fixtures-v2"
EXPECTED_PROTOCOL = "axiograph-verifier-stdio-v2"
EXPECTED_BUILD_ID = "axiograph-verify-main-v3"
EXPECTED_ENTRYPOINT = "lean/Axiograph/VerifyMain.lean"


class FixtureSuiteError(RuntimeError):
    pass


def _fail(message: str) -> FixtureSuiteError:
    return FixtureSuiteError(message)


def _absolute_no_resolve(path: Path) -> Path:
    return path if path.is_absolute() else Path.cwd() / path


def _exact_keys(value: object, keys: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise _fail(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise _fail(
            f"{label} keys differ: missing={sorted(keys - actual)}, "
            f"extra={sorted(actual - keys)}"
        )
    return value


def _repo_file(repo_root: Path, raw_path: object, label: str) -> Path:
    if not isinstance(raw_path, str) or not raw_path:
        raise _fail(f"{label} path must be a non-empty string")
    pure = PurePosixPath(raw_path)
    if pure.is_absolute() or ".." in pure.parts or "." in pure.parts:
        raise _fail(f"{label} path is not repository-relative: {raw_path!r}")
    path = repo_root
    try:
        root_metadata = path.lstat()
        if not stat.S_ISDIR(root_metadata.st_mode) or path.is_symlink():
            raise _fail("fixture repository root must be a real directory")
        for index, part in enumerate(pure.parts):
            path = path / part
            metadata = path.lstat()
            if path.is_symlink():
                raise _fail(f"{label} path contains a symlink: {raw_path!r}")
            is_last = index + 1 == len(pure.parts)
            if is_last and not stat.S_ISREG(metadata.st_mode):
                raise _fail(f"{label} must be a regular file: {raw_path!r}")
            if not is_last and not stat.S_ISDIR(metadata.st_mode):
                raise _fail(f"{label} parent must be a directory: {raw_path!r}")
    except OSError as error:
        raise _fail(f"cannot stat {label} {raw_path!r}: {error}") from error
    return path


def _read_hashed_fixture(
    repo_root: Path, entry: dict[str, object], label: str
) -> tuple[Path, str]:
    path = _repo_file(repo_root, entry["path"], label)
    size = entry["bytes"]
    digest = entry["sha256"]
    if (
        not isinstance(size, int)
        or isinstance(size, bool)
        or not 0 < size <= MAX_FIXTURE_BYTES
    ):
        raise _fail(f"{label} bytes must be in 1..={MAX_FIXTURE_BYTES}")
    if not isinstance(digest, str) or HEX64.fullmatch(digest) is None:
        raise _fail(f"{label} sha256 must be 64 lowercase hexadecimal characters")
    try:
        data = read_regular_bounded(path, MAX_FIXTURE_BYTES, label)
    except BoundedIoError as error:
        raise _fail(str(error)) from error
    if len(data) != size:
        raise _fail(f"{label} byte count changed: manifest={size}, actual={len(data)}")
    actual = hashlib.sha256(data).hexdigest()
    if actual != digest:
        raise _fail(f"{label} sha256 changed: manifest={digest}, actual={actual}")
    try:
        return path, data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise _fail(f"{label} is not UTF-8: {error}") from error


def load_manifest(manifest_path: Path, repo_root: Path) -> dict[str, object]:
    try:
        raw = read_regular_bounded(manifest_path, MAX_FIXTURE_BYTES, "fixture manifest")
    except (BoundedIoError, OSError) as error:
        raise _fail(f"cannot read fixture manifest: {error}") from error
    return validate_manifest_bytes(raw, repo_root)


def validate_manifest_bytes(raw: bytes, repo_root: Path) -> dict[str, object]:
    if not raw or len(raw) > MAX_FIXTURE_BYTES:
        raise _fail(f"fixture manifest size must be in 1..={MAX_FIXTURE_BYTES}")
    try:
        validate_json_nesting(raw, "fixture manifest")
        parsed = json.loads(raw)
    except (BoundedIoError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise _fail(f"fixture manifest is not valid UTF-8 JSON: {error}") from error
    manifest = _exact_keys(
        parsed,
        {
            "schema",
            "trusted_checker",
            "claim",
            "module",
            "cases",
            "non_claims",
        },
        "fixture manifest",
    )
    if manifest["schema"] != EXPECTED_SCHEMA:
        raise _fail(f"unsupported fixture manifest schema {manifest['schema']!r}")

    checker = _exact_keys(
        manifest["trusted_checker"],
        {"build_id", "entrypoint", "protocol"},
        "trusted_checker",
    )
    if checker != {
        "build_id": EXPECTED_BUILD_ID,
        "entrypoint": EXPECTED_ENTRYPOINT,
        "protocol": EXPECTED_PROTOCOL,
    }:
        raise _fail(
            "fixture manifest checker contract does not match the release contract"
        )
    _repo_file(repo_root, checker["entrypoint"], "trusted checker entrypoint")

    claim = _exact_keys(
        manifest["claim"],
        {
            "certificate_kind",
            "claim_kind",
            "expected_answer_digest",
            "expected_prepared_query_digest",
            "expected_revision_digest",
            "scope",
        },
        "claim",
    )
    if claim["certificate_kind"] != "query_result_v4":
        raise _fail("release fixture must exercise query_result_v4")
    if claim["claim_kind"] != "finite_exact_complete":
        raise _fail("release fixture must exercise finite_exact_complete")
    for field, prefix in (
        ("expected_answer_digest", "axi:answer:v2:sha256:"),
        ("expected_prepared_query_digest", "axi:query:v2:sha256:"),
        ("expected_revision_digest", "axi:revision:v2:sha256:"),
    ):
        value = claim[field]
        if (
            not isinstance(value, str)
            or not value.startswith(prefix)
            or HEX64.fullmatch(value[len(prefix) :]) is None
        ):
            raise _fail(f"claim {field} has an invalid typed SHA-256 digest")
    if not isinstance(claim["scope"], str) or not claim["scope"]:
        raise _fail("claim scope must be explicit")

    module = _exact_keys(manifest["module"], {"bytes", "path", "sha256"}, "module")
    _read_hashed_fixture(repo_root, module, "module")

    cases = manifest["cases"]
    if not isinstance(cases, list) or len(cases) < 2:
        raise _fail("fixture manifest must contain a positive and adversarial cases")
    seen_ids: set[str] = set()
    seen_paths: set[str] = set()
    accept_count = 0
    reject_count = 0
    for index, raw_case in enumerate(cases):
        case = _exact_keys(
            raw_case,
            {"bytes", "expect", "id", "message_contains", "path", "sha256"},
            f"case[{index}]",
        )
        case_id = case["id"]
        if not isinstance(case_id, str) or not re.fullmatch(
            r"[a-z0-9]+(?:-[a-z0-9]+)*", case_id
        ):
            raise _fail(f"case[{index}] id is not canonical kebab-case")
        if case_id in seen_ids:
            raise _fail(f"duplicate fixture case id {case_id!r}")
        seen_ids.add(case_id)
        case_path = case["path"]
        if not isinstance(case_path, str) or case_path in seen_paths:
            raise _fail(f"duplicate or invalid fixture case path {case_path!r}")
        seen_paths.add(case_path)
        expectation = case["expect"]
        if expectation == "accept":
            accept_count += 1
        elif expectation == "reject":
            reject_count += 1
        else:
            raise _fail(f"case {case_id!r} expectation must be accept or reject")
        if (
            not isinstance(case["message_contains"], str)
            or not case["message_contains"]
        ):
            raise _fail(f"case {case_id!r} must pin an expected receipt message")
        _read_hashed_fixture(repo_root, case, f"case {case_id}")
    if accept_count != 1 or reject_count < 1:
        raise _fail(
            "fixture manifest must contain exactly one accept and at least one reject"
        )

    non_claims = manifest["non_claims"]
    if (
        not isinstance(non_claims, list)
        or len(non_claims) < 3
        or any(not isinstance(item, str) or not item for item in non_claims)
        or len(set(non_claims)) != len(non_claims)
    ):
        raise _fail("fixture manifest must contain unique, explicit non-claims")
    return manifest


def _stage_checker(
    checker: Path,
) -> tuple[tempfile.TemporaryDirectory[str], Path, str]:
    metadata = checker.lstat()
    if not stat.S_ISREG(metadata.st_mode) or checker.is_symlink():
        raise _fail("checker must be a regular non-symlink file")
    if os.name != "nt" and metadata.st_mode & 0o111 == 0:
        raise _fail("checker is not executable")
    try:
        checker_bytes = read_regular_bounded(
            checker, MAX_CHECKER_BYTES, "release checker"
        )
    except BoundedIoError as error:
        raise _fail(str(error)) from error
    digest = hashlib.sha256(checker_bytes).hexdigest()
    staging = tempfile.TemporaryDirectory(prefix="axiograph-release-checker-")
    name = "axiograph_verify.exe" if os.name == "nt" else "axiograph_verify"
    staged = Path(staging.name) / name
    descriptor = os.open(
        staged,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0),
        0o500,
    )
    try:
        remaining = memoryview(checker_bytes)
        while remaining:
            written = os.write(descriptor, remaining)
            if written <= 0:
                raise _fail("short write while staging release checker")
            remaining = remaining[written:]
        if os.name != "nt":
            os.fchmod(descriptor, 0o500)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    return staging, staged, digest


def _parse_receipt(stdout: str, case_id: str) -> dict[str, object]:
    if len(stdout.encode("utf-8")) > 64 * 1024:
        raise _fail(f"checker receipt for {case_id} exceeds 64 KiB")
    try:
        validate_json_nesting(stdout.encode("utf-8"), f"checker receipt {case_id}")
        receipt = json.loads(stdout)
    except (BoundedIoError, json.JSONDecodeError) as error:
        raise _fail(
            f"checker receipt for {case_id} is not one JSON object: {error}"
        ) from error
    if not isinstance(receipt, dict):
        raise _fail(f"checker receipt for {case_id} is not an object")
    return receipt


def run_suite(
    checker: Path,
    manifest_path: Path,
    repo_root: Path,
    timeout_seconds: float,
) -> dict[str, object]:
    manifest = load_manifest(manifest_path, repo_root)
    checker_staging, staged_checker, checker_digest = _stage_checker(checker)
    _, module_text = _read_hashed_fixture(repo_root, manifest["module"], "module")
    claim = manifest["claim"]
    results: list[dict[str, object]] = []

    for index, raw_case in enumerate(manifest["cases"], start=1):
        case = raw_case
        case_id = case["id"]
        _, certificate_text = _read_hashed_fixture(repo_root, case, f"case {case_id}")
        nonce = f"00000000-0000-4000-8000-{index:012d}"
        request = {
            "version": EXPECTED_PROTOCOL,
            "nonce": nonce,
            "checker_sha256": checker_digest,
            "module_axi": module_text,
            "certificate_json": certificate_text,
            "expected_prepared_query_digest": claim["expected_prepared_query_digest"],
            "expected_answer_digest": claim["expected_answer_digest"],
        }
        try:
            completed = run_bounded(
                [str(staged_checker), "--stdio-v2"],
                input_bytes=json.dumps(
                    request, sort_keys=True, separators=(",", ":")
                ).encode("utf-8"),
                timeout_seconds=timeout_seconds,
                max_stdout_bytes=64 * 1024,
                max_stderr_bytes=64 * 1024,
            )
        except subprocess.TimeoutExpired as error:
            raise _fail(f"checker timed out for case {case_id}") from error
        except BoundedProcessError as error:
            raise _fail(
                f"checker exceeded process limits for case {case_id}: {error}"
            ) from error
        if completed.stderr:
            raise _fail(
                f"checker wrote stderr for case {case_id}: {completed.stderr!r}"
            )
        try:
            stdout = completed.stdout.decode("utf-8")
        except UnicodeDecodeError as error:
            raise _fail(f"checker receipt for {case_id} is not UTF-8") from error
        receipt = _parse_receipt(stdout, case_id)
        required_bindings = {
            "version": EXPECTED_PROTOCOL,
            "checker_build_id": EXPECTED_BUILD_ID,
            "checker_sha256": checker_digest,
            "nonce": nonce,
            "certificate_kind": claim["certificate_kind"],
            "claim_kind": claim["claim_kind"],
            "revision_digest_v2": claim["expected_revision_digest"],
            "prepared_query_digest_v1": claim["expected_prepared_query_digest"],
            "answer_digest_v1": claim["expected_answer_digest"],
        }
        for field, expected in required_bindings.items():
            if receipt.get(field) != expected:
                raise _fail(
                    f"case {case_id} receipt {field}={receipt.get(field)!r}; "
                    f"expected {expected!r}"
                )
        message = receipt.get("message")
        if not isinstance(message, str) or case["message_contains"] not in message:
            raise _fail(
                f"case {case_id} receipt did not contain pinned message fragment "
                f"{case['message_contains']!r}"
            )
        expectation = case["expect"]
        if expectation == "accept":
            if completed.returncode != 0 or receipt.get("decision") != "accepted":
                raise _fail(
                    f"positive case {case_id} did not return an accepted receipt"
                )
        else:
            if completed.returncode == 0 or receipt.get("decision") != "rejected":
                raise _fail(f"adversarial case {case_id} did not fail closed")
        results.append(
            {
                "id": case_id,
                "expected": expectation,
                "decision": receipt.get("decision"),
                "exit_code": completed.returncode,
            }
        )

    report = {
        "schema": EXPECTED_SCHEMA,
        "checker_sha256": checker_digest,
        "accepted": sum(result["decision"] == "accepted" for result in results),
        "rejected": sum(result["decision"] == "rejected" for result in results),
        "cases": results,
        "scope": manifest["claim"]["scope"],
        "non_claims": manifest["non_claims"],
    }
    checker_staging.cleanup()
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checker", required=True, type=Path)
    parser.add_argument("--manifest", type=Path, default=Path("release/fixtures.json"))
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    args = parser.parse_args()
    if not 0 < args.timeout_seconds <= 300:
        parser.error("--timeout-seconds must be in (0, 300]")
    try:
        report = run_suite(
            _absolute_no_resolve(args.checker),
            _absolute_no_resolve(args.manifest),
            args.repo_root.resolve(),
            args.timeout_seconds,
        )
    except (FixtureSuiteError, OSError) as error:
        print(f"release fixture suite failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
