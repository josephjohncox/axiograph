from __future__ import annotations

import copy
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.check_axi_v1_contract import (
    CONTRACT_PATH,
    CROSS_LANGUAGE_REJECTION_STAGES,
    LEAN_AST_PATH,
    RUST_AST_PATH,
    check_inventory,
    diagnostic_taxonomy_contract,
    differential_envelope_contract,
    require_cross_language_rejection,
    validate_boundary_coverage,
)

REPO = Path(__file__).parents[2]


class AxiV1PayloadInventoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = json.loads((REPO / CONTRACT_PATH).read_text(encoding="utf-8"))

    def copied_root(self) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        for relative in (RUST_AST_PATH, LEAN_AST_PATH):
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / relative, destination)
        return temporary, root

    def assert_drift_rejected(self, path: Path, old: str, new: str) -> None:
        temporary, root = self.copied_root()
        self.addCleanup(temporary.cleanup)
        target = root / path
        source = target.read_text(encoding="utf-8")
        self.assertEqual(source.count(old), 1, f"test mutation must be unique: {old!r}")
        target.write_text(source.replace(old, new), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "payload type drift"):
            check_inventory(self.contract, root)

    def test_rust_structure_bool_drift_is_rejected(self) -> None:
        self.assert_drift_rejected(
            RUST_AST_PATH,
            "    pub reversible: bool,",
            "    pub reversible: String,",
        )

    def test_lean_structure_text_drift_is_rejected(self) -> None:
        self.assert_drift_rejected(
            LEAN_AST_PATH,
            "  lhs : String\n  rhs : String",
            "  lhs : Bool\n  rhs : String",
        )

    def test_rust_constructor_name_drift_is_rejected(self) -> None:
        self.assert_drift_rejected(
            RUST_AST_PATH,
            "        rel: Name,\n        to: Name,",
            "        rel: String,\n        to: Name,",
        )

    def matrix(self) -> dict[str, object]:
        reference = self.contract["adversarial_matrix"]
        return json.loads((REPO / reference["path"]).read_text(encoding="utf-8"))

    def test_cartesian_boundary_coverage_is_complete(self) -> None:
        matrix = self.matrix()
        self.assertEqual(len(validate_boundary_coverage(matrix)), 1107)

    def test_operational_taxonomy_is_closed(self) -> None:
        self.assertEqual(
            self.contract["rejection_classes"],
            list(CROSS_LANGUAGE_REJECTION_STAGES),
        )
        self.assertEqual(
            self.contract["diagnostic_taxonomy"],
            diagnostic_taxonomy_contract(),
        )

    def test_differential_envelope_is_closed(self) -> None:
        self.assertEqual(
            self.contract["differential_envelope"],
            differential_envelope_contract(),
        )

    def test_omitted_cartesian_cell_is_rejected(self) -> None:
        matrix = self.matrix()
        del matrix["boundary_cases"][0]
        with self.assertRaisesRegex(ValueError, "complete generated Cartesian coverage"):
            validate_boundary_coverage(matrix)

    def test_omitted_object_declaration_axis_is_rejected(self) -> None:
        matrix = self.matrix()
        matrix["boundary_cases"] = [
            case
            for case in matrix["boundary_cases"]
            if case.get("coverage", {}).get("axis") != "object_declaration_separator"
        ]
        with self.assertRaisesRegex(ValueError, "complete generated Cartesian coverage"):
            validate_boundary_coverage(matrix)

    def test_omitted_object_declaration_requirement_is_rejected(self) -> None:
        matrix = self.matrix()
        del matrix["coverage_requirements"]["object_declaration_separator"]
        with self.assertRaisesRegex(ValueError, "coverage requirements drift"):
            validate_boundary_coverage(matrix)

    def test_relabelled_cartesian_cell_is_rejected(self) -> None:
        matrix = copy.deepcopy(self.matrix())
        matrix["boundary_cases"][0]["coverage"]["separator"] = "cr"
        with self.assertRaisesRegex(ValueError, "complete generated Cartesian coverage"):
            validate_boundary_coverage(matrix)

    def test_duplicate_cartesian_cell_is_rejected(self) -> None:
        matrix = self.matrix()
        matrix["cases"].append(copy.deepcopy(matrix["boundary_cases"][0]))
        with self.assertRaisesRegex(ValueError, "must not duplicate"):
            validate_boundary_coverage(matrix)

    @staticmethod
    def failed(stderr: str) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess([], 1, "", stderr)

    def header_order_observations(
        self,
    ) -> tuple[str, subprocess.CompletedProcess[str], subprocess.CompletedProcess[str]]:
        source = "schema S\n  object A\n"
        rust = self.failed(
            "parse error on line 1: the module header must be the first canonical header\n"
        )
        lean = self.failed(
            "parse error on line 1: the module header must be the first canonical header\n"
        )
        return source, rust, lean

    def test_mutated_class_label_is_rejected(self) -> None:
        source, rust, lean = self.header_order_observations()
        with self.assertRaisesRegex(ValueError, "expected class"):
            require_cross_language_rejection(
                "mutated label",
                "parse.section_syntax",
                "parse",
                source,
                rust,
                lean,
            )

    def test_swapped_parse_typecheck_stage_is_rejected(self) -> None:
        source, rust, lean = self.header_order_observations()
        with self.assertRaisesRegex(ValueError, "belongs to stage"):
            require_cross_language_rejection(
                "swapped stage",
                "typecheck.schema_formation",
                "parse",
                source,
                rust,
                lean,
            )

    def test_changed_diagnostic_is_rejected(self) -> None:
        source, rust, _lean = self.header_order_observations()
        changed = self.failed("type error: schema `S` has a subtype cycle\n")
        with self.assertRaisesRegex(ValueError, "parse-stage diagnostic"):
            require_cross_language_rejection(
                "changed diagnostic",
                "parse.header_order",
                "parse",
                source,
                rust,
                changed,
            )

    def test_object_separator_diagnostic_divergence_is_rejected(self) -> None:
        source = "module M\nschema S\n  object  A\n"
        rust = self.failed("parse error on line 3: object name must be an ASCII identifier\n")
        lean = self.failed("parse error on line 3: object name expects exactly one ASCII identifier\n")
        self.assertEqual(
            require_cross_language_rejection(
                "repeated object separator",
                "parse.section_syntax",
                "parse",
                source,
                rust,
                lean,
            ),
            "parse.section_syntax",
        )
        unrelated = self.failed(
            "parse error on line 3: schema header expects canonical content and at most one trailing `:`\n"
        )
        with self.assertRaisesRegex(ValueError, "object-declaration separator diagnostic"):
            require_cross_language_rejection(
                "divergent object separator reason",
                "parse.section_syntax",
                "parse",
                source,
                rust,
                unrelated,
            )

    def test_typecheck_observation_cannot_hide_a_parse_rejection(self) -> None:
        source = "module M\nschema S\n  object A\ntheory T on S\n  constraint key Missing(left)\n"
        rust = self.failed("type error: unknown relation `Missing` in schema `S`\n")
        lean_parse = self.failed(
            "parse error: parse error on line 5: key expects: `key Relation(field, ...)`\n"
        )
        with self.assertRaisesRegex(ValueError, "typecheck-stage diagnostic"):
            require_cross_language_rejection(
                "typecheck entry-point parse rejection",
                "typecheck.theory_formation",
                "typecheck",
                source,
                rust,
                lean_parse,
            )

    def test_languages_failing_for_different_reasons_are_rejected(self) -> None:
        source, rust, _lean = self.header_order_observations()
        different = self.failed(
            "parse error on line 1: schema header expects canonical content and at most one trailing `:`\n"
        )
        with self.assertRaisesRegex(ValueError, "different classes"):
            require_cross_language_rejection(
                "different reasons",
                "parse.header_order",
                "parse",
                source,
                rust,
                different,
            )

    def source_prioritized_observations(
        self,
    ) -> list[
        tuple[
            str,
            str,
            subprocess.CompletedProcess[str],
            subprocess.CompletedProcess[str],
            subprocess.CompletedProcess[str],
        ]
    ]:
        return [
            (
                "parse.lexical_domain",
                "module M\nschema S\n  object A\n  relation R(left:A,right:A)\ntheory T on S\n  constraint functional\vR.left -> R.right\n",
                self.failed("parse error on line 6: functional expects canonical relation fields\n"),
                self.failed("parse error on line 6: functional expects canonical relation fields\n"),
                self.failed("parse error on line 6: key expects: `key Relation(field, ...)`\n"),
            ),
            (
                "parse.noncanonical_alias",
                "module M\nschema S\n  object Child\n  object Parent\n  subtype Child <: Parent\n",
                self.failed("parse error on line 5: subtype expects canonical `<` syntax\n"),
                self.failed("parse error on line 5: condition not satisfied\n"),
                self.failed("parse error on line 5: unknown rewrite orientation `both`\n"),
            ),
            (
                "parse.numeric_domain",
                "module M\nschema S\n  object A\n  relation R(left:A,right:A)\ntheory T on S\n  constraint at_most +1 R.left -> R.right\n",
                self.failed("parse error on line 6: at_most bound must be within u32\n"),
                self.failed("parse error on line 6: at_most expects a non-negative integer bound\n"),
                self.failed("parse error on line 6: relation expects exactly canonical role syntax\n"),
            ),
            (
                "parse.list_syntax",
                "module M\nschema S\n  object A\n  relation R(a:A,b:A)\ntheory T on S\n  constraint key R(a,)\n",
                self.failed("parse error on line 6: key field list must not contain empty elements\n"),
                self.failed("parse error on line 6: key expects: `key Relation(field, ...)`\n"),
                self.failed("parse error on line 6: relation expects exactly canonical role syntax\n"),
            ),
        ]

    def test_source_prioritized_unrelated_diagnostics_are_rejected(self) -> None:
        unrelated = self.failed(
            "parse error on line 1: schema header expects canonical content and at most one trailing `:`\n"
        )
        for rejection_class, source, rust, lean, _divergent in self.source_prioritized_observations():
            with self.subTest(rejection_class=rejection_class):
                self.assertEqual(
                    require_cross_language_rejection(
                        "valid source-prioritized observation",
                        rejection_class,
                        "parse",
                        source,
                        rust,
                        lean,
                    ),
                    rejection_class,
                )
                with self.assertRaisesRegex(ValueError, "diagnostic does not match source reason"):
                    require_cross_language_rejection(
                        "unrelated same-stage diagnostic",
                        rejection_class,
                        "parse",
                        source,
                        rust,
                        unrelated,
                    )

    def test_source_prioritized_language_reason_divergence_is_rejected(self) -> None:
        for rejection_class, source, rust, _lean, divergent in self.source_prioritized_observations():
            with self.subTest(rejection_class=rejection_class):
                with self.assertRaisesRegex(ValueError, "diagnostic does not match source reason"):
                    require_cross_language_rejection(
                        "cross-language reason divergence",
                        rejection_class,
                        "parse",
                        source,
                        rust,
                        divergent,
                    )


if __name__ == "__main__":
    unittest.main()
