from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.check_book import local_markdown_target
from scripts.check_book_output import validate_output


class BookValidationTests(unittest.TestCase):
    def test_external_markdown_url_is_not_treated_as_a_local_file(self) -> None:
        source = Path("docs/DEVELOPMENT.md")

        self.assertIsNone(
            local_markdown_target(
                source,
                "https://github.com/josephjohncox/axiograph/blob/main/README.md",
            )
        )

    def test_relative_markdown_url_resolves_from_the_source_page(self) -> None:
        source = Path("docs/DEVELOPMENT.md").resolve()

        self.assertEqual(
            local_markdown_target(source, "howto/TESTING.md"),
            (source.parent / "howto/TESTING.md").resolve(),
        )

    def write_rendered_fixture(self, output: Path, href: str) -> None:
        (output / "theme").mkdir(parents=True)
        (output / "index.html").write_text(
            f'<a href="{href}">target</a>', encoding="utf-8"
        )
        (output / "target.html").write_text(
            '<h2 id="bounded-claim">Bounded claim</h2>', encoding="utf-8"
        )
        (output / "404.html").write_text("not found", encoding="utf-8")
        (output / "searchindex-test.js").write_text("", encoding="utf-8")
        (output / "theme/axiograph-test.css").write_text("", encoding="utf-8")
        (output / "favicon-test.svg").write_text("<svg></svg>", encoding="utf-8")

    def test_rendered_local_anchor_is_checked(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            self.write_rendered_fixture(output, "target.html#bounded-claim")
            self.assertEqual(validate_output(output)[0], [])

    def test_missing_rendered_local_anchor_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            self.write_rendered_fixture(output, "target.html#unsupported-claim")
            problems, _ = validate_output(output)
            self.assertTrue(any("missing anchor" in item for item in problems))


if __name__ == "__main__":
    unittest.main()
