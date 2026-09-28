#!/usr/bin/env python3
"""Recreate only the pinned Kani library caches needed by CI scanner tests.

This is a CI fixture bootstrap, not a new ownership exception. The scanner
still validates both complete cache homes against its checked manifest.
"""

from __future__ import annotations

import hashlib
import json
import os
import stat
import tarfile
import tempfile
from pathlib import Path
from typing import Any

from scripts.check_no_unsafe import (
    CANDIDATE_HOMES,
    MANIFEST_LIMIT,
    MANIFEST_REL,
    _duplicate_pairs,
    validate_closed_manifest,
)
from scripts.generate_no_unsafe_external_cache_manifest import (
    ARCHIVE_BYTES,
    ARCHIVE_REL,
    ARCHIVE_SHA256,
    PRODUCTION_ARCHIVE_LIMITS,
)

ROOT = Path(__file__).resolve().parents[1]
MAX_CACHE_BYTES = 1024 * 1024


class FixtureError(ValueError):
    pass


def _read_manifest(root: Path) -> dict[str, Any]:
    path = root / MANIFEST_REL
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MANIFEST_LIMIT:
            raise FixtureError("checked cache manifest is not bounded regular input")
        with os.fdopen(descriptor, "rb") as source:
            descriptor = -1
            raw = source.read(MANIFEST_LIMIT + 1)
        if len(raw) != metadata.st_size:
            raise FixtureError("checked cache manifest changed during read")
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_duplicate_pairs)
        return validate_closed_manifest(value, code="E_MANIFEST_SCHEMA")
    finally:
        if descriptor != -1:
            os.close(descriptor)


def _parts(value: str) -> tuple[str, ...]:
    parts = tuple(value.split("/"))
    if not parts or any(part in ("", ".", "..") for part in parts) or "\\" in value:
        raise FixtureError("noncanonical cache fixture path")
    return parts


def _selected_archive_bytes(archive: Path, record: dict[str, Any]) -> dict[str, bytes]:
    distribution = record["distribution"]
    if (distribution["archive_bytes"], distribution["archive_sha256"]) != (
        ARCHIVE_BYTES, ARCHIVE_SHA256
    ):
        raise FixtureError("checked cache archive identity differs from pinned input")
    files = {item["archive_member"]: item for item in record["files"]}
    directories = {item["archive_member"] for item in record["directories"]}
    if len(files) != len(record["files"]) or len(directories) != len(record["directories"]):
        raise FixtureError("duplicate checked cache archive member")
    if sum(item["bytes"] for item in files.values()) > MAX_CACHE_BYTES:
        raise FixtureError("checked cache fixture exceeds byte budget")
    for item in record["files"] + record["directories"]:
        _parts(item["path"])
        _parts(item["archive_member"])
    descriptor = os.open(archive, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_size != ARCHIVE_BYTES:
            raise FixtureError("pinned Kani archive is not the exact regular input")
        digest = hashlib.sha256()
        with tempfile.TemporaryFile() as snapshot:
            with os.fdopen(descriptor, "rb") as source:
                descriptor = -1
                remaining = ARCHIVE_BYTES
                while remaining:
                    chunk = source.read(min(1024 * 1024, remaining))
                    if not chunk:
                        raise FixtureError("pinned Kani archive ended early")
                    digest.update(chunk)
                    snapshot.write(chunk)
                    remaining -= len(chunk)
                after = os.fstat(source.fileno())
                if source.read(1) or (
                    after.st_dev, after.st_ino, after.st_mode,
                    after.st_size, after.st_mtime_ns, after.st_ctime_ns,
                ) != (
                    before.st_dev, before.st_ino, before.st_mode,
                    before.st_size, before.st_mtime_ns, before.st_ctime_ns,
                ):
                    raise FixtureError("pinned Kani archive changed during snapshot")
            if digest.hexdigest() != ARCHIVE_SHA256:
                raise FixtureError("pinned Kani archive SHA-256 differs")
            snapshot.seek(0)
            selected: dict[str, bytes] = {}
            seen_dirs: set[str] = set()
            count = 0
            expanded = 0
            with tarfile.open(fileobj=snapshot, mode="r|gz") as source_tar:
                for member in source_tar:
                    count += 1
                    expanded += member.size
                    if (
                        count > PRODUCTION_ARCHIVE_LIMITS.members
                        or member.size > PRODUCTION_ARCHIVE_LIMITS.one_member
                        or expanded > PRODUCTION_ARCHIVE_LIMITS.decompressed
                    ):
                        raise FixtureError("pinned Kani archive exceeds expansion budget")
                    if member.name in directories:
                        if not member.isdir() or member.name in seen_dirs:
                            raise FixtureError("pinned Kani directory member differs")
                        seen_dirs.add(member.name)
                    if member.name in files:
                        item = files[member.name]
                        if not member.isfile() or member.name in selected or member.size != item["bytes"]:
                            raise FixtureError("pinned Kani file member differs")
                        stream = source_tar.extractfile(member)
                        if stream is None:
                            raise FixtureError("pinned Kani file has no payload")
                        with stream:
                            data = stream.read(item["bytes"] + 1)
                        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
                            raise FixtureError("pinned Kani library bytes differ")
                        selected[member.name] = data
            if set(selected) != set(files) or seen_dirs != directories:
                raise FixtureError("pinned Kani library members are incomplete")
            return selected
    finally:
        if descriptor != -1:
            os.close(descriptor)


def _ensure_directories(root: Path, parts: tuple[str, ...]) -> Path:
    path = root
    for part in parts:
        path /= part
        try:
            mode = path.lstat().st_mode
        except FileNotFoundError:
            path.mkdir(mode=0o755)
            mode = path.lstat().st_mode
        if not stat.S_ISDIR(mode):
            raise FixtureError("cache fixture parent is not a real directory")
    return path


def prepare(root: Path) -> dict[str, Any]:
    manifest = _read_manifest(root)
    record = manifest["records"][0]
    if record["cache_home_paths"] != list(CANDIDATE_HOMES):
        raise FixtureError("checked cache homes differ from scanner policy")
    selected = _selected_archive_bytes(root / ARCHIVE_REL, record)
    for home in record["cache_home_paths"]:
        parts = _parts(home)
        parent = _ensure_directories(root, parts[:-1])
        destination = parent / parts[-1]
        if destination.exists() or destination.is_symlink():
            raise FixtureError("cache fixture destination already exists")
        with tempfile.TemporaryDirectory(prefix=".axi-kani-fixture-", dir=parent) as staging:
            staged_home = Path(staging) / parts[-1]
            library = staged_home / record["library_relative_path"]
            library.mkdir(parents=True)
            for item in record["directories"]:
                (library / item["path"]).mkdir(parents=True, exist_ok=False)
            for item in record["files"]:
                file_path = library / item["path"]
                with file_path.open("xb") as output:
                    output.write(selected[item["archive_member"]])
                file_path.chmod(0o644)
            staged_home.rename(destination)
    return {"homes": len(record["cache_home_paths"]), "files_per_home": len(record["files"])}


if __name__ == "__main__":
    print(json.dumps(prepare(ROOT), sort_keys=True))
