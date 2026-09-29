#!/usr/bin/env python3
"""Prepare and run the root-required no-unsafe fixtures on hosted Ubuntu VMs.

Only the workflow performs privileged operations. This program prepares regular
files as the runner user, validates the resulting device and bind mount, and
runs copied production CLIs as that same unprivileged user.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any, Protocol

from scripts.bounded_subprocess import BoundedProcessError, run_bounded
from scripts.check_no_unsafe import CANDIDATE_HOMES
from scripts.generate_no_unsafe_external_cache_manifest import (
    ARCHIVE_BYTES,
    ARCHIVE_REL,
    ARCHIVE_SHA256,
    MANIFEST_REL,
    OUTPUT_BASENAME,
)

REPO = Path(__file__).resolve().parents[2]
COPIED_SCRIPTS = (
    "bounded_subprocess.py",
    "check_no_unsafe.py",
    "generate_no_unsafe_external_cache_manifest.py",
    "no_unsafe_fs.py",
)
REPORT_SCHEMA = "axiograph-hosted-no-unsafe-capability-report-v1"
CONTRACT_SCHEMA = "axiograph-hosted-no-unsafe-fixture-contract-v1"
PRIVILEGED_STATE_SCHEMA = "axiograph-hosted-no-unsafe-privileged-state-v1"
CLEANUP_REPORT_SCHEMA = "axiograph-hosted-no-unsafe-cleanup-report-v1"
EVIDENCE_MANIFEST_SCHEMA = "axiograph-hosted-no-unsafe-upload-safety-v1"
DEVICE_MAJOR = 1
DEVICE_MINOR = 3
PRIVILEGED_STATE_NAME = "privileged-fixture-state.json"
EVIDENCE_NAMES = frozenset(
    {
        "prepare.log",
        "setup.log",
        "run.log",
        "capability.json",
        "cleanup.log",
        "cleanup.json",
    }
)
MAX_EVIDENCE_FILE_BYTES = 1024 * 1024
MAX_EVIDENCE_TOTAL_BYTES = len(EVIDENCE_NAMES) * MAX_EVIDENCE_FILE_BYTES


class CapabilityFixtureError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _stable_regular_bytes(path: Path, limit: int) -> bytes:
    value = path.lstat()
    if (
        not stat.S_ISREG(value.st_mode)
        or stat.S_ISLNK(value.st_mode)
        or value.st_size > limit
    ):
        raise CapabilityFixtureError(
            f"input is not a bounded nonsymlink regular file: {path}"
        )
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_size) != (
            value.st_dev,
            value.st_ino,
            value.st_size,
        ):
            raise CapabilityFixtureError(f"input changed before open: {path}")
        chunks: list[bytes] = []
        remaining = limit + 1
        while remaining:
            chunk = os.read(descriptor, min(65_536, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        data = b"".join(chunks)
        after = os.fstat(descriptor)
        if len(data) > limit or (
            opened.st_dev,
            opened.st_ino,
            opened.st_size,
            opened.st_mtime_ns,
        ) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise CapabilityFixtureError(f"input changed during read: {path}")
        return data
    finally:
        os.close(descriptor)


def require_archive_identity(
    path: Path,
    *,
    expected_bytes: int = ARCHIVE_BYTES,
    expected_sha256: str = ARCHIVE_SHA256,
) -> None:
    value = path.lstat()
    if not stat.S_ISREG(value.st_mode) or value.st_size != expected_bytes:
        raise CapabilityFixtureError("archive is not the exact bounded regular input")
    if sha256_file(path) != expected_sha256:
        raise CapabilityFixtureError("archive SHA-256 differs from the pinned identity")


def _copy_scripts(target: Path) -> dict[str, str]:
    target.mkdir(mode=0o700)
    hashes: dict[str, str] = {}
    for name in COPIED_SCRIPTS:
        source = REPO / "scripts" / name
        destination = target / name
        shutil.copyfile(source, destination)
        hashes[name] = sha256_file(source)
    shutil.copyfile(REPO / MANIFEST_REL, target / Path(MANIFEST_REL).name)
    hashes[Path(MANIFEST_REL).name] = sha256_file(REPO / MANIFEST_REL)
    return hashes


def _run_checked(argv: list[str], cwd: Path) -> None:
    result = run_bounded(
        argv,
        timeout_seconds=60,
        max_stdout_bytes=1024 * 1024,
        max_stderr_bytes=1024 * 1024,
        cwd=cwd,
    )
    if result.returncode != 0:
        raise CapabilityFixtureError(
            f"fixture command failed ({result.returncode}): {argv!r}; "
            f"stderr={result.stderr.decode('utf-8', errors='replace')!r}"
        )


def prepare(fixture_root: Path, archive_source: Path) -> dict[str, Any]:
    if not fixture_root.is_absolute() or fixture_root.exists():
        raise CapabilityFixtureError("fixture root must be a new absolute path")
    require_archive_identity(archive_source)
    fixture_root.mkdir(mode=0o700, parents=False)

    device_repo = fixture_root / "device-repo"
    device_repo.mkdir(mode=0o700)
    device_hashes = _copy_scripts(device_repo / "scripts")
    crate = device_repo / "rust/crate/src"
    crate.mkdir(parents=True, mode=0o700)
    (device_repo / "rust/Cargo.toml").write_text(
        '[workspace]\nmembers=["crate"]\nresolver="2"\n'
        '[workspace.lints.rust]\nunsafe_code="forbid"\n',
        encoding="utf-8",
    )
    (device_repo / "rust/crate/Cargo.toml").write_text(
        '[package]\nname="hosted-capability-fixture"\nversion="0.0.0"\n'
        'edition="2024"\n[lints]\nworkspace=true\n',
        encoding="utf-8",
    )
    (crate / "lib.rs").write_text("pub fn fixture_is_safe() {}\n", encoding="utf-8")
    # The production scanner uses `cargo metadata --locked`. Generate the
    # dependency-free lock offline before Git captures the fixture baseline.
    _run_checked(
        [
            "cargo",
            "generate-lockfile",
            "--manifest-path",
            "rust/Cargo.toml",
            "--offline",
        ],
        device_repo,
    )
    _run_checked(["git", "init", "-q"], device_repo)
    _run_checked(
        ["git", "config", "user.email", "fixture@example.invalid"], device_repo
    )
    _run_checked(
        ["git", "config", "user.name", "Hosted capability fixture"], device_repo
    )
    _run_checked(["git", "add", "scripts", "rust"], device_repo)
    _run_checked(["git", "commit", "-qm", "hosted capability fixture"], device_repo)
    device_parent = device_repo / CANDIDATE_HOMES[0] / "library"
    device_parent.mkdir(parents=True, mode=0o700)

    mount_repo = fixture_root / "mount-repo"
    mount_repo.mkdir(mode=0o700)
    mount_hashes = _copy_scripts(mount_repo / "scripts-source")
    (mount_repo / "scripts").mkdir(mode=0o700)
    archive = mount_repo / ARCHIVE_REL
    archive.parent.mkdir(parents=True, mode=0o700)
    try:
        os.link(archive_source, archive)
    except OSError:
        shutil.copyfile(archive_source, archive)
    output_parent = mount_repo / "build/engineering-quality/hosted-capability/run"
    output_parent.mkdir(parents=True, mode=0o700)

    contract = {
        "schema": CONTRACT_SCHEMA,
        "prepared_uid": os.getuid(),
        "prepared_gid": os.getgid(),
        "fixture_root": str(fixture_root),
        "device_repo": str(device_repo),
        "device_path": str(device_parent / "candidate-device.rs"),
        "mount_repo": str(mount_repo),
        "mount_source": str(mount_repo / "scripts-source"),
        "mount_target": str(mount_repo / "scripts"),
        "archive": str(archive),
        "output": str(output_parent / OUTPUT_BASENAME),
        "source_hashes": {"device": device_hashes, "mount": mount_hashes},
        "archive_bytes": ARCHIVE_BYTES,
        "archive_sha256": ARCHIVE_SHA256,
    }
    contract_path = fixture_root / "fixture-contract.json"
    with contract_path.open("x", encoding="utf-8") as target:
        json.dump(contract, target, indent=2, sort_keys=True)
        target.write("\n")
    return contract


def _decode_mount_path(value: str) -> str:
    decoded: list[str] = []
    index = 0
    escapes = {"040": " ", "011": "\t", "012": "\n", "134": "\\"}
    while index < len(value):
        if value[index] != "\\":
            decoded.append(value[index])
            index += 1
            continue
        code = value[index + 1 : index + 4]
        if len(code) != 3 or code not in escapes:
            raise CapabilityFixtureError(
                "mountinfo path contains an unsupported escape"
            )
        decoded.append(escapes[code])
        index += 4
    return "".join(decoded)


def mount_observation(
    mountinfo: str,
    target: Path,
    *,
    required_options: frozenset[str] = frozenset({"ro", "nosuid", "nodev", "noexec"}),
) -> dict[str, Any]:
    matches: list[dict[str, Any]] = []
    for line in mountinfo.splitlines():
        fields = line.split()
        if len(fields) < 10 or "-" not in fields:
            continue
        separator = fields.index("-")
        if separator < 6 or len(fields) <= separator + 2:
            continue
        if _decode_mount_path(fields[4]) == str(target):
            root = _decode_mount_path(fields[3])
            if not root.startswith("/"):
                raise CapabilityFixtureError("mountinfo root is not absolute")
            matches.append(
                {
                    "mount_id": int(fields[0]),
                    "parent_id": int(fields[1]),
                    "device": fields[2],
                    "root": root,
                    "target": _decode_mount_path(fields[4]),
                    "options": sorted(fields[5].split(",")),
                    "filesystem": fields[separator + 1],
                    "mount_source": _decode_mount_path(fields[separator + 2]),
                }
            )
    if len(matches) != 1:
        raise CapabilityFixtureError(
            f"expected one observed owned mount, found {len(matches)}"
        )
    missing = required_options - set(matches[0]["options"])
    if missing:
        raise CapabilityFixtureError(
            f"owned mount is missing options: {sorted(missing)}"
        )
    return matches[0]


def validate_device(path: Path) -> dict[str, Any]:
    value = path.lstat()
    if not stat.S_ISCHR(value.st_mode):
        raise CapabilityFixtureError("fixture is not a real character device")
    if (os.major(value.st_rdev), os.minor(value.st_rdev)) != (
        DEVICE_MAJOR,
        DEVICE_MINOR,
    ):
        raise CapabilityFixtureError("fixture device identity differs from 1:3")
    if stat.S_IMODE(value.st_mode) != 0:
        raise CapabilityFixtureError(
            "fixture device must deny all data-open permissions"
        )
    if value.st_uid == os.getuid() or os.getuid() == 0:
        raise CapabilityFixtureError("fixture setup and test UIDs are not separated")
    return {
        "type": "character-device",
        "major": os.major(value.st_rdev),
        "minor": os.minor(value.st_rdev),
        "mode": oct(stat.S_IMODE(value.st_mode)),
        "owner_uid": value.st_uid,
        "test_uid": os.getuid(),
    }


def _read_contract(fixture_root: Path) -> dict[str, Any]:
    if not fixture_root.is_absolute():
        raise CapabilityFixtureError("fixture root must be absolute")
    contract = json.loads(
        (fixture_root / "fixture-contract.json").read_text(encoding="utf-8")
    )
    if contract.get("schema") != CONTRACT_SCHEMA or contract.get("fixture_root") != str(
        fixture_root
    ):
        raise CapabilityFixtureError("fixture contract identity differs")
    if contract.get("prepared_uid") != os.getuid() or os.getuid() == 0:
        raise CapabilityFixtureError("tests must run as the preparing non-root user")
    for key in (
        "device_repo",
        "device_path",
        "mount_repo",
        "mount_source",
        "mount_target",
        "archive",
        "output",
    ):
        value = Path(contract[key])
        if not value.is_absolute() or not value.is_relative_to(fixture_root):
            raise CapabilityFixtureError(f"contract path escapes fixture root: {key}")
    return contract


def scanner_invocation(contract: dict[str, Any]) -> tuple[list[str], Path]:
    device_repo = Path(contract["device_repo"])
    return (
        [sys.executable, "-B", str(device_repo / "scripts/check_no_unsafe.py")],
        device_repo,
    )


def generator_invocation(contract: dict[str, Any]) -> tuple[list[str], Path]:
    mount_repo = Path(contract["mount_repo"])
    return (
        [
            sys.executable,
            "-B",
            str(
                Path(contract["mount_target"])
                / "generate_no_unsafe_external_cache_manifest.py"
            ),
            "--archive",
            str(Path(contract["archive"])),
            "--out",
            str(Path(contract["output"])),
            "--check-manifest",
            str(mount_repo / MANIFEST_REL),
        ],
        mount_repo,
    )


def _directory_identity(path: Path) -> tuple[int, int]:
    value = path.lstat()
    if not stat.S_ISDIR(value.st_mode):
        raise CapabilityFixtureError(
            f"bind fixture path is not a real directory: {path}"
        )
    return value.st_dev, value.st_ino


def validate_bind_mount(
    source: Path,
    target: Path,
    mountinfo: str,
    *,
    required_options: frozenset[str] = frozenset({"ro", "nosuid", "nodev", "noexec"}),
    expected: dict[str, Any] | None = None,
) -> dict[str, Any]:
    source_identity = _directory_identity(source)
    target_identity = _directory_identity(target)
    if source_identity != target_identity:
        raise CapabilityFixtureError("bind source and mounted target identities differ")
    observation = mount_observation(
        mountinfo,
        target,
        required_options=required_options,
    )
    expected_device = f"{os.major(source_identity[0])}:{os.minor(source_identity[0])}"
    if observation["device"] != expected_device:
        raise CapabilityFixtureError(
            "mountinfo device differs from the bound directory"
        )
    observation["source_identity"] = list(source_identity)
    observation["target_identity"] = list(target_identity)
    if expected is not None:
        for key in ("mount_id", "device", "root", "source_identity"):
            if observation.get(key) != expected.get(key):
                raise CapabilityFixtureError(f"mounted identity changed: {key}")
    return observation


def _owned_directory_identity(path: Path, owner_uid: int) -> tuple[int, int]:
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise CapabilityFixtureError(
            f"privileged fixture directory path is linked or noncanonical: {path}"
        )
    value = path.lstat()
    if not stat.S_ISDIR(value.st_mode) or stat.S_ISLNK(value.st_mode):
        raise CapabilityFixtureError(
            f"privileged fixture parent is not a real directory: {path}"
        )
    if value.st_uid != owner_uid:
        raise CapabilityFixtureError(f"privileged fixture parent owner differs: {path}")
    return value.st_dev, value.st_ino


def _privileged_contract(fixture_root: Path) -> dict[str, Any]:
    if (
        not fixture_root.is_absolute()
        or fixture_root.resolve(strict=True) != fixture_root
    ):
        raise CapabilityFixtureError(
            "privileged fixture root must be an existing real absolute directory"
        )
    contract_path = fixture_root / "fixture-contract.json"
    try:
        contract = json.loads(_stable_regular_bytes(contract_path, 65_536))
    except UnicodeDecodeError as error:
        raise CapabilityFixtureError(
            "privileged fixture contract is not UTF-8"
        ) from error
    owner_uid = contract.get("prepared_uid")
    if (
        contract.get("schema") != CONTRACT_SCHEMA
        or contract.get("fixture_root") != str(fixture_root)
        or not isinstance(owner_uid, int)
        or owner_uid <= 0
    ):
        raise CapabilityFixtureError("privileged fixture contract identity differs")
    expected = {
        "device_repo": fixture_root / "device-repo",
        "device_path": fixture_root
        / "device-repo"
        / CANDIDATE_HOMES[0]
        / "library/candidate-device.rs",
        "mount_repo": fixture_root / "mount-repo",
        "mount_source": fixture_root / "mount-repo/scripts-source",
        "mount_target": fixture_root / "mount-repo/scripts",
    }
    for key, path in expected.items():
        if contract.get(key) != str(path):
            raise CapabilityFixtureError(
                f"privileged fixture contract path differs: {key}"
            )
    root_identity = _owned_directory_identity(fixture_root, owner_uid)
    if stat.S_IMODE(fixture_root.lstat().st_mode) & 0o077:
        raise CapabilityFixtureError(
            "privileged fixture root permissions are too broad"
        )
    directory_identities = {
        key: list(_owned_directory_identity(path, owner_uid))
        for key, path in expected.items()
        if key != "device_path"
    }
    device_parent = expected["device_path"].parent
    directory_identities["device_parent"] = list(
        _owned_directory_identity(device_parent, owner_uid)
    )
    contract["privileged_expected"] = {key: str(path) for key, path in expected.items()}
    contract["root_identity"] = list(root_identity)
    contract["directory_identities"] = directory_identities
    return contract


def _mountinfo() -> str:
    return Path("/proc/self/mountinfo").read_text(encoding="utf-8")


def _run_privileged_command(argv: list[str]) -> None:
    result = run_bounded(
        argv,
        timeout_seconds=30,
        max_stdout_bytes=65_536,
        max_stderr_bytes=65_536,
        cwd=Path("/"),
        env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"},
    )
    if result.returncode != 0:
        raise CapabilityFixtureError(
            f"privileged fixture command failed ({result.returncode}): {argv!r}; "
            f"stderr={result.stderr.decode('utf-8', errors='replace')!r}"
        )


def _device_record(path: Path) -> dict[str, Any]:
    value = path.lstat()
    if not stat.S_ISCHR(value.st_mode):
        raise CapabilityFixtureError("created fixture is not a character device")
    major_minor = (os.major(value.st_rdev), os.minor(value.st_rdev))
    if major_minor != (DEVICE_MAJOR, DEVICE_MINOR) or stat.S_IMODE(value.st_mode) != 0:
        raise CapabilityFixtureError("created fixture device identity or mode differs")
    return {
        "path": str(path),
        "st_dev": value.st_dev,
        "st_ino": value.st_ino,
        "st_rdev": value.st_rdev,
        "major": major_minor[0],
        "minor": major_minor[1],
        "mode": oct(stat.S_IMODE(value.st_mode)),
        "owner_uid": value.st_uid,
    }


def _device_matches(path: Path, record: dict[str, Any]) -> bool:
    try:
        value = path.lstat()
    except FileNotFoundError:
        return False
    return (
        stat.S_ISCHR(value.st_mode)
        and value.st_dev == record.get("st_dev")
        and value.st_ino == record.get("st_ino")
        and value.st_rdev == record.get("st_rdev")
        and (os.major(value.st_rdev), os.minor(value.st_rdev))
        == (DEVICE_MAJOR, DEVICE_MINOR)
    )


def _persist_privileged_state(path: Path, payload: dict[str, Any]) -> None:
    raw = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode("utf-8")
    if len(raw) > 65_536:
        raise CapabilityFixtureError("privileged fixture state exceeds its bound")
    if path.exists() or path.is_symlink():
        current = path.lstat()
        if (
            not stat.S_ISREG(current.st_mode)
            or stat.S_ISLNK(current.st_mode)
            or current.st_size > 65_536
            or current.st_uid != os.geteuid()
            or stat.S_IMODE(current.st_mode) != 0o600
        ):
            raise CapabilityFixtureError(
                "privileged fixture state is not an owned bounded regular file"
            )
        previous = json.loads(_stable_regular_bytes(path, 65_536))
        if previous.get("schema") != PRIVILEGED_STATE_SCHEMA or previous.get(
            "fixture_root"
        ) != payload.get("fixture_root"):
            raise CapabilityFixtureError("privileged fixture state identity differs")
    temporary = path.with_name(path.name + ".tmp")
    descriptor = os.open(
        temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600
    )
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as target:
            target.write(raw)
            target.flush()
            os.fsync(target.fileno())
    finally:
        os.close(descriptor)
    os.replace(temporary, path)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def _mount_target_pre_identity(
    record: dict[str, Any], *, expected: tuple[int, int] | None = None
) -> tuple[int, int]:
    value = record.get("target_pre_identity")
    if (
        not isinstance(value, list)
        or len(value) != 2
        or any(not isinstance(item, int) or item <= 0 for item in value)
    ):
        raise CapabilityFixtureError("mount target pre-identity differs")
    identity = tuple(value)
    if expected is not None and identity != expected:
        raise CapabilityFixtureError("mount target pre-identity differs")
    return identity


class PrivilegedFixtureOperations(Protocol):
    def create_device(self, contract: dict[str, Any]) -> dict[str, Any]: ...
    def create_mount(self, contract: dict[str, Any]) -> dict[str, Any]: ...
    def harden_mount(
        self, contract: dict[str, Any], record: dict[str, Any]
    ) -> dict[str, Any]: ...
    def remove_device(
        self, contract: dict[str, Any], record: dict[str, Any]
    ) -> None: ...
    def unmount(self, contract: dict[str, Any], record: dict[str, Any]) -> None: ...


class RealPrivilegedFixtureOperations:
    def create_device(self, contract: dict[str, Any]) -> dict[str, Any]:
        path = Path(contract["device_path"])
        parent = path.parent
        expected_parent = tuple(contract["directory_identities"]["device_parent"])
        if _directory_identity(parent) != expected_parent or os.path.lexists(path):
            raise CapabilityFixtureError(
                "device path or parent changed before creation"
            )
        os.mknod(path, stat.S_IFCHR, os.makedev(DEVICE_MAJOR, DEVICE_MINOR))
        observed: dict[str, Any] | None = None
        try:
            observed = _device_record(path)
            observed["parent_identity"] = list(expected_parent)
            os.chmod(path, 0)
            final = _device_record(path)
            final["parent_identity"] = list(expected_parent)
            if final != observed or _directory_identity(parent) != expected_parent:
                raise CapabilityFixtureError("device identity changed during creation")
            return final
        except (CapabilityFixtureError, OSError, ValueError) as error:
            cleanup = "device identity was not established; refusing unrecorded removal"
            if observed is not None:
                try:
                    if _directory_identity(
                        parent
                    ) == expected_parent and _device_matches(path, observed):
                        os.unlink(path)
                        cleanup = "observed device was rolled back"
                    else:
                        cleanup = "device identity changed; refusing removal"
                except OSError as cleanup_error:
                    cleanup = f"observed device rollback failed: {cleanup_error}"
            raise CapabilityFixtureError(
                f"device creation failed: {error}; {cleanup}"
            ) from error

    def create_mount(self, contract: dict[str, Any]) -> dict[str, Any]:
        source = Path(contract["mount_source"])
        target = Path(contract["mount_target"])
        source_before = tuple(contract["directory_identities"]["mount_source"])
        target_before = tuple(contract["directory_identities"]["mount_target"])
        if (
            _directory_identity(source) != source_before
            or _directory_identity(target) != target_before
        ):
            raise CapabilityFixtureError("mount source or target changed before bind")
        _run_privileged_command(["/usr/bin/mount", "--bind", str(source), str(target)])
        try:
            record = validate_bind_mount(
                source,
                target,
                _mountinfo(),
                required_options=frozenset(),
            )
            record["target_pre_identity"] = list(target_before)
            return record
        except (CapabilityFixtureError, OSError, ValueError) as error:
            try:
                observed = validate_bind_mount(
                    source,
                    target,
                    _mountinfo(),
                    required_options=frozenset(),
                )
            except (CapabilityFixtureError, OSError, ValueError) as observation_error:
                raise CapabilityFixtureError(
                    "bind observation failed and the created mount identity is ambiguous; "
                    f"refusing unmount: {observation_error}"
                ) from error
            try:
                _run_privileged_command(["/usr/bin/umount", "--", str(target)])
            except (CapabilityFixtureError, OSError, ValueError) as rollback_error:
                raise CapabilityFixtureError(
                    f"bind observation failed; observed mount rollback failed: {rollback_error}"
                ) from error
            raise CapabilityFixtureError(
                f"bind observation failed and observed mount {observed['mount_id']} was rolled back: {error}"
            ) from error

    def harden_mount(
        self,
        contract: dict[str, Any],
        record: dict[str, Any],
    ) -> dict[str, Any]:
        target = Path(contract["mount_target"])
        target_pre_identity = _mount_target_pre_identity(
            record,
            expected=tuple(contract["directory_identities"]["mount_target"]),
        )
        _run_privileged_command(
            [
                "/usr/bin/mount",
                "-o",
                "remount,bind,ro,nosuid,nodev,noexec",
                str(target),
            ]
        )
        hardened = validate_bind_mount(
            Path(contract["mount_source"]),
            target,
            _mountinfo(),
            expected=record,
        )
        hardened["target_pre_identity"] = list(target_pre_identity)
        return hardened

    def remove_device(self, contract: dict[str, Any], record: dict[str, Any]) -> None:
        path = Path(contract["device_path"])
        if _directory_identity(path.parent) != tuple(
            record.get("parent_identity", ())
        ) or not _device_matches(path, record):
            raise CapabilityFixtureError(
                "refusing to remove an absent or replaced device"
            )
        os.unlink(path)

    def unmount(self, contract: dict[str, Any], record: dict[str, Any]) -> None:
        target = Path(contract["mount_target"])
        # While mounted, the target exposes the source identity. Validate the
        # persisted pre-mount identity after unmount, not against that view.
        target_pre_identity = _mount_target_pre_identity(record)
        validate_bind_mount(
            Path(contract["mount_source"]),
            target,
            _mountinfo(),
            required_options=frozenset(),
            expected=record,
        )
        _run_privileged_command(["/usr/bin/umount", "--", str(target)])
        if _directory_identity(target) != target_pre_identity:
            raise CapabilityFixtureError("mount target identity changed after unmount")


def setup_privileged(
    fixture_root: Path,
    *,
    operations: PrivilegedFixtureOperations | None = None,
    persist: Callable[[Path, dict[str, Any]], None] = _persist_privileged_state,
) -> dict[str, Any]:
    contract = _privileged_contract(fixture_root)
    state_path = fixture_root / PRIVILEGED_STATE_NAME
    if state_path.exists() or state_path.is_symlink():
        raise CapabilityFixtureError("privileged fixture state already exists")
    operations = RealPrivilegedFixtureOperations() if operations is None else operations
    state: dict[str, Any] = {
        "schema": PRIVILEGED_STATE_SCHEMA,
        "fixture_root": str(fixture_root),
        "prepared_uid": contract["prepared_uid"],
        "setup_uid": os.geteuid(),
        "phase": "starting",
        "device": None,
        "mount": None,
        "rollback_errors": [],
        "guarantee": "observed-operation identity only; not crash-atomic and not a future-path promise",
    }
    device: dict[str, Any] | None = None
    mount: dict[str, Any] | None = None
    device_started = False
    mount_started = False
    try:
        device_started = True
        device = operations.create_device(contract)
        state["device"] = device
        state["phase"] = "device-observed"
        persist(state_path, state)
        mount_started = True
        mount = operations.create_mount(contract)
        state["mount"] = mount
        state["phase"] = "mount-observed"
        persist(state_path, state)
        state["mount"] = operations.harden_mount(contract, mount)
        state["phase"] = "ready"
        persist(state_path, state)
        return state
    except (CapabilityFixtureError, OSError, ValueError) as error:
        rollback_errors: list[str] = []
        if device_started and device is None:
            rollback_errors.append(
                "device creation returned no observed identity; cleanup must inspect the exact path"
            )
        if mount_started and mount is None:
            rollback_errors.append(
                "mount creation returned no observed identity; cleanup must inspect the exact target"
            )
        if mount is not None:
            try:
                operations.unmount(contract, mount)
            except (CapabilityFixtureError, OSError, ValueError) as rollback_error:
                rollback_errors.append(f"mount rollback: {rollback_error}")
        if device is not None:
            try:
                operations.remove_device(contract, device)
            except (CapabilityFixtureError, OSError, ValueError) as rollback_error:
                rollback_errors.append(f"device rollback: {rollback_error}")
        state["phase"] = "rolled-back" if not rollback_errors else "rollback-incomplete"
        state["rollback_errors"] = rollback_errors
        state["setup_error"] = str(error)
        try:
            persist(state_path, state)
        except (CapabilityFixtureError, OSError, ValueError) as persist_error:
            rollback_errors.append(f"failure-state persistence: {persist_error}")
        detail = "; ".join(rollback_errors) if rollback_errors else "rollback complete"
        raise CapabilityFixtureError(
            f"privileged setup failed: {error}; {detail}"
        ) from error


def _load_privileged_state(path: Path, fixture_root: Path) -> dict[str, Any] | None:
    if not path.exists() and not path.is_symlink():
        return None
    value = path.lstat()
    if (
        not stat.S_ISREG(value.st_mode)
        or stat.S_ISLNK(value.st_mode)
        or value.st_size > 65_536
        or value.st_uid != os.geteuid()
        or stat.S_IMODE(value.st_mode) != 0o600
    ):
        raise CapabilityFixtureError(
            "privileged fixture state is not an owned bounded regular file"
        )
    state = json.loads(_stable_regular_bytes(path, 65_536))
    if state.get("schema") != PRIVILEGED_STATE_SCHEMA or state.get(
        "fixture_root"
    ) != str(fixture_root):
        raise CapabilityFixtureError("privileged fixture state identity differs")
    return state


def _target_has_mount(target: Path) -> bool:
    try:
        mount_observation(_mountinfo(), target, required_options=frozenset())
        return True
    except CapabilityFixtureError as error:
        if "found 0" in str(error):
            return False
        raise


def cleanup_privileged(
    fixture_root: Path,
    *,
    operations: PrivilegedFixtureOperations | None = None,
) -> dict[str, Any]:
    contract = _privileged_contract(fixture_root)
    operations = RealPrivilegedFixtureOperations() if operations is None else operations
    state = _load_privileged_state(fixture_root / PRIVILEGED_STATE_NAME, fixture_root)
    device_path = Path(contract["device_path"])
    target = Path(contract["mount_target"])
    if state is None:
        if os.path.lexists(device_path) or _target_has_mount(target):
            raise CapabilityFixtureError(
                "refusing cleanup of an unrecorded privileged object"
            )
        return {"status": "clean", "mount": "absent", "device": "absent"}
    result = {"status": "clean", "mount": "absent", "device": "absent"}
    mount = state.get("mount")
    if _target_has_mount(target):
        if not isinstance(mount, dict):
            raise CapabilityFixtureError("refusing to unmount an unrecorded mount")
        operations.unmount(contract, mount)
        result["mount"] = "unmounted-recorded"
    if os.path.lexists(device_path):
        device = state.get("device")
        if not isinstance(device, dict):
            raise CapabilityFixtureError("refusing to remove an unrecorded device")
        operations.remove_device(contract, device)
        result["device"] = "removed-recorded"
    return result


def _bounded_regular_bytes(path: Path) -> bytes:
    value = path.lstat()
    if not stat.S_ISREG(value.st_mode) or stat.S_ISLNK(value.st_mode):
        raise CapabilityFixtureError(
            f"evidence input is not a nonsymlink regular file: {path.name}"
        )
    if value.st_size > MAX_EVIDENCE_FILE_BYTES:
        raise CapabilityFixtureError(f"evidence input exceeds its bound: {path.name}")
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_size) != (
            value.st_dev,
            value.st_ino,
            value.st_size,
        ):
            raise CapabilityFixtureError(
                f"evidence input changed before open: {path.name}"
            )
        chunks: list[bytes] = []
        remaining = MAX_EVIDENCE_FILE_BYTES + 1
        while remaining:
            chunk = os.read(descriptor, min(65_536, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        data = b"".join(chunks)
        after = os.fstat(descriptor)
        if len(data) > MAX_EVIDENCE_FILE_BYTES or (
            opened.st_dev,
            opened.st_ino,
            opened.st_size,
            opened.st_mtime_ns,
        ) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise CapabilityFixtureError(
                f"evidence input changed during read: {path.name}"
            )
        return data
    finally:
        os.close(descriptor)


def collect_evidence(source_dir: Path, output_dir: Path) -> dict[str, Any]:
    if (
        not source_dir.is_absolute()
        or not output_dir.is_absolute()
        or source_dir.parent != output_dir.parent
        or source_dir.resolve(strict=True) != source_dir
        or output_dir.parent.resolve(strict=True) != output_dir.parent
        or output_dir.exists()
        or output_dir.is_symlink()
    ):
        raise CapabilityFixtureError(
            "evidence directories must be canonical absolute siblings and output must be new"
        )
    source_value = source_dir.lstat()
    if not stat.S_ISDIR(source_value.st_mode) or stat.S_ISLNK(source_value.st_mode):
        raise CapabilityFixtureError("evidence source is not a real directory")
    inputs: dict[str, bytes] = {}
    total = 0
    for path in source_dir.iterdir():
        if path.name not in EVIDENCE_NAMES:
            raise CapabilityFixtureError(f"unexpected evidence entry: {path.name}")
        data = _bounded_regular_bytes(path)
        total += len(data)
        if total > MAX_EVIDENCE_TOTAL_BYTES:
            raise CapabilityFixtureError("evidence inputs exceed the aggregate bound")
        inputs[path.name] = data
    output_dir.mkdir(mode=0o700)
    files: list[dict[str, Any]] = []
    try:
        for name in sorted(inputs):
            destination = output_dir / name
            with destination.open("xb") as target:
                target.write(inputs[name])
            files.append(
                {
                    "name": name,
                    "bytes": len(inputs[name]),
                    "sha256": hashlib.sha256(inputs[name]).hexdigest(),
                }
            )
        manifest = {
            "schema": EVIDENCE_MANIFEST_SCHEMA,
            "upload_safety": "pass",
            "semantic_assessment": "not-performed-by-upload-safety-validator",
            "present": files,
            "missing": sorted(EVIDENCE_NAMES - inputs.keys()),
        }
        _write_report(output_dir / "evidence-manifest.json", manifest)
        return manifest
    except (CapabilityFixtureError, OSError, ValueError):
        for path in output_dir.iterdir():
            if path.is_file() and not path.is_symlink():
                path.unlink()
        output_dir.rmdir()
        raise


def record_cleanup(log_path: Path, exit_code: int, report_path: Path) -> dict[str, Any]:
    if (
        exit_code < 0
        or exit_code > 255
        or not log_path.is_absolute()
        or not report_path.is_absolute()
        or log_path.parent != report_path.parent
        or log_path.name != "cleanup.log"
        or report_path.name != "cleanup.json"
        or log_path.parent.resolve(strict=True) != log_path.parent
        or report_path.exists()
        or report_path.is_symlink()
    ):
        raise CapabilityFixtureError("cleanup report arguments are invalid")
    data = _bounded_regular_bytes(log_path)
    report = {
        "schema": CLEANUP_REPORT_SCHEMA,
        "cleanup_exit": exit_code,
        "cleanup_status": "clean" if exit_code == 0 else "failed",
        "cleanup_log": {
            "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        },
        "authority": "observed cleanup command result only",
    }
    _write_report(report_path, report)
    return report


def _verify_copied_sources(contract: dict[str, Any]) -> None:
    for fixture, directory_key in (
        ("device", "device_repo"),
        ("mount", "mount_source"),
    ):
        scripts = Path(contract[directory_key]) / (
            "scripts" if fixture == "device" else ""
        )
        for name, expected in contract["source_hashes"][fixture].items():
            copied = scripts / name
            source = REPO / "scripts" / name
            if sha256_file(copied) != expected or sha256_file(source) != expected:
                raise CapabilityFixtureError(
                    f"copied production bytes differ: {fixture}/{name}"
                )


def _run_cli(argv: list[str], cwd: Path, timeout: float) -> dict[str, Any]:
    allowed_environment = {
        key: os.environ[key]
        for key in ("HOME", "PATH", "LANG", "LC_ALL", "CARGO_HOME", "RUSTUP_HOME")
        if key in os.environ
    }
    started = time.monotonic()
    try:
        result = run_bounded(
            argv,
            timeout_seconds=timeout,
            max_stdout_bytes=1024 * 1024,
            max_stderr_bytes=1024 * 1024,
            cwd=cwd,
            env=allowed_environment,
        )
    except (BoundedProcessError, subprocess.TimeoutExpired, OSError) as error:
        raise CapabilityFixtureError(
            f"bounded CLI execution failed: {error}"
        ) from error
    return {
        "argv": argv,
        "cwd": str(cwd),
        "allowed_environment": dict(sorted(allowed_environment.items())),
        "exit": result.returncode,
        "elapsed_seconds": round(time.monotonic() - started, 6),
        "stdout": result.stdout.decode("utf-8", errors="replace"),
        "stderr": result.stderr.decode("utf-8", errors="replace"),
    }


def _write_report(report_path: Path, report: dict[str, Any]) -> None:
    report_path.parent.mkdir(parents=True, exist_ok=True)
    with report_path.open("x", encoding="utf-8") as target:
        json.dump(report, target, indent=2, sort_keys=True)
        target.write("\n")


def _fail_report(report_path: Path, report: dict[str, Any], message: str) -> None:
    report["result"] = "fail"
    report["failure"] = message
    _write_report(report_path, report)
    raise CapabilityFixtureError(message)


def verify_and_run(fixture_root: Path, report_path: Path) -> dict[str, Any]:
    if report_path.exists():
        raise CapabilityFixtureError("report path already exists")
    contract = _read_contract(fixture_root)
    _verify_copied_sources(contract)
    require_archive_identity(Path(contract["archive"]))
    device = validate_device(Path(contract["device_path"]))
    mount = validate_bind_mount(
        Path(contract["mount_source"]),
        Path(contract["mount_target"]),
        Path("/proc/self/mountinfo").read_text(encoding="utf-8"),
    )

    report: dict[str, Any] = {
        "schema": REPORT_SCHEMA,
        "result": "running",
        "scope": ["TYPE-05 real-device CLI", "CHECK-03 real-mount-crossing CLI"],
        "non_claims": [
            "not a full no-unsafe suite result",
            "not roadmap acceptance",
            "not a release or publication decision",
        ],
        "runner": {
            "architecture": os.uname().machine,
            "kernel": os.uname().release,
            "uid": os.getuid(),
            "gid": os.getgid(),
            "ci_context": {
                key: os.environ[key]
                for key in ("GITHUB_SHA", "GITHUB_REF", "RUNNER_ARCH", "RUNNER_OS")
                if key in os.environ
            },
        },
        "device": device,
        "mount": mount,
        "archive": {
            "bytes": ARCHIVE_BYTES,
            "sha256": ARCHIVE_SHA256,
            "payload_executed": False,
            "production_parser_completed_before_mount_rejection": False,
        },
    }

    scanner_argv, device_repo = scanner_invocation(contract)
    scanner = _run_cli(scanner_argv, device_repo, 180)
    expected_device = (
        f"candidate special file: {CANDIDATE_HOMES[0]}/library/candidate-device.rs"
    )
    report["scanner"] = scanner
    if scanner["exit"] != 1:
        _fail_report(report_path, report, "device scanner CLI did not reject")
    if (
        "[E_FS_NONREGULAR]" not in scanner["stderr"]
        or expected_device not in scanner["stderr"]
    ):
        _fail_report(
            report_path,
            report,
            "device scanner CLI did not report the exact leaf cause",
        )
    for forbidden in ("[E_FS_DATA_OPEN]", "Traceback", '"external_cache_rust": 28'):
        if forbidden in scanner["stderr"]:
            _fail_report(
                report_path,
                report,
                f"device scanner emitted forbidden evidence: {forbidden}",
            )
    if "coverage_complete=false; exemptions=0" not in scanner["stderr"]:
        _fail_report(
            report_path,
            report,
            "device scanner did not deny the whole candidate exemption",
        )

    generator_argv, mount_repo = generator_invocation(contract)
    generator = _run_cli(generator_argv, mount_repo, 600)
    report["generator"] = generator
    if generator["exit"] != 1 or "[E_REGEN_CONFINEMENT]" not in generator["stderr"]:
        _fail_report(
            report_path,
            report,
            "mounted checked-manifest parent did not produce confinement rejection",
        )
    for forbidden in (
        "[E_REGEN_CHECK_OPEN]",
        "[E_REGEN_CHECK_LINK]",
        "[E_REGEN_CHECK_RACE]",
        "[E_REGEN_PATH]",
        "[E_REGEN_ROOT_BINDING]",
        "[E_REGEN_ARCHIVE_",
        "Traceback",
    ):
        if forbidden in generator["stderr"]:
            _fail_report(
                report_path,
                report,
                f"mount generator emitted an earlier or substituted cause: {forbidden}",
            )
    if Path(contract["output"]).exists():
        _fail_report(report_path, report, "rejected mount fixture created an output")
    report["archive"]["production_parser_completed_before_mount_rejection"] = True

    git_head = _run_cli(["git", "rev-parse", "HEAD"], REPO, 30)
    git_tree = _run_cli(["git", "rev-parse", "HEAD^{tree}"], REPO, 30)
    tool_identities = {
        "python": _run_cli([sys.executable, "--version"], REPO, 30),
        "git": _run_cli(["git", "--version"], REPO, 30),
        "cargo": _run_cli(["cargo", "--version"], REPO, 30),
        "rustc": _run_cli(["rustc", "--version", "--verbose"], REPO, 30),
    }
    if (
        git_head["exit"] != 0
        or git_tree["exit"] != 0
        or any(identity["exit"] != 0 for identity in tool_identities.values())
    ):
        _fail_report(
            report_path,
            report,
            "could not bind capability report to exact source and tools",
        )
    report["result"] = "pass"
    report["source"] = {
        "commit": git_head["stdout"].strip(),
        "head_tree": git_tree["stdout"].strip(),
        "harness_sha256": sha256_file(Path(__file__)),
        "workflow_sha256": sha256_file(
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ),
    }
    report["tool_identities"] = tool_identities
    _write_report(report_path, report)
    return report


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--fixture-root", type=Path, required=True)
    prepare_parser.add_argument("--archive", type=Path, required=True)
    setup_parser = subparsers.add_parser("setup-privileged")
    setup_parser.add_argument("--fixture-root", type=Path, required=True)
    cleanup_parser = subparsers.add_parser("cleanup-privileged")
    cleanup_parser.add_argument("--fixture-root", type=Path, required=True)
    run_parser = subparsers.add_parser("verify-and-run")
    run_parser.add_argument("--fixture-root", type=Path, required=True)
    run_parser.add_argument("--report", type=Path, required=True)
    record_parser = subparsers.add_parser("record-cleanup")
    record_parser.add_argument("--log", type=Path, required=True)
    record_parser.add_argument("--exit-code", type=int, required=True)
    record_parser.add_argument("--report", type=Path, required=True)
    evidence_parser = subparsers.add_parser("collect-evidence")
    evidence_parser.add_argument("--source-dir", type=Path, required=True)
    evidence_parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    arguments = _parse_args(sys.argv[1:] if argv is None else argv)
    try:
        if arguments.command == "prepare":
            if os.geteuid() == 0:
                raise CapabilityFixtureError(
                    "fixture preparation must run as an ordinary user"
                )
            value = prepare(arguments.fixture_root, arguments.archive)
        elif arguments.command == "setup-privileged":
            if os.geteuid() != 0:
                raise CapabilityFixtureError(
                    "privileged setup requires the fixture-only sudo boundary"
                )
            value = setup_privileged(arguments.fixture_root)
        elif arguments.command == "cleanup-privileged":
            if os.geteuid() != 0:
                raise CapabilityFixtureError(
                    "privileged cleanup requires the fixture-only sudo boundary"
                )
            value = cleanup_privileged(arguments.fixture_root)
        elif arguments.command == "verify-and-run":
            if os.geteuid() == 0:
                raise CapabilityFixtureError(
                    "production CLIs must run as an ordinary user"
                )
            value = verify_and_run(arguments.fixture_root, arguments.report)
        elif arguments.command == "record-cleanup":
            if os.geteuid() == 0:
                raise CapabilityFixtureError(
                    "cleanup recording must run as an ordinary user"
                )
            value = record_cleanup(arguments.log, arguments.exit_code, arguments.report)
        else:
            if os.geteuid() == 0:
                raise CapabilityFixtureError(
                    "evidence collection must run as an ordinary user"
                )
            value = collect_evidence(arguments.source_dir, arguments.output_dir)
    except (CapabilityFixtureError, OSError, ValueError, json.JSONDecodeError) as error:
        if arguments.command == "verify-and-run" and not arguments.report.exists():
            failure = {
                "schema": REPORT_SCHEMA,
                "result": "fail",
                "failure": str(error),
                "runner": {
                    "architecture": os.uname().machine,
                    "kernel": os.uname().release,
                    "uid": os.getuid(),
                    "gid": os.getgid(),
                },
            }
            try:
                _write_report(arguments.report, failure)
            except OSError as report_error:
                print(f"[E_HOSTED_CAPABILITY_REPORT] {report_error}", file=sys.stderr)
        print(f"[E_HOSTED_CAPABILITY_FIXTURE] {error}", file=sys.stderr)
        return 1
    print(json.dumps(value, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
