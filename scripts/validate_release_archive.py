#!/usr/bin/env python3
"""Validate and optionally extract one strict Axiograph release bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import struct
import sys
import tarfile
import tempfile
import zipfile
import zlib
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import BinaryIO

try:
    from .build_release_bundle import (
        CHECKER_BUILD_ID,
        CHECKER_PROTOCOL,
        HOSTS,
        SCHEMA,
        SEMANTIC_SCOPE,
        canonical_json,
    )
    from .generate_release_source_manifest import (
        SourceManifestError,
        validate_source_manifest_bytes,
    )
    from .release_version import CalVerError, parse_calver
except ImportError:
    from build_release_bundle import (
        CHECKER_BUILD_ID,
        CHECKER_PROTOCOL,
        HOSTS,
        SCHEMA,
        SEMANTIC_SCOPE,
        canonical_json,
    )
    from generate_release_source_manifest import (
        SourceManifestError,
        validate_source_manifest_bytes,
    )
    from release_version import CalVerError, parse_calver

MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_TOTAL_BYTES = 512 * 1024 * 1024
MAX_EXECUTABLE_BYTES = 256 * 1024 * 1024
MAX_MANIFEST_BYTES = 16 * 1024 * 1024
MAX_CHECKSUM_BYTES = 1024
READ_CHUNK = 1024 * 1024
EXPECTED_ENTRY_COUNT = 5
MAX_JSON_DEPTH = 64
MAX_TAR_STREAM_BYTES = MAX_TOTAL_BYTES + 1024 * 1024
HEX64 = re.compile(r"[0-9a-f]{64}")
EXACT_TOOLCHAIN = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")


class ArchiveValidationError(ValueError):
    pass


def fail(message: str) -> ArchiveValidationError:
    return ArchiveValidationError(message)


@dataclass(frozen=True)
class ArchivedFile:
    name: str
    mode: int
    data: bytes


def validate_name(name: str) -> None:
    if not name or "\\" in name or "\x00" in name:
        raise fail(f"invalid archive path {name!r}")
    path = PurePosixPath(name)
    if path.is_absolute() or len(path.parts) != 1 or path.parts[0] in {".", ".."}:
        raise fail(f"archive path must be one top-level filename: {name!r}")
    if re.fullmatch(r"[A-Za-z0-9_.-]+", name) is None:
        raise fail(f"archive filename is not canonical ASCII: {name!r}")


def expected_files(executable_suffix: str) -> dict[str, tuple[int, int]]:
    if executable_suffix not in {"", ".exe"}:
        raise fail("executable suffix must be empty or .exe")
    return {
        f"axiograph{executable_suffix}": (0o755, MAX_EXECUTABLE_BYTES),
        f"axiograph_verify{executable_suffix}": (0o755, MAX_EXECUTABLE_BYTES),
        "axiograph_verify.sha256": (0o644, MAX_CHECKSUM_BYTES),
        "axiograph-source-manifest.json": (0o644, MAX_MANIFEST_BYTES),
        "axiograph-release.json": (0o644, MAX_MANIFEST_BYTES),
    }


@contextmanager
def open_regular_archive(path: Path) -> Iterator[tuple[BinaryIO, int]]:
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or path.is_symlink():
        raise fail("release archive must be a regular file, not a symlink")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode):
            raise fail("opened release archive is not a regular file")
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise fail("release archive changed while opening")
        if opened.st_size <= 0 or opened.st_size > MAX_ARCHIVE_BYTES:
            raise fail(f"release archive size must be in 1..={MAX_ARCHIVE_BYTES} bytes")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            yield stream, opened.st_size
    finally:
        os.close(descriptor)


def validate_json_nesting(raw: bytes, label: str) -> None:
    stack: list[int] = []
    in_string = False
    escaped = False
    for byte in raw:
        if in_string:
            if escaped:
                escaped = False
            elif byte == 0x5C:
                escaped = True
            elif byte == 0x22:
                in_string = False
            continue
        if byte == 0x22:
            in_string = True
        elif byte in (0x7B, 0x5B):
            stack.append(byte)
            if len(stack) > MAX_JSON_DEPTH:
                raise fail(f"{label} JSON nesting exceeds {MAX_JSON_DEPTH}")
        elif byte == 0x7D:
            if not stack or stack.pop() != 0x7B:
                raise fail(f"{label} JSON containers are unbalanced")
        elif byte == 0x5D and (not stack or stack.pop() != 0x5B):
            raise fail(f"{label} JSON containers are unbalanced")
    if in_string or escaped or stack:
        raise fail(f"{label} JSON is structurally unbalanced")


def sha256_stream(stream: BinaryIO) -> str:
    stream.seek(0)
    digest = hashlib.sha256()
    while chunk := stream.read(READ_CHUNK):
        digest.update(chunk)
    stream.seek(0)
    return digest.hexdigest()


def validate_external_payload(
    path: Path, archived: ArchivedFile, label: str
) -> None:
    with open_regular_archive(path) as (source, source_size):
        if source_size != len(archived.data):
            raise fail(f"packaged {label} byte count differs from built bytes")
        if sha256_stream(source) != hashlib.sha256(archived.data).hexdigest():
            raise fail(f"packaged {label} differs from built bytes")


def snapshot_archive(source: BinaryIO, target: BinaryIO, expected_size: int) -> str:
    """Copy the opened archive once; hashing and parsing use this private image."""
    source.seek(0)
    digest = hashlib.sha256()
    copied = 0
    while chunk := source.read(min(READ_CHUNK, expected_size - copied + 1)):
        copied += len(chunk)
        if copied > expected_size or copied > MAX_ARCHIVE_BYTES:
            raise fail("release archive grew while snapshotting")
        digest.update(chunk)
        target.write(chunk)
    if copied != expected_size:
        raise fail("release archive size changed while snapshotting")
    target.flush()
    target.seek(0)
    return digest.hexdigest()


def read_bounded(stream: object, expected_size: int, limit: int, label: str) -> bytes:
    if expected_size < 0 or expected_size > limit:
        raise fail(f"{label} size {expected_size} exceeds {limit} bytes")
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = stream.read(min(READ_CHUNK, limit + 1 - total))
        if not chunk:
            break
        chunks.append(chunk)
        total += len(chunk)
        if total > limit:
            raise fail(f"{label} streamed beyond {limit} bytes")
    if total != expected_size:
        raise fail(f"{label} size mismatch: header {expected_size}, stream {total}")
    return b"".join(chunks)


def _validate_entries(entries: list[ArchivedFile]) -> dict[str, ArchivedFile]:
    if len(entries) != EXPECTED_ENTRY_COUNT:
        raise fail(
            f"release archive must contain exactly {EXPECTED_ENTRY_COUNT} entries"
        )
    suffix = ".exe" if any(entry.name.endswith(".exe") for entry in entries) else ""
    expected = expected_files(suffix)
    names = [entry.name for entry in entries]
    if len(names) != len(set(names)):
        raise fail("release archive contains duplicate filenames")
    if set(names) != set(expected):
        raise fail("archive must contain exactly: " + ", ".join(sorted(expected)))
    total = 0
    by_name: dict[str, ArchivedFile] = {}
    for entry in entries:
        required_mode, limit = expected[entry.name]
        if entry.mode != required_mode:
            raise fail(
                f"archive entry {entry.name!r} mode {entry.mode:04o} != {required_mode:04o}"
            )
        if len(entry.data) > limit:
            raise fail(f"archive entry {entry.name!r} exceeds {limit} bytes")
        total += len(entry.data)
        if total > MAX_TOTAL_BYTES:
            raise fail(f"archive expands beyond {MAX_TOTAL_BYTES} bytes")
        by_name[entry.name] = entry
    return by_name


def validate_single_gzip_member(stream: BinaryIO) -> None:
    """Reject concatenated/trailing gzip data and bound the raw tar expansion."""
    stream.seek(0)
    decoder = zlib.decompressobj(16 + zlib.MAX_WBITS)
    expanded = 0
    while chunk := stream.read(READ_CHUNK):
        pending = chunk
        while pending:
            output = decoder.decompress(pending, READ_CHUNK)
            expanded += len(output)
            if expanded > MAX_TAR_STREAM_BYTES:
                raise fail(f"tar stream expands beyond {MAX_TAR_STREAM_BYTES} bytes")
            if decoder.unused_data:
                raise fail(
                    "tar.gz must contain exactly one gzip member with no trailing data"
                )
            pending = decoder.unconsumed_tail
        if decoder.eof:
            if stream.read(1):
                raise fail(
                    "tar.gz must contain exactly one gzip member with no trailing data"
                )
            break
    if not decoder.eof:
        raise fail("tar.gz gzip member is truncated")
    stream.seek(0)


def validate_tar(stream: BinaryIO, suffix: str) -> dict[str, ArchivedFile]:
    stream.seek(0)
    header = stream.read(10)
    if len(header) != 10 or header[:3] != b"\x1f\x8b\x08":
        raise fail("tar.gz has an invalid gzip header")
    if header[3] != 0 or header[4:8] != b"\0\0\0\0":
        raise fail("tar.gz gzip header must have no optional fields and zero mtime")
    validate_single_gzip_member(stream)
    expected = expected_files(suffix)
    entries: list[ArchivedFile] = []
    with tarfile.open(fileobj=stream, mode="r|gz") as archive:
        for member in archive:
            if len(entries) >= EXPECTED_ENTRY_COUNT:
                raise fail(f"tar contains more than {EXPECTED_ENTRY_COUNT} entries")
            validate_name(member.name)
            if member.name not in expected:
                raise fail(f"unexpected tar entry {member.name!r}")
            if not member.isreg():
                raise fail(f"tar entry {member.name!r} is not a regular file")
            if member.uid != 0 or member.gid != 0 or member.uname or member.gname:
                raise fail(f"tar entry {member.name!r} has non-canonical ownership")
            if member.mtime != 0:
                raise fail(f"tar entry {member.name!r} has non-zero mtime")
            if member.pax_headers:
                raise fail(f"tar entry {member.name!r} has unsupported PAX metadata")
            payload = archive.extractfile(member)
            if payload is None:
                raise fail(f"cannot read tar entry {member.name!r}")
            with payload:
                data = read_bounded(
                    payload,
                    member.size,
                    expected[member.name][1],
                    member.name,
                )
            entries.append(ArchivedFile(member.name, member.mode & 0o777, data))
    return _validate_entries(entries)


def validate_zip_directory(stream: BinaryIO, archive_size: int) -> None:
    tail_size = min(archive_size, 65_557)
    stream.seek(archive_size - tail_size)
    tail = stream.read(tail_size)
    eocd_offset = tail.rfind(b"PK\x05\x06")
    if eocd_offset < 0 or len(tail) - eocd_offset < 22:
        raise fail("zip end-of-central-directory record is missing")
    fields = struct.unpack("<4s4H2LH", tail[eocd_offset : eocd_offset + 22])
    (
        _,
        disk,
        central_disk,
        disk_entries,
        total_entries,
        central_size,
        central_offset,
        comment,
    ) = fields
    if comment != 0 or eocd_offset + 22 != len(tail):
        raise fail("zip archive comment or trailing data is not allowed")
    if disk != 0 or central_disk != 0 or disk_entries != total_entries:
        raise fail("multi-disk zip archives are not allowed")
    if total_entries != EXPECTED_ENTRY_COUNT:
        raise fail(
            f"zip duplicate/extra/missing entry count; expected exactly {EXPECTED_ENTRY_COUNT}"
        )
    if central_offset + central_size != archive_size - tail_size + eocd_offset:
        raise fail("zip central-directory extent is inconsistent")
    if b"PK\x06\x06" in tail or b"PK\x06\x07" in tail:
        raise fail("Zip64 release archives are not allowed")
    stream.seek(0)


def validate_zip(
    stream: BinaryIO, archive_size: int, suffix: str
) -> dict[str, ArchivedFile]:
    validate_zip_directory(stream, archive_size)
    expected = expected_files(suffix)
    entries: list[ArchivedFile] = []
    with zipfile.ZipFile(stream, mode="r") as archive:
        if len(archive.filelist) != EXPECTED_ENTRY_COUNT or archive.comment:
            raise fail("zip entry count or archive comment is not canonical")
        for info in archive.infolist():
            validate_name(info.filename)
            if info.filename not in expected:
                raise fail(f"unexpected zip entry {info.filename!r}")
            if info.is_dir() or info.flag_bits != 0:
                raise fail(
                    f"zip entry {info.filename!r} is a directory or has unsupported flags"
                )
            if (
                info.compress_type != zipfile.ZIP_STORED
                or info.compress_size != info.file_size
            ):
                raise fail(
                    f"zip entry {info.filename!r} must use deterministic stored encoding"
                )
            if info.extra or info.comment:
                raise fail(f"zip entry {info.filename!r} has unsupported metadata")
            mode_word = info.external_attr >> 16
            if info.create_system != 3 or stat.S_IFMT(mode_word) != stat.S_IFREG:
                raise fail(f"zip entry {info.filename!r} is not a Unix regular file")
            if info.date_time != (1980, 1, 1, 0, 0, 0):
                raise fail(f"zip entry {info.filename!r} has non-canonical timestamp")
            with archive.open(info, mode="r") as payload:
                data = read_bounded(
                    payload,
                    info.file_size,
                    expected[info.filename][1],
                    info.filename,
                )
            entries.append(ArchivedFile(info.filename, mode_word & 0o777, data))
    return _validate_entries(entries)


def validate_inner_checksum(entries: dict[str, ArchivedFile], suffix: str) -> None:
    checker_name = f"axiograph_verify{suffix}"
    manifest = entries["axiograph_verify.sha256"].data
    try:
        text = manifest.decode("ascii")
    except UnicodeDecodeError as error:
        raise fail("checker checksum manifest is not ASCII") from error
    match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.-]+)\n", text)
    if match is None or match.group(2) != checker_name:
        raise fail("checker checksum manifest has invalid grammar or filename")
    actual = hashlib.sha256(entries[checker_name].data).hexdigest()
    if actual != match.group(1):
        raise fail("checker checksum does not match archived checker bytes")


def _exact_keys(value: object, keys: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != keys:
        raise fail(f"{label} has missing or unknown fields")
    return value


def validate_release_manifest(
    entries: dict[str, ArchivedFile],
    host: str,
    expected_version: str | None,
    expected_source_commit: str | None,
    expected_fixture_manifest_sha256: str | None,
    expected_rust_toolchain: str | None,
) -> dict[str, object]:
    raw_source = entries["axiograph-source-manifest.json"].data
    validate_json_nesting(raw_source, "source manifest")
    try:
        source = validate_source_manifest_bytes(raw_source)
    except (SourceManifestError, RecursionError) as error:
        raise fail(str(error)) from error
    raw_release = entries["axiograph-release.json"].data
    validate_json_nesting(raw_release, "release manifest")
    try:
        parsed = json.loads(raw_release)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise fail(f"release manifest is not valid UTF-8 JSON: {error}") from error
    release = _exact_keys(
        parsed,
        {
            "schema",
            "host",
            "version",
            "rust_toolchain",
            "source_commit",
            "source_tree",
            "source_manifest_sha256",
            "fixture_manifest_sha256",
            "checker",
            "files",
            "semantic_scope",
        },
        "release manifest",
    )
    if canonical_json(release) != raw_release:
        raise fail("release manifest is not canonical JSON with one trailing newline")
    if release["schema"] != SCHEMA:
        raise fail(f"unsupported release manifest schema {release['schema']!r}")
    if release["host"] != host:
        raise fail("release manifest host does not match archive name")
    try:
        release_version = str(parse_calver(release["version"]))
    except CalVerError as error:
        raise fail(f"release manifest version is not canonical CalVer: {error}") from error
    if expected_version is not None and release_version != expected_version:
        raise fail("release manifest version does not match caller expectation")
    if (
        not isinstance(release["rust_toolchain"], str)
        or EXACT_TOOLCHAIN.fullmatch(release["rust_toolchain"]) is None
    ):
        raise fail("release manifest Rust toolchain is not an exact patch release")
    if (
        expected_rust_toolchain is not None
        and release["rust_toolchain"] != expected_rust_toolchain
    ):
        raise fail("release manifest Rust toolchain does not match caller expectation")
    if release["source_commit"] != source["source_commit"]:
        raise fail("release and source manifests disagree on source commit")
    if release["source_tree"] != source["source_tree"]:
        raise fail("release and source manifests disagree on source tree")
    if (
        expected_source_commit is not None
        and release["source_commit"] != expected_source_commit
    ):
        raise fail("release source commit does not match caller expectation")
    source_hash = hashlib.sha256(raw_source).hexdigest()
    if release["source_manifest_sha256"] != source_hash:
        raise fail("release manifest source-manifest SHA-256 mismatch")
    fixture_hash = release["fixture_manifest_sha256"]
    if not isinstance(fixture_hash, str) or HEX64.fullmatch(fixture_hash) is None:
        raise fail("release manifest fixture SHA-256 is invalid")
    if (
        expected_fixture_manifest_sha256 is not None
        and fixture_hash != expected_fixture_manifest_sha256
    ):
        raise fail("release fixture manifest SHA-256 does not match caller expectation")
    checker = _exact_keys(
        release["checker"], {"build_id", "protocol"}, "checker contract"
    )
    if checker != {"build_id": CHECKER_BUILD_ID, "protocol": CHECKER_PROTOCOL}:
        raise fail("release checker contract does not match the approved checker")
    if release["semantic_scope"] != SEMANTIC_SCOPE:
        raise fail("release semantic scope or non-claims changed")

    records = release["files"]
    if not isinstance(records, list):
        raise fail("release manifest files must be a list")
    expected_record_names = sorted(set(entries) - {"axiograph-release.json"})
    if len(records) != len(expected_record_names):
        raise fail("release manifest payload record count is not exact")
    names: list[str] = []
    for index, raw_record in enumerate(records):
        record = _exact_keys(
            raw_record, {"name", "mode", "bytes", "sha256"}, f"file[{index}]"
        )
        name = record["name"]
        if (
            not isinstance(name, str)
            or name not in entries
            or name == "axiograph-release.json"
        ):
            raise fail(f"release manifest file[{index}] names an invalid payload")
        entry = entries[name]
        if record["mode"] != f"{entry.mode:04o}":
            raise fail(f"release manifest mode mismatch for {name}")
        if record["bytes"] != len(entry.data):
            raise fail(f"release manifest byte-count mismatch for {name}")
        if record["sha256"] != hashlib.sha256(entry.data).hexdigest():
            raise fail(f"release manifest SHA-256 mismatch for {name}")
        names.append(name)
    if names != expected_record_names:
        raise fail("release manifest payload records are not exact and sorted")
    return release


def extract_validated(entries: dict[str, ArchivedFile], destination: Path) -> None:
    if os.open not in os.supports_dir_fd:
        raise fail("secure dirfd-relative extraction is unavailable on this platform")
    flags = (
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    directory = os.open(destination, flags)
    try:
        metadata = os.fstat(directory)
        if not stat.S_ISDIR(metadata.st_mode):
            raise fail("extraction destination must be a real directory")
        with os.scandir(directory) as children:
            if next(children, None) is not None:
                raise fail("extraction destination must be empty")
        for name, entry in sorted(entries.items()):
            descriptor = os.open(
                name,
                os.O_WRONLY
                | os.O_CREAT
                | os.O_EXCL
                | getattr(os, "O_CLOEXEC", 0)
                | getattr(os, "O_NOFOLLOW", 0),
                0o600,
                dir_fd=directory,
            )
            try:
                remaining = memoryview(entry.data)
                while remaining:
                    written = os.write(descriptor, remaining)
                    if written <= 0:
                        raise fail(f"short write while extracting {name!r}")
                    remaining = remaining[written:]
                os.fchmod(descriptor, entry.mode)
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
        os.fsync(directory)
    finally:
        os.close(directory)


def _host_from_archive_name(path: Path) -> tuple[str, str]:
    for host, (extension, suffix) in HOSTS.items():
        if path.name == f"axiograph-{host}{extension}":
            return host, suffix
    raise fail(
        "release archive name must contain one configured exact Rust host triple"
    )


def validate_archive(
    path: Path,
    extract_to: Path | None = None,
    *,
    expected_host: str | None = None,
    expected_version: str | None = None,
    expected_source_commit: str | None = None,
    expected_fixture_manifest_sha256: str | None = None,
    expected_rust_toolchain: str | None = None,
    expected_runtime: Path | None = None,
    expected_checker: Path | None = None,
) -> dict[str, object]:
    host, suffix = _host_from_archive_name(path)
    if expected_host is not None and host != expected_host:
        raise fail("release archive host does not match caller expectation")
    extension = HOSTS[host][0]
    with (
        open_regular_archive(path) as (source, archive_size),
        tempfile.TemporaryFile(mode="w+b") as snapshot,
    ):
        archive_sha256 = snapshot_archive(source, snapshot, archive_size)
        if extension == ".tar.gz":
            entries = validate_tar(snapshot, suffix)
        else:
            entries = validate_zip(snapshot, archive_size, suffix)
    validate_inner_checksum(entries, suffix)
    release = validate_release_manifest(
        entries,
        host,
        expected_version,
        expected_source_commit,
        expected_fixture_manifest_sha256,
        expected_rust_toolchain,
    )
    if expected_runtime is not None:
        validate_external_payload(
            expected_runtime, entries[f"axiograph{suffix}"], "runtime"
        )
    if expected_checker is not None:
        validate_external_payload(
            expected_checker,
            entries[f"axiograph_verify{suffix}"],
            "trusted checker",
        )
    if extract_to is not None:
        extract_validated(entries, extract_to)
    return {
        "archive": path.name,
        "archive_sha256": archive_sha256,
        "host": host,
        "version": release["version"],
        "source_commit": release["source_commit"],
        "source_tree": release["source_tree"],
        "rust_toolchain": release["rust_toolchain"],
        "entries": [
            {"name": name, "bytes": len(entry.data), "mode": f"{entry.mode:04o}"}
            for name, entry in sorted(entries.items())
        ],
        "validated": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--extract-to", type=Path)
    parser.add_argument("--expected-host")
    parser.add_argument("--expected-version")
    parser.add_argument("--expected-source-commit")
    parser.add_argument("--expected-fixture-manifest-sha256")
    parser.add_argument("--expected-rust-toolchain")
    parser.add_argument("--expected-runtime", type=Path)
    parser.add_argument("--expected-checker", type=Path)
    args = parser.parse_args()
    try:
        report = validate_archive(
            args.archive,
            args.extract_to,
            expected_host=args.expected_host,
            expected_version=args.expected_version,
            expected_source_commit=args.expected_source_commit,
            expected_fixture_manifest_sha256=args.expected_fixture_manifest_sha256,
            expected_rust_toolchain=args.expected_rust_toolchain,
            expected_runtime=args.expected_runtime,
            expected_checker=args.expected_checker,
        )
    except (
        ArchiveValidationError,
        OSError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as error:
        print(f"release archive validation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
