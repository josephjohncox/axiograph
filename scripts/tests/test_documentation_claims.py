from __future__ import annotations

import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_documentation_claims import validate

ROOT = Path(__file__).resolve().parents[2]


class DocumentationClaimTests(unittest.TestCase):
    def copy_documentation(self, destination: Path) -> None:
        shutil.copytree(ROOT / "docs", destination / "docs")

    def test_current_documentation_satisfies_claim_contract(self) -> None:
        self.assertEqual(validate(ROOT), [])

    def test_missing_page_label_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/AGENT_CONTEXT.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "**Claim status:** `current_implementation` and `design_target`. See\n",
                    "",
                    1,
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("missing a Claim status" in item for item in validate(root)))

    def test_unknown_status_label_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/AGENT_CONTEXT.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "`current_implementation`", "`verified_everything`", 1
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("unknown Claim status" in item for item in validate(root)))

    def test_unknown_wrapped_status_label_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "`operational_evidence`, and `historical_baseline`",
                    "`verified_everything`, and `historical_baseline`",
                    1,
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("verified_everything" in item for item in validate(root)))

    def test_unapproved_roadmap_marker_change_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "- [ ] Reconcile completed versus planned entries",
                    "- [x] Reconcile completed versus planned entries",
                    1,
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("marker state" in item for item in validate(root)))

    def test_compensating_roadmap_marker_swap_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md"
            changed = path.read_text(encoding="utf-8").replace(
                "- [ ] Reconcile completed versus planned entries",
                "- [x] Reconcile completed versus planned entries",
                1,
            ).replace(
                "- [x] Correct the stale category-IR/VerifyMain status",
                "- [ ] Correct the stale category-IR/VerifyMain status",
                1,
            )
            path.write_text(changed, encoding="utf-8")
            problems = validate(root)
            self.assertEqual(sum("marker state" in item for item in problems), 2)

    def test_compensating_ledger_and_roadmap_swap_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            roadmap_path = root / "docs/roadmaps/ROADMAP_ENGINEERING_QUALITY.md"
            changed = roadmap_path.read_text(encoding="utf-8").replace(
                "- [ ] Reconcile completed versus planned entries",
                "- [x] Reconcile completed versus planned entries",
                1,
            ).replace(
                "- [x] Correct the stale category-IR/VerifyMain status",
                "- [ ] Correct the stale category-IR/VerifyMain status",
                1,
            )
            roadmap_path.write_text(changed, encoding="utf-8")
            ledger_path = root / "docs/roadmaps/ENGINEERING_QUALITY_REQUIREMENTS_V1.json"
            ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
            replacements = {
                "EQ-18-M001": ("[ ]", "unchecked"),
                "EQ-18-M003": ("[x]", "checked"),
            }
            for requirement in ledger["requirements"]:
                replacement = replacements.get(requirement["id"])
                if replacement is None:
                    continue
                original_marker, current_state = replacement
                source_lines = requirement["sourceText"].splitlines()
                source_lines[0] = source_lines[0].replace(
                    requirement["originalMarker"], original_marker, 1
                )
                requirement["sourceText"] = "\n".join(source_lines)
                requirement["sourceSha256"] = hashlib.sha256(
                    requirement["sourceText"].encode("utf-8")
                ).hexdigest()
                requirement["originalMarker"] = original_marker
                requirement["currentState"] = current_state
            ledger_path.write_text(json.dumps(ledger), encoding="utf-8")
            problems = validate(root)
            self.assertTrue(
                any("independently pinned baseline" in item for item in problems)
            )

    def test_release_identity_change_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/RELEASE_BASELINE_V20260908.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "a2d9c80f8e5acc1a1ef6b106f9cf97bb2c0c30df",
                    "0" * 40,
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("missing release identity" in item for item in validate(root)))

    def test_plain_text_index_mention_does_not_count_as_link(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/README.md"
            changed = path.read_text(encoding="utf-8").replace(
                "[Documentation Claim Status](CLAIM_STATUS.md)",
                "Documentation Claim Status (`CLAIM_STATUS.md`)",
            ).replace(
                "[Documentation claim status](CLAIM_STATUS.md)",
                "Documentation claim status (`CLAIM_STATUS.md`)",
            )
            path.write_text(changed, encoding="utf-8")
            self.assertTrue(
                any(
                    item.startswith("docs/reference/README.md: missing required Markdown link")
                    and item.endswith("docs/reference/CLAIM_STATUS.md")
                    for item in validate(root)
                )
            )

    def test_commented_index_link_does_not_count_as_visible(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/README.md"
            changed = path.read_text(encoding="utf-8").replace(
                "[Documentation Claim Status](CLAIM_STATUS.md)",
                "<!-- [Documentation Claim Status](CLAIM_STATUS.md) -->",
                1,
            ).replace(
                "[Documentation claim status](CLAIM_STATUS.md)",
                "<!-- [Documentation claim status](CLAIM_STATUS.md) -->",
                1,
            )
            path.write_text(changed, encoding="utf-8")
            self.assertTrue(
                any(
                    item.startswith("docs/reference/README.md: missing required Markdown link")
                    and item.endswith("docs/reference/CLAIM_STATUS.md")
                    for item in validate(root)
                )
            )

    def test_fenced_index_link_does_not_count_as_visible(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/reference/README.md"
            changed = path.read_text(encoding="utf-8").replace(
                "[Documentation Claim Status](CLAIM_STATUS.md)",
                "```md\n[Documentation Claim Status](CLAIM_STATUS.md)\n```",
                1,
            ).replace(
                "[Documentation claim status](CLAIM_STATUS.md)",
                "```md\n[Documentation claim status](CLAIM_STATUS.md)\n```",
                1,
            )
            path.write_text(changed, encoding="utf-8")
            self.assertTrue(
                any(
                    item.startswith("docs/reference/README.md: missing required Markdown link")
                    and item.endswith("docs/reference/CLAIM_STATUS.md")
                    for item in validate(root)
                )
            )

    def test_link_in_other_indexes_does_not_compensate_for_missing_link(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            path = root / "docs/roadmaps/README.md"
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "[Engineering quality execution plan](ENGINEERING_QUALITY_EXECUTION_PLAN.md)",
                    "Engineering quality execution plan",
                    1,
                ),
                encoding="utf-8",
            )
            self.assertTrue(
                any(
                    item.startswith("docs/roadmaps/README.md: missing required Markdown link")
                    and item.endswith("docs/roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md")
                    for item in validate(root)
                )
            )

    def test_required_index_link_destination_must_exist(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.copy_documentation(root)
            (root / "docs/roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md").unlink()
            self.assertTrue(
                any("required durable link target does not exist" in item for item in validate(root))
            )


if __name__ == "__main__":
    unittest.main()
