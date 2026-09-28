"""Canonical fixture bytes must survive Git storage and fresh checkout."""

from __future__ import annotations

import hashlib
import json
import subprocess
import unittest
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "fixtures/canonical/contract/axi_v1_contract.json"


class CanonicalFixtureGitBytesTests(unittest.TestCase):
    def test_indexed_fixture_bytes_match_exact_byte_contract(self) -> None:
        fixtures = json.loads(CONTRACT.read_text(encoding="utf-8"))["fixtures"]
        self.assertGreater(len(fixtures), 0)
        for fixture in fixtures:
            path = fixture["path"]
            parts = PurePosixPath(path).parts
            self.assertTrue(path.startswith("fixtures/canonical/"))
            self.assertNotIn("..", parts)
            with self.subTest(path=path):
                working = (ROOT / path).read_bytes()
                self.assertEqual(hashlib.sha256(working).hexdigest(), fixture["sha256"])
                committed = subprocess.run(
                    ["git", "show", f"HEAD:{path}"],
                    cwd=ROOT,
                    check=True,
                    capture_output=True,
                    timeout=10,
                ).stdout
                self.assertEqual(committed, working)
