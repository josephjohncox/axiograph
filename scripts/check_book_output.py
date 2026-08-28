#!/usr/bin/env python3
"""Reject broken local links and assets in a rendered Axiograph book."""

from __future__ import annotations

import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

SITE_PREFIX = "/axiograph/"


class LinkCollector(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.targets: list[tuple[str, str]] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        attribute = "href" if tag in {"a", "link"} else "src" if tag in {"img", "script"} else None
        if attribute is None:
            return
        for name, value in attrs:
            if name == attribute and value:
                self.targets.append((tag, value))


def resolve_target(output: Path, source: Path, raw_target: str) -> Path | None:
    parsed = urlsplit(raw_target)
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None
    target = unquote(parsed.path)
    if target.startswith(SITE_PREFIX):
        candidate = output / target.removeprefix(SITE_PREFIX)
    elif target.startswith("/"):
        return None
    else:
        candidate = source.parent / target
    if target.endswith("/"):
        candidate /= "index.html"
    return candidate.resolve()


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check_book_output.py OUTPUT_DIRECTORY", file=sys.stderr)
        return 2
    output = Path(sys.argv[1]).resolve()
    if not (output / "index.html").is_file():
        print(f"error: rendered book has no index.html: {output}", file=sys.stderr)
        return 1

    problems: list[str] = []
    html_files = sorted(output.rglob("*.html"))
    for source in html_files:
        parser = LinkCollector()
        parser.feed(source.read_text(encoding="utf-8"))
        for tag, raw_target in parser.targets:
            target = resolve_target(output, source, raw_target)
            if target is None:
                continue
            try:
                target.relative_to(output)
            except ValueError:
                problems.append(
                    f"{source.relative_to(output)}: {tag} target escapes the site: {raw_target}"
                )
                continue
            if target.is_dir():
                target /= "index.html"
            if not target.exists():
                problems.append(
                    f"{source.relative_to(output)}: missing {tag} target: {raw_target}"
                )

    required_patterns = [
        "404.html",
        "searchindex-*.js",
        "theme/axiograph-*.css",
        "favicon-*.svg",
    ]
    for pattern in required_patterns:
        if not any(output.glob(pattern)):
            problems.append(f"missing required book artifact matching: {pattern}")

    if problems:
        print("Rendered book validation failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    print(f"Rendered book validation passed: {len(html_files)} HTML pages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
