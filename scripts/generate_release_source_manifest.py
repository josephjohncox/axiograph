#!/usr/bin/env python3
"""Generate a canonical source manifest from one completely clean Git checkout."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

try:
    from .bounded_io import BoundedIoError, validate_json_nesting
    from .bounded_subprocess import BoundedProcessError, run_bounded
except ImportError:
    from bounded_io import BoundedIoError, validate_json_nesting
    from bounded_subprocess import BoundedProcessError, run_bounded

SCHEMA = "axiograph-clean-source-manifest-v2"
HEX_OBJECT = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
HEX64 = re.compile(r"[0-9a-f]{64}")
MAX_MANIFEST_BYTES = 16 * 1024 * 1024
MAX_FILE_COUNT = 100_000
MAX_SOURCE_FILE_BYTES = 512 * 1024 * 1024
MAX_TOTAL_SOURCE_BYTES = 512 * 1024 * 1024
ALLOWED_MODES = {"100644", "100755"}
RELEASE_SOURCE_SUFFIXES = {".rs", ".lean", ".py", ".sh", ".ts", ".tsx"}
SCOPE = {
    "authority": "exact canonical Git tree bytes for the recorded commit",
    "generated_from": "git-ls-tree-and-git-cat-file-after-clean-status",
    "non_claims": [
        "compiler output reproducibility",
        "dependency-source inclusion outside the repository",
        "semantic correctness of untrusted Rust",
        "trusted-kernel expansion beyond VerifyMain import closure",
    ],
}


class SourceManifestError(RuntimeError):
    pass


def _fail(message: str) -> SourceManifestError:
    return SourceManifestError(message)


def _git(
    repo_root: Path,
    *args: str,
    text: bool = False,
    max_stdout_bytes: int = 64 * 1024 * 1024,
) -> bytes | str:
    environment = os.environ.copy()
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_INDEX_FILE",
        "GIT_EXEC_PATH",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG",
    ]:
        environment.pop(name, None)
    environment.update(
        {
            "GIT_CONFIG_COUNT": "0",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_ALLOW_PROTOCOL": "file",
            "GIT_PROTOCOL_FROM_USER": "0",
        }
    )
    try:
        completed = run_bounded(
            [
                "git",
                "-c",
                "core.fsmonitor=false",
                "-c",
                f"core.hooksPath={os.devnull}",
                "-C",
                str(repo_root),
                *args,
            ],
            timeout_seconds=120,
            max_stdout_bytes=max_stdout_bytes,
            max_stderr_bytes=1024 * 1024,
            env=environment,
        )
    except subprocess.TimeoutExpired as error:
        raise _fail(f"git {' '.join(args)} timed out") from error
    except BoundedProcessError as error:
        raise _fail(f"git {' '.join(args)} exceeded process limits: {error}") from error
    if completed.returncode != 0:
        stderr = completed.stderr.decode(errors="replace").strip()
        raise _fail(f"git {' '.join(args)} failed: {stderr}")
    if text:
        try:
            return completed.stdout.decode("utf-8")
        except UnicodeDecodeError as error:
            raise _fail(f"git {' '.join(args)} output is not UTF-8") from error
    return completed.stdout


def dirty_status_entries(status: bytes) -> list[str]:
    entries: list[str] = []
    for raw in status.split(b"\0"):
        if not raw:
            continue
        try:
            entries.append(raw.decode("utf-8"))
        except UnicodeDecodeError as error:
            raise _fail(f"git status contains a non-UTF-8 path: {error}") from error
    return entries


def _is_release_source_path(path: Path, repo_root: Path) -> bool:
    try:
        relative = path.relative_to(repo_root)
    except ValueError:
        return False
    if path.suffix not in RELEASE_SOURCE_SUFFIXES:
        return False
    parts = relative.parts
    return (
        (len(parts) >= 5 and parts[:2] == ("rust", "crates") and parts[3] == "src")
        or (len(parts) >= 3 and parts[:2] == ("rust", "verus") and parts[2] == "src")
        or (
            len(parts) >= 3
            and parts[:2] == ("rust", "fuzz")
            and parts[2] in {"fuzz_targets", "tests"}
        )
        or (len(parts) >= 2 and parts[0] == "lean" and parts[1] == "Axiograph")
        or (len(parts) >= 2 and parts[0] == "scripts")
        or (len(parts) >= 3 and parts[:3] == ("frontend", "viz", "src"))
    )


def _ignored_entry_can_contain_release_source(relative: str) -> bool:
    normalized = relative.rstrip("/")
    direct_source_prefixes = (
        "rust/verus/src/",
        "rust/fuzz/fuzz_targets/",
        "rust/fuzz/tests/",
        "lean/Axiograph/",
        "frontend/viz/src/",
    )
    is_crate_source = normalized.startswith("rust/crates/") and "/src/" in (
        normalized + "/"
    )
    is_script_source = (
        normalized.startswith("scripts/")
        and "/__pycache__" not in normalized
        and "/.ruff_cache" not in normalized
    )
    return is_crate_source or is_script_source or normalized.startswith(
        direct_source_prefixes
    )


def ignored_release_source_candidates(
    repo_root: Path, status_entries: list[str]
) -> list[str]:
    candidates: set[str] = set()
    inspected = 0
    for entry in status_entries:
        if not entry.startswith("!! "):
            continue
        relative = entry[3:]
        if not _ignored_entry_can_contain_release_source(relative):
            continue
        path = repo_root / relative
        paths = [path] if not relative.endswith("/") else path.rglob("*")
        for candidate in paths:
            inspected += 1
            if inspected > MAX_FILE_COUNT:
                raise _fail(
                    f"ignored release-source scan exceeds {MAX_FILE_COUNT} entries"
                )
            if candidate.is_file() and _is_release_source_path(candidate, repo_root):
                candidates.add(candidate.relative_to(repo_root).as_posix())
    return sorted(candidates, key=lambda path: path.encode("utf-8"))


def assert_clean_checkout(repo_root: Path) -> None:
    status = _git(
        repo_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "-z",
    )
    if not isinstance(status, bytes):
        raise _fail("git status unexpectedly returned text")
    entries = dirty_status_entries(status)
    if entries:
        preview = ", ".join(repr(entry) for entry in entries[:8])
        suffix = "" if len(entries) <= 8 else f", ... ({len(entries)} entries total)"
        raise _fail(
            "release source manifest requires a clean checkout; dirty entries: "
            + preview
            + suffix
        )

    ignored_status = _git(
        repo_root,
        "status",
        "--porcelain=v1",
        "--ignored=matching",
        "--untracked-files=all",
        "-z",
    )
    if not isinstance(ignored_status, bytes):
        raise _fail("ignored git status unexpectedly returned text")
    ignored_sources = ignored_release_source_candidates(
        repo_root, dirty_status_entries(ignored_status)
    )
    if ignored_sources:
        preview = ", ".join(repr(path) for path in ignored_sources[:8])
        suffix = (
            ""
            if len(ignored_sources) <= 8
            else f", ... ({len(ignored_sources)} entries total)"
        )
        raise _fail(
            "release checkout contains ignored source files that cannot enter the Git manifest: "
            + preview
            + suffix
        )


def _canonical_path(raw: bytes) -> str:
    try:
        value = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise _fail(f"tracked path is not UTF-8: {error}") from error
    pure = PurePosixPath(value)
    if (
        not value
        or pure.is_absolute()
        or ".." in pure.parts
        or "." in pure.parts
        or "\\" in value
        or "\x00" in value
    ):
        raise _fail(
            f"tracked path is not canonical repository-relative UTF-8: {value!r}"
        )
    return value


def build_source_manifest(
    source_commit: str,
    source_tree: str,
    entries: list[tuple[str, str, str, bytes]],
) -> dict[str, object]:
    if HEX_OBJECT.fullmatch(source_commit) is None:
        raise _fail("source commit must be a full Git object id")
    if HEX_OBJECT.fullmatch(source_tree) is None:
        raise _fail("source tree must be a full Git object id")
    if not entries or len(entries) > MAX_FILE_COUNT:
        raise _fail(f"source manifest file count must be in 1..={MAX_FILE_COUNT}")
    files: list[dict[str, object]] = []
    seen: set[str] = set()
    for path, mode, git_blob, data in entries:
        canonical = _canonical_path(path.encode("utf-8"))
        if canonical in seen:
            raise _fail(f"duplicate source path {canonical!r}")
        seen.add(canonical)
        if mode not in ALLOWED_MODES:
            raise _fail(f"unsupported Git mode {mode!r} for {canonical}")
        if HEX_OBJECT.fullmatch(git_blob) is None:
            raise _fail(f"invalid Git blob id for {canonical}")
        if len(data) > MAX_SOURCE_FILE_BYTES:
            raise _fail(
                f"source file exceeds {MAX_SOURCE_FILE_BYTES} bytes: {canonical}"
            )
        files.append(
            {
                "path": canonical,
                "git_mode": mode,
                "git_blob": git_blob,
                "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    files.sort(key=lambda entry: entry["path"].encode("utf-8"))
    return {
        "schema": SCHEMA,
        "source_commit": source_commit,
        "source_tree": source_tree,
        "files": files,
        "scope": SCOPE,
    }


def generate(repo_root: Path) -> dict[str, object]:
    root = Path(str(_git(repo_root, "rev-parse", "--show-toplevel", text=True)).strip())
    if root.resolve() != repo_root.resolve():
        raise _fail(
            f"repository root mismatch: expected {repo_root}, Git reported {root}"
        )
    assert_clean_checkout(repo_root)
    source_commit = str(
        _git(
            repo_root,
            "rev-parse",
            "--verify",
            "HEAD",
            text=True,
            max_stdout_bytes=1024,
        )
    ).strip()
    source_tree = str(
        _git(
            repo_root,
            "rev-parse",
            "--verify",
            "HEAD^{tree}",
            text=True,
            max_stdout_bytes=1024,
        )
    ).strip()
    raw_tree = _git(repo_root, "ls-tree", "-r", "-z", "--full-tree", "HEAD")
    if not isinstance(raw_tree, bytes):
        raise _fail("git tree listing unexpectedly returned text")
    entries: list[tuple[str, str, str, bytes]] = []
    total_source_bytes = 0
    for raw_entry in raw_tree.split(b"\0"):
        if not raw_entry:
            continue
        if len(entries) >= MAX_FILE_COUNT:
            raise _fail(f"source file count exceeds {MAX_FILE_COUNT}")
        try:
            metadata, raw_path = raw_entry.split(b"\t", 1)
            mode, object_type, raw_oid = metadata.split(b" ", 2)
        except ValueError as error:
            raise _fail("git ls-tree returned malformed data") from error
        if object_type != b"blob":
            path = _canonical_path(raw_path)
            raise _fail(
                f"release source tree contains unsupported non-blob entry: {path}"
            )
        path = _canonical_path(raw_path)
        mode_text = mode.decode("ascii")
        oid = raw_oid.decode("ascii")
        size_text = str(
            _git(
                repo_root,
                "cat-file",
                "-s",
                oid,
                text=True,
                max_stdout_bytes=1024,
            )
        ).strip()
        try:
            size = int(size_text)
        except ValueError as error:
            raise _fail(f"Git reported an invalid blob size for {path}") from error
        if not 0 <= size <= MAX_SOURCE_FILE_BYTES:
            raise _fail(f"source file size is outside bounds: {path}")
        total_source_bytes += size
        if total_source_bytes > MAX_TOTAL_SOURCE_BYTES:
            raise _fail(f"source tree bytes exceed {MAX_TOTAL_SOURCE_BYTES}")
        blob = _git(
            repo_root,
            "cat-file",
            "blob",
            oid,
            max_stdout_bytes=max(1, size + 1),
        )
        if not isinstance(blob, bytes):
            raise _fail(f"Git blob unexpectedly returned text for {path}")
        if len(blob) != size:
            raise _fail(f"Git blob size changed while reading {path}")
        entries.append((path, mode_text, oid, blob))
    return build_source_manifest(source_commit, source_tree, entries)


def canonical_bytes(manifest: dict[str, object]) -> bytes:
    return (json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n").encode(
        "utf-8"
    )


def validate_source_manifest_bytes(raw: bytes) -> dict[str, object]:
    if not raw or len(raw) > MAX_MANIFEST_BYTES:
        raise _fail(f"source manifest size must be in 1..={MAX_MANIFEST_BYTES} bytes")
    try:
        validate_json_nesting(raw, "source manifest")
        parsed = json.loads(raw)
    except (BoundedIoError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise _fail(f"source manifest is not valid UTF-8 JSON: {error}") from error
    if not isinstance(parsed, dict):
        raise _fail("source manifest must be an object")
    expected_keys = {"schema", "source_commit", "source_tree", "files", "scope"}
    if set(parsed) != expected_keys:
        raise _fail("source manifest has missing or unknown top-level fields")
    if parsed["schema"] != SCHEMA:
        raise _fail(f"unsupported source manifest schema {parsed['schema']!r}")
    if parsed["scope"] != SCOPE:
        raise _fail("source manifest scope/non-claims changed")
    source_commit = parsed["source_commit"]
    source_tree = parsed["source_tree"]
    if (
        not isinstance(source_commit, str)
        or HEX_OBJECT.fullmatch(source_commit) is None
    ):
        raise _fail("source manifest commit is not a full Git object id")
    if not isinstance(source_tree, str) or HEX_OBJECT.fullmatch(source_tree) is None:
        raise _fail("source manifest tree is not a full Git object id")
    files = parsed["files"]
    if not isinstance(files, list) or not files or len(files) > MAX_FILE_COUNT:
        raise _fail("source manifest has an invalid file list")
    paths: list[str] = []
    for index, item in enumerate(files):
        if not isinstance(item, dict) or set(item) != {
            "path",
            "git_mode",
            "git_blob",
            "bytes",
            "sha256",
        }:
            raise _fail(f"source manifest file[{index}] has invalid fields")
        path = item["path"]
        if not isinstance(path, str):
            raise _fail(f"source manifest file[{index}] path is not a string")
        _canonical_path(path.encode("utf-8"))
        if item["git_mode"] not in ALLOWED_MODES:
            raise _fail(f"source manifest file[{index}] has unsupported mode")
        if (
            not isinstance(item["git_blob"], str)
            or HEX_OBJECT.fullmatch(item["git_blob"]) is None
        ):
            raise _fail(f"source manifest file[{index}] has invalid blob id")
        size = item["bytes"]
        if (
            not isinstance(size, int)
            or isinstance(size, bool)
            or not 0 <= size <= MAX_SOURCE_FILE_BYTES
        ):
            raise _fail(f"source manifest file[{index}] has invalid byte count")
        if (
            not isinstance(item["sha256"], str)
            or HEX64.fullmatch(item["sha256"]) is None
        ):
            raise _fail(f"source manifest file[{index}] has invalid SHA-256")
        paths.append(path)
    if paths != sorted(paths, key=lambda path: path.encode("utf-8")):
        raise _fail("source manifest paths are not bytewise sorted")
    if len(paths) != len(set(paths)):
        raise _fail("source manifest contains duplicate paths")
    if canonical_bytes(parsed) != raw:
        raise _fail("source manifest is not canonical JSON with one trailing newline")
    return parsed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root", type=Path, default=Path(__file__).resolve().parents[1]
    )
    destination = parser.add_mutually_exclusive_group(required=True)
    destination.add_argument("--output", type=Path)
    destination.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    try:
        manifest = generate(args.repo_root.resolve())
        raw = canonical_bytes(manifest)
        validate_source_manifest_bytes(raw)
        if args.output is not None:
            parent = args.output.parent if args.output.parent != Path("") else Path(".")
            parent.mkdir(parents=True, exist_ok=True)
            parent_metadata = parent.lstat()
            if not parent_metadata.is_dir() or parent.is_symlink():
                raise _fail("source manifest output parent must be a real directory")
            with args.output.open("xb") as output:
                output.write(raw)
                output.flush()
                os.fsync(output.fileno())
    except (SourceManifestError, OSError) as error:
        print(f"release source manifest failed: {error}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "manifest": str(args.output) if args.output is not None else None,
                "source_commit": manifest["source_commit"],
                "source_tree": manifest["source_tree"],
                "files": len(manifest["files"]),
                "sha256": hashlib.sha256(raw).hexdigest(),
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
