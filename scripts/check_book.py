#!/usr/bin/env python3
"""Validate the curated mdBook graph before publishing it."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
SUMMARY = DOCS / "SUMMARY.md"
CHAPTER_RE = re.compile(r"^\s*-\s+\[[^]]+\]\(([^)]+)\)\s*$")
LINK_RE = re.compile(r"!?\[[^]]*\]\(([^)]+)\)")


def local_markdown_target(source: Path, raw_target: str) -> Path | None:
    parsed = urlsplit(raw_target.strip())
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None
    target = parsed.path
    target = unquote(target)
    if not target.lower().endswith(".md"):
        return None
    if target.startswith("/"):
        return ROOT / target.removeprefix("/")
    if target.startswith("docs/"):
        return ROOT / target
    return (source.parent / target).resolve()


def main() -> int:
    problems: list[str] = []
    if not SUMMARY.is_file():
        print("error: docs/SUMMARY.md is missing", file=sys.stderr)
        return 1

    chapter_paths: list[Path] = []
    for line_number, line in enumerate(SUMMARY.read_text(encoding="utf-8").splitlines(), 1):
        match = CHAPTER_RE.match(line)
        if match is None:
            continue
        raw_path = urlsplit(match.group(1)).path
        chapter = (DOCS / unquote(raw_path)).resolve()
        try:
            chapter.relative_to(DOCS.resolve())
        except ValueError:
            problems.append(
                f"docs/SUMMARY.md:{line_number}: chapter escapes the docs source: {raw_path}"
            )
            continue
        if not chapter.is_file():
            problems.append(
                f"docs/SUMMARY.md:{line_number}: missing chapter: {raw_path}"
            )
            continue
        chapter_paths.append(chapter)

    duplicates = sorted(
        str(path.relative_to(ROOT))
        for path in set(chapter_paths)
        if chapter_paths.count(path) > 1
    )
    for duplicate in duplicates:
        problems.append(f"docs/SUMMARY.md: duplicate chapter: {duplicate}")

    published = set(chapter_paths)
    for chapter in chapter_paths:
        text = chapter.read_text(encoding="utf-8")
        if not re.search(r"^#\s+\S", text, re.MULTILINE):
            problems.append(f"{chapter.relative_to(ROOT)}: chapter has no level-one title")
        for match in LINK_RE.finditer(text):
            target = local_markdown_target(chapter, match.group(1))
            if target is None:
                continue
            if not target.is_file():
                line_number = text.count("\n", 0, match.start()) + 1
                problems.append(
                    f"{chapter.relative_to(ROOT)}:{line_number}: missing local link: "
                    f"{match.group(1)}"
                )
                continue
            if (
                target.suffix.lower() == ".md"
                and target.is_relative_to(DOCS.resolve())
                and target not in published
                and target not in {SUMMARY.resolve()}
            ):
                line_number = text.count("\n", 0, match.start()) + 1
                problems.append(
                    f"{chapter.relative_to(ROOT)}:{line_number}: linked docs page is not "
                    f"published in SUMMARY.md: {target.relative_to(ROOT)}"
                )

    if len(chapter_paths) < 20:
        problems.append(
            f"docs/SUMMARY.md: expected a substantial book, found {len(chapter_paths)} chapters"
        )

    if problems:
        print("Book validation failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    print(f"Book validation passed: {len(chapter_paths)} unique chapters")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
