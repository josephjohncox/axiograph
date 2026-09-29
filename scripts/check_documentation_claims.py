#!/usr/bin/env python3
"""Validate documentation claim labels, roadmap markers, and authority anchors."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

STATUS_LABELS = {
    "design_target",
    "current_implementation",
    "runtime_check",
    "operational_evidence",
    "trusted_formal_result",
    "historical_baseline",
}

LABELED_PATHS = (
    "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md",
    "docs/roadmaps/ROADMAP_AGENT_BACKLOG.md",
    "docs/roadmaps/ROADMAP_PRODUCTION_READINESS.md",
    "docs/roadmaps/ROADMAP_RUNTIME_THEORY_AND_TYPED_WORKFLOWS.md",
    "docs/roadmaps/ROADMAP_SEMANTIC_KERNEL_AND_VCS.md",
    "docs/reference/CLAIM_STATUS.md",
    "docs/reference/TRUSTED_KERNEL.md",
    "docs/reference/LEAN_THEORY_EVALUATION.md",
    "docs/reference/AGENT_CONTEXT.md",
    "docs/reference/RELEASE_BASELINE_V20260908.md",
    "docs/explanation/TYPE_THEORY_DESIGN.md",
    "docs/README.md",
    "docs/reference/README.md",
    "docs/roadmaps/README.md",
    "docs/SUMMARY.md",
)

CLAIM_PREFIX = "**Claim status:**"
CODE_SPAN_RE = re.compile(r"`([^`\n]+)`")
ROADMAP_MARKER_RE = re.compile(r"^\s*-\s+\[([ x~])]\s+(.+?)\s*$", re.MULTILINE)
LEDGER_MARKER_RE = re.compile(r"^\s*-\s+\[([ x~])]\s+(.+?)\s*$")
MARKDOWN_LINK_RE = re.compile(r"(?<!!)\[[^]\n]+]\(([^)\n]+)\)")
HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
FENCE_OPEN_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
INLINE_CODE_RE = re.compile(r"(?<!`)(`+)(?!`)(.+?)(?<!`)\1(?!`)", re.DOTALL)
STATE_MARKERS = {"unchecked": " ", "partial": "~", "checked": "x"}
ORIGINAL_MARKERS = {"[ ]": " ", "[~]": "~", "[x]": "x"}
AUTHORIZED_TRANSITIONS = {
    "EQ-19-M005": (" ", "x", "parent-accepted-release-gate-closure"),
}
EXPECTED_MARKERS = {" ": 64, "~": 22, "x": 28}
FROZEN_MARKER_BASELINE_COUNT = 114
FROZEN_MARKER_BASELINE_SHA256 = (
    "63b142ea68d35ccc401f1682fa3f9c94101a94569b7a30dc1fe331d0cc65fae2"
)
RELEASE_COMMIT = "a2d9c80f8e5acc1a1ef6b106f9cf97bb2c0c30df"
RELEASE_TREE = "084782076e71ca0e938436d29deed31592b30d28"
RELEASE_TAG = "v20260908.0.0"
LEDGER_PATH = "docs/roadmaps/ENGINEERING_QUALITY_REQUIREMENTS_V1.json"
ROADMAP_PATH = "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md"

# Each index has its own discoverability duty. A mention in another index cannot
# compensate for a missing link here.
REQUIRED_INDEX_LINKS = {
    "docs/README.md": (
        "docs/reference/CLAIM_STATUS.md",
        "docs/reference/ENGINEERING_AUDIT_70C568B.md",
        "docs/reference/RELEASE_BASELINE_V20260908.md",
        "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md",
        "docs/roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md",
    ),
    "docs/reference/README.md": (
        "docs/reference/CLAIM_STATUS.md",
        "docs/reference/ENGINEERING_AUDIT_70C568B.md",
        "docs/reference/RELEASE_BASELINE_V20260908.md",
        "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md",
    ),
    "docs/roadmaps/README.md": (
        "docs/reference/CLAIM_STATUS.md",
        "docs/reference/RELEASE_BASELINE_V20260908.md",
        "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md",
        "docs/roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md",
    ),
    "docs/SUMMARY.md": (
        "docs/reference/CLAIM_STATUS.md",
        "docs/reference/ENGINEERING_AUDIT_70C568B.md",
        "docs/reference/RELEASE_BASELINE_V20260908.md",
        "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md",
        "docs/roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md",
    ),
}


def read_text(root: Path, relative: str, problems: list[str]) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        problems.append(f"{relative}: cannot read UTF-8 text: {error}")
        return ""


def claim_declaration(text: str) -> str | None:
    """Return the complete leading Claim status paragraph."""
    lines = text.splitlines()
    for start, line in enumerate(lines[:20]):
        if not line.startswith(CLAIM_PREFIX):
            continue
        declaration: list[str] = []
        for continuation in lines[start:]:
            if not continuation.strip():
                break
            declaration.append(continuation)
        return "\n".join(declaration)
    return None


def marker_ledger(ledger: object, problems: list[str]) -> dict[str, tuple[str, str]]:
    """Build requirement-id -> (unique first-line identity, expected marker)."""
    if not isinstance(ledger, dict):
        problems.append(f"{LEDGER_PATH}: ledger root must be an object")
        return {}
    requirements = ledger.get("requirements")
    if not isinstance(requirements, list):
        problems.append(f"{LEDGER_PATH}: requirements must be an array")
        return {}

    keyed: dict[str, tuple[str, str]] = {}
    identities: dict[str, str] = {}
    frozen_rows: list[dict[str, str | None]] = []
    for index, requirement in enumerate(requirements):
        location = f"{LEDGER_PATH}: requirements[{index}]"
        if not isinstance(requirement, dict):
            problems.append(f"{location} must be an object")
            continue
        requirement_id = requirement.get("id")
        source_text = requirement.get("sourceText")
        source_sha256 = requirement.get("sourceSha256")
        current_state = requirement.get("currentState")
        original_marker_text = requirement.get("originalMarker")
        authorized_transition = requirement.get("authorizedTransition")
        if not isinstance(requirement_id, str) or not requirement_id:
            problems.append(f"{location} has no string id")
            continue
        if requirement_id in keyed:
            problems.append(f"{LEDGER_PATH}: duplicate requirement id {requirement_id}")
            continue
        if not isinstance(source_text, str) or not source_text:
            problems.append(f"{location} ({requirement_id}) has no sourceText")
            continue
        digest = hashlib.sha256(source_text.encode("utf-8")).hexdigest()
        if source_sha256 != digest:
            problems.append(f"{location} ({requirement_id}) has an invalid sourceSha256")
            continue
        match = LEDGER_MARKER_RE.match(source_text.splitlines()[0])
        if match is None:
            problems.append(f"{location} ({requirement_id}) has no keyed marker identity")
            continue
        identity = match.group(2)
        if identity in identities:
            problems.append(
                f"{LEDGER_PATH}: marker identity is shared by {identities[identity]} "
                f"and {requirement_id}: {identity}"
            )
            continue
        original_marker = ORIGINAL_MARKERS.get(original_marker_text)
        if original_marker is None or original_marker != match.group(1):
            problems.append(
                f"{location} ({requirement_id}) has invalid originalMarker "
                f"{original_marker_text!r}"
            )
            continue
        transition = AUTHORIZED_TRANSITIONS.get(requirement_id)
        expected_marker = transition[1] if transition is not None else original_marker
        expected_transition = transition[2] if transition is not None else None
        if authorized_transition != expected_transition:
            problems.append(
                f"{location} ({requirement_id}) has unauthorized transition "
                f"{authorized_transition!r}"
            )
            continue
        marker = STATE_MARKERS.get(current_state) if isinstance(current_state, str) else None
        if marker != expected_marker:
            problems.append(
                f"{location} ({requirement_id}) has currentState {current_state!r}; "
                f"the parent-authorized marker is [{expected_marker}]"
            )
            continue
        identities[identity] = requirement_id
        keyed[requirement_id] = (identity, expected_marker)
        frozen_rows.append(
            {
                "authorizedMarker": expected_marker,
                "authorizedTransition": expected_transition,
                "id": requirement_id,
                "identity": identity,
            }
        )

    frozen_rows.sort(key=lambda row: row["id"] or "")
    frozen_payload = json.dumps(
        frozen_rows, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    frozen_digest = hashlib.sha256(frozen_payload).hexdigest()
    if (
        len(frozen_rows) != FROZEN_MARKER_BASELINE_COUNT
        or frozen_digest != FROZEN_MARKER_BASELINE_SHA256
    ):
        problems.append(
            f"{LEDGER_PATH}: requirement ids, marker identities, or authorized states "
            "differ from the independently pinned baseline"
        )
    return keyed


def validate_roadmap_markers(
    roadmap: str, ledger: object, problems: list[str]
) -> None:
    expected = marker_ledger(ledger, problems)
    actual_entries = ROADMAP_MARKER_RE.findall(roadmap)
    actual: dict[str, str] = {}
    for marker, identity in actual_entries:
        if identity in actual:
            problems.append(f"{ROADMAP_PATH}: duplicate marker identity: {identity}")
        else:
            actual[identity] = marker

    if len(actual_entries) != len(expected):
        problems.append(
            f"{ROADMAP_PATH}: found {len(actual_entries)} markers; "
            f"the frozen keyed ledger requires {len(expected)}"
        )

    for requirement_id, (identity, expected_marker) in expected.items():
        actual_marker = actual.get(identity)
        if actual_marker is None:
            problems.append(
                f"{ROADMAP_PATH}: missing marker identity {requirement_id}: {identity}"
            )
        elif actual_marker != expected_marker:
            problems.append(
                f"{ROADMAP_PATH}: marker state for {requirement_id} is "
                f"[{actual_marker}], expected [{expected_marker}] from the frozen keyed ledger"
            )

    expected_identities = {identity for identity, _marker in expected.values()}
    for identity in sorted(set(actual) - expected_identities):
        problems.append(f"{ROADMAP_PATH}: marker is absent from the frozen keyed ledger: {identity}")

    computed_counts = {marker: 0 for marker in EXPECTED_MARKERS}
    for _identity, marker in expected.values():
        computed_counts[marker] += 1
    if computed_counts != EXPECTED_MARKERS:
        problems.append(
            f"{LEDGER_PATH}: keyed marker states {computed_counts!r} changed from "
            f"the authorized {EXPECTED_MARKERS!r}"
        )

    try:
        accounting = ledger["markerAccounting"]["current"]  # type: ignore[index]
        recorded_counts = {
            " ": accounting["unchecked"],
            "~": accounting["partial"],
            "x": accounting["checked"],
        }
    except (KeyError, TypeError) as error:
        problems.append(f"{LEDGER_PATH}: invalid marker accounting: {error}")
    else:
        if recorded_counts != computed_counts:
            problems.append(
                f"{LEDGER_PATH}: marker accounting {recorded_counts!r} does not match "
                f"keyed requirement states {computed_counts!r}"
            )


def markdown_link_target(root: Path, source: Path, raw: str) -> Path | None:
    destination = raw.strip()
    if destination.startswith("<"):
        closing = destination.find(">")
        if closing < 0:
            return None
        destination = destination[1:closing]
    else:
        destination = destination.split(maxsplit=1)[0]
    parsed = urlsplit(destination)
    if parsed.scheme or parsed.netloc or parsed.query or parsed.fragment or not parsed.path:
        return None
    decoded = unquote(parsed.path)
    if decoded.startswith("/"):
        return (root / decoded.removeprefix("/")).resolve()
    return (source.parent / decoded).resolve()


def rendered_markdown_text(text: str) -> str:
    """Remove Markdown regions that do not render as navigable prose."""
    without_comments = HTML_COMMENT_RE.sub("", text)
    visible_lines: list[str] = []
    fence_character: str | None = None
    fence_length = 0
    for line in without_comments.splitlines(keepends=True):
        if fence_character is not None:
            candidate = line.lstrip(" ")
            indentation = len(line) - len(candidate)
            closing = candidate.rstrip(" \t\r\n")
            if (
                indentation <= 3
                and len(closing) >= fence_length
                and set(closing) == {fence_character}
            ):
                fence_character = None
                fence_length = 0
            continue
        opening = FENCE_OPEN_RE.match(line)
        if opening is not None:
            fence = opening.group(1)
            fence_character = fence[0]
            fence_length = len(fence)
            continue
        if line.startswith("    ") or line.startswith("\t"):
            continue
        visible_lines.append(line)
    return INLINE_CODE_RE.sub("", "".join(visible_lines))


def validate_index_links(root: Path, texts: dict[str, str], problems: list[str]) -> None:
    root = root.resolve()
    for index_path, required_targets in REQUIRED_INDEX_LINKS.items():
        source = (root / index_path).resolve()
        rendered_text = rendered_markdown_text(texts.get(index_path, ""))
        linked_targets = {
            target
            for raw in MARKDOWN_LINK_RE.findall(rendered_text)
            if (target := markdown_link_target(root, source, raw)) is not None
        }
        for required_relative in required_targets:
            required = (root / required_relative).resolve()
            if not required.is_file():
                problems.append(
                    f"{index_path}: required durable link target does not exist: "
                    f"{required_relative}"
                )
            if required not in linked_targets:
                problems.append(
                    f"{index_path}: missing required Markdown link to {required_relative}"
                )


def validate(root: Path) -> list[str]:
    root = root.resolve()
    problems: list[str] = []
    texts: dict[str, str] = {}

    for relative in LABELED_PATHS:
        text = read_text(root, relative, problems)
        texts[relative] = text
        declaration = claim_declaration(text)
        if declaration is None:
            problems.append(f"{relative}: missing a Claim status declaration in the first 20 lines")
            continue
        if declaration.count("`") % 2:
            problems.append(f"{relative}: Claim status declaration has an unmatched backtick")
            continue
        labels = set(CODE_SPAN_RE.findall(declaration))
        if not labels:
            problems.append(f"{relative}: Claim status has no machine-readable label")
            continue
        unknown = sorted(labels - STATUS_LABELS)
        if unknown:
            problems.append(f"{relative}: unknown Claim status labels: {', '.join(unknown)}")

    taxonomy = texts.get("docs/reference/CLAIM_STATUS.md", "")
    for label in sorted(STATUS_LABELS):
        if f"`{label}`" not in taxonomy:
            problems.append(f"docs/reference/CLAIM_STATUS.md: missing label `{label}`")

    ledger_text = read_text(root, LEDGER_PATH, problems)
    try:
        ledger: object = json.loads(ledger_text)
    except json.JSONDecodeError as error:
        problems.append(f"{LEDGER_PATH}: invalid JSON: {error}")
        ledger = None
    validate_roadmap_markers(texts.get(ROADMAP_PATH, ""), ledger, problems)

    release = texts.get("docs/reference/RELEASE_BASELINE_V20260908.md", "")
    for identity in (RELEASE_TAG, RELEASE_COMMIT, RELEASE_TREE):
        if identity not in release:
            problems.append(f"docs/reference/RELEASE_BASELINE_V20260908.md: missing release identity {identity}")
    for non_claim in (
        "It does not authorize another release",
        "It is not an Axiograph semantic certificate",
        "Earlier failed and partial release records remain historical evidence",
    ):
        if non_claim not in release:
            problems.append(f"docs/reference/RELEASE_BASELINE_V20260908.md: missing scope text: {non_claim}")

    trusted = texts.get("docs/reference/TRUSTED_KERNEL.md", "")
    for boundary in (
        "import closure of the executable verifier target",
        "lean/Axiograph/VerifyMain.lean",
        "Rust remains an untrusted producer",
        "SQLite",
        "AxiStore",
    ):
        if boundary not in trusted and boundary not in taxonomy:
            problems.append(f"docs/reference/TRUSTED_KERNEL.md: missing trust-boundary text: {boundary}")

    validate_index_links(root, texts, problems)
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args(argv)
    problems = validate(args.root)
    if problems:
        print("Documentation claim validation failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1
    print(f"Documentation claim validation passed: {len(LABELED_PATHS)} labeled pages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
