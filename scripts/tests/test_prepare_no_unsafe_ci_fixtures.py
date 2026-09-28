"""CI cache bootstrap must use the exact pinned external distribution bytes."""

from __future__ import annotations

import hashlib
import os
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_no_unsafe import CANDIDATE_HOMES, MANIFEST_REL
from scripts.generate_no_unsafe_external_cache_manifest import ARCHIVE_REL
from scripts.no_unsafe_fs import PolicyFailure
from scripts.prepare_no_unsafe_ci_fixtures import PINNED_EVIDENCE, FixtureError, prepare

ROOT = Path(__file__).resolve().parents[2]


class PrepareNoUnsafeCiFixturesTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        manifest = self.root / MANIFEST_REL
        manifest.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / MANIFEST_REL, manifest)
        for source, _, _ in PINNED_EVIDENCE:
            pinned = self.root / source
            pinned.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / source, pinned)
        self.archive = self.root / ARCHIVE_REL
        self.archive.parent.mkdir(parents=True)

    def test_exact_archive_creates_only_the_two_checked_cache_homes(self) -> None:
        source = ROOT / ARCHIVE_REL
        try:
            os.link(source, self.archive)
        except OSError:
            shutil.copyfile(source, self.archive)
        self.assertEqual(prepare(self.root), {"homes": 2, "files_per_home": 32, "pins": 2})
        for home in CANDIDATE_HOMES:
            library = self.root / home / "library"
            files = list(library.rglob("*"))
            self.assertEqual(sum(item.is_file() for item in files), 32)
            self.assertEqual(sum(item.is_dir() for item in files), 10)
        self.assertEqual(
            hashlib.sha256(
                (self.root / CANDIDATE_HOMES[0] / "library/kani/src/lib.rs").read_bytes()
            ).digest(),
            hashlib.sha256(
                (self.root / CANDIDATE_HOMES[1] / "library/kani/src/lib.rs").read_bytes()
            ).digest(),
        )
        for source, destination, expected in PINNED_EVIDENCE:
            copied = (self.root / destination).read_bytes()
            self.assertEqual(copied, (self.root / source).read_bytes())
            self.assertEqual(hashlib.sha256(copied).hexdigest(), expected)
        self.assertTrue((self.root / "build/engineering-quality/roadmap-wave-01-full-loop").is_dir())
        with self.assertRaises(FixtureError):
            prepare(self.root)

    def test_archive_symlink_fails_before_any_cache_is_created(self) -> None:
        self.archive.symlink_to(ROOT / ARCHIVE_REL)
        with self.assertRaises(OSError):
            prepare(self.root)
        self.assertTrue(all(not (self.root / home).exists() for home in CANDIDATE_HOMES))

    def test_mutated_historical_pin_fails_before_any_cache_is_created(self) -> None:
        source, _, _ = PINNED_EVIDENCE[0]
        pinned = self.root / source
        pinned.write_bytes(pinned.read_bytes() + b"x")
        with self.assertRaises(FixtureError):
            prepare(self.root)
        self.assertTrue(all(not (self.root / home).exists() for home in CANDIDATE_HOMES))

    def test_manifest_mutation_fails_before_any_cache_is_created(self) -> None:
        manifest = self.root / MANIFEST_REL
        original = manifest.read_bytes()
        altered = original.replace(b"model-checking/kani", b"another-source/kani", 1)
        self.assertNotEqual(original, altered)
        manifest.write_bytes(altered)
        with self.assertRaises(PolicyFailure):
            prepare(self.root)
        self.assertTrue(all(not (self.root / home).exists() for home in CANDIDATE_HOMES))
