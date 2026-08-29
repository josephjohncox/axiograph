#!/usr/bin/env python3
"""Canonical Axiograph calendar-version policy and command-line validator."""

from __future__ import annotations

import argparse
import re
import tomllib
from dataclasses import dataclass
from datetime import date
from pathlib import Path

CALVER_PATTERN = re.compile(
    r"(?P<calendar>[1-9][0-9]{7})\.0\.(?P<sequence>0|[1-9][0-9]{0,19})"
)
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_DAILY_SEQUENCE = 999_999


class CalVerError(ValueError):
    """The release version does not satisfy the canonical CalVer policy."""


@dataclass(frozen=True)
class CalVer:
    release_date: date
    sequence: int

    def __str__(self) -> str:
        return f"{self.release_date:%Y%m%d}.0.{self.sequence}"


def parse_calver(value: object) -> CalVer:
    """Parse exactly YYYYMMDD.0.N and reject impossible calendar dates."""
    if not isinstance(value, str):
        raise CalVerError("release version must be a string")
    match = CALVER_PATTERN.fullmatch(value)
    if match is None:
        raise CalVerError("release version must use canonical YYYYMMDD.0.N CalVer")
    calendar = match.group("calendar")
    try:
        release_date = date(
            int(calendar[0:4]),
            int(calendar[4:6]),
            int(calendar[6:8]),
        )
        sequence = int(match.group("sequence"))
    except ValueError as error:
        raise CalVerError(f"release version contains an invalid date: {error}") from error
    if sequence > MAX_DAILY_SEQUENCE:
        raise CalVerError(
            f"release sequence exceeds the daily limit {MAX_DAILY_SEQUENCE}"
        )
    parsed = CalVer(release_date, sequence)
    if str(parsed) != value:
        raise CalVerError("release version is not canonically encoded")
    return parsed


def validate_release_tag(tag: str, expected_version: str) -> str:
    """Require one immutable tag spelling for an already validated version."""
    version = str(parse_calver(expected_version))
    if tag != f"v{version}":
        raise CalVerError(
            f"release tag {tag!r} does not match workspace version v{version}"
        )
    return version


def workspace_version(repo_root: Path) -> str:
    """Read the sole authoritative application version from the Rust workspace."""
    manifest = repo_root / "rust" / "Cargo.toml"
    try:
        raw = manifest.read_bytes()
    except OSError as error:
        raise CalVerError(f"cannot read Rust workspace manifest: {error}") from error
    if len(raw) > MAX_MANIFEST_BYTES:
        raise CalVerError("Rust workspace manifest exceeds the 1 MiB limit")
    try:
        data = tomllib.loads(raw.decode("utf-8"))
        value = data["workspace"]["package"]["version"]
    except (UnicodeDecodeError, tomllib.TOMLDecodeError, KeyError, TypeError) as error:
        raise CalVerError(f"Rust workspace version is unavailable: {error}") from error
    return str(parse_calver(value))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate = subparsers.add_parser("validate", help="validate one version")
    validate.add_argument("version")

    tag = subparsers.add_parser("validate-tag", help="validate tag/version equality")
    tag.add_argument("tag")
    tag.add_argument("expected_version")

    workspace = subparsers.add_parser(
        "workspace", help="print the validated Rust workspace version"
    )
    workspace.add_argument("--repo-root", type=Path, default=Path.cwd())

    arguments = parser.parse_args()
    try:
        if arguments.command == "validate":
            value = str(parse_calver(arguments.version))
        elif arguments.command == "validate-tag":
            value = validate_release_tag(arguments.tag, arguments.expected_version)
        else:
            value = workspace_version(arguments.repo_root)
    except CalVerError as error:
        parser.error(str(error))
    print(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
