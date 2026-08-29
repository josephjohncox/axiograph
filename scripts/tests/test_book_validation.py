from __future__ import annotations

import unittest
from pathlib import Path

from scripts.check_book import local_markdown_target


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


if __name__ == "__main__":
    unittest.main()
