#!/usr/bin/env python3
"""Validate the examples test index and typed runnable artifacts.

The JSON catalog is an agent/test index, not the primary human teaching
surface. User-facing flow explanations belong in Markdown READMEs and scripts.
"""

from __future__ import annotations

import json
import re
import shlex
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent
CATALOG = ROOT / "catalog.json"
HOST_INTEGRATION_CONFIGS = ROOT / "software_authoring" / "host_integrations"
COMMAND_PATH_PREFIXES = (
    "examples/",
    "./examples/",
    "fixtures/",
    "./fixtures/",
    "scripts/",
    "./scripts/",
)
GENERATED_DIR_NAMES = {"__pycache__", "build", "node_modules", "target"}

ALLOWED_SURFACES = {
    "backend_projection_contract",
    "behavior_case_bundle",
    "canonical_axi_module",
    "competency_question_suite",
    "derived_projection_manifest_v1",
    "evidence_overlay_bundle",
    "evidence_source_input",
    "implementation_source_input",
    "rdf_shacl_boundary",
    "repl_canonical_workflow",
    "repl_synthetic_workflow",
    "review_candidate_bundle",
    "runtime_theory_module_set",
    "semantic_vcs_flow",
    "usefulness_workflow",
}

ALLOWED_TEACHING_TIERS = {
    "evidence_plane",
    "projection_or_ops",
    "teaching",
}


def fail(message: str) -> None:
    print(f"examples catalog check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def is_host_integration_config(path: Path) -> bool:
    return HOST_INTEGRATION_CONFIGS in path.parents


def is_public_example_dir(path: Path) -> bool:
    return (
        path.is_dir()
        and not path.name.startswith(".")
        and path.name not in GENERATED_DIR_NAMES
    )


def check_versioned_json_artifacts() -> None:
    missing_versions: list[str] = []
    non_object_artifacts: list[str] = []
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
            non_object_artifacts.append(str(relative_path))
            continue

        version = value.get("version")
        if not isinstance(version, (str, int)) or version == "":
            missing_versions.append(str(relative_path))

        embedded_query_keys.extend(
            f"{relative_path}:{json_path}"
            for json_path in find_json_keys(value, {"query", "axql"})
        )

    if non_object_artifacts:
        fail(
            "Axiograph-owned JSON artifacts must be top-level objects: "
            + ", ".join(non_object_artifacts)
        )

    if missing_versions:
        fail(
            "Axiograph-owned JSON artifacts must carry top-level `version`: "
            + ", ".join(missing_versions)
        )

    if embedded_query_keys:
        fail(
            "public JSON examples must not embed ad hoc query strings; use `.cq`, `.axi`, or typed tool commands: "
            + ", ".join(embedded_query_keys)
        )


def command_input_paths(command: str) -> list[str]:
    try:
        tokens = shlex.split(command)
    except ValueError as error:
        fail(f"catalog command is not shell-parseable: {command}: {error}")

    paths: list[str] = []
    for token in tokens:
        if token.startswith(COMMAND_PATH_PREFIXES):
            paths.append(token[2:] if token.startswith("./") else token)
    return paths


def behavior_case_command_missing_overlay(command: str) -> bool:
    return "discover behavior-case" in command and "--overlay" not in command


def command_contains_inline_lowered_cq(command: str) -> bool:
    return (
        re.search(r"--cq(?![-\w])(?:=|\s+)[^\n]*\bselect\b", command, re.IGNORECASE)
        is not None
    )


def command_cq_reference_paths(command: str) -> list[str]:
    try:
        tokens = shlex.split(command)
    except ValueError as error:
        fail(f"command is not shell-parseable: {command}: {error}")

    paths: list[str] = []
    options = {"--from-cq", "--cq-file", "--cq"}
    for index, token in enumerate(tokens):
        if token in options and index + 1 < len(tokens):
            value = tokens[index + 1]
        elif any(token.startswith(option + "=") for option in options):
            value = token.split("=", 1)[1]
        else:
            continue

        if "$" in value or "{" in value or value.startswith("build/"):
            continue
        if value.endswith((".cq", ".json")):
            paths.append(value[2:] if value.startswith("./") else value)
    return paths


def check_cq_reference_paths(command: str, source_label: str, repo_root: Path) -> None:
    for cq_path in command_cq_reference_paths(command):
        path = repo_root / cq_path
        if not path.exists():
            fail(f"{source_label} references missing CQ input `{cq_path}`")
        if path.suffix == ".cq" and not path.read_text().startswith(
            "version competency_question_bundle_v1"
        ):
            fail(
                f"{source_label} references `{cq_path}`, but it is not a competency-question `.cq` file"
            )


def shell_logical_commands(text: str) -> list[str]:
    commands: list[str] = []
    current: list[str] = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.endswith("\\"):
            current.append(line[:-1].strip())
            continue
        if current:
            current.append(line)
            commands.append(" ".join(current))
            current = []
        else:
            commands.append(line)
    if current:
        commands.append(" ".join(current))
    return commands


def check_public_shell_scripts() -> None:
    repo_root = ROOT.parent
    for path in sorted(ROOT.rglob("*.sh")):
        if "host_integrations" in path.parts:
            continue
        relative_path = path.relative_to(ROOT)
        for command in shell_logical_commands(path.read_text()):
            if behavior_case_command_missing_overlay(command):
                fail(
                    f"{relative_path} runs `discover behavior-case` without required `--overlay`"
                )
            if command_contains_inline_lowered_cq(command):
                fail(
                    f"{relative_path} uses inline lowered query syntax with `--cq`; "
                    "use a `.cq` file and `--cq-file` instead"
                )
            check_cq_reference_paths(command, str(relative_path), repo_root)


def check_repl_scripts() -> None:
    repo_root = ROOT.parent
    for path in sorted((ROOT / "repl_scripts").rglob("*.repl")):
        relative_path = path.relative_to(ROOT)
        for line_number, raw_line in enumerate(path.read_text().splitlines(), start=1):
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            if re.search(r"\bexport_axi\s", line):
                fail(
                    f"{relative_path}:{line_number} uses removed REPL `export_axi`; "
                    "use `export_axi_module` for canonical module export"
                )
            if command_contains_inline_lowered_cq(line):
                fail(
                    f"{relative_path}:{line_number} uses inline lowered query syntax with `--cq`; "
                    "use a `.cq` file and `--cq-file` instead"
                )
            check_cq_reference_paths(line, f"{relative_path}:{line_number}", repo_root)


def check_cq_artifacts() -> None:
    cq_paths = sorted(ROOT.rglob("*.cq"))
    if not cq_paths:
        fail("expected at least one public `.cq` competency-question fixture")

    for path in cq_paths:
        relative_path = path.relative_to(ROOT)
        lines = [
            line.strip()
            for line in path.read_text().splitlines()
            if line.strip() and not line.strip().startswith("#")
        ]
        if not lines or lines[0] != "version competency_question_bundle_v1":
            fail(
                f"{relative_path} must start with `version competency_question_bundle_v1`"
            )
        if not any(
            line.startswith("question ") and line.endswith(":") for line in lines
        ):
            fail(f"{relative_path} must define at least one `question <id>:` block")

        for line_number, line in enumerate(lines, start=1):
            lowered = line.lower()
            if lowered.startswith(("query:", "axql:")) or "select " in lowered:
                fail(
                    f"{relative_path}:{line_number} should be question-first CQ authoring, "
                    "not lowered query syntax"
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
    try:
        catalog = json.loads(CATALOG.read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot load catalog `{CATALOG}`: {error}")
    examples = catalog.get("examples")
    if not isinstance(examples, list) or not examples:
        fail("catalog must contain a non-empty examples array")

    repo_root = ROOT.parent
    paths: set[Path] = set()
    ids = set()

    for index, entry in enumerate(examples):
        if not isinstance(entry, dict):
            fail(f"entry {index} is not an object")

        for field in (
            "id",
            "title",
            "path",
            "surface",
            "teaching_tier",
            "feature_tags",
        ):
            if field not in entry:
                fail(f"entry {index} is missing required field `{field}`")

        entry_id = entry["id"]
        if entry_id in ids:
            fail(f"duplicate id `{entry_id}`")
        ids.add(entry_id)

        surface = entry["surface"]
        if surface not in ALLOWED_SURFACES:
            fail(f"entry `{entry_id}` uses unknown surface `{surface}`")

        teaching_tier = entry["teaching_tier"]
        if teaching_tier not in ALLOWED_TEACHING_TIERS:
            fail(f"entry `{entry_id}` uses unknown teaching tier `{teaching_tier}`")

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

        if commands:
            for command in commands:
                if "cargo test" in command:
                    fail(
                        f"entry `{entry_id}` uses `cargo test` as a public example command; "
                        "move command-only behavior to tests or expose a CLI/script flow"
                    )
                if "--lang axql" in command:
                    fail(
                        f"entry `{entry_id}` exposes lowered AxQL as a catalog command; "
                        "use `.cq`, `ask`, or a higher-level typed tool flow in public examples"
                    )
                if behavior_case_command_missing_overlay(command):
                    fail(
                        f"entry `{entry_id}` runs `discover behavior-case` without required `--overlay`"
                    )
                if command_contains_inline_lowered_cq(command):
                    fail(
                        f"entry `{entry_id}` uses inline lowered query syntax with `--cq`; "
                        "use `.cq` files and `--cq-file` for public examples"
                    )
                check_cq_reference_paths(command, f"entry `{entry_id}`", repo_root)
                for command_path in command_input_paths(command):
                    if not (repo_root / command_path).exists():
                        fail(
                            f"entry `{entry_id}` command references missing path `{command_path}`"
                        )

        related = entry.get("related", [])
        if related:
            if not isinstance(related, list):
                fail(f"entry `{entry_id}` field `related` must be an array")
            for related_path in related:
                if not isinstance(related_path, str):
                    fail(f"entry `{entry_id}` related path must be a string")
                if not (repo_root / related_path).exists():
                    fail(
                        f"entry `{entry_id}` references missing related path `{related_path}`"
                    )

    top_level_dirs = {
        path.resolve() for path in ROOT.iterdir() if is_public_example_dir(path)
    }
    missing = sorted(
        str(directory)
        for directory in top_level_dirs
        if directory not in paths
        and not any(directory in path.parents for path in paths)
    )
    if missing:
        fail(
            "top-level example directories missing from catalog: " + ", ".join(missing)
        )

    check_versioned_json_artifacts()
    check_cq_artifacts()
    check_public_shell_scripts()
    check_repl_scripts()


if __name__ == "__main__":
    main()
