from __future__ import annotations

import tempfile
import unittest
from datetime import date
from pathlib import Path

from scripts.release_version import (
    CalVerError,
    parse_calver,
    validate_release_tag,
    workspace_version,
)


class ReleaseVersionTests(unittest.TestCase):
    def test_accepts_cargo_compatible_calendar_version_and_tag(self) -> None:
        parsed = parse_calver("20260829.0.0")

        self.assertEqual(parsed.release_date, date(2026, 8, 29))
        self.assertEqual(parsed.sequence, 0)
        self.assertEqual(str(parsed), "20260829.0.0")
        self.assertEqual(
            validate_release_tag("v20260829.0.0", "20260829.0.0"),
            "20260829.0.0",
        )

    def test_accepts_monotonic_same_day_sequence(self) -> None:
        parsed = parse_calver("20260829.0.17")

        self.assertEqual(parsed.release_date, date(2026, 8, 29))
        self.assertEqual(parsed.sequence, 17)
        self.assertEqual(parse_calver("20260829.0.999999").sequence, 999_999)

    def test_rejects_noncanonical_or_invalid_versions(self) -> None:
        invalid = [
            "0.6.0",
            "20260229.0.0",
            "20261301.0.0",
            "20260829.1.0",
            "20260829.0.01",
            "20260829.0.1000000",
            "20260829.0",
            "20260829.0.0-alpha",
            "20260829.0.0+build",
            "v20260829.0.0",
            " 20260829.0.0",
        ]

        for version in invalid:
            with self.subTest(version=version):
                with self.assertRaises(CalVerError):
                    parse_calver(version)

    def test_workspace_version_reads_and_validates_the_authoritative_manifest(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            rust = root / "rust"
            rust.mkdir()
            (rust / "Cargo.toml").write_text(
                '[workspace]\n[workspace.package]\nversion = "20260829.0.2"\n',
                encoding="utf-8",
            )

            self.assertEqual(workspace_version(root), "20260829.0.2")

    def test_release_tag_must_equal_the_workspace_version(self) -> None:
        with self.assertRaisesRegex(CalVerError, "does not match"):
            validate_release_tag("v20260829.0.1", "20260829.0.0")


if __name__ == "__main__":
    unittest.main()
