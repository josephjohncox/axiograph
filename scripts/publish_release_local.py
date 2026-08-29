#!/usr/bin/env python3
"""Verify and atomically publish release assets to a local rehearsal directory."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import sys
import tempfile
from pathlib import Path

try:
    from .bounded_io import BoundedIoError, read_regular_bounded
    from .build_release_bundle import HOSTS, canonical_json
    from .validate_release_archive import (
        ArchiveValidationError,
        validate_archive,
    )
except ImportError:
    from bounded_io import BoundedIoError, read_regular_bounded
    from build_release_bundle import HOSTS, canonical_json
    from validate_release_archive import (
        ArchiveValidationError,
        validate_archive,
    )

HEX64 = re.compile(r"[0-9a-f]{64}")
IMAGE_DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
INJECTION_POINTS = {"after-verification", "after-first-copy", "before-commit"}
PUBLICATION_SCHEMA = "axiograph-local-publication-rehearsal-v3"
PUBLICATION_SCOPE = {
    "proved": "all local bundle bytes were verified before one atomic directory rename",
    "non_claims": [
        "GitHub release transactionality",
        "GHCR registry transactionality",
        "network or credential behavior",
        "remote deletion after a partial provider outage",
    ],
}


class LocalPublicationError(RuntimeError):
    pass


class InjectedPublicationFailure(LocalPublicationError):
    pass


def _fail(message: str) -> LocalPublicationError:
    return LocalPublicationError(message)


def _regular_file(path: Path, label: str, limit: int = 512 * 1024 * 1024) -> bytes:
    try:
        return read_regular_bounded(path, limit, label)
    except BoundedIoError as error:
        raise _fail(str(error)) from error


def _outer_checksum(path: Path, archive_name: str) -> str:
    raw = _regular_file(path, f"checksum for {archive_name}", 1024)
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError as error:
        raise _fail(f"checksum for {archive_name} is not ASCII") from error
    match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.-]+)\n", text)
    if match is None or match.group(2) != archive_name:
        raise _fail(f"checksum for {archive_name} has invalid grammar or filename")
    return match.group(1)


def verify_asset_set(
    dist: Path,
    image_digest_dir: Path,
) -> dict[str, object]:
    dist_metadata = dist.lstat()
    if not stat.S_ISDIR(dist_metadata.st_mode) or dist.is_symlink():
        raise _fail("release asset directory must be a real directory")
    expected_names: list[str] = []
    for host, (extension, _) in HOSTS.items():
        name = f"axiograph-{host}{extension}"
        expected_names.extend([name, name + ".sha256"])
    actual_names = sorted(path.name for path in dist.iterdir())
    if actual_names != sorted(expected_names):
        raise _fail(
            f"release asset set is not exact: expected={sorted(expected_names)}, "
            f"actual={actual_names}"
        )

    reports: list[dict[str, object]] = []
    common: dict[str, object] | None = None
    for host, (extension, _) in sorted(HOSTS.items()):
        archive_name = f"axiograph-{host}{extension}"
        archive = dist / archive_name
        checksum = dist / f"{archive_name}.sha256"
        declared = _outer_checksum(checksum, archive_name)
        actual = hashlib.sha256(_regular_file(archive, archive_name)).hexdigest()
        if declared != actual:
            raise _fail(f"outer SHA-256 mismatch for {archive_name}")
        report = validate_archive(archive, expected_host=host)
        identity = {
            "version": report["version"],
            "source_commit": report["source_commit"],
            "source_tree": report["source_tree"],
            "rust_toolchain": report["rust_toolchain"],
        }
        if common is None:
            common = identity
        elif identity != common:
            raise _fail(
                "release bundles disagree on version, source, tree, or toolchain"
            )
        reports.append(
            {
                "host": host,
                "archive": archive_name,
                "sha256": actual,
            }
        )
    if common is None:
        raise _fail("release asset set contained no bundles")

    digest_metadata = image_digest_dir.lstat()
    if not stat.S_ISDIR(digest_metadata.st_mode) or image_digest_dir.is_symlink():
        raise _fail("image digest directory must be a real directory")
    expected_digest_names = ["amd64.digest", "arm64.digest"]
    actual_digest_names = sorted(path.name for path in image_digest_dir.iterdir())
    if actual_digest_names != expected_digest_names:
        raise _fail(
            "image digest set must contain exactly amd64.digest and arm64.digest"
        )
    image_digests: dict[str, str] = {}
    for name in expected_digest_names:
        raw = _regular_file(image_digest_dir / name, name, 256)
        try:
            value = raw.decode("ascii")
        except UnicodeDecodeError as error:
            raise _fail(f"{name} is not ASCII") from error
        if not value.endswith("\n") or IMAGE_DIGEST.fullmatch(value[:-1]) is None:
            raise _fail(f"{name} is not one canonical sha256 image digest")
        image_digests[name.removesuffix(".digest")] = value[:-1]
    if len(set(image_digests.values())) != len(image_digests):
        raise _fail("architecture image digests must be distinct")

    return {
        "schema": PUBLICATION_SCHEMA,
        **common,
        "bundles": reports,
        "image_digests": image_digests,
        "scope": PUBLICATION_SCOPE,
    }


def _inject(point: str, requested: str | None) -> None:
    if requested == point:
        raise InjectedPublicationFailure(
            f"injected local publication failure at {point}"
        )


def _fsync_dir(path: Path) -> None:
    if os.name == "nt":
        return
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def publish_local(
    dist: Path,
    image_digest_dir: Path,
    destination: Path,
    inject_failure: str | None = None,
) -> dict[str, object]:
    if inject_failure is not None and inject_failure not in INJECTION_POINTS:
        raise _fail(f"unknown injection point {inject_failure!r}")
    if destination.exists() or destination.is_symlink():
        raise _fail("publication destination must not already exist")
    parent = destination.parent
    parent_metadata = parent.lstat()
    if not stat.S_ISDIR(parent_metadata.st_mode) or parent.is_symlink():
        raise _fail("publication parent must be a real directory")

    staging = Path(tempfile.mkdtemp(prefix=".axiograph-release-stage-", dir=parent))
    committed = False
    try:
        assets = staging / "assets"
        digests = staging / "image-digests"
        assets.mkdir()
        digests.mkdir()
        for index, source in enumerate(sorted(dist.iterdir())):
            target = assets / source.name
            with target.open("xb") as output:
                output.write(_regular_file(source, source.name))
                os.chmod(target, 0o644)
                output.flush()
                os.fsync(output.fileno())
            if index == 0:
                _inject("after-first-copy", inject_failure)
        for source in sorted(image_digest_dir.iterdir()):
            target = digests / source.name
            with target.open("xb") as output:
                output.write(_regular_file(source, source.name, 256))
                os.chmod(target, 0o644)
                output.flush()
                os.fsync(output.fileno())
        _fsync_dir(assets)
        _fsync_dir(digests)
        report = verify_asset_set(assets, digests)
        _inject("after-verification", inject_failure)
        publication = staging / "publication.json"
        with publication.open("xb") as output:
            output.write(canonical_json(report))
            os.chmod(publication, 0o644)
            output.flush()
            os.fsync(output.fileno())
        _fsync_dir(assets)
        _fsync_dir(digests)
        _fsync_dir(staging)
        _inject("before-commit", inject_failure)
        os.replace(staging, destination)
        committed = True
        _fsync_dir(parent)
    finally:
        if not committed and staging.exists():
            shutil.rmtree(staging)
    return report


def _absolute_no_resolve(path: Path) -> Path:
    return path if path.is_absolute() else Path.cwd() / path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist", required=True, type=Path)
    parser.add_argument("--image-digests", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--inject-failure", choices=sorted(INJECTION_POINTS))
    args = parser.parse_args()
    try:
        report = publish_local(
            _absolute_no_resolve(args.dist),
            _absolute_no_resolve(args.image_digests),
            _absolute_no_resolve(args.destination),
            args.inject_failure,
        )
    except InjectedPublicationFailure as error:
        print(str(error), file=sys.stderr)
        return 75
    except (ArchiveValidationError, LocalPublicationError, OSError) as error:
        print(f"local publication failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
