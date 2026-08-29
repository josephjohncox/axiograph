#!/usr/bin/env python3
"""Build one deterministic, manifest-bearing Axiograph release bundle."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import re
import stat
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

try:
    from .bounded_io import BoundedIoError, read_regular_bounded, sha256_regular_bounded
    from .generate_release_source_manifest import (
        SourceManifestError,
        validate_source_manifest_bytes,
    )
    from .release_version import CalVerError, parse_calver
    from .run_release_fixture_suite import FixtureSuiteError, validate_manifest_bytes
except ImportError:
    from bounded_io import BoundedIoError, read_regular_bounded, sha256_regular_bounded
    from generate_release_source_manifest import (
        SourceManifestError,
        validate_source_manifest_bytes,
    )
    from release_version import CalVerError, parse_calver
    from run_release_fixture_suite import FixtureSuiteError, validate_manifest_bytes

SCHEMA = "axiograph-release-bundle-v3"
CHECKER_PROTOCOL = "axiograph-verifier-stdio-v2"
CHECKER_BUILD_ID = "axiograph-verify-main-v3"
MAX_BINARY_BYTES = 256 * 1024 * 1024
MAX_MANIFEST_BYTES = 16 * 1024 * 1024
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
HOSTS = {
    "x86_64-unknown-linux-gnu": (".tar.gz", ""),
    "aarch64-apple-darwin": (".tar.gz", ""),
    "x86_64-pc-windows-msvc": (".zip", ".exe"),
}
SEMANTIC_SCOPE = {
    "meaning_plane": "canonical accepted .axi plus one compiled category/kernel IR",
    "trusted_checker": "import closure of lean/Axiograph/VerifyMain.lean",
    "runtime": "Rust is an untrusted certificate and execution producer",
    "certified_fragment": "bounded finite query denotation pinned by release/fixtures.json",
    "non_claims": [
        "open-world ontology completeness",
        "general dependent type theory",
        "univalence, higher inductive types, or unrestricted higher paths",
        "semantic authority for PathDB or SQLite bytes",
        "compiler or linker reproducibility from source",
    ],
}


class BundleBuildError(RuntimeError):
    pass


def _fail(message: str) -> BundleBuildError:
    return BundleBuildError(message)


def _absolute_no_resolve(path: Path) -> Path:
    return path if path.is_absolute() else Path.cwd() / path


def _regular_file(path: Path, label: str, limit: int) -> bytes:
    try:
        return read_regular_bounded(path, limit, label)
    except BoundedIoError as error:
        raise _fail(str(error)) from error


def _toolchain(repo_root: Path) -> str:
    path = repo_root / "rust-toolchain.toml"
    data = tomllib.loads(
        _regular_file(path, "Rust toolchain file", 64 * 1024).decode("utf-8")
    )
    if set(data) != {"toolchain"} or not isinstance(data["toolchain"], dict):
        raise _fail("rust-toolchain.toml must contain only [toolchain]")
    toolchain = data["toolchain"]
    if set(toolchain) != {"channel", "profile", "components"}:
        raise _fail("rust-toolchain.toml has missing or unknown toolchain fields")
    channel = toolchain["channel"]
    if (
        not isinstance(channel, str)
        or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", channel) is None
    ):
        raise _fail("Rust toolchain channel must be an exact stable patch release")
    if toolchain["profile"] != "minimal":
        raise _fail("Rust release toolchain profile must be minimal")
    components = toolchain["components"]
    if components != ["cargo", "clippy", "rustfmt"]:
        raise _fail("Rust release toolchain components must be cargo, clippy, rustfmt")
    return channel


def _workspace_version(repo_root: Path) -> str:
    path = repo_root / "rust" / "Cargo.toml"
    data = tomllib.loads(
        _regular_file(path, "Rust workspace manifest", 1024 * 1024).decode("utf-8")
    )
    try:
        version = data["workspace"]["package"]["version"]
    except (KeyError, TypeError) as error:
        raise _fail("Rust workspace version is missing") from error
    try:
        return str(parse_calver(version))
    except CalVerError as error:
        raise _fail(f"Rust workspace version is not canonical CalVer: {error}") from error


def _file_record(name: str, mode: int, data: bytes) -> dict[str, object]:
    return {
        "name": name,
        "mode": f"{mode:04o}",
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode(
        "utf-8"
    )


def release_manifest(
    host: str,
    version: str,
    rust_toolchain: str,
    source_manifest: dict[str, object],
    fixture_manifest_sha256: str,
    payloads: dict[str, tuple[int, bytes]],
) -> dict[str, object]:
    records = [
        _file_record(name, mode, data)
        for name, (mode, data) in sorted(payloads.items())
    ]
    return {
        "schema": SCHEMA,
        "host": host,
        "version": version,
        "rust_toolchain": rust_toolchain,
        "source_commit": source_manifest["source_commit"],
        "source_tree": source_manifest["source_tree"],
        "source_manifest_sha256": hashlib.sha256(
            payloads["axiograph-source-manifest.json"][1]
        ).hexdigest(),
        "fixture_manifest_sha256": fixture_manifest_sha256,
        "checker": {
            "build_id": CHECKER_BUILD_ID,
            "protocol": CHECKER_PROTOCOL,
        },
        "files": records,
        "semantic_scope": SEMANTIC_SCOPE,
    }


def _write_tar(path: Path, files: dict[str, tuple[int, bytes]]) -> None:
    with (
        path.open("xb") as raw,
        gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
        ) as compressed,
        tarfile.open(
            fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT
        ) as archive,
    ):
        for name, (mode, data) in sorted(files.items()):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = mode
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))


def _write_zip(path: Path, files: dict[str, tuple[int, bytes]]) -> None:
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_STORED) as archive:
        for name, (mode, data) in sorted(files.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = (stat.S_IFREG | mode) << 16
            archive.writestr(info, data)


def build_bundle(
    repo_root: Path,
    runtime_path: Path,
    checker_path: Path,
    source_manifest_path: Path,
    fixture_manifest_path: Path,
    host: str,
    output_dir: Path,
) -> tuple[Path, Path, dict[str, object]]:
    if host not in HOSTS:
        raise _fail(f"unsupported release host {host!r}")
    extension, executable_suffix = HOSTS[host]
    runtime = _regular_file(runtime_path, "Rust runtime", MAX_BINARY_BYTES)
    checker = _regular_file(checker_path, "Lean checker", MAX_BINARY_BYTES)
    source_raw = _regular_file(
        source_manifest_path, "source manifest", MAX_MANIFEST_BYTES
    )
    try:
        source_manifest = validate_source_manifest_bytes(source_raw)
    except SourceManifestError as error:
        raise _fail(str(error)) from error
    fixture_raw = _regular_file(
        fixture_manifest_path, "fixture manifest", MAX_MANIFEST_BYTES
    )
    try:
        validate_manifest_bytes(fixture_raw, repo_root)
    except FixtureSuiteError as error:
        raise _fail(str(error)) from error

    version = _workspace_version(repo_root)
    rust_toolchain = _toolchain(repo_root)
    runtime_name = f"axiograph{executable_suffix}"
    checker_name = f"axiograph_verify{executable_suffix}"
    checker_checksum = (
        f"{hashlib.sha256(checker).hexdigest()}  {checker_name}\n".encode("ascii")
    )
    payloads: dict[str, tuple[int, bytes]] = {
        runtime_name: (0o755, runtime),
        checker_name: (0o755, checker),
        "axiograph_verify.sha256": (0o644, checker_checksum),
        "axiograph-source-manifest.json": (0o644, source_raw),
    }
    manifest = release_manifest(
        host,
        version,
        rust_toolchain,
        source_manifest,
        hashlib.sha256(fixture_raw).hexdigest(),
        payloads,
    )
    files = dict(payloads)
    files["axiograph-release.json"] = (0o644, canonical_json(manifest))

    if output_dir.exists():
        metadata = output_dir.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or output_dir.is_symlink():
            raise _fail("output directory must be a real directory")
        if any(output_dir.iterdir()):
            raise _fail("output directory must be empty")
    else:
        output_dir.mkdir(parents=True)
    archive = output_dir / f"axiograph-{host}{extension}"
    if extension == ".zip":
        _write_zip(archive, files)
    else:
        _write_tar(archive, files)
    try:
        archive_digest = sha256_regular_bounded(
            archive, MAX_ARCHIVE_BYTES, "release archive"
        )
    except BoundedIoError as error:
        raise _fail(str(error)) from error
    checksum = archive.with_name(archive.name + ".sha256")
    with checksum.open("xb") as output:
        output.write(f"{archive_digest}  {archive.name}\n".encode("ascii"))
        output.flush()
        os.fsync(output.fileno())
    return archive, checksum, manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root", type=Path, default=Path(__file__).resolve().parents[1]
    )
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--checker", required=True, type=Path)
    parser.add_argument("--source-manifest", required=True, type=Path)
    parser.add_argument(
        "--fixture-manifest", type=Path, default=Path("release/fixtures.json")
    )
    parser.add_argument("--host", required=True, choices=sorted(HOSTS))
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    try:
        archive, checksum, manifest = build_bundle(
            args.repo_root.resolve(),
            _absolute_no_resolve(args.runtime),
            _absolute_no_resolve(args.checker),
            _absolute_no_resolve(args.source_manifest),
            _absolute_no_resolve(args.fixture_manifest),
            args.host,
            _absolute_no_resolve(args.output_dir),
        )
        archive_sha256 = sha256_regular_bounded(
            archive, MAX_ARCHIVE_BYTES, "release archive"
        )
    except (
        BoundedIoError,
        BundleBuildError,
        OSError,
        UnicodeDecodeError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(f"release bundle build failed: {error}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "archive": str(archive),
                "archive_sha256": archive_sha256,
                "checksum": str(checksum),
                "host": manifest["host"],
                "source_commit": manifest["source_commit"],
                "version": manifest["version"],
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
