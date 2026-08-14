from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from scripts.generate_release_source_manifest import (
    SourceManifestError,
    build_source_manifest,
    canonical_bytes,
    dirty_status_entries,
    ignored_release_source_candidates,
    validate_source_manifest_bytes,
)
from scripts.run_release_fixture_suite import FixtureSuiteError, load_manifest

REPO_ROOT = Path(__file__).resolve().parents[2]


class ReleaseManifestTests(unittest.TestCase):
    def test_source_manifest_is_sorted_canonical_and_deterministic(self) -> None:
        entries = [
            ("z.txt", "100644", "1" * 40, b"z\n"),
            ("bin/tool", "100755", "2" * 40, b"tool\n"),
        ]
        first = build_source_manifest("3" * 40, "4" * 40, entries)
        second = build_source_manifest("3" * 40, "4" * 40, list(reversed(entries)))
        first_bytes = canonical_bytes(first)
        second_bytes = canonical_bytes(second)
        self.assertEqual(first_bytes, second_bytes)
        validated = validate_source_manifest_bytes(first_bytes)
        self.assertEqual(
            [item["path"] for item in validated["files"]],
            ["bin/tool", "z.txt"],
        )

    def test_source_manifest_rejects_noncanonical_duplicate_and_unsafe_paths(
        self,
    ) -> None:
        manifest = build_source_manifest(
            "3" * 40,
            "4" * 40,
            [("a.txt", "100644", "1" * 40, b"a")],
        )
        noncanonical = json.dumps(manifest, indent=2).encode("utf-8")
        with self.assertRaisesRegex(SourceManifestError, "not canonical JSON"):
            validate_source_manifest_bytes(noncanonical)
        with self.assertRaisesRegex(SourceManifestError, "duplicate source path"):
            build_source_manifest(
                "3" * 40,
                "4" * 40,
                [
                    ("a.txt", "100644", "1" * 40, b"a"),
                    ("a.txt", "100644", "2" * 40, b"b"),
                ],
            )
        with self.assertRaisesRegex(SourceManifestError, "not canonical"):
            build_source_manifest(
                "3" * 40,
                "4" * 40,
                [("../escape", "100644", "1" * 40, b"x")],
            )
        with self.assertRaisesRegex(SourceManifestError, "unsupported Git mode"):
            build_source_manifest(
                "3" * 40,
                "4" * 40,
                [("link", "120000", "1" * 40, b"target")],
            )

    def test_dirty_status_parser_preserves_every_entry(self) -> None:
        self.assertEqual(
            dirty_status_entries(b" M tracked\0?? untracked\0R  old -> new\0"),
            [" M tracked", "?? untracked", "R  old -> new"],
        )
        with self.assertRaisesRegex(SourceManifestError, "non-UTF-8"):
            dirty_status_entries(b"?? \xff\0")

    def test_ignored_release_source_scan_finds_hidden_sources_not_build_outputs(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            hidden = root / "rust/crates/demo/src/bin/required.rs"
            hidden.parent.mkdir(parents=True)
            hidden.write_text("fn main() {}\n")
            generated = root / "rust/crates/demo/target/generated.rs"
            generated.parent.mkdir(parents=True)
            generated.write_text("generated\n")

            self.assertEqual(
                ignored_release_source_candidates(
                    root,
                    [
                        "!! rust/crates/demo/src/bin/",
                        "!! rust/crates/demo/target/",
                    ],
                ),
                ["rust/crates/demo/src/bin/required.rs"],
            )

    def test_checked_in_fixture_manifest_is_complete_and_hash_pinned(self) -> None:
        manifest_path = REPO_ROOT / "release" / "fixtures.json"
        manifest = load_manifest(manifest_path, REPO_ROOT)
        self.assertEqual(
            [case["expect"] for case in manifest["cases"]].count("accept"),
            1,
        )
        self.assertGreaterEqual(
            [case["expect"] for case in manifest["cases"]].count("reject"),
            7,
        )

        with tempfile.TemporaryDirectory() as temporary:
            try:
                altered = json.loads(manifest_path.read_text())
            except json.JSONDecodeError as error:
                self.fail(f"checked-in fixture manifest was invalid JSON: {error}")
            altered["cases"][0]["sha256"] = "0" * 64
            candidate = Path(temporary) / "fixtures.json"
            candidate.write_text(json.dumps(altered))
            with self.assertRaisesRegex(FixtureSuiteError, "sha256 changed"):
                load_manifest(candidate, REPO_ROOT)


if __name__ == "__main__":
    unittest.main()
