"""Exercise real Rust/Lean parser and trusted Lean file-mode N/N+1 boundaries.

This corpus reports loader observations, not a certificate or ontology acceptance.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from scripts.check_axi_contract_conformance import (
    ConformanceError,
    bounded_command,
    canonical_json,
    load_json_object,
    require_keys,
    run_digest,
    run_envelope,
    sha256_bytes,
)

CORPUS = Path("fixtures/canonical/contract/limits/corpus.json")
CERTIFICATE = "rust/fuzz/corpus/certificate_json/typecheck_v2.json"
LIMITS = {
    "module_bytes": 4 * 1024 * 1024,
    "syntax_depth": 64,
    "anchor_modules": 16,
    "file_inputs": 32,
}
EXECUTION_BOUNDS = {
    "per_process_timeout_seconds": 30,
    "total_timeout_seconds": 600,
    "max_stdout_bytes": 1024 * 1024,
    "max_stderr_bytes": 256 * 1024,
}
GENERATED_NAMES = {
    "module_n",
    "module_n_plus_one",
    "depth_n",
    "depth_n_plus_one",
    "invalid_utf8",
    "identity_lf",
    "identity_crlf_unicode",
    "anchor_bundle_n",
    "anchor_bundle_n_plus_one",
}


def load_spec(root: Path) -> dict[str, Any]:
    spec = load_json_object(root / CORPUS, "trusted file boundary corpus")
    require_keys(
        spec,
        {
            "schema",
            "version",
            "limits",
            "generated_sha256",
            "missing_anchor_certificate",
            "execution_bounds",
        },
        "trusted file boundary corpus",
    )
    if (
        spec["schema"] != "axiograph.axi_v1_trusted_file_boundary_corpus"
        or type(spec["version"]) is not int
        or spec["version"] != 1
        or spec["limits"] != LIMITS
        or spec["execution_bounds"] != EXECUTION_BOUNDS
    ):
        raise ConformanceError(
            "trusted file boundary corpus changes fixed verifier limits"
        )
    require_keys(spec["generated_sha256"], GENERATED_NAMES, "generated boundary bytes")
    certificate = spec["missing_anchor_certificate"]
    require_keys(certificate, {"path", "sha256"}, "missing-anchor certificate")
    if certificate["path"] != CERTIFICATE:
        raise ConformanceError("missing-anchor certificate path differs")
    raw = (root / CERTIFICATE).read_bytes()
    if len(raw) > 8192 or sha256_bytes(raw) != certificate["sha256"]:
        raise ConformanceError("missing-anchor certificate bytes differ")
    return spec


def generate_inputs(spec: dict[str, Any]) -> tuple[dict[str, bytes], list[bytes]]:
    maximum = LIMITS["module_bytes"]
    prefix = b"module Limit\n" + (b"#" + b"a" * 524000 + b"\n") * 8 + b"#"
    if len(prefix) >= maximum:
        raise ConformanceError("bounded comment generator exceeds module limit")
    module_n = prefix + b"a" * (maximum - len(prefix))
    depth_prefix = (
        b"module Depth\nschema S\n  object A\ntheory T on S\n  constraint review_only "
    )

    def depth(count: int) -> bytes:
        return depth_prefix + b"(" * count + b"x" + b")" * count + b"\n"

    anchors = [f"module Anchor{index:02d}\n".encode() for index in range(17)]
    generated = {
        "module_n": module_n,
        "module_n_plus_one": module_n + b"a",
        "depth_n": depth(LIMITS["syntax_depth"]),
        "depth_n_plus_one": depth(LIMITS["syntax_depth"] + 1),
        "invalid_utf8": b"module Invalid\n#\xff",
        "identity_lf": "module Identity\nschema S\n  object A\n# é 😀\n".encode(),
        "identity_crlf_unicode": (
            "module Identity\r\nschema S\r\n  object A\r\n# é 😀 updated\r\n".encode()
        ),
        "anchor_bundle_n": b"".join(item + b"\0" for item in anchors[:16]),
        "anchor_bundle_n_plus_one": b"".join(item + b"\0" for item in anchors),
    }
    for name, raw in generated.items():
        if sha256_bytes(raw) != spec["generated_sha256"][name]:
            raise ConformanceError(f"{name}: generated boundary source SHA-256 drift")
    return generated, anchors


def require_verifier_result(
    identifier: str,
    verify: Path,
    arguments: list[Path],
    *,
    code: int,
    loaded: int,
    reason: str,
    bounds: dict[str, Any],
    deadline: float,
    reason_count: int = 1,
    expected_revision: str | None = None,
) -> dict[str, Any]:
    result = bounded_command(
        [str(verify), *(str(path) for path in arguments)], bounds, deadline
    )
    try:
        stdout = result.stdout.decode("utf-8")
        stderr = result.stderr.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ConformanceError(f"{identifier}: verifier output is not UTF-8") from error
    actual_loaded = stdout.count("ok: loaded axi module revision=")
    if (
        result.returncode != code
        or actual_loaded != loaded
        or stderr.count(reason) != reason_count
        or (
            expected_revision is not None
            and stdout.count(f"revision={expected_revision}") != 1
        )
    ):
        raise ConformanceError(
            f"{identifier}: expected code={code} loaded={loaded} "
            f"reason={reason!r} count={reason_count}, got code={result.returncode} "
            f"loaded={actual_loaded} stderr={stderr[:400]!r}"
        )
    return {
        "id": identifier,
        "exit_code": code,
        "loaded_anchors": loaded,
        "rejection_reason": reason,
        "certificate_accepted": False,
        "input_sha256": [sha256_bytes(path.read_bytes()) for path in arguments],
    }


def build_report(
    root: Path, rust_parse: Path, lean_parse: Path, rust_digest: Path, lean_verify: Path
) -> dict[str, Any]:
    spec = load_spec(root)
    sources, anchors = generate_inputs(spec)
    bounds = spec["execution_bounds"]
    deadline = time.monotonic() + bounds["total_timeout_seconds"]
    observations: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(
        prefix="axiograph-trusted-file-boundaries-"
    ) as directory:
        work = Path(directory)
        paths = {
            name: work / f"{name}.axi"
            for name in sources
            if not name.startswith("anchor_bundle")
        }
        for name, path in paths.items():
            path.write_bytes(sources[name])
        anchor_paths = []
        for index, raw in enumerate(anchors):
            path = work / f"anchor_{index:02d}.axi"
            path.write_bytes(raw)
            anchor_paths.append(path)
        cert = root / CERTIFICATE
        exact = paths["module_n"]
        digest = run_digest(rust_digest, [], exact, bounds, deadline)
        if (
            run_digest(lean_verify, ["--revision-digest-v2"], exact, bounds, deadline)
            != digest
        ):
            raise ConformanceError("exact-N Rust/Lean revision digest differs")
        for name, expected, reason in (
            ("depth_n", "accepted", ""),
            ("depth_n_plus_one", "rejected", "syntax nesting exceeds 64 delimiters"),
        ):
            observed = {}
            for implementation, binary in (("rust", rust_parse), ("lean", lean_parse)):
                envelope = run_envelope(
                    binary, implementation, "parse", paths[name], bounds, deadline
                )
                if envelope["decision"] != expected or reason not in (
                    envelope["diagnostic"] or ""
                ):
                    raise ConformanceError(
                        f"{name}: {implementation} parser boundary differs"
                    )
                observed[implementation] = envelope
            if expected == "accepted" and (
                observed["rust"]["normalized_ast"] != observed["lean"]["normalized_ast"]
            ):
                raise ConformanceError(f"{name}: Rust/Lean normalized AST differs")
            observations.append(
                {
                    "id": name,
                    "source_sha256": spec["generated_sha256"][name],
                    "rust_parse": expected,
                    "lean_parse": expected,
                }
            )
        identity_asts = {}
        identity_digests = {}
        for name in ("identity_lf", "identity_crlf_unicode"):
            parsed = {
                implementation: run_envelope(
                    binary, implementation, "parse", paths[name], bounds, deadline
                )
                for implementation, binary in (
                    ("rust", rust_parse),
                    ("lean", lean_parse),
                )
            }
            if (
                any(envelope["decision"] != "accepted" for envelope in parsed.values())
                or parsed["rust"]["normalized_ast"] != parsed["lean"]["normalized_ast"]
            ):
                raise ConformanceError(f"{name}: independently parsed AST differs")
            identity_asts[name] = parsed["rust"]["normalized_ast"]
            rust_revision = run_digest(rust_digest, [], paths[name], bounds, deadline)
            lean_revision = run_digest(
                lean_verify, ["--revision-digest-v2"], paths[name], bounds, deadline
            )
            if rust_revision != lean_revision:
                raise ConformanceError(f"{name}: Rust/Lean exact-byte digest differs")
            identity_digests[name] = rust_revision
            observations.append(
                {
                    "id": name,
                    "source_sha256": spec["generated_sha256"][name],
                    "revision_digest_v2": rust_revision,
                    "parse": "accepted",
                }
            )
        if (
            identity_asts["identity_lf"] != identity_asts["identity_crlf_unicode"]
            or identity_digests["identity_lf"]
            == identity_digests["identity_crlf_unicode"]
        ):
            raise ConformanceError(
                "CRLF/Unicode/comment bytes changed source identity or AST"
            )
        observations.append(
            require_verifier_result(
                "distinct_exact_byte_anchors",
                lean_verify,
                [paths["identity_lf"], paths["identity_crlf_unicode"], cert],
                code=1,
                loaded=2,
                reason="missing canonical `.axi` module context",
                bounds=bounds,
                deadline=deadline,
            )
        )
        for implementation, binary in (("rust", rust_parse), ("lean", lean_parse)):
            envelope = run_envelope(
                binary, implementation, "parse", paths["invalid_utf8"], bounds, deadline
            )
            if (
                envelope["decision"] != "rejected"
                or envelope["observed_stage"] != "boundary"
                or envelope["normalized_ast"] is not None
                or "UTF-8" not in (envelope["diagnostic"] or "")
            ):
                raise ConformanceError(
                    f"invalid_utf8: {implementation} boundary differs"
                )
        observations.append(
            {
                "id": "invalid_utf8_parsers",
                "source_sha256": spec["generated_sha256"]["invalid_utf8"],
                "rust": "rejected",
                "lean": "rejected",
            }
        )
        kwargs = {"bounds": bounds, "deadline": deadline}
        observations.append(
            require_verifier_result(
                "module_bytes_n",
                lean_verify,
                [exact, cert],
                code=1,
                loaded=1,
                reason="missing canonical `.axi` module context",
                expected_revision=digest,
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "module_bytes_n_plus_one",
                lean_verify,
                [paths["module_n_plus_one"], cert],
                code=1,
                loaded=0,
                reason="exceeds 4194304 bytes",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "invalid_utf8",
                lean_verify,
                [paths["invalid_utf8"], cert],
                code=1,
                loaded=0,
                reason="is not UTF-8",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "anchor_modules_n",
                lean_verify,
                [*anchor_paths[:16], cert],
                code=1,
                loaded=16,
                reason="missing canonical `.axi` module context",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "anchor_modules_n_plus_one",
                lean_verify,
                [*anchor_paths, cert],
                code=2,
                loaded=16,
                reason="too many `.axi` anchors: maximum is 16",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "duplicate_anchor",
                lean_verify,
                [anchor_paths[0], anchor_paths[0], cert],
                code=1,
                loaded=1,
                reason="duplicate axi revision digest",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "missing_anchor",
                lean_verify,
                [anchor_paths[0], cert],
                code=1,
                loaded=1,
                reason="missing canonical `.axi` module context",
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "file_inputs_n",
                lean_verify,
                [exact, *([cert] * 31)],
                code=1,
                loaded=1,
                reason="missing canonical `.axi` module context",
                reason_count=31,
                expected_revision=digest,
                **kwargs,
            )
        )
        observations.append(
            require_verifier_result(
                "file_inputs_n_plus_one",
                lean_verify,
                [exact, *([cert] * 32)],
                code=2,
                loaded=0,
                reason="too many verifier inputs: maximum is 32",
                **kwargs,
            )
        )
    return {
        "schema": "axiograph.axi_v1_trusted_file_boundary_report",
        "version": 1,
        "status": "pass",
        "corpus_sha256": sha256_bytes((root / CORPUS).read_bytes()),
        "limits": LIMITS,
        "cases": observations,
        "exact_n_revision_digest_v2": digest,
        "authority": "exact_utf8_axi_bytes",
        "accepted": False,
        "duration_excluded": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--rust-parse", required=True, type=Path)
    parser.add_argument("--lean-parse", required=True, type=Path)
    parser.add_argument("--rust-digest", required=True, type=Path)
    parser.add_argument("--lean-verify", required=True, type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = canonical_json(
        build_report(
            args.root.resolve(strict=True),
            args.rust_parse.resolve(strict=True),
            args.lean_parse.resolve(strict=True),
            args.rust_digest.resolve(strict=True),
            args.lean_verify.resolve(strict=True),
        )
    )
    if args.report is None:
        sys.stdout.buffer.write(report)
    else:
        if args.report.exists():
            raise ConformanceError("trusted boundary report already exists")
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("xb") as output:
            output.write(report)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ConformanceError, OSError, subprocess.SubprocessError) as error:
        print(f"trusted file boundary corpus failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
