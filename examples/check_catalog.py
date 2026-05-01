#!/usr/bin/env python3
"""Validate that public examples are cataloged and use typed fixtures."""

from __future__ import annotations

import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parent
CATALOG = ROOT / "catalog.json"
HOST_INTEGRATION_CONFIGS = ROOT / "software_authoring" / "host_integrations"


def fail(message: str) -> None:
    print(f"examples catalog check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def is_host_integration_config(path: Path) -> bool:
    return HOST_INTEGRATION_CONFIGS in path.parents


def check_versioned_json_fixtures() -> None:
    missing_versions: list[str] = []
    non_object_fixtures: list[str] = []
    embedded_query_keys: list[str] = []

    for path in sorted(ROOT.rglob("*.json")):
        if is_host_integration_config(path):
            continue

        relative_path = path.relative_to(ROOT)
        try:
            value = json.loads(path.read_text())
        except json.JSONDecodeError as error:
            fail(f"{relative_path} is not valid JSON: {error}")

        if not isinstance(value, dict):
            non_object_fixtures.append(str(relative_path))
            continue

        version = value.get("version")
        if not isinstance(version, (str, int)) or version == "":
            missing_versions.append(str(relative_path))

        embedded_query_keys.extend(
            f"{relative_path}:{json_path}"
            for json_path in find_json_keys(value, {"query", "axql"})
        )

    if non_object_fixtures:
        fail(
            "Axiograph-owned JSON fixtures must be top-level objects: "
            + ", ".join(non_object_fixtures)
        )

    if missing_versions:
        fail(
            "Axiograph-owned JSON fixtures must carry top-level `version`: "
            + ", ".join(missing_versions)
        )

    if embedded_query_keys:
        fail(
            "public JSON examples must not embed ad hoc query strings; use `.cq`, `.axi`, or typed tool commands: "
            + ", ".join(embedded_query_keys)
        )


def find_json_keys(value: object, banned: set[str], prefix: str = "$") -> list[str]:
    found: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{prefix}.{key}"
            if key in banned:
                found.append(child_path)
            found.extend(find_json_keys(child, banned, child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(find_json_keys(child, banned, f"{prefix}[{index}]"))
    return found


def main() -> None:
    catalog = json.loads(CATALOG.read_text())
    examples = catalog.get("examples")
    if not isinstance(examples, list) or not examples:
        fail("catalog must contain a non-empty examples array")

    repo_root = ROOT.parent
    paths: set[Path] = set()
    ids = set()

    for index, entry in enumerate(examples):
        if not isinstance(entry, dict):
            fail(f"entry {index} is not an object")

        for field in ("id", "title", "path", "kind", "feature_tags"):
            if field not in entry:
                fail(f"entry {index} is missing required field `{field}`")

        entry_id = entry["id"]
        if entry_id in ids:
            fail(f"duplicate id `{entry_id}`")
        ids.add(entry_id)

        raw_path = entry["path"].rstrip("/")
        path = (repo_root / raw_path).resolve()
        if not path.exists():
            fail(f"entry `{entry_id}` references missing path `{raw_path}`")
        paths.add(path)

        tags = entry["feature_tags"]
        if not isinstance(tags, list) or not tags:
            fail(f"entry `{entry_id}` must have at least one feature tag")

        flow = entry.get("flow")
        commands = entry.get("commands")
        if not flow and not commands:
            fail(f"entry `{entry_id}` must explain either a flow or commands")

    top_level_dirs = {path.resolve() for path in ROOT.iterdir() if path.is_dir()}
    missing = sorted(
        str(directory)
        for directory in top_level_dirs
        if directory not in paths and not any(directory in path.parents for path in paths)
    )
    if missing:
        fail("top-level example directories missing from catalog: " + ", ".join(missing))

    check_versioned_json_fixtures()


if __name__ == "__main__":
    main()
