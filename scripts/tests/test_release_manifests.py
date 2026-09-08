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
    require_real_directory,
    validate_source_manifest_bytes,
)
from scripts.release_version import workspace_version
from scripts.run_release_fixture_suite import FixtureSuiteError, load_manifest

REPO_ROOT = Path(__file__).resolve().parents[2]


class ReleaseManifestTests(unittest.TestCase):
    def test_release_output_parent_must_be_a_real_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            require_real_directory(directory, "output parent")
            regular_file = directory / "not-a-directory"
            regular_file.write_text("file\n", encoding="utf-8")
            with self.assertRaisesRegex(
                SourceManifestError, "output parent must be a real directory"
            ):
                require_real_directory(regular_file, "output parent")

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

    def test_workspace_and_helm_application_versions_are_identical_calver(self) -> None:
        version = workspace_version(REPO_ROOT)
        chart = (REPO_ROOT / "deploy/helm/axiograph/Chart.yaml").read_text(
            encoding="utf-8"
        )
        app_versions = [
            line.removeprefix("appVersion:").strip().strip('"')
            for line in chart.splitlines()
            if line.startswith("appVersion:")
        ]

        self.assertEqual(app_versions, [version])

        values = (REPO_ROOT / "deploy/helm/axiograph/values.yaml").read_text(
            encoding="utf-8"
        )
        self.assertIn("repository: ghcr.io/josephjohncox/axiograph", values)
        self.assertIn('tag: ""', values)

        manifest = (
            REPO_ROOT / "deploy/k8s/axiograph-db-statefulset.yaml"
        ).read_text(encoding="utf-8")
        self.assertIn(
            f"image: ghcr.io/josephjohncox/axiograph:v{version}", manifest
        )

    def test_release_publication_downloads_only_native_bundle_artifacts(self) -> None:
        workflow = (REPO_ROOT / ".github/workflows/release.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "with:\n"
            "          path: dist\n"
            "          pattern: axiograph-*\n"
            "          merge-multiple: true\n"
            "      - name: Re-verify exact downloaded release assets",
            workflow,
        )

    def test_container_copies_read_only_api_contract_before_rust_build(self) -> None:
        dockerfile = (REPO_ROOT / "Dockerfile").read_text(encoding="utf-8")
        required_copy = (
            "COPY frontend/viz/src/server/read-only-api.json "
            "/app/frontend/viz/src/server/read-only-api.json"
        )
        rust_copy = "COPY rust/ /app/rust/"
        cargo_build = "cargo build -p axiograph-cli --release --locked"

        self.assertEqual(dockerfile.count(required_copy), 1)
        self.assertLess(dockerfile.index(required_copy), dockerfile.index(rust_copy))
        self.assertLess(dockerfile.index(required_copy), dockerfile.index(cargo_build))

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
