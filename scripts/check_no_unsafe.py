#!/usr/bin/env python3
"""Fail closed when first-party Rust can use unsafe code.

The compiler is the authority: every workspace package must inherit the workspace
`unsafe_code = "forbid"` lint. A lexical repository scan also catches unsafe
keywords in cfg-disabled or currently unattached Rust source files.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_ROOT = REPO_ROOT / "rust"
ROOT_MANIFEST = RUST_ROOT / "Cargo.toml"
EXCLUDED_PARTS = {
    ".codebase-index",
    ".git",
    ".lake",
    ".pi-subagents",
    "node_modules",
    "target",
}


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
    """Return lines containing Rust's `unsafe` keyword outside comments/literals."""
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
            # Skip a character literal, but do not consume a lifetime such as 'a.
            char_end = _char_literal_end(text, index)
            if char_end is not None:
                index = char_end
                continue

        if char == "_" or char.isalpha():
            end = index + 1
            while end < len(text) and (text[end] == "_" or text[end].isalnum()):
                end += 1
            token = text[index:end]
            is_raw_identifier = index >= 2 and text[index - 2 : index] == "r#"
            if token == "unsafe" and not is_raw_identifier:
                lines.append(line)
            index = end
            continue

        if char == "\n":
            line += 1
        index += 1

    return lines


def _self_test_scanner() -> None:
    harmless = """
// unsafe { ignored(); }
/* unsafe fn ignored() {} /* unsafe */ */
const WORD: &str = "unsafe";
const RAW: &str = r#"unsafe"#;
let r#unsafe = 1;
fn lifetime<'value>(value: &'value str) {}
"""
    dangerous = (
        "fn f<'a>(x: &'a str) { let c = 'x'; unsafe { call(); } }\nunsafe fn g() {}\n"
    )
    if unsafe_keyword_lines(harmless):
        raise RuntimeError(
            "unsafe scanner reported a comment, literal, or raw identifier"
        )
    if unsafe_keyword_lines(dangerous) != [1, 2]:
        raise RuntimeError("unsafe scanner failed its positive control")


def _lint_level(value: object) -> object:
    if isinstance(value, dict):
        return value.get("level")
    return value


def workspace_manifest_errors() -> tuple[list[str], int]:
    root_data = tomllib.loads(ROOT_MANIFEST.read_text(encoding="utf-8"))
    unsafe_lint = (
        root_data.get("workspace", {})
        .get("lints", {})
        .get("rust", {})
        .get("unsafe_code")
    )
    errors: list[str] = []
    if _lint_level(unsafe_lint) != "forbid":
        errors.append(
            'rust/Cargo.toml must set [workspace.lints.rust] unsafe_code = "forbid"'
        )

    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--manifest-path",
            str(ROOT_MANIFEST),
            "--format-version",
            "1",
            "--locked",
            "--no-deps",
        ],
        cwd=REPO_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        return errors + [f"cargo metadata failed: {detail}"], 0

    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        return errors + [f"cargo metadata returned invalid JSON: {error}"], 0
    workspace_members = set(metadata["workspace_members"])
    packages = [
        package
        for package in metadata["packages"]
        if package["id"] in workspace_members
    ]
    for package in packages:
        manifest = Path(package["manifest_path"])
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if not data.get("lints", {}).get("workspace"):
            relative = manifest.relative_to(REPO_ROOT)
            errors.append(f"{relative} must set [lints] workspace = true")
    return errors, len(packages)


def source_errors() -> tuple[list[str], int]:
    errors: list[str] = []
    source_count = 0
    for path in sorted(REPO_ROOT.rglob("*.rs")):
        if any(part in EXCLUDED_PARTS for part in path.parts):
            continue
        source_count += 1
        text = path.read_text(encoding="utf-8", errors="replace")
        for line in unsafe_keyword_lines(text):
            relative = path.relative_to(REPO_ROOT)
            errors.append(f"{relative}:{line}: Rust `unsafe` keyword is forbidden")
    return errors, source_count


def main() -> int:
    _self_test_scanner()
    manifest_errors, package_count = workspace_manifest_errors()
    rust_errors, source_count = source_errors()
    errors = manifest_errors + rust_errors
    if errors:
        print("first-party unsafe-code audit failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "unsafe-code audit passed: "
        f"{package_count} workspace packages inherit unsafe_code=forbid; "
        f"{source_count} Rust source files contain no unsafe keyword"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
