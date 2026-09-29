"""Negative controls for the exact-byte trusted file-mode boundary corpus."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.check_axi_contract_conformance import ConformanceError
from scripts.check_axi_trusted_file_boundaries import (
    CERTIFICATE,
    CORPUS,
    LIMITS,
    generate_inputs,
    load_spec,
    require_verifier_result,
)

ROOT = Path(__file__).resolve().parents[2]


class TrustedFileBoundaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in (CORPUS, Path(CERTIFICATE)):
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)

    def test_generated_boundaries_bind_exact_bytes_and_no_valid_utf8_replacement(
        self,
    ) -> None:
        spec = load_spec(self.root)
        sources, anchors = generate_inputs(spec)
        self.assertEqual(len(sources["module_n"]), LIMITS["module_bytes"])
        self.assertEqual(len(sources["module_n_plus_one"]), LIMITS["module_bytes"] + 1)
        self.assertEqual(len(anchors), LIMITS["anchor_modules"] + 1)
        with self.assertRaises(UnicodeDecodeError):
            sources["invalid_utf8"].decode("utf-8")
        self.assertNotEqual(sources["identity_lf"], sources["identity_crlf_unicode"])
        self.assertIn(b"\r\n", sources["identity_crlf_unicode"])
        self.assertIn("😀".encode(), sources["identity_lf"])
        self.assertEqual(generate_inputs(spec), (sources, anchors))

    def test_mutated_digest_and_widened_limit_reject(self) -> None:
        corpus = self.root / CORPUS
        original = json.loads(corpus.read_text(encoding="utf-8"))
        mutated = json.loads(corpus.read_text(encoding="utf-8"))
        mutated["generated_sha256"]["module_n"] = "0" * 64
        corpus.write_text(json.dumps(mutated), encoding="utf-8")
        with self.assertRaises(ConformanceError):
            generate_inputs(load_spec(self.root))
        original["limits"]["anchor_modules"] += 1
        corpus.write_text(json.dumps(original), encoding="utf-8")
        with self.assertRaises(ConformanceError):
            load_spec(self.root)

    def test_modified_certificate_rejects_before_execution(self) -> None:
        cert = self.root / CERTIFICATE
        cert.write_bytes(cert.read_bytes() + b" ")
        with self.assertRaises(ConformanceError):
            load_spec(self.root)

    def test_unrelated_verifier_failure_cannot_satisfy_expected_rejection(self) -> None:
        unrelated = subprocess.CompletedProcess(
            args=["axiograph_verify"],
            returncode=1,
            stdout=b"ok: loaded axi module revision=fake\n",
            stderr=b"certificate JSON failed to parse",
        )
        with (
            mock.patch(
                "scripts.check_axi_trusted_file_boundaries.bounded_command",
                return_value=unrelated,
            ),
            self.assertRaises(ConformanceError),
        ):
            require_verifier_result(
                "missing_anchor",
                Path("axiograph_verify"),
                [Path("a.axi")],
                code=1,
                loaded=1,
                reason="missing canonical `.axi` module context",
                bounds={"per_process_timeout_seconds": 1},
                deadline=1.0,
            )
