#!/usr/bin/env python3
"""Fail closed when first-party Rust can use unsafe code.

The scanner treats only two byte-identical, untracked Kani library caches as an
ownership-exempt external distribution. All other Rust, including ignored,
untracked, unattached, and cfg-disabled files, remains first-party lexical input.
The compiler gate in ``make check-no-unsafe`` is independent and authoritative.
"""

from __future__ import annotations

import errno
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import tomllib

try:
    from scripts.bounded_subprocess import BoundedProcessError, run_bounded
    from scripts.no_unsafe_fs import (
        O_CLOEXEC,
        O_NOFOLLOW,
        O_PATH,
        REAL_FILE_OPS,
        FileOps,
        HeldDirectory,
        PolicyFailure,
        close_chain,
        close_descriptor,
        directory_tuple,
        full_tuple,
        metadata_observe_at,
        open_directory_at,
        open_root_directory,
        read_held_regular,
        read_regular_at,
        upgrade_metadata_fd,
        walk_directory_components,
    )
except ModuleNotFoundError:  # Direct ``python3 scripts/check_no_unsafe.py``.
    from bounded_subprocess import BoundedProcessError, run_bounded
    from no_unsafe_fs import (
        O_CLOEXEC,
        O_NOFOLLOW,
        O_PATH,
        REAL_FILE_OPS,
        FileOps,
        HeldDirectory,
        PolicyFailure,
        close_chain,
        close_descriptor,
        directory_tuple,
        full_tuple,
        metadata_observe_at,
        open_directory_at,
        open_root_directory,
        read_held_regular,
        read_regular_at,
        upgrade_metadata_fd,
        walk_directory_components,
    )

EXPECTED_BASENAME = "check_no_unsafe.py"
MANIFEST_REL = "scripts/no_unsafe_external_cache_manifest_v1.json"
MANIFEST_SEMANTIC_SHA256 = "2a00fe1b2b098368aacfaf525a89171d2543a80f5cea114706d6312f5af80d0b"
MANIFEST_LIMIT = 65_536
ROOT_MANIFEST_REL = "rust/Cargo.toml"
CANDIDATE_HOMES = (
    "build/engineering-quality/release-roadmap/complete-kani-with-verified-path-20260908T044738Z/tools/kani-home/kani-0.67.0",
    "build/engineering-quality/release-roadmap/recover-kani-and-prepare-candidate-20260908T042334Z/tools/kani-home/kani-0.67.0",
)
EXCLUDED_PARTS = {
    ".codebase-index",
    ".git",
    ".lake",
    ".pi-subagents",
    "node_modules",
    "target",
}
MAX_REPOSITORY_DIRECTORIES = 32_768
MAX_REPOSITORY_ENTRIES = 262_144
MAX_REPOSITORY_DEPTH = 64
MAX_RUST_FILES = 8_192
MAX_SOURCE_BYTES = 256 * 1024 * 1024
MAX_SOURCE_FILE_BYTES = 4 * 1024 * 1024
MAX_CANDIDATE_DIRECTORIES = 32
MAX_CANDIDATE_FILES = 64
MAX_CANDIDATE_ENTRIES = 96
MAX_CANDIDATE_FILE_BYTES = 131_072
MAX_CANDIDATE_BYTES = 1024 * 1024
MAX_ALL_CANDIDATE_BYTES = 8 * 1024 * 1024
MAX_CARGO_MANIFESTS = 512
MAX_CARGO_MANIFEST_BYTES = 1024 * 1024
MAX_ALL_CARGO_MANIFEST_BYTES = 64 * 1024 * 1024
# linux_dirent64 is 19 header bytes, a 255-byte name, NUL, then 8-byte alignment.
# Dot records are smaller in practice, but charging the maximum record size for
# both `.` and `..` in every reachable directory gives a simple finite bound.
MAX_DIRENT64_BYTES = 280


def _directory_byte_budget(entries: int, directories: int) -> int:
    if entries < 0 or directories < 1:
        raise ValueError("directory budget inputs must include the root")
    return (entries + 2 * directories) * MAX_DIRENT64_BYTES


def _candidate_directory_byte_budget(entries: int, descendant_directories: int) -> int:
    if entries < 0 or descendant_directories < 0:
        raise ValueError("candidate directory budget inputs must be nonnegative")
    # The candidate directory counter excludes the enumerated library root.
    return (entries + 2 * (descendant_directories + 1)) * MAX_DIRENT64_BYTES


MAX_REPOSITORY_DIRECTORY_BYTES = _directory_byte_budget(
    MAX_REPOSITORY_ENTRIES, MAX_REPOSITORY_DIRECTORIES
)
MAX_CANDIDATE_DIRECTORY_BYTES = _candidate_directory_byte_budget(
    MAX_CANDIDATE_ENTRIES, MAX_CANDIDATE_DIRECTORIES
)


@dataclass(frozen=True)
class BoundRepository:
    path: str
    directory: HeldDirectory

    def close(
        self,
        ops: FileOps = REAL_FILE_OPS,
        code: str = "E_FS_CLOSE",
    ) -> None:
        self.directory.close(ops, code)


@dataclass(frozen=True)
class Diagnostic:
    code: str
    path: str = ""
    line: int = 0
    detail: str = ""


@dataclass
class ScanCounters:
    directories: int = 0
    entries: int = 0
    rust_files: int = 0
    rust_bytes: int = 0
    lexically_scanned_rust: int = 0
    external_rust: int = 0
    candidate_bytes: int = 0


@dataclass
class CandidateResult:
    home: str
    exact: bool
    diagnostics: list[Diagnostic]
    rust_findings: list[Diagnostic]
    rust_count: int
    candidate_bytes: int
    rust_bytes: int


@dataclass
class CargoView:
    package_count: int
    manifest_count: int
    manifest_bytes: int
    package_roots: tuple[str, ...]
    target_paths: tuple[str, ...]


@dataclass
class GitView:
    head: bytes
    index_snapshot: tuple[tuple[int, int, int, int, int, int], str]
    index_query: bytes
    tree_query: bytes
    owned_paths: set[str] = field(default_factory=set)


# Keep the lexical scanner independent and importable by its focused tests.
def _skip_quoted(text: str, start: int, quote: str) -> int:
    index = start + 1
    while index < len(text):
        if text[index] == "\\":
            index += 2
        elif text[index] == quote:
            return index + 1
        else:
            index += 1
    return len(text)


def _char_literal_end(text: str, start: int) -> int | None:
    index = start + 1
    if index >= len(text) or text[index] == "\n":
        return None
    if text[index] != "\\":
        index += 1
    elif text.startswith("\\x", index):
        index += 4
    elif text.startswith("\\u{", index):
        end = text.find("}", index + 3)
        if end < 0:
            return None
        index = end + 1
    else:
        index += 2
    if index < len(text) and text[index] == "'":
        return index + 1
    return None


def _raw_string_end(text: str, start: int) -> int | None:
    for prefix in ("br", "cr", "r"):
        if not text.startswith(prefix, start):
            continue
        index = start + len(prefix)
        hashes = 0
        while index < len(text) and text[index] == "#":
            hashes += 1
            index += 1
        if index >= len(text) or text[index] != '"':
            continue
        terminator = '"' + ("#" * hashes)
        end = text.find(terminator, index + 1)
        return len(text) if end < 0 else end + len(terminator)
    return None


def unsafe_keyword_lines(text: str) -> list[int]:
    """Return lines containing Rust's ``unsafe`` keyword outside literals/comments."""
    lines: list[int] = []
    index = 0
    line = 1
    block_depth = 0
    while index < len(text):
        if block_depth:
            if text.startswith("/*", index):
                block_depth += 1
                index += 2
            elif text.startswith("*/", index):
                block_depth -= 1
                index += 2
            else:
                if text[index] == "\n":
                    line += 1
                index += 1
            continue
        if text.startswith("//", index):
            end = text.find("\n", index + 2)
            if end < 0:
                break
            line += 1
            index = end + 1
            continue
        if text.startswith("/*", index):
            block_depth = 1
            index += 2
            continue
        raw_end = _raw_string_end(text, index)
        if raw_end is not None:
            line += text.count("\n", index, raw_end)
            index = raw_end
            continue
        char = text[index]
        if char == '"':
            end = _skip_quoted(text, index, '"')
            line += text.count("\n", index, end)
            index = end
            continue
        if char == "'":
            char_end = _char_literal_end(text, index)
            if char_end is not None:
                index = char_end
                continue
        if char == "_" or char.isalpha():
            end = index + 1
            while end < len(text) and (text[end] == "_" or text[end].isalnum()):
                end += 1
            token = text[index:end]
            if token == "unsafe" and not (index >= 2 and text[index - 2 : index] == "r#"):
                lines.append(line)
            index = end
            continue
        if char == "\n":
            line += 1
        index += 1
    return lines


def _self_test_scanner() -> None:
    harmless = '// unsafe\n/* unsafe /* unsafe */ */\n"unsafe" r#"unsafe"# r#unsafe \'u\''
    dangerous = "unsafe fn f() {}\nfn g() { unsafe { f(); } }\n"
    if unsafe_keyword_lines(harmless) or unsafe_keyword_lines(dangerous) != [1, 2]:
        raise PolicyFailure("E_UNSAFE", "lexical scanner positive/negative control failed")


def _ascii_components(value: str, *, absolute: bool) -> list[str]:
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError as error:
        raise PolicyFailure("E_ROOT_BINDING", "invocation path is not ASCII") from error
    if len(encoded) > 4096 or "\\" in value or "\x00" in value:
        raise PolicyFailure("E_ROOT_BINDING", "invocation path grammar rejected")
    if absolute != value.startswith("/") or (absolute and value.startswith("//")):
        raise PolicyFailure("E_ROOT_BINDING", "invocation path absolute form rejected")
    parts = value.split("/")[1:] if absolute else value.split("/")
    if not parts or any(part in ("", ".", "..") for part in parts):
        raise PolicyFailure("E_ROOT_BINDING", "invocation path components rejected")
    return parts


def bind_invocation(
    argv0: str,
    expected_basename: str,
    *,
    ops: FileOps = REAL_FILE_OPS,
    unsupported_family: str = "E_FS",
) -> BoundRepository:
    """Bind root from one stable no-follow invocation-name observation."""
    ops.require_supported(unsupported_family)
    close_code = f"{unsupported_family}_CLOSE"
    try:
        if argv0.startswith("/"):
            components = _ascii_components(argv0, absolute=True)
        else:
            relative = _ascii_components(argv0, absolute=False)
            cwd = os.getcwd()
            components = [*_ascii_components(cwd, absolute=True), *relative]
        if len(components) < 2 or components[-2:] != ["scripts", expected_basename]:
            raise PolicyFailure("E_ROOT_BINDING", "invocation suffix is not the fixed script path")
        directory_components = components[:-1]
        chain = [open_root_directory(family=unsupported_family, ops=ops)]
        script_fds: list[int] = []
        try:
            for component in directory_components:
                chain.append(
                    open_directory_at(
                        chain[-1].data_fd,
                        component,
                        family=unsupported_family,
                        ops=ops,
                    )
                )
            scripts_dir = chain[-1]
            first_fd, first = metadata_observe_at(
                scripts_dir.data_fd,
                expected_basename,
                family=unsupported_family,
                ops=ops,
            )
            script_fds.append(first_fd)
            second_fd, second = metadata_observe_at(
                scripts_dir.data_fd,
                expected_basename,
                family=unsupported_family,
                ops=ops,
            )
            script_fds.append(second_fd)
            if full_tuple(first) != full_tuple(second):
                raise PolicyFailure("E_ROOT_BINDING", "script name changed between observations")
            for fd in reversed(script_fds):
                close_descriptor(fd, family=unsupported_family, ops=ops)
            script_fds.clear()
            replay = [open_root_directory(family=unsupported_family, ops=ops)]
            try:
                for index, component in enumerate(directory_components, start=1):
                    replay.append(
                        open_directory_at(
                            replay[-1].data_fd,
                            component,
                            family=unsupported_family,
                            ops=ops,
                        )
                    )
                    if directory_tuple(replay[-1].metadata) != directory_tuple(chain[index].metadata):
                        raise PolicyFailure("E_ROOT_BINDING", "invocation parent chain changed")
            finally:
                close_chain(replay, family=unsupported_family, ops=ops)
            try:
                scripts_final = ops.fstat(scripts_dir.data_fd)
            except OSError as error:
                raise PolicyFailure(
                    "E_ROOT_BINDING", "final held scripts-directory observation failed"
                ) from error
            if directory_tuple(scripts_final) != directory_tuple(scripts_dir.metadata):
                raise PolicyFailure(
                    "E_ROOT_BINDING", "held scripts directory changed after parent replay"
                )
            root_index = len(components) - 2
            root_directory = chain[root_index]
            for index in reversed(range(len(chain))):
                if index == root_index:
                    continue
                chain[index].close(ops, close_code)
            root_path = "/" + "/".join(components[:root_index])
            return BoundRepository(root_path, root_directory)
        except Exception as error:
            primary = error if isinstance(error, PolicyFailure) else PolicyFailure(
                "E_ROOT_BINDING", str(error)
            )
            for fd in reversed(script_fds):
                try:
                    close_descriptor(fd, family=unsupported_family, ops=ops)
                except PolicyFailure:
                    primary = primary.with_additional(close_code)
            for item in reversed(chain):
                try:
                    item.close(ops, close_code)
                except PolicyFailure:
                    primary = primary.with_additional(close_code)
            raise primary from error
    except (OSError, PolicyFailure) as error:
        if isinstance(error, PolicyFailure) and error.code == "E_ROOT_BINDING":
            raise
        raise PolicyFailure("E_ROOT_BINDING", str(error)) from error


def _open_relative_directory(
    root_fd: int,
    components: list[str],
    *,
    family: str = "E_FS",
    ops: FileOps = REAL_FILE_OPS,
) -> list[HeldDirectory]:
    chain: list[HeldDirectory] = []
    parent = root_fd
    try:
        for component in components:
            item = open_directory_at(parent, component, family=family, ops=ops)
            chain.append(item)
            parent = item.data_fd
        return chain
    except Exception:
        close_chain(chain, family=family, ops=ops)
        raise


def _read_relative(
    bound: BoundRepository,
    relative: str,
    *,
    family: str,
    limit: int,
    ops: FileOps = REAL_FILE_OPS,
) -> bytes:
    components = relative.split("/")
    chain = _open_relative_directory(bound.directory.data_fd, components[:-1], family=family, ops=ops)
    parent = chain[-1].data_fd if chain else bound.directory.data_fd
    try:
        return read_regular_at(parent, components[-1], limit=limit, family=family, ops=ops).data
    finally:
        close_chain(chain, family=family, ops=ops)


def _duplicate_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise PolicyFailure("E_MANIFEST_DUPLICATE", f"duplicate JSON key: {key[:64]}")
        result[key] = value
    return result


def validate_closed_manifest(value: object, *, code: str) -> dict[str, Any]:
    """Validate the complete semantic manifest while permitting JSON formatting changes."""
    shapes = {
        "top": {"schema", "records"},
        "record": {"record_id", "source_owner", "ownership_class", "rationale", "tool", "distribution", "cache_home_paths", "library_relative_path", "directories", "files"},
        "tool": {"name", "version", "target"},
        "distribution": {"archive_name", "archive_bytes", "archive_sha256", "archive_root", "library_member_prefix", "original_bytes_definition"},
        "directory": {"path", "archive_member"},
        "file": {"path", "archive_member", "bytes", "sha256"},
    }
    try:
        if not isinstance(value, dict) or set(value) != shapes["top"]:
            raise ValueError("top-level fields")
        records = value["records"]
        if value["schema"] != "axiograph-no-unsafe-external-cache-manifest-v1" or not isinstance(records, list) or len(records) != 1:
            raise ValueError("schema or record count")
        record = records[0]
        if not isinstance(record, dict) or set(record) != shapes["record"]:
            raise ValueError("record fields")
        if not isinstance(record["tool"], dict) or set(record["tool"]) != shapes["tool"]:
            raise ValueError("tool fields")
        if not isinstance(record["distribution"], dict) or set(record["distribution"]) != shapes["distribution"]:
            raise ValueError("distribution fields")
        for key, fields in (("directories", shapes["directory"]), ("files", shapes["file"])):
            if not isinstance(record[key], list) or any(not isinstance(item, dict) or set(item) != fields for item in record[key]):
                raise ValueError(f"{key} fields")
        canonical = json.dumps(
            value,
            ensure_ascii=True,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("ascii")
    except (KeyError, TypeError, ValueError, UnicodeEncodeError) as error:
        raise PolicyFailure(code, f"manifest closed schema rejected: {error}") from error
    if hashlib.sha256(canonical).hexdigest() != MANIFEST_SEMANTIC_SHA256:
        raise PolicyFailure(code, "manifest semantic values, array ordering, types, or bounds differ")
    return value


def load_manifest(bound: BoundRepository, *, ops: FileOps = REAL_FILE_OPS) -> dict[str, Any]:
    raw = _read_relative(bound, MANIFEST_REL, family="E_MANIFEST", limit=MANIFEST_LIMIT, ops=ops)
    try:
        text = raw.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise PolicyFailure("E_MANIFEST_UTF8", "manifest is not strict UTF-8") from error
    try:
        value = json.loads(
            text,
            object_pairs_hook=_duplicate_pairs,
            parse_constant=lambda token: (_ for _ in ()).throw(ValueError(token)),
        )
    except PolicyFailure:
        raise
    except (ValueError, RecursionError) as error:
        raise PolicyFailure("E_MANIFEST_JSON", f"manifest JSON rejected: {error}") from error
    return validate_closed_manifest(value, code="E_MANIFEST_SCHEMA")


def _lint_level(value: object) -> object:
    return value.get("level") if isinstance(value, dict) else value


def validate_root_manifest_before_metadata(
    bound: BoundRepository,
    *,
    ops: FileOps = REAL_FILE_OPS,
) -> None:
    raw = _read_relative(
        bound,
        ROOT_MANIFEST_REL,
        family="E_CARGO_MANIFEST",
        limit=MAX_CARGO_MANIFEST_BYTES,
        ops=ops,
    )
    try:
        parsed = tomllib.loads(raw.decode("utf-8", errors="strict"))
    except UnicodeDecodeError as error:
        raise PolicyFailure("E_CARGO_MANIFEST_UTF8", "root Cargo manifest is not UTF-8") from error
    except tomllib.TOMLDecodeError as error:
        raise PolicyFailure("E_CARGO_MANIFEST_TOML", "root Cargo manifest TOML rejected") from error
    unsafe_lint = parsed.get("workspace", {}).get("lints", {}).get("rust", {}).get("unsafe_code")
    if _lint_level(unsafe_lint) != "forbid":
        raise PolicyFailure("E_CARGO_MANIFEST_TOML", "root unsafe_code lint is not forbid")


def _cargo_metadata(bound: BoundRepository) -> dict[str, Any]:
    try:
        result = run_bounded(
            [
                "cargo",
                "metadata",
                "--manifest-path",
                f"{bound.path}/{ROOT_MANIFEST_REL}",
                "--format-version",
                "1",
                "--locked",
                "--no-deps",
            ],
            timeout_seconds=60,
            max_stdout_bytes=16 * 1024 * 1024,
            max_stderr_bytes=1024 * 1024,
            cwd=Path(bound.path),
        )
    except subprocess.TimeoutExpired as error:
        raise PolicyFailure("E_METADATA_TIMEOUT", "cargo metadata timed out") from error
    except BoundedProcessError as error:
        code = "E_METADATA_OUTPUT" if " exceeded " in str(error) else "E_METADATA_SPAWN"
        raise PolicyFailure(code, f"cargo metadata failed: {error}") from error
    except OSError as error:
        raise PolicyFailure("E_METADATA_SPAWN", f"cargo metadata failed: {error}") from error
    if result.returncode != 0:
        raise PolicyFailure("E_METADATA_EXIT", f"cargo metadata exit {result.returncode}")
    if result.stderr:
        raise PolicyFailure("E_METADATA_OUTPUT", "cargo metadata wrote stderr")
    try:
        return json.loads(result.stdout)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise PolicyFailure("E_METADATA_JSON", f"cargo metadata JSON rejected: {error}") from error


def _root_relative(bound: BoundRepository, absolute: object) -> str:
    if not isinstance(absolute, str):
        raise PolicyFailure("E_METADATA_PATH", "Cargo path is not a string")
    try:
        absolute.encode("ascii")
    except UnicodeEncodeError as error:
        raise PolicyFailure("E_METADATA_PATH", "Cargo path is not ASCII") from error
    prefix = bound.path + "/"
    if not absolute.startswith(prefix) or "//" in absolute or "\\" in absolute:
        raise PolicyFailure("E_METADATA_PATH", "Cargo path is outside the bound root")
    relative = absolute[len(prefix) :]
    if not relative or any(part in ("", ".", "..") for part in relative.split("/")):
        raise PolicyFailure("E_METADATA_PATH", "Cargo path grammar rejected")
    return relative


def cargo_view(bound: BoundRepository, *, ops: FileOps = REAL_FILE_OPS) -> CargoView:
    metadata = _cargo_metadata(bound)
    if not isinstance(metadata, dict):
        raise PolicyFailure("E_METADATA_SCHEMA", "Cargo metadata is not an object")
    packages = metadata.get("packages")
    members = metadata.get("workspace_members")
    if not isinstance(packages, list) or not isinstance(members, list):
        raise PolicyFailure("E_METADATA_SCHEMA", "Cargo package/member arrays are malformed")
    if len(packages) > 1024:
        raise PolicyFailure("E_METADATA_LIMIT", "Cargo package limit exceeded")
    if any(not isinstance(member, str) for member in members) or len(set(members)) != len(members):
        raise PolicyFailure("E_METADATA_SCHEMA", "Cargo workspace member IDs are malformed or duplicate")
    package_ids: set[str] = set()
    package_by_id: dict[str, dict[str, Any]] = {}
    for package in packages:
        if not isinstance(package, dict) or not isinstance(package.get("id"), str):
            raise PolicyFailure("E_METADATA_SCHEMA", "Cargo package record or ID is malformed")
        package_id = package["id"]
        if package_id in package_ids:
            raise PolicyFailure("E_METADATA_SCHEMA", "Cargo package ID is duplicate")
        package_ids.add(package_id)
        package_by_id[package_id] = package
    if any(member not in package_by_id for member in members):
        raise PolicyFailure("E_METADATA_SCHEMA", "workspace member package is absent")
    selected = [package_by_id[member] for member in members]

    manifest_list = [ROOT_MANIFEST_REL]
    package_roots: list[str] = []
    target_paths: list[str] = []
    target_count = 0
    for package in selected:
        manifest_rel = _root_relative(bound, package.get("manifest_path"))
        manifest_list.append(manifest_rel)
        package_roots.append(manifest_rel.rsplit("/", 1)[0] if "/" in manifest_rel else "")
        targets = package.get("targets")
        if not isinstance(targets, list):
            raise PolicyFailure("E_METADATA_SCHEMA", "Cargo targets are not an array")
        target_count += len(targets)
        if target_count > 8192:
            raise PolicyFailure("E_METADATA_LIMIT", "Cargo target limit exceeded")
        for target in targets:
            if not isinstance(target, dict):
                raise PolicyFailure("E_METADATA_SCHEMA", "Cargo target is not an object")
            target_paths.append(_root_relative(bound, target.get("src_path")))
    if len(set(manifest_list)) != len(manifest_list):
        # The root path may also be reported by a package; retain one exact root
        # occurrence but reject all other duplicate metadata manifest paths.
        if manifest_list.count(ROOT_MANIFEST_REL) != 2 or len(set(manifest_list)) != len(manifest_list) - 1:
            raise PolicyFailure("E_METADATA_SCHEMA", "Cargo manifest path is duplicate")
        manifest_list.remove(ROOT_MANIFEST_REL)
    if len(set(target_paths)) != len(target_paths):
        raise PolicyFailure("E_METADATA_SCHEMA", "Cargo target source path is duplicate")
    manifest_paths = sorted(set(manifest_list))
    if len(manifest_paths) > MAX_CARGO_MANIFESTS:
        raise PolicyFailure("E_CARGO_MANIFEST_COUNT", "Cargo manifest count exceeds 512")

    # Classify every distinct path and charge all declared bytes before any
    # manifest buffer is allocated or read.
    declared: dict[str, os.stat_result] = {}
    inode_paths: dict[tuple[int, int], str] = {}
    declared_aggregate = 0
    for relative in manifest_paths:
        parts = relative.split("/")
        chain = _open_relative_directory(
            bound.directory.data_fd, parts[:-1], family="E_CARGO_MANIFEST", ops=ops
        )
        parent = chain[-1].data_fd if chain else bound.directory.data_fd
        metadata_fd: int | None = None
        try:
            metadata_fd, metadata_value = metadata_observe_at(
                parent, parts[-1], family="E_CARGO_MANIFEST", ops=ops
            )
            if metadata_value.st_size > MAX_CARGO_MANIFEST_BYTES:
                raise PolicyFailure("E_CARGO_MANIFEST_SIZE", f"Cargo manifest exceeds 1 MiB: {relative}")
            key = (metadata_value.st_dev, metadata_value.st_ino)
            if key in inode_paths and inode_paths[key] != relative:
                raise PolicyFailure("E_CARGO_MANIFEST_PATH", "two Cargo paths identify one inode")
            inode_paths[key] = relative
            declared_aggregate += metadata_value.st_size
            if declared_aggregate > MAX_ALL_CARGO_MANIFEST_BYTES:
                raise PolicyFailure("E_CARGO_MANIFEST_AGGREGATE", "Cargo manifest aggregate exceeds 64 MiB")
            declared[relative] = metadata_value
        finally:
            try:
                if metadata_fd is not None:
                    close_descriptor(metadata_fd, family="E_CARGO_MANIFEST", ops=ops)
            finally:
                close_chain(chain, family="E_CARGO_MANIFEST", ops=ops)

    # Reopen and retain every actual manifest inode. Authorize the complete
    # aggregate over these held read authorities before allocating or reading
    # any manifest content. The earlier declaration cannot authorize a later
    # pathname replacement.
    held_manifests: list[tuple[str, int, os.stat_result, tuple[tuple[int, int, int, int, int], ...]]] = []
    actual_aggregate = 0
    actual_inode_paths: dict[tuple[int, int], str] = {}
    try:
        for relative in manifest_paths:
            parts = relative.split("/")
            chain = _open_relative_directory(
                bound.directory.data_fd, parts[:-1], family="E_CARGO_MANIFEST", ops=ops
            )
            parent = chain[-1].data_fd if chain else bound.directory.data_fd
            metadata_fd: int | None = None
            try:
                metadata_fd, metadata_value = metadata_observe_at(
                    parent, parts[-1], family="E_CARGO_MANIFEST", ops=ops
                )
                if full_tuple(metadata_value) != full_tuple(declared[relative]):
                    raise PolicyFailure(
                        "E_CARGO_MANIFEST_RACE",
                        f"Cargo manifest changed before held-inode authorization: {relative}",
                    )
                if metadata_value.st_size > MAX_CARGO_MANIFEST_BYTES:
                    raise PolicyFailure("E_CARGO_MANIFEST_SIZE", f"Cargo manifest exceeds 1 MiB: {relative}")
                key = (metadata_value.st_dev, metadata_value.st_ino)
                if key in actual_inode_paths and actual_inode_paths[key] != relative:
                    raise PolicyFailure("E_CARGO_MANIFEST_PATH", "two Cargo paths identify one read inode")
                actual_inode_paths[key] = relative
                actual_aggregate += metadata_value.st_size
                if actual_aggregate > MAX_ALL_CARGO_MANIFEST_BYTES:
                    raise PolicyFailure(
                        "E_CARGO_MANIFEST_AGGREGATE",
                        "actual held Cargo manifest aggregate exceeds 64 MiB",
                    )
                held_manifests.append(
                    (
                        relative,
                        metadata_fd,
                        metadata_value,
                        tuple(directory_tuple(item.metadata) for item in chain),
                    )
                )
                metadata_fd = None
            finally:
                if metadata_fd is not None:
                    close_descriptor(metadata_fd, family="E_CARGO_MANIFEST", ops=ops)
                close_chain(chain, family="E_CARGO_MANIFEST", ops=ops)

        aggregate = 0
        for relative, metadata_fd, metadata_value, authorized_parents in held_manifests:
            parts = relative.split("/")
            chain = _open_relative_directory(
                bound.directory.data_fd, parts[:-1], family="E_CARGO_MANIFEST", ops=ops
            )
            parent = chain[-1].data_fd if chain else bound.directory.data_fd
            before_parents = tuple(directory_tuple(item.metadata) for item in chain)
            try:
                if before_parents != authorized_parents:
                    raise PolicyFailure("E_CARGO_MANIFEST_RACE", f"Cargo parent changed before read: {relative}")
                stable = read_held_regular(
                    metadata_fd,
                    metadata_value,
                    parts[-1],
                    limit=MAX_CARGO_MANIFEST_BYTES,
                    family="E_CARGO_MANIFEST",
                    ops=ops,
                )
                aggregate += len(stable.data)
                if aggregate > actual_aggregate:
                    raise PolicyFailure(
                        "E_CARGO_MANIFEST_RACE",
                        "Cargo manifest bytes exceed the authorized held aggregate",
                    )
                try:
                    text = stable.data.decode("utf-8", errors="strict")
                except UnicodeDecodeError as error:
                    raise PolicyFailure("E_CARGO_MANIFEST_UTF8", f"invalid UTF-8: {relative}") from error
                try:
                    parsed = tomllib.loads(text)
                except tomllib.TOMLDecodeError as error:
                    raise PolicyFailure("E_CARGO_MANIFEST_TOML", f"invalid TOML: {relative}") from error
                if not isinstance(parsed, dict):
                    raise PolicyFailure("E_CARGO_MANIFEST_TOML", f"top-level TOML is not a table: {relative}")
                observation_fd, observation = metadata_observe_at(
                    parent, parts[-1], family="E_CARGO_MANIFEST", ops=ops
                )
                try:
                    if full_tuple(observation) != full_tuple(stable.metadata):
                        raise PolicyFailure("E_CARGO_MANIFEST_RACE", f"Cargo manifest name changed: {relative}")
                finally:
                    close_descriptor(observation_fd, family="E_CARGO_MANIFEST", ops=ops)
                try:
                    parent_after = tuple(directory_tuple(ops.fstat(item.data_fd)) for item in chain)
                except OSError as error:
                    raise PolicyFailure("E_CARGO_MANIFEST_RACE", f"Cargo parent metadata failed: {relative}") from error
                if parent_after != before_parents:
                    raise PolicyFailure("E_CARGO_MANIFEST_RACE", f"Cargo parent changed: {relative}")
                replay = _open_relative_directory(
                    bound.directory.data_fd, parts[:-1], family="E_CARGO_MANIFEST", ops=ops
                )
                try:
                    if tuple(directory_tuple(item.metadata) for item in replay) != before_parents:
                        raise PolicyFailure("E_CARGO_MANIFEST_RACE", f"Cargo parent name changed: {relative}")
                finally:
                    close_chain(replay, family="E_CARGO_MANIFEST", ops=ops)
                if relative == ROOT_MANIFEST_REL:
                    unsafe_lint = parsed.get("workspace", {}).get("lints", {}).get("rust", {}).get("unsafe_code")
                    if _lint_level(unsafe_lint) != "forbid":
                        raise PolicyFailure("E_CARGO_MANIFEST_TOML", "root unsafe_code lint is not forbid")
                elif not parsed.get("lints", {}).get("workspace"):
                    raise PolicyFailure("E_CARGO_MANIFEST_TOML", f"workspace lint inheritance missing: {relative}")
            finally:
                close_chain(chain, family="E_CARGO_MANIFEST", ops=ops)
    finally:
        primary = sys.exc_info()[1]
        close_failure = primary if isinstance(primary, PolicyFailure) else None
        for _relative, metadata_fd, _metadata, _parents in reversed(held_manifests):
            try:
                close_descriptor(metadata_fd, family="E_CARGO_MANIFEST", ops=ops)
            except PolicyFailure:
                if close_failure is None:
                    raise
                close_failure = close_failure.with_additional("E_CARGO_MANIFEST_CLOSE")
        if close_failure is not None and close_failure is not primary:
            raise close_failure
    return CargoView(
        len(selected),
        len(manifest_paths),
        aggregate,
        tuple(sorted(set(package_roots))),
        tuple(sorted(target_paths)),
    )


def _git_env() -> dict[str, str]:
    return {
        "PATH": os.environ.get("PATH", os.defpath),
        "LC_ALL": "C",
        "LANG": "C",
        "GIT_OPTIONAL_LOCKS": "0",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_COUNT": "0",
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_NO_LAZY_FETCH": "1",
        "GIT_LITERAL_PATHSPECS": "1",
    }


def _git(bound: BoundRepository, suffix: list[str], *, max_stdout: int = 1024 * 1024) -> Any:
    argv = ["git", "--no-pager", "--no-replace-objects", f"--git-dir={bound.path}/.git", f"--work-tree={bound.path}", *suffix]
    try:
        return run_bounded(argv, timeout_seconds=10, max_stdout_bytes=max_stdout, max_stderr_bytes=256 * 1024, cwd=Path(bound.path), env=_git_env())
    except subprocess.TimeoutExpired as error:
        raise PolicyFailure("E_GIT_TIMEOUT", "Git command timed out") from error
    except BoundedProcessError as error:
        code = "E_GIT_OUTPUT" if " exceeded " in str(error) else "E_GIT_SPAWN"
        raise PolicyFailure(code, f"Git command failed: {error}") from error
    except OSError as error:
        raise PolicyFailure("E_GIT_SPAWN", f"Git command failed: {error}") from error


def _git_exact(
    bound: BoundRepository,
    suffix: list[str],
    expected: bytes,
    *,
    identity: HeldDirectory | None = None,
    ops: FileOps = REAL_FILE_OPS,
) -> None:
    result = _git(bound, suffix)
    if result.returncode != 0:
        raise PolicyFailure("E_GIT_EXIT", f"Git command exited {result.returncode}")
    if result.stderr or result.stdout != expected:
        raise PolicyFailure("E_GIT_BINDING", f"Git binding output mismatch for {suffix[-1]}")
    if identity is None:
        return
    try:
        reported = result.stdout[:-1].decode("ascii", errors="strict")
        components = _ascii_components(reported, absolute=True)
        reopened = walk_directory_components(components, family="E_FS", ops=ops)
    except (PolicyFailure, UnicodeDecodeError, ValueError) as error:
        additional = error.additional if isinstance(error, PolicyFailure) else ()
        raise PolicyFailure(
            "E_GIT_BINDING",
            f"cannot reopen reported Git path for {suffix[-1]}",
            additional=additional,
        ) from error
    primary: PolicyFailure | None = None
    try:
        if full_tuple(reopened[-1].metadata) != full_tuple(identity.metadata):
            raise PolicyFailure(
                "E_GIT_BINDING",
                f"reported Git path identity mismatch for {suffix[-1]}",
            )
    except PolicyFailure as error:
        primary = error
    finally:
        try:
            close_chain(reopened, family="E_FS", ops=ops)
        except PolicyFailure as error:
            if primary is None:
                primary = PolicyFailure(
                    "E_GIT_BINDING",
                    f"reported Git path close failed for {suffix[-1]}",
                    additional=(error.code, *error.additional),
                )
            else:
                primary = primary.with_additional(error.code)
                for code in error.additional:
                    primary = primary.with_additional(code)
    if primary is not None:
        raise primary


def _index_snapshot(bound: BoundRepository, git_dir: HeldDirectory, *, ops: FileOps = REAL_FILE_OPS) -> tuple[tuple[int, int, int, int, int, int], str]:
    stable = read_regular_at(git_dir.data_fd, "index", limit=64 * 1024 * 1024, family="E_GIT_INDEX", ops=ops)
    return full_tuple(stable.metadata), hashlib.sha256(stable.data).hexdigest()


def _candidate_pathspecs(manifest: dict[str, Any]) -> list[str]:
    values: set[str] = set()
    record = manifest["records"][0]
    for home in record["cache_home_paths"]:
        parts = home.split("/")
        for index in range(1, len(parts) + 1):
            values.add("/".join(parts[:index]))
        values.add(f"{home}/library")
        for item in [*record["directories"], *record["files"]]:
            values.add(f"{home}/library/{item['path']}")
    result = sorted(values, key=lambda value: value.encode("ascii"))
    if sum(len(value) + 1 for value in result) > 512 * 1024:
        raise PolicyFailure("E_GIT_PATH", "Git pathspec argv exceeds 512 KiB")
    return result


def _validate_git_output_path(raw_path: bytes, closure: set[str]) -> str:
    try:
        path = raw_path.decode("ascii")
    except UnicodeDecodeError as error:
        raise PolicyFailure("E_GIT_SCHEMA", "Git path is not ASCII") from error
    if not path or b"\n" in raw_path or b"\r" in raw_path or "\\" in path:
        raise PolicyFailure("E_GIT_SCHEMA", "Git path grammar rejected")
    parts = path.split("/")
    if path.startswith("/") or any(part in ("", ".", "..") for part in parts):
        raise PolicyFailure("E_GIT_SCHEMA", "Git path grammar rejected")
    # Proper candidate ancestors are exact closure members. Descendants are
    # allowed only below an exact declared home, never merely below `build`.
    if path not in closure and not any(
        path.startswith(home + "/") for home in CANDIDATE_HOMES
    ):
        raise PolicyFailure("E_GIT_SCHEMA", "Git path is outside candidate closure")
    return path


def _parse_index_paths(raw: bytes, closure: set[str]) -> set[str]:
    if raw and not raw.endswith(b"\0"):
        raise PolicyFailure("E_GIT_SCHEMA", "Git index output is not NUL terminated")
    paths: set[str] = set()
    records: set[bytes] = set()
    for record in raw.split(b"\0")[:-1]:
        if record in records:
            raise PolicyFailure("E_GIT_SCHEMA", "duplicate Git index record")
        records.add(record)
        try:
            header, path = record.split(b"\t", 1)
            mode, oid, stage = header.split(b" ")
        except ValueError as error:
            raise PolicyFailure("E_GIT_SCHEMA", "Git index record rejected") from error
        if mode not in (b"100644", b"100755", b"120000", b"160000") or not re.fullmatch(rb"[0-9a-f]{40}", oid) or stage not in (b"0", b"1", b"2", b"3"):
            raise PolicyFailure("E_GIT_SCHEMA", "Git index record grammar rejected")
        paths.add(_validate_git_output_path(path, closure))
    return paths


def _parse_tree_paths(raw: bytes, closure: set[str]) -> set[str]:
    if raw and not raw.endswith(b"\0"):
        raise PolicyFailure("E_GIT_SCHEMA", "Git tree output is not NUL terminated")
    paths: set[str] = set()
    records: set[bytes] = set()
    coherent = {
        (b"040000", b"tree"),
        (b"100644", b"blob"),
        (b"100755", b"blob"),
        (b"120000", b"blob"),
        (b"160000", b"commit"),
    }
    for record in raw.split(b"\0")[:-1]:
        if record in records:
            raise PolicyFailure("E_GIT_SCHEMA", "duplicate Git tree record")
        records.add(record)
        try:
            header, path = record.split(b"\t", 1)
            mode, kind, oid = header.split(b" ")
        except ValueError as error:
            raise PolicyFailure("E_GIT_SCHEMA", "Git tree record rejected") from error
        if (mode, kind) not in coherent or not re.fullmatch(rb"[0-9a-f]{40}", oid):
            raise PolicyFailure("E_GIT_SCHEMA", "Git tree mode/type grammar rejected")
        text = _validate_git_output_path(path, closure)
        if kind != b"tree":
            paths.add(text)
    return paths


def git_view_initial(bound: BoundRepository, manifest: dict[str, Any], *, ops: FileOps = REAL_FILE_OPS) -> tuple[GitView, HeldDirectory]:
    git_dir = open_directory_at(bound.directory.data_fd, ".git", family="E_FS", ops=ops)

    def require_absent(parent_fd: int, name: str) -> None:
        try:
            descriptor = ops.openat(parent_fd, name, O_PATH | O_CLOEXEC | O_NOFOLLOW)
        except OSError as error:
            if error.errno == errno.ENOENT:
                return
            raise PolicyFailure("E_GIT_BINDING", f"cannot prove absent Git control: {name}") from error
        try:
            raise PolicyFailure("E_GIT_BINDING", f"unsupported Git control exists: {name}")
        finally:
            close_descriptor(descriptor, family="E_GIT_INDEX", ops=ops)

    require_absent(git_dir.data_fd, "commondir")
    require_absent(git_dir.data_fd, "config.worktree")
    objects = open_directory_at(git_dir.data_fd, "objects", family="E_FS", ops=ops)
    try:
        info = open_directory_at(objects.data_fd, "info", family="E_FS", ops=ops)
        try:
            require_absent(info.data_fd, "alternates")
        finally:
            info.close(ops, "E_FS_CLOSE")
    finally:
        objects.close(ops, "E_FS_CLOSE")
    # Reject a linked or special default index before any Git child can touch
    # it. This metadata-only classification is not the I0 byte snapshot.
    index_metadata_fd, _index_metadata = metadata_observe_at(
        git_dir.data_fd,
        "index",
        family="E_GIT_INDEX",
        ops=ops,
    )
    close_descriptor(index_metadata_fd, family="E_GIT_INDEX", ops=ops)
    expected_root = bound.path.encode("ascii") + b"\n"
    expected_git = (bound.path + "/.git").encode("ascii") + b"\n"
    checks = [
        (
            ["rev-parse", "--path-format=absolute", "--show-toplevel"],
            expected_root,
            bound.directory,
        ),
        (
            ["rev-parse", "--path-format=absolute", "--absolute-git-dir"],
            expected_git,
            git_dir,
        ),
        (
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
            expected_git,
            git_dir,
        ),
        (["rev-parse", "--show-object-format=storage"], b"sha1\n", None),
        (["rev-parse", "--is-bare-repository"], b"false\n", None),
        (["rev-parse", "--is-inside-work-tree"], b"true\n", None),
        (
            ["rev-parse", "--path-format=absolute", "--git-path", "index"],
            (bound.path + "/.git/index").encode("ascii") + b"\n",
            None,
        ),
        (["rev-parse", "--shared-index-path"], b"", None),
    ]
    for suffix, expected, identity in checks:
        _git_exact(bound, suffix, expected, identity=identity, ops=ops)
    config_checks = [
        (["config", "--local", "--no-includes", "--null", "--get-all", "core.worktree"], 1, b""),
        (["config", "--local", "--no-includes", "--null", "--get-all", "extensions.worktreeConfig"], 1, b""),
        (["config", "--local", "--no-includes", "--null", "--get-regexp", r"^(include\.path|includeif\..*\.path)$"], 1, b""),
        (["config", "--local", "--no-includes", "--null", "--get-all", "core.bare"], 0, b"false\0"),
    ]
    for suffix, code, stdout in config_checks:
        result = _git(bound, suffix)
        if result.returncode != code or result.stdout != stdout or result.stderr:
            raise PolicyFailure("E_GIT_CONFIG", f"Git local configuration rejected: {suffix[-1]}")
    head_result = _git(bound, ["rev-parse", "--verify", "HEAD^{commit}"])
    if head_result.returncode or head_result.stderr or not re.fullmatch(rb"[0-9a-f]{40}\n", head_result.stdout):
        raise PolicyFailure("E_GIT_HEAD", "Git HEAD is not one exact SHA-1 commit")
    head = head_result.stdout[:-1]
    # The coherent ownership sequence begins exactly C0 -> I0 -> X0 -> I1 -> T0.
    i0 = _index_snapshot(bound, git_dir, ops=ops)
    pathspecs = _candidate_pathspecs(manifest)
    index_result = _git(bound, ["ls-files", "--cached", "--stage", "--deduplicate", "-z", "--", *pathspecs])
    if index_result.returncode or index_result.stderr:
        raise PolicyFailure("E_GIT_EXIT", "Git index query failed")
    i1 = _index_snapshot(bound, git_dir, ops=ops)
    if i1 != i0:
        raise PolicyFailure("E_GIT_RACE", "Git index changed across initial query")
    tree_result = _git(bound, ["ls-tree", "-r", "-t", "-z", "--full-tree", head.decode("ascii"), "--", *pathspecs])
    if tree_result.returncode or tree_result.stderr:
        raise PolicyFailure("E_GIT_EXIT", "Git tree query failed")
    closure = set(pathspecs)
    owned = _parse_index_paths(index_result.stdout, closure) | _parse_tree_paths(tree_result.stdout, closure)
    return GitView(head, i0, index_result.stdout, tree_result.stdout, owned), git_dir


def git_view_finalize(bound: BoundRepository, manifest: dict[str, Any], view: GitView, git_dir: HeldDirectory, *, ops: FileOps = REAL_FILE_OPS) -> None:
    pathspecs = _candidate_pathspecs(manifest)
    i2 = _index_snapshot(bound, git_dir, ops=ops)
    repeat_index = _git(bound, ["ls-files", "--cached", "--stage", "--deduplicate", "-z", "--", *pathspecs])
    i3 = _index_snapshot(bound, git_dir, ops=ops)
    c1 = _git(bound, ["rev-parse", "--verify", "HEAD^{commit}"])
    repeat_tree = _git(bound, ["ls-tree", "-r", "-t", "-z", "--full-tree", view.head.decode("ascii"), "--", *pathspecs])
    c2 = _git(bound, ["rev-parse", "--verify", "HEAD^{commit}"])
    i4 = _index_snapshot(bound, git_dir, ops=ops)
    if any(result.returncode or result.stderr for result in (repeat_index, c1, repeat_tree, c2)):
        raise PolicyFailure("E_GIT_RACE", "Git repeat observation failed")
    if not (i2 == i3 == i4 == view.index_snapshot and repeat_index.stdout == view.index_query and repeat_tree.stdout == view.tree_query and c1.stdout == view.head + b"\n" and c2.stdout == view.head + b"\n"):
        raise PolicyFailure("E_GIT_RACE", "Git ownership observations changed")


def _owned_home(home: str, paths: set[str]) -> bool:
    home_parts = home.split("/")
    ancestors = {"/".join(home_parts[:index]) for index in range(1, len(home_parts) + 1)}
    library = home + "/library"
    return any(path in ancestors or path == library or path.startswith(home + "/") for path in paths)


def _workspace_owned(home: str, cargo: CargoView) -> bool:
    return any(home == root or home.startswith(root + "/") or root.startswith(home + "/") for root in cargo.package_roots if root) or any(path == home or path.startswith(home + "/") for path in cargo.target_paths)


def _ascii_name(name: str, relative: str, *, max_bytes: int = 255) -> None:
    try:
        raw = name.encode("ascii")
    except UnicodeEncodeError as error:
        raise PolicyFailure("E_PATH", f"non-ASCII name below {relative[:256]}") from error
    if not raw or len(raw) > max_bytes or name in (".", "..") or "/" in name or "\\" in name:
        raise PolicyFailure("E_PATH", f"invalid component below {relative[:256]}")


def _observe_any(parent_fd: int, name: str, *, family: str, ops: FileOps) -> tuple[int, os.stat_result]:
    try:
        fd = ops.openat(parent_fd, name, O_PATH | O_CLOEXEC | O_NOFOLLOW)
    except OSError as error:
        code = f"{family}_LINK" if error.errno == errno.ELOOP else f"{family}_OPEN"
        raise PolicyFailure(code, f"metadata open failed: {name}") from error
    try:
        value = ops.fstat(fd)
    except OSError as error:
        try:
            raise PolicyFailure(f"{family}_METADATA", f"metadata fstat failed: {name}") from error
        finally:
            close_descriptor(fd, family=family, ops=ops)
    return fd, value


def _scan_rust_bytes(relative: str, raw: bytes) -> list[Diagnostic]:
    text = raw.decode("utf-8", errors="replace")
    return [Diagnostic("E_UNSAFE", relative, line, "Rust `unsafe` keyword is forbidden") for line in unsafe_keyword_lines(text)]


def scan_candidate(parent_fd: int, name: str, home: str, record: dict[str, Any], counters: ScanCounters, *, ops: FileOps = REAL_FILE_OPS) -> CandidateResult:
    diagnostics: list[Diagnostic] = []
    findings: list[Diagnostic] = []
    directory_paths: set[str] = set()
    files: dict[str, tuple[int, str]] = {}
    rust_count = 0
    candidate_bytes = 0
    rust_bytes = 0
    candidate_entries = 0
    home_dir = open_directory_at(parent_fd, name, family="E_FS", ops=ops)
    try:
        library = open_directory_at(home_dir.data_fd, "library", family="E_FS", ops=ops)
        try:
            def visit(directory: HeldDirectory, prefix: str, depth: int) -> None:
                nonlocal rust_count, candidate_bytes, rust_bytes, candidate_entries
                if depth > 16:
                    raise PolicyFailure("E_LIMIT_CANDIDATE_DEPTH", f"candidate depth exceeds 16: {home}")
                try:
                    before = directory_tuple(ops.fstat(directory.data_fd))
                except OSError as error:
                    raise PolicyFailure("E_FS_METADATA", f"candidate directory metadata failed: {home}") from error
                names: list[str] = []
                try:
                    for child in ops.iter_directory(
                        directory.data_fd,
                        max_bytes=_candidate_directory_byte_budget(
                            MAX_CANDIDATE_ENTRIES, MAX_CANDIDATE_DIRECTORIES
                        ),
                    ):
                        counters.entries += 1
                        candidate_entries += 1
                        if counters.entries > MAX_REPOSITORY_ENTRIES:
                            raise PolicyFailure("E_LIMIT_REPOSITORY_ENTRIES", "repository entry limit exceeded")
                        if candidate_entries > MAX_CANDIDATE_ENTRIES:
                            raise PolicyFailure("E_LIMIT_CANDIDATE_ENTRIES", f"candidate entry limit: {home}")
                        names.append(child)
                except OSError as error:
                    code = "E_LIMIT_CANDIDATE_ENTRIES" if error.errno == errno.EOVERFLOW else "E_FS_READ"
                    raise PolicyFailure(code, f"candidate directory enumeration failed: {home}") from error
                for child in sorted(names, key=lambda item: os.fsencode(item)):
                    _ascii_name(child, f"{home}/library/{prefix}", max_bytes=96)
                    relative = f"{prefix}/{child}" if prefix else child
                    metadata_fd, metadata = _observe_any(directory.data_fd, child, family="E_FS", ops=ops)
                    try:
                        if stat.S_ISLNK(metadata.st_mode):
                            raise PolicyFailure("E_FS_LINK", f"candidate link: {home}/library/{relative}")
                        if stat.S_ISDIR(metadata.st_mode):
                            directory_paths.add(relative)
                            counters.directories += 1
                            if counters.directories > MAX_REPOSITORY_DIRECTORIES:
                                raise PolicyFailure("E_LIMIT_REPOSITORY_DIRECTORIES", "repository directory limit exceeded")
                            if len(directory_paths) > MAX_CANDIDATE_DIRECTORIES:
                                raise PolicyFailure("E_LIMIT_CANDIDATE_DIRECTORIES", f"candidate directory limit: {home}")
                            child_dir_fd = upgrade_metadata_fd(metadata_fd, metadata, family="E_FS", ops=ops, expected="directory")
                            child_dir = HeldDirectory(metadata_fd, child_dir_fd, metadata)
                            metadata_fd = -1
                            try:
                                visit(child_dir, relative, depth + 1)
                            finally:
                                child_dir.close(ops, "E_FS_CLOSE")
                        elif stat.S_ISREG(metadata.st_mode):
                            if len(files) >= MAX_CANDIDATE_FILES:
                                raise PolicyFailure("E_LIMIT_CANDIDATE_FILES", f"candidate file limit: {home}")
                            is_rust = relative.endswith(".rs")

                            def charge_held_candidate(
                                value: os.stat_result, is_rust_file: bool = is_rust
                            ) -> None:
                                nonlocal candidate_bytes, rust_count, rust_bytes
                                declared_size = value.st_size
                                candidate_bytes += declared_size
                                counters.candidate_bytes += declared_size
                                if (
                                    candidate_bytes > MAX_CANDIDATE_BYTES
                                    or counters.candidate_bytes > MAX_ALL_CANDIDATE_BYTES
                                ):
                                    raise PolicyFailure(
                                        "E_LIMIT_CANDIDATE_BYTES",
                                        f"candidate byte limit: {home}",
                                    )
                                if is_rust_file:
                                    rust_count += 1
                                    rust_bytes += declared_size
                                    counters.rust_files += 1
                                    counters.rust_bytes += declared_size
                                    if (
                                        counters.rust_files > MAX_RUST_FILES
                                        or counters.rust_bytes > MAX_SOURCE_BYTES
                                    ):
                                        raise PolicyFailure(
                                            "E_LIMIT_REPOSITORY_RUST",
                                            "repository Rust limit exceeded",
                                        )

                            stable = read_held_regular(
                                metadata_fd,
                                metadata,
                                child,
                                limit=MAX_CANDIDATE_FILE_BYTES,
                                family="E_FS",
                                ops=ops,
                                before_read=charge_held_candidate,
                            )
                            files[relative] = (
                                len(stable.data),
                                hashlib.sha256(stable.data).hexdigest(),
                            )
                            if is_rust:
                                findings.extend(
                                    _scan_rust_bytes(
                                        f"{home}/library/{relative}", stable.data
                                    )
                                )
                        else:
                            raise PolicyFailure("E_FS_NONREGULAR", f"candidate special file: {home}/library/{relative}")
                    finally:
                        if metadata_fd >= 0:
                            close_descriptor(metadata_fd, family="E_FS", ops=ops)
                try:
                    after = directory_tuple(ops.fstat(directory.data_fd))
                except OSError as error:
                    raise PolicyFailure("E_FS_METADATA", f"candidate directory post-enumeration metadata failed: {home}") from error
                if after != before:
                    raise PolicyFailure("E_FS_RACE", f"candidate directory changed: {home}/library/{prefix}")
            visit(library, "", 0)
        finally:
            library.close(ops, "E_FS_CLOSE")
    finally:
        home_dir.close(ops, "E_FS_CLOSE")
    expected_dirs = {item["path"] for item in record["directories"]}
    expected_files = {item["path"]: (item["bytes"], item["sha256"]) for item in record["files"]}
    if directory_paths - expected_dirs or files.keys() - expected_files.keys():
        diagnostics.append(Diagnostic("E_CACHE_EXTRA", home, detail="candidate has undeclared entries"))
    if expected_dirs - directory_paths or expected_files.keys() - files.keys():
        diagnostics.append(Diagnostic("E_CACHE_MISSING", home, detail="candidate is incomplete"))
    for relative in sorted(files.keys() & expected_files.keys()):
        actual = files[relative]
        expected = expected_files[relative]
        if actual[0] != expected[0]:
            diagnostics.append(Diagnostic("E_CACHE_SIZE", f"{home}/library/{relative}", detail="candidate size differs"))
        elif actual[1] != expected[1]:
            diagnostics.append(Diagnostic("E_CACHE_HASH", f"{home}/library/{relative}", detail="candidate hash differs"))
    return CandidateResult(home, not diagnostics, diagnostics, findings, rust_count, candidate_bytes, rust_bytes)


def scan_sources(
    bound: BoundRepository,
    record: dict[str, Any],
    *,
    first_party_homes: set[str] | None = None,
    ops: FileOps = REAL_FILE_OPS,
) -> tuple[list[Diagnostic], list[CandidateResult], ScanCounters]:
    diagnostics: list[Diagnostic] = []
    candidates: list[CandidateResult] = []
    counters = ScanCounters()
    homes = set(record["cache_home_paths"])
    first_party_homes = set() if first_party_homes is None else first_party_homes
    home_prefixes = {
        "/".join(parts[:index])
        for home in homes
        for parts in [home.split("/")]
        for index in range(1, len(parts))
    }

    def visit(directory: HeldDirectory, prefix: str, depth: int) -> None:
        if depth > MAX_REPOSITORY_DEPTH:
            raise PolicyFailure("E_LIMIT_REPOSITORY_DEPTH", f"repository depth exceeds {MAX_REPOSITORY_DEPTH}")
        counters.directories += 1
        if counters.directories > MAX_REPOSITORY_DIRECTORIES:
            raise PolicyFailure("E_LIMIT_REPOSITORY_DIRECTORIES", "repository directory limit exceeded")
        try:
            before = directory_tuple(ops.fstat(directory.data_fd))
        except OSError as error:
            raise PolicyFailure("E_FS_METADATA", f"repository directory metadata failed: {prefix or '.'}") from error
        names: list[str] = []
        try:
            for name in ops.iter_directory(
                directory.data_fd,
                max_bytes=_directory_byte_budget(
                    MAX_REPOSITORY_ENTRIES, MAX_REPOSITORY_DIRECTORIES
                ),
            ):
                counters.entries += 1
                if counters.entries > MAX_REPOSITORY_ENTRIES:
                    raise PolicyFailure("E_LIMIT_REPOSITORY_ENTRIES", "repository entry limit exceeded")
                names.append(name)
        except OSError as error:
            code = "E_LIMIT_REPOSITORY_ENTRIES" if error.errno == errno.EOVERFLOW else "E_FS_READ"
            raise PolicyFailure(code, f"repository directory enumeration failed: {prefix or '.'}") from error
        for name in sorted(names, key=lambda item: os.fsencode(item)):
            _ascii_name(name, prefix)
            relative = f"{prefix}/{name}" if prefix else name
            if relative in homes:
                counters.directories += 2  # The candidate home and its library root.
                if counters.directories > MAX_REPOSITORY_DIRECTORIES:
                    raise PolicyFailure("E_LIMIT_REPOSITORY_DIRECTORIES", "repository directory limit exceeded")
                candidates.append(scan_candidate(directory.data_fd, name, relative, record, counters, ops=ops))
                continue
            candidate_prefix = relative in home_prefixes and not any(
                _owned_home(home, {relative}) or relative.startswith(home + "/")
                for home in first_party_homes
            )
            if name in EXCLUDED_PARTS and not candidate_prefix:
                continue
            is_source = name.endswith(".rs")
            source_family = "E_SOURCE" if is_source else "E_FS"
            metadata_fd, metadata = _observe_any(
                directory.data_fd, name, family=source_family, ops=ops
            )
            try:
                if stat.S_ISLNK(metadata.st_mode):
                    if candidate_prefix:
                        raise PolicyFailure("E_FS_LINK", f"candidate home ancestor is a link: {relative}")
                    if is_source:
                        diagnostics.append(Diagnostic("E_SOURCE_LINK", relative, detail="Rust source is a link"))
                    continue
                if candidate_prefix and not stat.S_ISDIR(metadata.st_mode):
                    raise PolicyFailure("E_FS_NONREGULAR", f"candidate home ancestor is not a directory: {relative}")
                if stat.S_ISDIR(metadata.st_mode):
                    if is_source:
                        diagnostics.append(
                            Diagnostic(
                                "E_SOURCE_NONREGULAR",
                                relative,
                                detail="Rust source is not regular",
                            )
                        )
                        continue
                    data_fd = upgrade_metadata_fd(metadata_fd, metadata, family="E_FS", ops=ops, expected="directory")
                    child = HeldDirectory(metadata_fd, data_fd, metadata)
                    metadata_fd = -1
                    try:
                        visit(child, relative, depth + 1)
                    finally:
                        child.close(ops, "E_FS_CLOSE")
                elif stat.S_ISREG(metadata.st_mode) and is_source:

                    def charge_held_source(value: os.stat_result) -> None:
                        counters.rust_files += 1
                        counters.rust_bytes += value.st_size
                        if (
                            counters.rust_files > MAX_RUST_FILES
                            or counters.rust_bytes > MAX_SOURCE_BYTES
                        ):
                            raise PolicyFailure(
                                "E_LIMIT_REPOSITORY_RUST",
                                "repository Rust limit exceeded",
                            )

                    stable = read_held_regular(
                        metadata_fd,
                        metadata,
                        name,
                        limit=MAX_SOURCE_FILE_BYTES,
                        family="E_SOURCE",
                        ops=ops,
                        before_read=charge_held_source,
                    )
                    counters.lexically_scanned_rust += 1
                    diagnostics.extend(_scan_rust_bytes(relative, stable.data))
                elif is_source:
                    diagnostics.append(Diagnostic("E_SOURCE_NONREGULAR", relative, detail="Rust source is not regular"))
            finally:
                if metadata_fd >= 0:
                    close_descriptor(metadata_fd, family=source_family, ops=ops)
        try:
            after = directory_tuple(ops.fstat(directory.data_fd))
        except OSError as error:
            raise PolicyFailure("E_FS_METADATA", f"repository directory post-enumeration metadata failed: {prefix or '.'}") from error
        if after != before:
            raise PolicyFailure("E_FS_RACE", f"repository directory changed: {prefix or '.'}")

    visit(bound.directory, "", 0)
    return diagnostics, candidates, counters


def audit(argv0: str, *, ops: FileOps = REAL_FILE_OPS) -> tuple[list[Diagnostic], dict[str, Any]]:
    _self_test_scanner()
    bound = bind_invocation(argv0, EXPECTED_BASENAME, ops=ops)
    git_dir: HeldDirectory | None = None
    try:
        validate_root_manifest_before_metadata(bound, ops=ops)
        manifest = load_manifest(bound, ops=ops)
        record = manifest["records"][0]
        cargo = cargo_view(bound, ops=ops)
        git, git_dir = git_view_initial(bound, manifest, ops=ops)
        first_party_homes = {
            home
            for home in record["cache_home_paths"]
            if _owned_home(home, git.owned_paths) or _workspace_owned(home, cargo)
        }
        diagnostics, candidates, counters = scan_sources(
            bound,
            record,
            first_party_homes=first_party_homes,
            ops=ops,
        )
        git_view_finalize(bound, manifest, git, git_dir, ops=ops)
        active: list[str] = []
        observed_homes = {candidate.home for candidate in candidates}
        for home in sorted(first_party_homes - observed_homes):
            code = "E_TRACKED_OVERRIDE" if _owned_home(home, git.owned_paths) else "E_WORKSPACE_OVERRIDE"
            diagnostics.append(Diagnostic(code, home, detail="candidate is first-party"))
        for candidate in candidates:
            active.append(candidate.home)
            first_party = _owned_home(candidate.home, git.owned_paths) or _workspace_owned(candidate.home, cargo)
            if first_party:
                code = "E_TRACKED_OVERRIDE" if _owned_home(candidate.home, git.owned_paths) else "E_WORKSPACE_OVERRIDE"
                diagnostics.append(Diagnostic(code, candidate.home, detail="candidate is first-party"))
            if candidate.exact and not first_party:
                counters.external_rust += candidate.rust_count
            else:
                diagnostics.extend(candidate.diagnostics)
                diagnostics.extend(candidate.rust_findings)
                counters.lexically_scanned_rust += candidate.rust_count
        report = {
            "schema": "axiograph-no-unsafe-report-v1",
            "bound_root": bound.path,
            "git_dir": bound.path + "/.git",
            "workspace_packages": cargo.package_count,
            "cargo_manifests": cargo.manifest_count,
            "cargo_manifest_bytes": cargo.manifest_bytes,
            "discovered_rust": counters.rust_files,
            "rust_bytes": counters.rust_bytes,
            "lexically_scanned_rust": counters.lexically_scanned_rust,
            "external_cache_rust": counters.external_rust,
            "active_homes": sorted(active),
            "coverage_complete": True,
            "classification": "ownership-exempt external cache bytes",
        }
        diagnostics = sorted(set(diagnostics), key=lambda item: (item.code.encode("ascii"), item.path.encode("utf-8", errors="replace"), item.line, item.detail.encode("ascii", errors="replace")))
        return diagnostics, report
    finally:
        try:
            if git_dir is not None:
                git_dir.close(ops, "E_GIT_INDEX_CLOSE")
        finally:
            bound.close(ops)


def main() -> int:
    try:
        diagnostics, report = audit(sys.argv[0])
    except PolicyFailure as error:
        print(f"[{error.code}] {error.detail}", file=sys.stderr)
        for code in error.additional:
            print(f"[{code}] additional descriptor-close failure", file=sys.stderr)
        print("coverage_complete=false; exemptions=0", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"[E_FS_UNSUPPORTED] unclassified required filesystem operation failed: {error}", file=sys.stderr)
        print("coverage_complete=false; exemptions=0", file=sys.stderr)
        return 1
    if diagnostics:
        print("first-party unsafe-code audit failed:", file=sys.stderr)
        for item in diagnostics:
            location = item.path + (f":{item.line}" if item.line else "")
            print(f"[{item.code}] {location}: {item.detail}", file=sys.stderr)
        print(json.dumps({**report, "coverage_complete": True}, sort_keys=True), file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
