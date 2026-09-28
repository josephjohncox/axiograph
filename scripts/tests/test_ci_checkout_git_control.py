"""Fail-closed tests for the CI-only checkout Git-control normalizer."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/ci.yml"
STEP = "      - name: Normalize checkout's disabled sparse settings"
NEXT_STEP = "      # actions/setup-node pinned to an immutable commit."


def normalizer_script() -> str:
    lines = WORKFLOW.read_text(encoding="utf-8").splitlines()
    start = lines.index(STEP)
    end = lines.index(NEXT_STEP, start + 1)
    if lines[start + 1] != "        run: |":
        raise AssertionError("checkout normalizer is not a shell block")
    block = lines[start + 2 : end]
    if not block or any(not line.startswith("          ") for line in block):
        raise AssertionError("checkout normalizer indentation changed")
    return "\n".join(line[10:] for line in block) + "\n"


class CheckoutGitControlTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.env = {
            **{key: value for key, value in os.environ.items() if not key.startswith("GIT_")},
            "HOME": str(self.root),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
        }
        subprocess.run(["git", "init", "-q"], cwd=self.root, env=self.env, check=True)
        self.control = self.root / ".git/config.worktree"
        self.script = normalizer_script()
        subprocess.run(["bash", "-n"], input=self.script, text=True, check=True)

    def run_normalizer(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", "-e"],
            input=self.script,
            text=True,
            cwd=self.root,
            env=self.env,
            capture_output=True,
            timeout=10,
            check=False,
        )

    def write_control(self, *, sparse: str = "false", extra: str = "") -> None:
        self.control.write_text(
            "[core]\n  sparseCheckout = " + sparse +
            "\n  sparseCheckoutCone = false\n[index]\n  sparse = false\n" + extra,
            encoding="utf-8",
        )

    def test_absent_control_is_accepted(self) -> None:
        self.assertEqual(self.run_normalizer().returncode, 0)

    def test_only_three_disabled_keys_are_removed(self) -> None:
        self.write_control()
        self.assertEqual(self.run_normalizer().returncode, 0)
        self.assertFalse(self.control.exists())

    def test_active_or_extra_control_is_rejected_without_removal(self) -> None:
        for options in ({"sparse": "true"}, {"extra": "[core]\n  worktree = outside\n"}):
            with self.subTest(options=options):
                self.write_control(**options)
                self.assertNotEqual(self.run_normalizer().returncode, 0)
                self.assertTrue(self.control.exists())

    def test_symlink_is_rejected_without_removal(self) -> None:
        self.control.symlink_to(self.root / ".git/config")
        self.assertNotEqual(self.run_normalizer().returncode, 0)
        self.assertTrue(self.control.is_symlink())

    def test_effective_sparse_setting_cannot_be_enabled_after_cleanup(self) -> None:
        subprocess.run(
            ["git", "config", "--local", "core.sparseCheckout", "true"],
            cwd=self.root, env=self.env, check=True,
        )
        self.write_control()
        self.assertNotEqual(self.run_normalizer().returncode, 0)
