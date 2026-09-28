#!/usr/bin/env python3
"""Run the bounded deterministic Rust/Lean axi_v1 differential contract."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from collections.abc import Callable
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from scripts.bounded_subprocess import BoundedProcessError, run_bounded
from scripts.check_axi_v1_contract import (
    CROSS_LANGUAGE_REJECTION_STAGES,
    NON_CROSS_LANGUAGE_OUTCOME_CLASSES,
    classify_actual_rejection,
    differential_envelope_contract,
)

CORPUS_PATH = Path("fixtures/canonical/contract/corpus.json")
CONTRACT_PATH = Path("fixtures/canonical/contract/axi_v1_contract.json")
REPORT_SCHEMA = "axiograph.axi_v1_differential_report"
CORPUS_SCHEMA = "axiograph.axi_v1_differential_corpus"
ENVELOPE_SCHEMA = "axiograph.axi_v1_differential_envelope"
GENERATOR_SCHEMA = "axiograph.axi_v1_differential_generator"
GENERATOR_VERSION = 2
MAX_CASES = 64
MAX_SHRINK_ATTEMPTS = 48


class ConformanceError(ValueError):
    """A closed differential contract or runtime observation was invalid."""


@dataclass(frozen=True)
class DifferentialFailureSignature:
    """Typed state that a minimized differential counterexample must preserve."""

    mismatch_kind: str
    stage: str
    expected_decision: str | None
    rust_decision: str | None
    lean_decision: str | None
    expected_class: str | None = None
    rust_class: str | None = None
    lean_class: str | None = None


class DifferentialFailure(ConformanceError):
    """A semantic implementation mismatch with a shrink-stable signature."""

    def __init__(self, message: str, signature: DifferentialFailureSignature):
        super().__init__(message)
        self.signature = signature


@dataclass(frozen=True)
class Binaries:
    rust_parse: Path
    lean_parse: Path
    rust_typecheck: Path
    lean_typecheck: Path
    rust_digest: Path
    lean_digest: Path
    rust_formation: Path


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def require_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    if actual != expected:
        raise ConformanceError(
            f"{label} fields differ: missing={sorted(expected - actual)} "
            f"extra={sorted(actual - expected)}"
        )


def load_json_object(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ConformanceError(f"{label} is not strict UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise ConformanceError(f"{label} must be a JSON object")
    return value


def splitmix64(state: int) -> tuple[int, int]:
    state = (state + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
    value = state
    value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
    return state, value ^ (value >> 31)


def shuffled_kinds(state: int, kinds: tuple[str, ...]) -> tuple[int, list[str]]:
    """Return one deterministic Fisher-Yates grammar permutation."""
    result = list(kinds)
    for index in range(len(result) - 1, 0, -1):
        state, random_value = splitmix64(state)
        selected = random_value % (index + 1)
        result[index], result[selected] = result[selected], result[index]
    return state, result


def generated_cases(generator: dict[str, Any]) -> list[dict[str, Any]]:
    """Build a finite, reproducible grammar sample with replayable derivations."""
    require_keys(
        generator,
        {
            "schema",
            "version",
            "algorithm",
            "seed",
            "case_count",
            "maximums",
            "stable_order",
            "expected_cases_sha256",
        },
        "generator",
    )
    if generator["schema"] != GENERATOR_SCHEMA or generator["version"] != GENERATOR_VERSION:
        raise ConformanceError("unsupported differential generator schema/version")
    if generator["algorithm"] != "splitmix64-grammar-permutation-v2":
        raise ConformanceError("unsupported differential generator algorithm")
    seed_text = generator["seed"]
    if not isinstance(seed_text, str) or not re.fullmatch(r"0x[0-9a-f]{16}", seed_text):
        raise ConformanceError("generator seed must be 0x plus 16 lowercase hex digits")
    count = generator["case_count"]
    if not isinstance(count, int) or isinstance(count, bool) or not 0 < count <= MAX_CASES:
        raise ConformanceError(f"generator case_count must be in 1..={MAX_CASES}")
    if generator["stable_order"] != "ascending generated case id":
        raise ConformanceError("generator stable ordering contract drift")
    maximums = generator["maximums"]
    if not isinstance(maximums, dict) or set(maximums) != {
        "source_bytes",
        "syntax_depth",
        "lines",
        "declarations",
        "roles_per_relation",
    } or any(
        not isinstance(value, int) or isinstance(value, bool) or value <= 0
        for value in maximums.values()
    ):
        raise ConformanceError("generator maximums must be a closed positive-integer object")

    kinds = (
        "accepted_order",
        "accepted_numeric",
        "parse_alias",
        "parse_constraint",
        "type_unknown_target",
        "type_dependent",
        "formation_key",
        "unsupported_constraint",
    )
    state = int(seed_text, 16)
    cases: list[dict[str, Any]] = []
    grammar_order: list[str] = []
    while len(grammar_order) < count:
        state, permutation = shuffled_kinds(state, kinds)
        grammar_order.extend(permutation)
    for index, kind in enumerate(grammar_order[:count]):
        state, random_value = splitmix64(state)
        suffix = f"{random_value:016x}"
        module = f"G{suffix}"
        comment_marker = "#" if random_value & 1 else "--"
        schema_colon = ":" if random_value & 2 else ""
        prefix = (
            f"module {module}\n{comment_marker} generator={GENERATOR_VERSION} "
            f"seed={seed_text} case={index:03d} choice={random_value:016x}\n"
        )
        expected: dict[str, Any]
        if kind == "accepted_order":
            objects = ("B", "A") if random_value & 4 else ("A", "B")
            source = prefix + (
                f"schema S{schema_colon}\n  object {objects[0]}\n  object {objects[1]}\n"
                "  relation R(left:A, right:B)\n"
            )
            expected = expected_outcomes(True, True, True, "accepted")
        elif kind == "accepted_numeric":
            source = prefix + (
                "schema S\n  object A\n"
                "  relation R(value:refined(A;cardinality(0|4294967295)))\n"
            )
            expected = expected_outcomes(True, True, True, "accepted")
        elif kind == "parse_alias":
            source = prefix + (
                "schema S\n  object Child\n  object Parent\n  subtype Child <: Parent\n"
            )
            expected = expected_outcomes(False, None, None, "parse.noncanonical_alias")
        elif kind == "parse_constraint":
            source = prefix + (
                "schema S\n  object A\n  relation R(left:A,right:A)\n"
                "theory T on S\n  constraint functional R.left ->\n"
            )
            expected = expected_outcomes(False, None, None, "parse.constraint_shape")
        elif kind == "type_unknown_target":
            source = prefix + "schema S\n  relation R(value:Missing)\n"
            expected = expected_outcomes(True, False, None, "typecheck.schema_formation")
        elif kind == "type_dependent":
            source = prefix + (
                "schema S\n  object A\n  relation R(value:indexed(A;missing))\n"
            )
            expected = expected_outcomes(True, False, None, "typecheck.dependent_role")
        elif kind == "formation_key":
            source = prefix + (
                "schema S\n  object A\n"
                "  relation R(base:A,value:refined(A;key(base)))\n"
            )
            expected = expected_outcomes(
                True, True, False, "formation.unsupported_refinement_witness"
            )
        else:
            source = prefix + (
                "schema S\n  object A\ntheory T on S\n"
                f"  constraint review_{suffix} material\n"
            )
            expected = expected_outcomes(True, True, True, "unsupported.opaque_constraint")
        cases.append(
            {
                "id": f"generated-{index:03d}",
                "origin": "generated",
                "generator_kind": kind,
                "generator_trace": {
                    "choice": f"0x{random_value:016x}",
                    "comment_marker": comment_marker,
                    "schema_colon": bool(schema_colon),
                },
                "source": source,
                "expected": expected,
            }
        )
    digest_payload = [
        {
            "id": case["id"],
            "generator_kind": case["generator_kind"],
            "generator_trace": case["generator_trace"],
            "source": case["source"],
            "expected": case["expected"],
        }
        for case in cases
    ]
    actual_digest = sha256_bytes(canonical_json(digest_payload))
    if generator["expected_cases_sha256"] != actual_digest:
        raise ConformanceError(
            "generated case digest drift: "
            f"expected {generator['expected_cases_sha256']}, got {actual_digest}"
        )
    return cases


def shrink_failure_source(
    source: str,
    reproduces: Callable[[str], bool],
    *,
    max_attempts: int = MAX_SHRINK_ATTEMPTS,
) -> dict[str, Any]:
    """Deterministically minimize a failing source and replay the final result."""
    if not isinstance(source, str) or not source:
        raise ConformanceError("generated shrink source must be non-empty")
    if not isinstance(max_attempts, int) or isinstance(max_attempts, bool) or max_attempts <= 0:
        raise ConformanceError("generated shrink attempt bound must be a positive integer")
    if not reproduces(source):
        raise ConformanceError("generated shrink seed does not reproduce the failure")
    attempts = 1
    current = source

    def reduce_units(units: list[str]) -> list[str]:
        nonlocal attempts
        granularity = 2
        current_units = units
        while len(current_units) > 1 and attempts < max_attempts:
            chunk = max(1, (len(current_units) + granularity - 1) // granularity)
            reduced = False
            for start in range(0, len(current_units), chunk):
                if attempts >= max_attempts:
                    break
                candidate_units = current_units[:start] + current_units[start + chunk :]
                if not candidate_units:
                    continue
                candidate = "".join(candidate_units)
                attempts += 1
                if len(candidate.encode("utf-8")) >= len("".join(current_units).encode("utf-8")):
                    continue
                if reproduces(candidate):
                    current_units = candidate_units
                    granularity = max(2, granularity - 1)
                    reduced = True
                    break
            if not reduced:
                if granularity >= len(current_units):
                    break
                granularity = min(len(current_units), granularity * 2)
        return current_units

    line_units = current.splitlines(keepends=True)
    if line_units:
        current = "".join(reduce_units(line_units))
    if attempts < max_attempts and len(current) > 1:
        current = "".join(reduce_units(list(current)))
    if attempts >= max_attempts:
        # The final replay is mandatory and is outside the candidate-attempt budget.
        replayed = reproduces(current)
    else:
        attempts += 1
        replayed = reproduces(current)
    if not replayed:
        raise ConformanceError("minimized generated failure did not replay")
    return {
        "algorithm": "ddmin-lines-then-codepoints-v1",
        "attempt_bound": max_attempts,
        "attempts": attempts,
        "bytes_before": len(source.encode("utf-8")),
        "bytes_after": len(current.encode("utf-8")),
        "source_sha256": sha256_bytes(current.encode("utf-8")),
        "source": current,
        "replayed": True,
    }


def expected_outcomes(
    parse: bool, typecheck: bool | None, formation: bool | None, outcome_class: str
) -> dict[str, Any]:
    return {
        "parse": parse,
        "typecheck": typecheck,
        "formation": formation,
        "outcome_class": outcome_class,
    }


def validate_expected(expected: dict[str, Any], classes: set[str], label: str) -> None:
    require_keys(expected, {"parse", "typecheck", "formation", "outcome_class"}, label)
    parse = expected["parse"]
    typecheck = expected["typecheck"]
    formation = expected["formation"]
    if not isinstance(parse, bool):
        raise ConformanceError(f"{label}.parse must be Boolean")
    if typecheck is not None and not isinstance(typecheck, bool):
        raise ConformanceError(f"{label}.typecheck must be Boolean or null")
    if formation is not None and not isinstance(formation, bool):
        raise ConformanceError(f"{label}.formation must be Boolean or null")
    if not parse and (typecheck is not None or formation is not None):
        raise ConformanceError(f"{label}: later stages must be null after parse rejection")
    if parse and typecheck is None:
        raise ConformanceError(f"{label}: accepted parse requires typecheck expectation")
    if typecheck is False and formation is not None:
        raise ConformanceError(f"{label}: formation must be null after typecheck rejection")
    if typecheck is True and not isinstance(formation, bool):
        raise ConformanceError(f"{label}: accepted typecheck requires formation expectation")
    if expected["outcome_class"] not in classes:
        raise ConformanceError(f"{label}: unrecognized outcome class {expected['outcome_class']!r}")


def validate_source_bounds(source: bytes, bounds: dict[str, Any], label: str) -> None:
    if len(source) > bounds["max_source_bytes"]:
        raise ConformanceError(f"{label}: source byte bound exceeded")
    text = source.decode("utf-8")
    lines = text.splitlines()
    if len(lines) > bounds["max_lines"]:
        raise ConformanceError(f"{label}: source line bound exceeded")
    depth = maximum_delimiter_depth(text)
    if depth > bounds["max_syntax_depth"]:
        raise ConformanceError(f"{label}: source syntax depth bound exceeded")
    declaration_count = sum(
        line.lstrip().startswith(
            ("object ", "subtype ", "relation ", "aspect ", "function ", "constraint ", "equation ", "rewrite ")
        )
        for line in lines
    )
    if declaration_count > bounds["max_declarations"]:
        raise ConformanceError(f"{label}: declaration count bound exceeded")
    for line in lines:
        stripped = line.lstrip()
        if stripped.startswith("relation ") and "(" in stripped and ")" in stripped:
            body = stripped.split("(", 1)[1].rsplit(")", 1)[0]
            roles = 0 if not body else body.count(",") + 1
            if roles > bounds["max_roles_per_relation"]:
                raise ConformanceError(f"{label}: relation-role count bound exceeded")


def maximum_delimiter_depth(text: str) -> int:
    opening = "([{"
    closing = ")]}"
    stack: list[str] = []
    maximum = 0
    for character in text:
        if character in opening:
            stack.append(character)
            maximum = max(maximum, len(stack))
        elif character in closing and stack:
            stack.pop()
    return maximum


SECTION_PROBES = {
    "lexical_surface": "numeric-ascii-token",
    "adversarial_matrix": "cross-language-parse-rejection",
    "authority": "exact-source-hash-and-revision",
    "exact_byte_anchor": "exact-source-hash-and-revision",
    "ordered_sequences": "all-ast-constructors-and-ordered-payloads",
    "ast_inventory": "all-ast-constructors-and-ordered-payloads",
    "ast_payload_types": "all-ast-constructors-and-ordered-payloads",
    "numeric_domains": "u32-cardinality-payload",
    "excluded_runtime_declarations": "opaque-constraint-constructor",
    "surface_forms": "indexed-dependent-role-surface",
    "unsupported_forms": "noncanonical-alias-rejection",
    "acceptance_stages": "parse-and-typecheck-before-formation-rejection",
    "operational_bounds": "exact-n-and-runner-n-plus-one",
    "normalized_comparison": "hand-normalized-ast-golden",
    "differential_envelope": "rust-and-lean-envelope-execution",
    "rejection_classes": "derived-parse-rejection-class",
    "fixtures": "fixture-path-hash-and-revision",
    "diagnostic_taxonomy": "source-specific-rejection-diagnostic",
    "non_cross_language_outcome_classes": "unsupported-success-class",
}


def validate_coverage_probe(
    section: str,
    probe: str,
    source: bytes,
    expected: dict[str, Any],
    golden: dict[str, Any] | None,
    ordered_golden: dict[str, Any] | None,
    revision: str,
    bounds: dict[str, Any],
    label: str,
) -> None:
    required_probe = SECTION_PROBES.get(section)
    if required_probe is None or probe != required_probe:
        raise ConformanceError(f"{label}: section {section!r} requires probe {required_probe!r}")
    text = source.decode("utf-8")
    if probe in {"exact-source-hash-and-revision", "fixture-path-hash-and-revision"}:
        if not revision.startswith("axi:revision:v2:sha256:"):
            raise ConformanceError(f"{label}: exact-byte probe lacks a revision golden")
    elif probe == "all-ast-constructors-and-ordered-payloads":
        required_surface_fragments = (
            "relation(Base)", "indexed(", "refined(", "@context", "@world",
            "@temporal", "@parameter", "@evidence", "constraint functional",
            "constraint at_most", "constraint typing", "constraint symmetric",
            "constraint transitive", "constraint key", "constraint ReviewOnly:",
            "equation identity_text:", "orientation: forward", "orientation: backward",
            "orientation: bidirectional", "refl(", "step(", "trans(", "inv(",
            "instance SurfaceData", "base0:",
        )
        missing = [fragment for fragment in required_surface_fragments if fragment not in text]
        if missing or expected["outcome_class"] != "typecheck.refinement":
            raise ConformanceError(f"{label}: all-constructor surface probe is incomplete: {missing}")
        validate_ordered_sequence_golden_shape(
            ordered_golden, f"{label}.ordered_sequence_golden"
        )
    elif probe in {"numeric-ascii-token", "u32-cardinality-payload"}:
        if "cardinality(0|4294967295)" not in text or golden is None:
            raise ConformanceError(f"{label}: numeric probe lacks the exact u32 endpoints")
    elif probe == "opaque-constraint-constructor":
        if golden is None or not has_unsupported_constructor(golden, "unsupported.opaque_constraint"):
            raise ConformanceError(f"{label}: opaque-constraint probe lacks the AST constructor")
    elif probe == "indexed-dependent-role-surface":
        if "indexed(" not in text or expected["outcome_class"] != "typecheck.dependent_role":
            raise ConformanceError(f"{label}: dependent-role probe is not exercised")
    elif probe == "noncanonical-alias-rejection":
        if "<:" not in text or expected["outcome_class"] != "parse.noncanonical_alias":
            raise ConformanceError(f"{label}: alias probe is not exercised")
    elif probe in {"cross-language-parse-rejection", "derived-parse-rejection-class", "source-specific-rejection-diagnostic"}:
        if expected["parse"] or not expected["outcome_class"].startswith("parse."):
            raise ConformanceError(f"{label}: parse-rejection probe is not exercised")
    elif probe == "parse-and-typecheck-before-formation-rejection":
        if expected != expected_outcomes(True, True, False, "formation.unsupported_refinement_witness"):
            raise ConformanceError(f"{label}: staged formation probe is not exercised")
    elif probe == "exact-n-and-runner-n-plus-one":
        if len(source) != bounds["max_source_bytes"]:
            raise ConformanceError(f"{label}: N bound fixture must contain exactly max_source_bytes")
        validate_source_bounds(source, bounds, f"{label}.N")
        try:
            validate_source_bounds(source + b"#", bounds, f"{label}.N_plus_1")
        except ConformanceError as error:
            if "source byte bound exceeded" not in str(error):
                raise
        else:
            raise ConformanceError(f"{label}: N+1 source unexpectedly satisfied its bound")
    elif probe == "hand-normalized-ast-golden":
        if golden is None:
            raise ConformanceError(f"{label}: normalized-comparison probe needs a hand golden")
        validate_normalized_ast(golden, f"{label}.normalized_ast_golden")
    elif probe == "rust-and-lean-envelope-execution":
        if not expected["parse"]:
            raise ConformanceError(f"{label}: envelope probe must reach accepted Rust/Lean parsing")
    elif probe == "unsupported-success-class":
        if expected != expected_outcomes(True, True, True, "unsupported.opaque_constraint"):
            raise ConformanceError(f"{label}: unsupported-success probe is not exercised")
    else:
        raise ConformanceError(f"{label}: no semantic validator exists for probe {probe!r}")


def load_corpus(root: Path) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any]]:
    corpus = load_json_object(root / CORPUS_PATH, "differential corpus")
    require_keys(
        corpus,
        {
            "schema",
            "version",
            "description",
            "contract",
            "required_contract_sections",
            "generator",
            "bounds",
            "hand_cases",
        },
        "differential corpus",
    )
    if corpus["schema"] != CORPUS_SCHEMA or corpus["version"] != 1:
        raise ConformanceError("unsupported differential corpus schema/version")
    contract = load_json_object(root / CONTRACT_PATH, "axi_v1 contract")
    if contract.get("differential_envelope") != differential_envelope_contract():
        raise ConformanceError("contract differential envelope drift")
    contract_ref = corpus["contract"]
    if contract_ref != {
        "path": CONTRACT_PATH.as_posix(),
        "sha256": sha256_bytes((root / CONTRACT_PATH).read_bytes()),
        "version": contract.get("version"),
    }:
        raise ConformanceError("differential corpus contract identity drift")
    sections = corpus["required_contract_sections"]
    if not isinstance(sections, list) or not sections or len(sections) != len(set(sections)):
        raise ConformanceError("required contract sections must be a non-empty unique array")
    if any(section not in contract for section in sections):
        raise ConformanceError("required contract section is absent from the contract")

    bounds = corpus["bounds"]
    require_keys(
        bounds,
        {
            "max_cases",
            "max_source_bytes",
            "max_syntax_depth",
            "max_lines",
            "max_declarations",
            "max_roles_per_relation",
            "per_process_timeout_seconds",
            "total_timeout_seconds",
            "max_stdout_bytes",
            "max_stderr_bytes",
        },
        "differential bounds",
    )
    integer_bounds = {key: value for key, value in bounds.items() if key != "per_process_timeout_seconds"}
    if any(not isinstance(value, int) or isinstance(value, bool) or value <= 0 for value in integer_bounds.values()):
        raise ConformanceError("differential integer bounds must be positive integers")
    if bounds["max_cases"] > MAX_CASES or not 0 < bounds["per_process_timeout_seconds"] <= 600:
        raise ConformanceError("differential process/case bounds exceed the runner maximum")
    if bounds["total_timeout_seconds"] > 600:
        raise ConformanceError("differential total timeout exceeds 600 seconds")

    hand_cases = corpus["hand_cases"]
    if not isinstance(hand_cases, list) or not hand_cases:
        raise ConformanceError("hand_cases must be a non-empty array")
    classes = set(CROSS_LANGUAGE_REJECTION_STAGES) | set(NON_CROSS_LANGUAGE_OUTCOME_CLASSES) | {"accepted"}
    cases: list[dict[str, Any]] = []
    covered_sections: dict[str, list[dict[str, str]]] = {}
    for position, declared in enumerate(hand_cases):
        label = f"hand_cases[{position}]"
        if not isinstance(declared, dict):
            raise ConformanceError(f"{label} must be an object")
        allowed = {
            "id", "path", "sha256", "revision_digest_v2", "coverage_probes",
            "expected", "normalized_ast_golden", "ordered_sequence_golden",
        }
        optional = {"normalized_ast_golden", "ordered_sequence_golden"}
        if not set(declared).issubset(allowed) or not allowed.difference(optional).issubset(declared):
            raise ConformanceError(f"{label} fields are incomplete or unknown")
        identifier = declared["id"]
        path_text = declared["path"]
        if not isinstance(identifier, str) or not identifier or not isinstance(path_text, str) or not path_text:
            raise ConformanceError(f"{label} id/path must be non-empty strings")
        path = root / path_text
        source = path.read_bytes()
        if sha256_bytes(source) != declared["sha256"]:
            raise ConformanceError(f"{label}: source SHA-256 drift")
        revision = declared["revision_digest_v2"]
        if not isinstance(revision, str) or not re.fullmatch(r"axi:revision:v2:sha256:[0-9a-f]{64}", revision):
            raise ConformanceError(f"{label}: malformed hand-calculated revision golden")
        coverage = declared["coverage_probes"]
        if (
            not isinstance(coverage, dict)
            or not coverage
            or any(not isinstance(section, str) or section not in sections for section in coverage)
            or any(not isinstance(probe, str) for probe in coverage.values())
        ):
            raise ConformanceError(f"{label}: invalid contract section coverage probes")
        expected = declared["expected"]
        if not isinstance(expected, dict):
            raise ConformanceError(f"{label}.expected must be an object")
        validate_expected(expected, classes, f"{label}.expected")
        golden = declared.get("normalized_ast_golden")
        if golden is not None and (not expected["parse"] or not isinstance(golden, dict)):
            raise ConformanceError(f"{label}: normalized AST golden requires accepted parse")
        if golden is not None:
            validate_normalized_ast(golden, f"{label}.normalized_ast_golden")
        ordered_golden = declared.get("ordered_sequence_golden")
        if ordered_golden is not None and not expected["parse"]:
            raise ConformanceError(f"{label}: ordered sequence golden requires accepted parse")
        for section, probe in coverage.items():
            validate_coverage_probe(
                section, probe, source, expected, golden, ordered_golden,
                revision, bounds, label
            )
            covered_sections.setdefault(section, []).append(
                {"case_id": identifier, "probe": probe}
            )
        cases.append(
            {
                "id": identifier,
                "origin": "hand",
                "source": source.decode("utf-8"),
                "expected": expected,
                "revision_digest_v2": revision,
                "normalized_ast_golden": golden,
                "ordered_sequence_golden": ordered_golden,
                "coverage_probes": coverage,
            }
        )
    if set(covered_sections) != set(sections):
        raise ConformanceError(
            "hand cases do not mechanically cover every required contract section: "
            f"missing={sorted(set(sections) - set(covered_sections))}"
        )

    generator_maximums = corpus["generator"]["maximums"]
    if generator_maximums != {
        "source_bytes": bounds["max_source_bytes"],
        "syntax_depth": bounds["max_syntax_depth"],
        "lines": bounds["max_lines"],
        "declarations": bounds["max_declarations"],
        "roles_per_relation": bounds["max_roles_per_relation"],
    }:
        raise ConformanceError("generator maximums differ from differential source bounds")
    generated = generated_cases(corpus["generator"])
    cases.extend(generated)
    identifiers = [case["id"] for case in cases]
    if len(identifiers) != len(set(identifiers)):
        raise ConformanceError("differential case ids must be unique")
    if not cases or len(cases) > bounds["max_cases"]:
        raise ConformanceError("differential case count is zero or exceeds its bound")
    for case in cases:
        validate_expected(case["expected"], classes, f"{case['id']}.expected")
        validate_source_bounds(case["source"].encode(), bounds, case["id"])
    return corpus, cases, contract


def bounded_command(
    argv: list[str], bounds: dict[str, Any], deadline: float
) -> subprocess.CompletedProcess[bytes]:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise ConformanceError("differential total timeout exceeded")
    timeout = min(float(bounds["per_process_timeout_seconds"]), remaining)
    try:
        return run_bounded(
            argv,
            timeout_seconds=timeout,
            max_stdout_bytes=bounds["max_stdout_bytes"],
            max_stderr_bytes=bounds["max_stderr_bytes"],
        )
    except subprocess.TimeoutExpired as error:
        raise ConformanceError(f"subprocess timeout: {argv[0]}") from error
    except BoundedProcessError as error:
        raise ConformanceError(f"bounded subprocess failure for {argv[0]}: {error}") from error


def require_json_string(value: Any, label: str) -> None:
    if not isinstance(value, str):
        raise ConformanceError(f"{label} must be a string")


def require_json_uint(value: Any, label: str, maximum: int = 0xFFFFFFFF) -> None:
    if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value <= maximum:
        raise ConformanceError(f"{label} must be an integer in 0..={maximum}")


def require_json_bool(value: Any, label: str) -> None:
    if not isinstance(value, bool):
        raise ConformanceError(f"{label} must be Boolean")


def require_json_array(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ConformanceError(f"{label} must be an array")
    return value


def require_closed_object(value: Any, fields: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ConformanceError(f"{label} must be an object")
    require_keys(value, fields, label)
    return value


def validate_string_array(value: Any, label: str) -> None:
    for index, item in enumerate(require_json_array(value, label)):
        require_json_string(item, f"{label}[{index}]")


def validate_type_expr(value: Any, label: str) -> None:
    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        raise ConformanceError(f"{label} must be a tagged type-expression object")
    kind = value["kind"]
    fields = {
        "object": {"kind", "name"},
        "relation_object": {"kind", "relation"},
        "indexed": {"kind", "base", "over_roles"},
        "refined": {"kind", "base", "predicates"},
    }.get(kind)
    if fields is None:
        raise ConformanceError(f"{label}.kind is unknown")
    require_keys(value, fields, label)
    if kind == "object":
        require_json_string(value["name"], f"{label}.name")
    elif kind == "relation_object":
        require_json_string(value["relation"], f"{label}.relation")
    else:
        validate_type_expr(value["base"], f"{label}.base")
        if kind == "indexed":
            validate_string_array(value["over_roles"], f"{label}.over_roles")
        else:
            for index, predicate in enumerate(require_json_array(value["predicates"], f"{label}.predicates")):
                validate_refinement(predicate, f"{label}.predicates[{index}]")


def validate_refinement(value: Any, label: str) -> None:
    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        raise ConformanceError(f"{label} must be a tagged refinement object")
    kind = value["kind"]
    fields = {
        "equals": {"kind", "value"},
        "member_of": {"kind", "values"},
        "cardinality": {"kind", "min", "max"},
        "key": {"kind", "roles"},
        "enum": {"kind", "values"},
        "predicate": {"kind", "name", "args"},
    }.get(kind)
    if fields is None:
        raise ConformanceError(f"{label}.kind is unknown")
    require_keys(value, fields, label)
    if kind == "equals":
        require_json_string(value["value"], f"{label}.value")
    elif kind in {"member_of", "enum"}:
        validate_string_array(value["values"], f"{label}.values")
    elif kind == "cardinality":
        require_json_uint(value["min"], f"{label}.min")
        require_json_uint(value["max"], f"{label}.max")
    elif kind == "key":
        validate_string_array(value["roles"], f"{label}.roles")
    else:
        require_json_string(value["name"], f"{label}.name")
        validate_string_array(value["args"], f"{label}.args")


def validate_constraint(value: Any, label: str) -> None:
    if not isinstance(value, dict) or not isinstance(value.get("tag"), str):
        raise ConformanceError(f"{label} must be a tagged constraint object")
    tag = value["tag"]
    required = {
        "functional": {"tag", "relation", "src_field", "dst_field"},
        "at_most": {"tag", "relation", "src_field", "dst_field", "max"},
        "typing": {"tag", "relation", "rule"},
        "symmetric_where_in": {"tag", "relation", "field", "values"},
        "symmetric": {"tag", "relation"},
        "transitive": {"tag", "relation"},
        "key": {"tag", "relation", "fields"},
        "named_block": {"tag", "name", "body"},
        "unknown": {"tag", "text"},
    }.get(tag)
    if required is None:
        raise ConformanceError(f"{label}.tag is unknown")
    optional = {"params"} if tag == "at_most" else set()
    if tag in {"symmetric_where_in", "symmetric", "transitive"}:
        optional = {"carriers", "params"}
    if not required.issubset(value) or not set(value).issubset(required | optional):
        raise ConformanceError(f"{label} fields differ from its closed {tag} variant")
    for key in required - {"tag", "max", "values", "fields", "body"}:
        require_json_string(value[key], f"{label}.{key}")
    if "max" in value:
        require_json_uint(value["max"], f"{label}.max")
    for key in ("values", "fields", "body", "params"):
        if key in value:
            validate_string_array(value[key], f"{label}.{key}")
    if "carriers" in value:
        carrier = require_closed_object(value["carriers"], {"left_field", "right_field"}, f"{label}.carriers")
        require_json_string(carrier["left_field"], f"{label}.carriers.left_field")
        require_json_string(carrier["right_field"], f"{label}.carriers.right_field")


def validate_path_expr(value: Any, label: str) -> None:
    if not isinstance(value, dict) or not isinstance(value.get("type"), str):
        raise ConformanceError(f"{label} must be a tagged path-expression object")
    tag = value["type"]
    fields = {
        "var": {"type", "name"},
        "reflexive": {"type", "entity"},
        "step": {"type", "from", "rel", "to"},
        "trans": {"type", "left", "right"},
        "inv": {"type", "path"},
    }.get(tag)
    if fields is None:
        raise ConformanceError(f"{label}.type is unknown")
    require_keys(value, fields, label)
    if tag == "trans":
        validate_path_expr(value["left"], f"{label}.left")
        validate_path_expr(value["right"], f"{label}.right")
    elif tag == "inv":
        validate_path_expr(value["path"], f"{label}.path")
    else:
        for key in fields - {"type"}:
            require_json_string(value[key], f"{label}.{key}")


def validate_normalized_ast(value: Any, label: str) -> None:
    module = require_closed_object(value, {"module_name", "imports", "schemas", "theories", "instances"}, label)
    require_json_string(module["module_name"], f"{label}.module_name")
    validate_string_array(module["imports"], f"{label}.imports")
    for si, schema_value in enumerate(require_json_array(module["schemas"], f"{label}.schemas")):
        sl = f"{label}.schemas[{si}]"
        schema = require_closed_object(schema_value, {"name", "objects", "subtypes", "relations", "generators"}, sl)
        require_json_string(schema["name"], f"{sl}.name")
        validate_string_array(schema["objects"], f"{sl}.objects")
        for i, subtype_value in enumerate(require_json_array(schema["subtypes"], f"{sl}.subtypes")):
            sub = require_closed_object(subtype_value, {"sub", "sup", "inclusion"}, f"{sl}.subtypes[{i}]")
            require_json_string(sub["sub"], f"{sl}.subtypes[{i}].sub")
            require_json_string(sub["sup"], f"{sl}.subtypes[{i}].sup")
            if sub["inclusion"] is not None:
                require_json_string(sub["inclusion"], f"{sl}.subtypes[{i}].inclusion")
        for i, relation_value in enumerate(require_json_array(schema["relations"], f"{sl}.relations")):
            rl = f"{sl}.relations[{i}]"
            relation = require_closed_object(relation_value, {"name", "fields"}, rl)
            require_json_string(relation["name"], f"{rl}.name")
            for fi, field_value in enumerate(require_json_array(relation["fields"], f"{rl}.fields")):
                fl = f"{rl}.fields[{fi}]"
                field = require_closed_object(field_value, {"field", "ty", "kind"}, fl)
                require_json_string(field["field"], f"{fl}.field")
                validate_type_expr(field["ty"], f"{fl}.ty")
                if field["kind"] not in {"data", "context", "world", "temporal", "parameter", "evidence"}:
                    raise ConformanceError(f"{fl}.kind is unknown")
        for i, generator_value in enumerate(require_json_array(schema["generators"], f"{sl}.generators")):
            gl = f"{sl}.generators[{i}]"
            generator = require_closed_object(generator_value, {"name", "source", "target", "kind", "reversible"}, gl)
            for key in ("name", "source", "target"):
                require_json_string(generator[key], f"{gl}.{key}")
            if generator["kind"] not in {"aspect", "function"}:
                raise ConformanceError(f"{gl}.kind is unknown")
            require_json_bool(generator["reversible"], f"{gl}.reversible")
    for ti, theory_value in enumerate(require_json_array(module["theories"], f"{label}.theories")):
        tl = f"{label}.theories[{ti}]"
        theory = require_closed_object(theory_value, {"name", "schema", "constraints", "equations", "rewrite_rules"}, tl)
        require_json_string(theory["name"], f"{tl}.name")
        require_json_string(theory["schema"], f"{tl}.schema")
        for i, constraint in enumerate(require_json_array(theory["constraints"], f"{tl}.constraints")):
            validate_constraint(constraint, f"{tl}.constraints[{i}]")
        for i, equation_value in enumerate(require_json_array(theory["equations"], f"{tl}.equations")):
            equation = require_closed_object(equation_value, {"name", "lhs", "rhs"}, f"{tl}.equations[{i}]")
            for key in ("name", "lhs", "rhs"):
                require_json_string(equation[key], f"{tl}.equations[{i}].{key}")
        for i, rewrite_value in enumerate(require_json_array(theory["rewrite_rules"], f"{tl}.rewrite_rules")):
            wl = f"{tl}.rewrite_rules[{i}]"
            rewrite = require_closed_object(rewrite_value, {"name", "orientation", "vars", "lhs", "rhs"}, wl)
            require_json_string(rewrite["name"], f"{wl}.name")
            if rewrite["orientation"] not in {"forward", "backward", "bidirectional"}:
                raise ConformanceError(f"{wl}.orientation is unknown")
            for vi, var_value in enumerate(require_json_array(rewrite["vars"], f"{wl}.vars")):
                vl = f"{wl}.vars[{vi}]"
                var = require_closed_object(var_value, {"name", "ty"}, vl)
                require_json_string(var["name"], f"{vl}.name")
                ty = var["ty"]
                if not isinstance(ty, dict) or ty.get("tag") not in {"object", "path"}:
                    raise ConformanceError(f"{vl}.ty has an unknown variant")
                expected = {"tag", "ty"} if ty["tag"] == "object" else {"tag", "from", "to"}
                require_keys(ty, expected, f"{vl}.ty")
                for key in expected - {"tag"}:
                    require_json_string(ty[key], f"{vl}.ty.{key}")
            validate_path_expr(rewrite["lhs"], f"{wl}.lhs")
            validate_path_expr(rewrite["rhs"], f"{wl}.rhs")
    for ii, instance_value in enumerate(require_json_array(module["instances"], f"{label}.instances")):
        il = f"{label}.instances[{ii}]"
        instance = require_closed_object(instance_value, {"name", "schema", "assignments"}, il)
        require_json_string(instance["name"], f"{il}.name")
        require_json_string(instance["schema"], f"{il}.schema")
        for ai, assignment_value in enumerate(require_json_array(instance["assignments"], f"{il}.assignments")):
            al = f"{il}.assignments[{ai}]"
            assignment = require_closed_object(assignment_value, {"name", "value"}, al)
            require_json_string(assignment["name"], f"{al}.name")
            set_value = require_closed_object(assignment["value"], {"items"}, f"{al}.value")
            for si, item_value in enumerate(require_json_array(set_value["items"], f"{al}.value.items")):
                sil = f"{al}.value.items[{si}]"
                if not isinstance(item_value, dict) or item_value.get("tag") not in {"ident", "tuple"}:
                    raise ConformanceError(f"{sil} has an unknown variant")
                if item_value["tag"] == "ident":
                    item = require_closed_object(item_value, {"tag", "name"}, sil)
                    require_json_string(item["name"], f"{sil}.name")
                else:
                    required = {"tag", "fields"}
                    if "label" in item_value:
                        required.add("label")
                    item = require_closed_object(item_value, required, sil)
                    if "label" in item:
                        require_json_string(item["label"], f"{sil}.label")
                    for fi, pair in enumerate(require_json_array(item["fields"], f"{sil}.fields")):
                        if not isinstance(pair, list) or len(pair) != 2:
                            raise ConformanceError(f"{sil}.fields[{fi}] must be a two-string array")
                        require_json_string(pair[0], f"{sil}.fields[{fi}][0]")
                        require_json_string(pair[1], f"{sil}.fields[{fi}][1]")


def validate_typecheck_summary(value: Any, label: str) -> None:
    summary = require_closed_object(value, {"module", "schemas", "theories", "instances", "assignments", "tuples"}, label)
    require_json_string(summary["module"], f"{label}.module")
    for key in ("schemas", "theories", "instances", "assignments", "tuples"):
        require_json_uint(summary[key], f"{label}.{key}", sys.maxsize)


def validate_formation_summary(value: Any, label: str) -> None:
    summary = require_closed_object(value, {"module", "import_closure", "schemas", "theories", "instances"}, label)
    require_json_string(summary["module"], f"{label}.module")
    for key in ("import_closure", "schemas", "theories", "instances"):
        require_json_uint(summary[key], f"{label}.{key}", sys.maxsize)


def parse_envelope(
    completed: subprocess.CompletedProcess[bytes], implementation: str, requested_stage: str
) -> dict[str, Any]:
    if completed.stderr:
        raise ConformanceError(f"{implementation} {requested_stage} envelope wrote stderr")
    try:
        text = completed.stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ConformanceError(f"{implementation} {requested_stage} envelope is not UTF-8") from error
    if not text.endswith("\n") or text.count("\n") != 1:
        raise ConformanceError(f"{implementation} {requested_stage} envelope must be one JSON line")
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ConformanceError(f"{implementation} {requested_stage} envelope is malformed JSON") from error
    if not isinstance(value, dict):
        raise ConformanceError(f"{implementation} {requested_stage} envelope must be an object")
    require_keys(
        value,
        {
            "schema",
            "version",
            "implementation",
            "requested_stage",
            "observed_stage",
            "decision",
            "normalized_ast",
            "summary",
            "rejection_class",
            "diagnostic",
        },
        f"{implementation} {requested_stage} envelope",
    )
    if not isinstance(value["schema"], str) or value["schema"] != ENVELOPE_SCHEMA:
        raise ConformanceError("unsupported differential envelope schema")
    if not isinstance(value["version"], int) or isinstance(value["version"], bool) or value["version"] != 1:
        raise ConformanceError("unsupported differential envelope version")
    for key in ("implementation", "requested_stage", "observed_stage", "decision"):
        require_json_string(value[key], f"differential envelope.{key}")
    if value["implementation"] != implementation or value["requested_stage"] != requested_stage:
        raise ConformanceError("differential envelope implementation/request mismatch")
    observed = value["observed_stage"]
    allowed_observed = {
        "parse": {"boundary", "parse"},
        "typecheck": {"boundary", "parse", "typecheck"},
        "formation": {"boundary", "parse", "formation"},
    }[requested_stage]
    if observed not in allowed_observed or value["decision"] not in {"accepted", "rejected"}:
        raise ConformanceError("differential envelope stage/decision is unknown")
    accepted = value["decision"] == "accepted"
    if completed.returncode != (0 if accepted else 1):
        raise ConformanceError("differential envelope decision and process exit disagree")
    if accepted:
        if observed != requested_stage or value["diagnostic"] is not None or value["rejection_class"] is not None:
            raise ConformanceError("accepted differential envelope has rejection data")
        if requested_stage in {"parse", "typecheck"}:
            validate_normalized_ast(
                value["normalized_ast"], f"{implementation} {requested_stage} normalized_ast"
            )
        if requested_stage == "parse" and value["summary"] is not None:
            raise ConformanceError("accepted parser envelope has an unexpected summary")
        if requested_stage == "typecheck":
            validate_typecheck_summary(value["summary"], f"{implementation} typecheck summary")
        if requested_stage == "formation":
            if value["normalized_ast"] is not None:
                raise ConformanceError("formation envelope must not become an AST authority")
            validate_formation_summary(value["summary"], f"{implementation} formation summary")
    else:
        if not isinstance(value["diagnostic"], str) or not value["diagnostic"]:
            raise ConformanceError("rejected differential envelope lacks a diagnostic")
        if value["summary"] is not None:
            raise ConformanceError("rejected differential envelope has a summary")
        if observed in {"boundary", "parse"} and value["normalized_ast"] is not None:
            raise ConformanceError("pre-typecheck rejection envelope has an AST")
        if observed == "typecheck":
            validate_normalized_ast(
                value["normalized_ast"], f"{implementation} rejected typecheck normalized_ast"
            )
        if requested_stage == "formation" and value["normalized_ast"] is not None:
            raise ConformanceError("formation envelope must not become an AST authority")
        if requested_stage != "formation" and value["rejection_class"] is not None:
            raise ConformanceError("parser/typechecker binary must not self-assign a rejection class")
        if requested_stage == "formation" and not isinstance(value["rejection_class"], str):
            raise ConformanceError("formation rejection lacks a typed class")
    return value


def envelope_observation(envelope: dict[str, Any]) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(
        [],
        0 if envelope["decision"] == "accepted" else 1,
        "",
        envelope["diagnostic"] or "",
    )


def make_differential_failure(
    case_id: str,
    message: str,
    *,
    mismatch_kind: str,
    stage: str,
    expected_decision: str | None,
    rust_decision: str | None,
    lean_decision: str | None,
    expected_class: str | None = None,
    rust_class: str | None = None,
    lean_class: str | None = None,
) -> DifferentialFailure:
    return DifferentialFailure(
        f"{case_id}: {message}",
        DifferentialFailureSignature(
            mismatch_kind=mismatch_kind,
            stage=stage,
            expected_decision=expected_decision,
            rust_decision=rust_decision,
            lean_decision=lean_decision,
            expected_class=expected_class,
            rust_class=rust_class,
            lean_class=lean_class,
        ),
    )


def run_envelope(
    binary: Path,
    implementation: str,
    stage: str,
    source_path: Path,
    bounds: dict[str, Any],
    deadline: float,
) -> dict[str, Any]:
    completed = bounded_command(
        [str(binary), "--contract-envelope-v1", str(source_path)], bounds, deadline
    )
    return parse_envelope(completed, implementation, stage)


def run_digest(
    binary: Path,
    extra_args: list[str],
    source_path: Path,
    bounds: dict[str, Any],
    deadline: float,
) -> str:
    completed = bounded_command([str(binary), *extra_args, str(source_path)], bounds, deadline)
    if completed.returncode != 0 or completed.stderr:
        raise ConformanceError(f"digest binary rejected bounded UTF-8 source: {binary}")
    try:
        value = completed.stdout.decode("utf-8").strip()
    except UnicodeDecodeError as error:
        raise ConformanceError(f"digest output is not UTF-8: {binary}") from error
    if not re.fullmatch(r"axi:revision:v2:sha256:[0-9a-f]{64}", value):
        raise ConformanceError(f"malformed revision digest output: {binary}")
    return value


def require_hand_golden(actual: dict[str, Any], golden: dict[str, Any], label: str) -> None:
    if actual != golden:
        raise ConformanceError(f"{label}: shared parser agreement differs from hand golden")


def has_unsupported_constructor(ast: dict[str, Any], outcome_class: str) -> bool:
    constraints = [
        constraint
        for theory in ast.get("theories", [])
        if isinstance(theory, dict)
        for constraint in theory.get("constraints", [])
        if isinstance(constraint, dict)
    ]
    expected_tag = {
        "unsupported.opaque_constraint": "unknown",
        "unsupported.named_constraint_semantics": "named_block",
    }.get(outcome_class)
    return expected_tag is not None and any(item.get("tag") == expected_tag for item in constraints)


def collect_discriminators(value: Any, key: str) -> set[str]:
    found: set[str] = set()
    if isinstance(value, dict):
        tag = value.get(key)
        if isinstance(tag, str):
            found.add(tag)
        for child in value.values():
            found.update(collect_discriminators(child, key))
    elif isinstance(value, list):
        for child in value:
            found.update(collect_discriminators(child, key))
    return found


ORDERED_SEQUENCE_NAMES = {
    "SchemaV1Module.imports",
    "SchemaV1Module.schemas",
    "SchemaV1Module.theories",
    "SchemaV1Module.instances",
    "SchemaV1Schema.objects",
    "SchemaV1Schema.subtypes",
    "SchemaV1Schema.relations",
    "SchemaV1Schema.generators",
    "RelationDeclV1.fields",
    "TypeExprV1.indexed.over_roles",
    "TypeExprV1.refined.predicates",
    "RefinementPredicateV1 member/key/enum values",
    "SchemaV1Theory.constraints",
    "SchemaV1Theory.equations",
    "SchemaV1Theory.rewrite_rules",
    "RewriteRuleV1.vars",
    "SetLiteralV1.items",
    "SetItemV1.tuple.fields",
    "SchemaV1Instance.assignments",
}


def validate_ordered_sequence_golden_shape(value: Any, label: str) -> None:
    golden = require_closed_object(value, ORDERED_SEQUENCE_NAMES, label)
    for name, sequence in golden.items():
        if not isinstance(sequence, list):
            raise ConformanceError(f"{label}.{name} must be an array")


def ordered_sequence_observations(ast: dict[str, Any]) -> dict[str, Any]:
    schemas = ast["schemas"]
    theories = ast["theories"]
    instances = ast["instances"]
    type_exprs: list[dict[str, Any]] = []

    def collect_type_expr(value: dict[str, Any]) -> None:
        type_exprs.append(value)
        if value["kind"] in {"indexed", "refined"}:
            collect_type_expr(value["base"])

    for schema in schemas:
        for relation in schema["relations"]:
            for field in relation["fields"]:
                collect_type_expr(field["ty"])

    refinements = [
        predicate
        for expression in type_exprs
        if expression["kind"] == "refined"
        for predicate in expression["predicates"]
    ]
    set_items = [
        assignment["value"]["items"]
        for instance in instances
        for assignment in instance["assignments"]
    ]

    def item_identity(item: dict[str, Any]) -> str:
        if item["tag"] == "ident":
            return f"ident:{item['name']}"
        label = item.get("label", "")
        fields = "|".join(f"{name}={value}" for name, value in item["fields"])
        return f"tuple:{label}:{fields}"

    return {
        "SchemaV1Module.imports": ast["imports"],
        "SchemaV1Module.schemas": [schema["name"] for schema in schemas],
        "SchemaV1Module.theories": [theory["name"] for theory in theories],
        "SchemaV1Module.instances": [instance["name"] for instance in instances],
        "SchemaV1Schema.objects": [schema["objects"] for schema in schemas],
        "SchemaV1Schema.subtypes": [
            [
                f"{item['sub']}<{item['sup']}:{item['inclusion'] or ''}"
                for item in schema["subtypes"]
            ]
            for schema in schemas
        ],
        "SchemaV1Schema.relations": [
            [relation["name"] for relation in schema["relations"]]
            for schema in schemas
        ],
        "SchemaV1Schema.generators": [
            [generator["name"] for generator in schema["generators"]]
            for schema in schemas
        ],
        "RelationDeclV1.fields": [
            [field["field"] for field in relation["fields"]]
            for schema in schemas
            for relation in schema["relations"]
        ],
        "TypeExprV1.indexed.over_roles": [
            expression["over_roles"]
            for expression in type_exprs
            if expression["kind"] == "indexed"
        ],
        "TypeExprV1.refined.predicates": [
            [predicate["kind"] for predicate in expression["predicates"]]
            for expression in type_exprs
            if expression["kind"] == "refined"
        ],
        "RefinementPredicateV1 member/key/enum values": [
            {
                "kind": predicate["kind"],
                "values": predicate["roles"]
                if predicate["kind"] == "key"
                else predicate["values"],
            }
            for predicate in refinements
            if predicate["kind"] in {"member_of", "key", "enum"}
        ],
        "SchemaV1Theory.constraints": [
            [constraint["tag"] for constraint in theory["constraints"]]
            for theory in theories
        ],
        "SchemaV1Theory.equations": [
            [equation["name"] for equation in theory["equations"]]
            for theory in theories
        ],
        "SchemaV1Theory.rewrite_rules": [
            [rewrite["name"] for rewrite in theory["rewrite_rules"]]
            for theory in theories
        ],
        "RewriteRuleV1.vars": [
            [var["name"] for var in rewrite["vars"]]
            for theory in theories
            for rewrite in theory["rewrite_rules"]
        ],
        "SetLiteralV1.items": [
            [item_identity(item) for item in items] for items in set_items
        ],
        "SetItemV1.tuple.fields": [
            item["fields"]
            for items in set_items
            for item in items
            if item["tag"] == "tuple"
        ],
        "SchemaV1Instance.assignments": [
            [assignment["name"] for assignment in instance["assignments"]]
            for instance in instances
        ],
    }


def require_ordered_sequence_golden(
    actual: dict[str, Any], golden: dict[str, Any], label: str
) -> None:
    if actual != golden:
        differing = sorted(
            name for name in set(actual) | set(golden) if actual.get(name) != golden.get(name)
        )
        raise ConformanceError(
            f"{label}: ordered AST sequences differ from hand golden: {differing}"
        )


def validate_runtime_coverage(case: dict[str, Any], ast: dict[str, Any]) -> None:
    probes = set(case.get("coverage_probes", {}).values())
    if "all-ast-constructors-and-ordered-payloads" not in probes:
        return
    ordered_golden = case.get("ordered_sequence_golden")
    validate_ordered_sequence_golden_shape(
        ordered_golden, f"{case['id']}.ordered_sequence_golden"
    )
    require_ordered_sequence_golden(
        ordered_sequence_observations(ast), ordered_golden, case["id"]
    )
    schemas = ast["schemas"]
    theories = ast["theories"]
    instances = ast["instances"]
    if not schemas or not theories or not instances or len(ast["imports"]) < 2:
        raise ConformanceError(f"{case['id']}: all-constructor AST lacks top-level structures/order")
    schema = schemas[0]
    theory = theories[0]
    instance = instances[0]
    if not all((schema["subtypes"], schema["relations"], schema["generators"])):
        raise ConformanceError(f"{case['id']}: all-constructor AST lacks schema structures")
    if not all((theory["constraints"], theory["equations"], theory["rewrite_rules"])):
        raise ConformanceError(f"{case['id']}: all-constructor AST lacks theory structures")
    if not instance["assignments"]:
        raise ConformanceError(f"{case['id']}: all-constructor AST lacks instance assignments")
    expected = {
        "kind": {
            "object", "relation_object", "indexed", "refined", "equals", "member_of",
            "cardinality", "key", "enum", "predicate",
        },
        "tag": {
            "functional", "at_most", "typing", "symmetric_where_in", "symmetric",
            "transitive", "key", "named_block", "unknown", "object", "path", "ident", "tuple",
        },
        "type": {"var", "reflexive", "step", "trans", "inv"},
    }
    for discriminator, required in expected.items():
        missing = required - collect_discriminators(ast, discriminator)
        if missing:
            raise ConformanceError(
                f"{case['id']}: all-constructor AST misses {discriminator} values {sorted(missing)}"
            )
    role_kinds = {
        field["kind"]
        for relation in schema["relations"]
        for field in relation["fields"]
    }
    if role_kinds != {"data", "context", "world", "temporal", "parameter", "evidence"}:
        raise ConformanceError(f"{case['id']}: all-constructor AST misses role kinds")
    if {item["kind"] for item in schema["generators"]} != {"aspect", "function"}:
        raise ConformanceError(f"{case['id']}: all-constructor AST misses generator kinds")
    if {item["orientation"] for item in theory["rewrite_rules"]} != {
        "forward", "backward", "bidirectional"
    }:
        raise ConformanceError(f"{case['id']}: all-constructor AST misses rewrite orientations")
    if not any("carriers" in item for item in theory["constraints"]):
        raise ConformanceError(f"{case['id']}: all-constructor AST misses carrier fields")


def evaluate_case(
    case: dict[str, Any], path: Path, bins: Binaries, bounds: dict[str, Any], deadline: float
) -> dict[str, Any]:
    source = case["source"]
    source_bytes = source.encode("utf-8")
    path.write_bytes(source_bytes)
    expected = case["expected"]
    rust_parse = run_envelope(bins.rust_parse, "rust", "parse", path, bounds, deadline)
    lean_parse = run_envelope(bins.lean_parse, "lean", "parse", path, bounds, deadline)
    parse_accepted = expected["parse"]
    expected_parse_decision = "accepted" if parse_accepted else "rejected"
    if rust_parse["decision"] != expected_parse_decision or lean_parse["decision"] != expected_parse_decision:
        raise make_differential_failure(
            case["id"],
            "parser decision differs from expected",
            mismatch_kind="decision_mismatch",
            stage="parse",
            expected_decision=expected_parse_decision,
            rust_decision=rust_parse["decision"],
            lean_decision=lean_parse["decision"],
        )

    ast_digest: str | None = None
    observed_class = "accepted"
    if parse_accepted:
        if rust_parse["normalized_ast"] != lean_parse["normalized_ast"]:
            raise make_differential_failure(
                case["id"],
                "normalized Rust/Lean AST values differ",
                mismatch_kind="normalized_ast_mismatch",
                stage="parse",
                expected_decision="accepted",
                rust_decision="accepted",
                lean_decision="accepted",
            )
        validate_runtime_coverage(case, rust_parse["normalized_ast"])
        golden = case.get("normalized_ast_golden")
        if golden is not None:
            require_hand_golden(rust_parse["normalized_ast"], golden, case["id"])
        ast_digest = sha256_bytes(canonical_json(rust_parse["normalized_ast"]))
    else:
        expected_class = expected["outcome_class"]
        actual = {
            language: classify_actual_rejection(
                "parse", language, source, envelope_observation(envelope)
            )
            for language, envelope in (("rust", rust_parse), ("lean", lean_parse))
        }
        if set(actual.values()) != {expected_class}:
            raise make_differential_failure(
                case["id"],
                f"parser rejection class differs: {actual}",
                mismatch_kind="rejection_class_mismatch",
                stage="parse",
                expected_decision="rejected",
                rust_decision="rejected",
                lean_decision="rejected",
                expected_class=expected_class,
                rust_class=actual["rust"],
                lean_class=actual["lean"],
            )
        observed_class = expected_class

    rust_revision = run_digest(bins.rust_digest, [], path, bounds, deadline)
    if parse_accepted:
        lean_revision = run_digest(
            bins.lean_digest, ["--revision-digest-v2"], path, bounds, deadline
        )
        if rust_revision != lean_revision:
            raise make_differential_failure(
                case["id"],
                "exact-byte revision digest differs",
                mismatch_kind="revision_digest_mismatch",
                stage="digest",
                expected_decision="accepted",
                rust_decision="accepted",
                lean_decision="accepted",
            )
    expected_revision = case.get("revision_digest_v2")
    if expected_revision is not None and rust_revision != expected_revision:
        raise ConformanceError(f"{case['id']}: shared digest differs from hand golden")

    typecheck_result: bool | None = None
    formation_result: bool | None = None
    if parse_accepted:
        rust_type = run_envelope(bins.rust_typecheck, "rust", "typecheck", path, bounds, deadline)
        lean_type = run_envelope(bins.lean_typecheck, "lean", "typecheck", path, bounds, deadline)
        typecheck_result = rust_type["decision"] == "accepted"
        expected_type_decision = "accepted" if expected["typecheck"] else "rejected"
        if rust_type["decision"] != expected_type_decision or lean_type["decision"] != expected_type_decision:
            raise make_differential_failure(
                case["id"],
                "typecheck decision differs",
                mismatch_kind="decision_mismatch",
                stage="typecheck",
                expected_decision=expected_type_decision,
                rust_decision=rust_type["decision"],
                lean_decision=lean_type["decision"],
            )
        if rust_type["normalized_ast"] != rust_parse["normalized_ast"]:
            raise make_differential_failure(
                case["id"],
                "Rust parser/typechecker normalized AST drift",
                mismatch_kind="parser_typechecker_ast_mismatch",
                stage="rust.typecheck",
                expected_decision=expected_type_decision,
                rust_decision=rust_type["decision"],
                lean_decision=lean_type["decision"],
            )
        if lean_type["normalized_ast"] != lean_parse["normalized_ast"]:
            raise make_differential_failure(
                case["id"],
                "Lean parser/typechecker normalized AST drift",
                mismatch_kind="parser_typechecker_ast_mismatch",
                stage="lean.typecheck",
                expected_decision=expected_type_decision,
                rust_decision=rust_type["decision"],
                lean_decision=lean_type["decision"],
            )
        if typecheck_result:
            if rust_type["summary"] != lean_type["summary"]:
                raise make_differential_failure(
                    case["id"],
                    "Rust/Lean typecheck summaries differ",
                    mismatch_kind="summary_mismatch",
                    stage="typecheck",
                    expected_decision="accepted",
                    rust_decision="accepted",
                    lean_decision="accepted",
                )
        else:
            expected_class = expected["outcome_class"]
            actual = {
                language: classify_actual_rejection(
                    "typecheck", language, source, envelope_observation(envelope)
                )
                for language, envelope in (("rust", rust_type), ("lean", lean_type))
            }
            if set(actual.values()) != {expected_class}:
                raise make_differential_failure(
                    case["id"],
                    f"typecheck rejection class differs: {actual}",
                    mismatch_kind="rejection_class_mismatch",
                    stage="typecheck",
                    expected_decision="rejected",
                    rust_decision="rejected",
                    lean_decision="rejected",
                    expected_class=expected_class,
                    rust_class=actual["rust"],
                    lean_class=actual["lean"],
                )
            observed_class = expected_class

        if typecheck_result:
            formation = run_envelope(
                bins.rust_formation, "rust", "formation", path, bounds, deadline
            )
            formation_result = formation["decision"] == "accepted"
            expected_formation_decision = "accepted" if expected["formation"] else "rejected"
            if formation["decision"] != expected_formation_decision:
                raise make_differential_failure(
                    case["id"],
                    "formation decision differs",
                    mismatch_kind="decision_mismatch",
                    stage="formation",
                    expected_decision=expected_formation_decision,
                    rust_decision=formation["decision"],
                    lean_decision=None,
                )
            if not formation_result:
                observed_class = formation["rejection_class"]
                if observed_class != expected["outcome_class"]:
                    raise make_differential_failure(
                        case["id"],
                        "formation rejection class differs",
                        mismatch_kind="rejection_class_mismatch",
                        stage="formation",
                        expected_decision="rejected",
                        rust_decision="rejected",
                        lean_decision=None,
                        expected_class=expected["outcome_class"],
                        rust_class=observed_class,
                    )
            elif expected["outcome_class"].startswith("unsupported."):
                if not has_unsupported_constructor(rust_parse["normalized_ast"], expected["outcome_class"]):
                    raise make_differential_failure(
                        case["id"],
                        "unsupported syntax was silently reported as accepted",
                        mismatch_kind="unsupported_as_success",
                        stage="formation",
                        expected_decision="accepted",
                        rust_decision="accepted",
                        lean_decision=None,
                        expected_class=expected["outcome_class"],
                    )
                observed_class = expected["outcome_class"]
    if observed_class != expected["outcome_class"]:
        raise make_differential_failure(
            case["id"],
            f"expected outcome {expected['outcome_class']}, observed {observed_class}",
            mismatch_kind="outcome_class_mismatch",
            stage="outcome",
            expected_decision=None,
            rust_decision=None,
            lean_decision=None,
            expected_class=expected["outcome_class"],
            rust_class=observed_class,
        )
    return {
        "id": case["id"],
        "origin": case["origin"],
        "source_bytes": len(source_bytes),
        "source_sha256": sha256_bytes(source_bytes),
        "revision_digest_v2": rust_revision,
        "lean_revision_compared": parse_accepted,
        "normalized_ast_sha256": ast_digest,
        "parse": parse_accepted,
        "typecheck": typecheck_result,
        "formation": formation_result,
        "outcome_class": observed_class,
        "generator_trace": case.get("generator_trace"),
        "coverage_probes": case.get("coverage_probes", {}),
    }


def build_report(root: Path, bins: Binaries) -> dict[str, Any]:
    corpus, cases, contract = load_corpus(root)
    for binary in bins.__dict__.values():
        if not binary.resolve(strict=True).is_file():
            raise ConformanceError(f"binary is not a regular file: {binary}")
    bounds = corpus["bounds"]
    deadline = time.monotonic() + bounds["total_timeout_seconds"]
    observations: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="axiograph-axi-v1-differential-") as directory:
        work = Path(directory)
        for position, case in enumerate(cases):
            case_path = work / f"{position:03d}.axi"
            try:
                observations.append(evaluate_case(case, case_path, bins, bounds, deadline))
            except DifferentialFailure as error:
                if case["origin"] != "generated":
                    raise
                failure_signature = error.signature

                def reproduces(candidate_source: str) -> bool:
                    candidate = dict(case)
                    candidate["source"] = candidate_source
                    try:
                        evaluate_case(candidate, case_path, bins, bounds, deadline)
                    except DifferentialFailure as candidate_error:
                        return candidate_error.signature == failure_signature
                    except ConformanceError:
                        return False
                    return False

                minimized = shrink_failure_source(case["source"], reproduces)
                minimized["failure_signature"] = {
                    "mismatch_kind": failure_signature.mismatch_kind,
                    "stage": failure_signature.stage,
                    "expected_decision": failure_signature.expected_decision,
                    "rust_decision": failure_signature.rust_decision,
                    "lean_decision": failure_signature.lean_decision,
                    "expected_class": failure_signature.expected_class,
                    "rust_class": failure_signature.rust_class,
                    "lean_class": failure_signature.lean_class,
                }
                raise ConformanceError(
                    f"{case['id']}: generated differential failure; "
                    f"counterexample={json.dumps(minimized, sort_keys=True, separators=(',', ':'))}"
                ) from error
    counts: dict[str, int] = {}
    section_coverage: dict[str, list[dict[str, str]]] = {}
    for observation in observations:
        outcome = observation["outcome_class"]
        counts[outcome] = counts.get(outcome, 0) + 1
        if observation["origin"] == "hand":
            for section, probe in observation["coverage_probes"].items():
                section_coverage.setdefault(section, []).append(
                    {"case_id": observation["id"], "probe": probe, "runtime": "passed"}
                )
    if set(section_coverage) != set(corpus["required_contract_sections"]):
        raise ConformanceError("runtime hand-case section coverage is incomplete")
    return {
        "schema": REPORT_SCHEMA,
        "version": 1,
        "status": "pass",
        "contract_sha256": sha256_bytes((root / CONTRACT_PATH).read_bytes()),
        "corpus_sha256": sha256_bytes((root / CORPUS_PATH).read_bytes()),
        "generator": {
            "version": corpus["generator"]["version"],
            "algorithm": corpus["generator"]["algorithm"],
            "seed": corpus["generator"]["seed"],
            "case_count": corpus["generator"]["case_count"],
            "maximums": corpus["generator"]["maximums"],
            "generated_cases_sha256": corpus["generator"]["expected_cases_sha256"],
            "stable_order": corpus["generator"]["stable_order"],
        },
        "bounds": bounds,
        "measurements": {
            "hand_cases": len(corpus["hand_cases"]),
            "generated_cases": corpus["generator"]["case_count"],
            "total_cases": len(observations),
            "outcomes": dict(sorted(counts.items())),
            "duration_excluded": True,
        },
        "cases": observations,
        "section_coverage": dict(sorted(section_coverage.items())),
        "authority": "exact_utf8_axi_bytes",
        "normalized_ast_authoritative": False,
        "rust_output_authoritative_for_lean": False,
        "accepted": False,
        "contract_version": contract["version"],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--rust-parse", type=Path, required=True)
    parser.add_argument("--lean-parse", type=Path, required=True)
    parser.add_argument("--rust-typecheck", type=Path, required=True)
    parser.add_argument("--lean-typecheck", type=Path, required=True)
    parser.add_argument("--rust-digest", type=Path, required=True)
    parser.add_argument("--lean-digest", type=Path, required=True)
    parser.add_argument("--rust-formation", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve(strict=True)
    bins = Binaries(
        rust_parse=args.rust_parse.resolve(strict=True),
        lean_parse=args.lean_parse.resolve(strict=True),
        rust_typecheck=args.rust_typecheck.resolve(strict=True),
        lean_typecheck=args.lean_typecheck.resolve(strict=True),
        rust_digest=args.rust_digest.resolve(strict=True),
        lean_digest=args.lean_digest.resolve(strict=True),
        rust_formation=args.rust_formation.resolve(strict=True),
    )
    report_bytes = canonical_json(build_report(root, bins))
    if args.report is None:
        sys.stdout.buffer.write(report_bytes)
    else:
        report = args.report
        if report.exists():
            raise ConformanceError(f"report path already exists: {report}")
        report.parent.mkdir(parents=True, exist_ok=True)
        with report.open("xb") as output:
            output.write(report_bytes)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ConformanceError, OSError, subprocess.SubprocessError) as error:
        print(f"axi_v1 differential conformance failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
