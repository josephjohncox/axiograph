#!/usr/bin/env python3
"""Require fail-closed rejection of malformed indexed-path certificates."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "lean/.lake/build/bin/axiograph_verify"
CERTIFICATES = {
    "normalize": ROOT / "build/normalize_path_from_rust_v2.json",
    "equivalence": ROOT / "build/path_equiv_from_rust_v2.json",
    "congruence": ROOT / "build/path_equiv_congr_from_rust_v2.json",
}


def run_checker(certificate: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(CHECKER), str(certificate)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def require_accept(name: str, certificate: Path) -> dict:
    payload = json.loads(certificate.read_text(encoding="utf-8"))
    checked = run_checker(certificate)
    if checked.returncode != 0:
        raise RuntimeError(
            f"{name} Rust certificate was rejected:\n{checked.stdout}{checked.stderr}"
        )
    return payload


def require_reject(name: str, payload: dict, directory: Path) -> None:
    certificate = directory / f"{name}.json"
    certificate.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    checked = run_checker(certificate)
    if checked.returncode == 0:
        raise RuntimeError(f"malformed indexed-path certificate was accepted: {name}")


def main() -> int:
    if not CHECKER.is_file():
        raise RuntimeError(f"missing trusted checker: {CHECKER}")
    for name, path in CERTIFICATES.items():
        if not path.is_file():
            raise RuntimeError(f"missing Rust-emitted {name} certificate: {path}")

    normalize = require_accept("normalize", CERTIFICATES["normalize"])
    equivalence = require_accept("equivalence", CERTIFICATES["equivalence"])
    congruence = require_accept("congruence", CERTIFICATES["congruence"])

    with tempfile.TemporaryDirectory(prefix="axiograph-indexed-path-") as temp:
        directory = Path(temp)

        missing_trace = copy.deepcopy(normalize)
        del missing_trace["proof"]["derivation"]
        require_reject("normalize-missing-trace", missing_trace, directory)

        bad_trace = copy.deepcopy(normalize)
        bad_trace["proof"]["derivation"][0] = {"pos": [], "rule": "id_left"}
        require_reject("normalize-bad-trace", bad_trace, directory)

        ill_typed = copy.deepcopy(normalize)
        ill_typed["proof"]["input"] = {
            "type": "trans",
            "left": {"type": "reflexive", "entity": 1},
            "right": {"type": "step", "from": 2, "rel_type": 99, "to": 3},
        }
        ill_typed["proof"]["normalized"] = {
            "type": "step",
            "from": 2,
            "rel_type": 99,
            "to": 3,
        }
        ill_typed["proof"]["derivation"] = []
        require_reject("normalize-noncomposable", ill_typed, directory)

        confidence_injected = copy.deepcopy(normalize)
        confidence_injected["proof"]["confidence_fp"] = 900_000
        require_reject("normalize-confidence-injected", confidence_injected, directory)

        missing_left_trace = copy.deepcopy(equivalence)
        del missing_left_trace["proof"]["left_derivation"]
        require_reject("equivalence-missing-left-trace", missing_left_trace, directory)

        wrong_endpoint = copy.deepcopy(equivalence)
        wrong_endpoint["proof"]["right"] = {
            "type": "step",
            "from": 8,
            "rel_type": 10,
            "to": 9,
        }
        wrong_endpoint["proof"]["right_derivation"] = []
        require_reject("equivalence-endpoint-mismatch", wrong_endpoint, directory)

        bad_congruence_trace = copy.deepcopy(congruence)
        bad_congruence_trace["proof"]["left_derivation"][0] = {
            "pos": [99],
            "rule": "assoc_right",
        }
        require_reject("congruence-invalid-position", bad_congruence_trace, directory)

        outer_confidence = copy.deepcopy(equivalence)
        outer_confidence["confidence_fp"] = 900_000
        require_reject("equivalence-outer-confidence", outer_confidence, directory)

        obsolete_rewrite = {
            "version": 2,
            "kind": "rewrite_derivation_v2",
            "proof": {
                "input": {"type": "reflexive", "entity": 7},
                "output": {"type": "reflexive", "entity": 7},
                "derivation": [],
            },
        }
        require_reject("obsolete-rewrite-derivation-v2", obsolete_rewrite, directory)

    print(
        "indexed-path parity: accepted 3 Rust traces; rejected missing/tampered traces, "
        "non-composable endpoints, confidence injection, and the obsolete V2 rewrite wire kind"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # fail closed with one actionable message
        print(f"indexed-path rejection suite failed: {error}", file=sys.stderr)
        raise SystemExit(1) from None
