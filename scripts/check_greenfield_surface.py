#!/usr/bin/env python3
"""Reject retired CLI aliases and commands from live examples and scripts."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
TEXT_SUFFIXES = {".md", ".py", ".sh", ".yaml", ".yml"}
SCAN_ROOTS = {"examples", "scripts", "docs/howto", "docs/tutorials"}
FORBIDDEN = {
    "bin/axiograph-cli": "the sole installed binary is bin/axiograph",
    "Compatibility alias": "greenfield builds do not publish compatibility aliases",
    "db pathdb": "PathDB publication goes through authenticated AxiStore commands",
    "materialize-axi": "bare .axi-to-.axpd materialization was removed",
    "import-chunks": "derived images are immutable; evidence overlays are explicit build inputs",
    '--cmd "load': "the REPL does not load bare .axpd files",
}


def tracked_files() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
    )
    return [
        path
        for raw in result.stdout.split(b"\0")
        if raw
        if (path := REPO_ROOT / raw.decode("utf-8")).is_file()
    ]


def in_scan_scope(path: Path) -> bool:
    relative = path.relative_to(REPO_ROOT).as_posix()
    return any(relative == root or relative.startswith(f"{root}/") for root in SCAN_ROOTS)


def check_retired_surfaces(paths: list[Path]) -> list[str]:
    errors: list[str] = []
    for path in paths:
        if path.resolve() == SELF or not in_scan_scope(path):
            continue
        if path.suffix not in TEXT_SUFFIXES:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for line_number, line in enumerate(text.splitlines(), start=1):
            for retired, replacement in FORBIDDEN.items():
                if retired in line:
                    errors.append(
                        f"{path.relative_to(REPO_ROOT)}:{line_number}: "
                        f"retired surface `{retired}`; {replacement}"
                    )
    return errors


def check_shell_syntax(paths: list[Path]) -> list[str]:
    errors: list[str] = []
    for path in paths:
        if path.suffix != ".sh":
            continue
        result = subprocess.run(
            ["bash", "-n", str(path)],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip() or "invalid shell syntax"
            errors.append(f"{path.relative_to(REPO_ROOT)}: {detail}")
    return errors


def check_ops_readme() -> list[str]:
    readme = REPO_ROOT / "scripts/ops/README.md"
    text = readme.read_text(encoding="utf-8")
    errors: list[str] = []
    for name in re.findall(r"`([^`/]+\.sh)`", text):
        target = readme.parent / name
        if not target.is_file():
            errors.append(f"scripts/ops/README.md: references missing script `{name}`")
    return errors


def main() -> int:
    paths = tracked_files()
    errors = check_retired_surfaces(paths) + check_shell_syntax(paths) + check_ops_readme()
    if errors:
        print("greenfield surface check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    shell_count = sum(path.suffix == ".sh" for path in paths)
    print(
        "greenfield surface check passed: "
        f"{shell_count} shell scripts parse; no retired binary, PathDB, or bare-load surface remains"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
