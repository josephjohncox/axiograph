from __future__ import annotations

import copy
import json
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from scripts.check_axi_contract_conformance import (
    CONTRACT_PATH,
    CORPUS_PATH,
    Binaries,
    ConformanceError,
    DifferentialFailure,
    DifferentialFailureSignature,
    bounded_command,
    build_report,
    canonical_json,
    generated_cases,
    load_corpus,
    parse_envelope,
    require_hand_golden,
    require_ordered_sequence_golden,
    shrink_failure_source,
)

REPO = Path(__file__).parents[2]


class AxiContractConformanceTests(unittest.TestCase):
    def corpus(self) -> dict[str, object]:
        return json.loads((REPO / CORPUS_PATH).read_text(encoding="utf-8"))

    def copied_contract(self) -> tuple[tempfile.TemporaryDirectory[str], Path, dict[str, object]]:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        corpus = self.corpus()
        for source in [
            CONTRACT_PATH,
            *[Path(case["path"]) for case in corpus["hand_cases"]],
        ]:
            target = root / source
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / source, target)
        return temporary, root, corpus

    @staticmethod
    def envelope_bytes(**updates: object) -> bytes:
        value: dict[str, object] = {
            "schema": "axiograph.axi_v1_differential_envelope",
            "version": 1,
            "implementation": "rust",
            "requested_stage": "parse",
            "observed_stage": "parse",
            "decision": "accepted",
            "normalized_ast": {
                "module_name": "M",
                "imports": [],
                "schemas": [],
                "theories": [],
                "instances": [],
            },
            "summary": None,
            "rejection_class": None,
            "diagnostic": None,
        }
        value.update(updates)
        return canonical_json(value)

    def test_generator_is_nonempty_deterministic_and_digest_bound(self) -> None:
        generator = self.corpus()["generator"]
        first = generated_cases(generator)
        second = generated_cases(copy.deepcopy(generator))
        self.assertEqual(first, second)
        self.assertEqual(len(first), 24)
        self.assertEqual([case["id"] for case in first], sorted(case["id"] for case in first))
        self.assertTrue(all(case["generator_trace"] for case in first))
        kinds = [case["generator_kind"] for case in first]
        expected_kinds = {
            "accepted_order",
            "accepted_numeric",
            "parse_alias",
            "parse_constraint",
            "type_unknown_target",
            "type_dependent",
            "formation_key",
            "unsupported_constraint",
        }
        self.assertEqual(set(kinds[:8]), expected_kinds)
        self.assertNotEqual(kinds[:8], kinds[8:16])

    def test_generated_failure_shrinks_and_replays_with_strict_bound(self) -> None:
        source = "module Noise\n# remove me\nTRIGGER\n# remove me too\n"
        result = shrink_failure_source(
            source,
            lambda candidate: "TRIGGER" in candidate,
            max_attempts=32,
        )
        self.assertTrue(result["replayed"])
        self.assertLess(result["bytes_after"], result["bytes_before"])
        self.assertLessEqual(result["attempts"], 33)
        self.assertEqual(result["source"], "TRIGGER")

    def test_shrinker_rejects_nonreproducing_seed(self) -> None:
        with self.assertRaisesRegex(ConformanceError, "does not reproduce"):
            shrink_failure_source("module M\n", lambda _source: False)

    def test_build_report_shrinks_and_replays_generated_differential_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(b"binary")
            bins = Binaries(*([binary] * 7))
            corpus = {
                "bounds": {"total_timeout_seconds": 60},
                "generator": {"case_count": 1},
            }
            case = {
                "id": "generated-000",
                "origin": "generated",
                "source": "noise\nTRIGGER\nnoise\n",
            }

            signature = DifferentialFailureSignature(
                mismatch_kind="decision_mismatch",
                stage="parse",
                expected_decision="accepted",
                rust_decision="accepted",
                lean_decision="rejected",
            )

            def failing_evaluation(
                candidate: dict[str, object],
                _path: Path,
                _bins: Binaries,
                _bounds: dict[str, object],
                _deadline: float,
            ) -> dict[str, object]:
                if "TRIGGER" in candidate["source"]:
                    raise DifferentialFailure(
                        "generated-000: synthetic divergence", signature
                    )
                return {}

            with (
                mock.patch(
                    "scripts.check_axi_contract_conformance.load_corpus",
                    return_value=(corpus, [case], {"version": 1}),
                ),
                mock.patch(
                    "scripts.check_axi_contract_conformance.evaluate_case",
                    side_effect=failing_evaluation,
                ),
            ):
                with self.assertRaisesRegex(
                    ConformanceError,
                    r'counterexample=.*"bytes_after":7.*"failure_signature".*"replayed":true',
                ):
                    build_report(root, bins)

    def test_shrinker_preserves_one_side_reject_signature(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(b"binary")
            bins = Binaries(*([binary] * 7))
            corpus = {
                "bounds": {"total_timeout_seconds": 60},
                "generator": {"case_count": 1},
            }
            case = {
                "id": "generated-000",
                "origin": "generated",
                "source": "NOISE\nONE_SIDE\nBOTH_REJECT\n",
            }

            def aliased_failure(
                candidate: dict[str, object],
                _path: Path,
                _bins: Binaries,
                _bounds: dict[str, object],
                _deadline: float,
            ) -> dict[str, object]:
                one_side = "ONE_SIDE" in candidate["source"]
                raise DifferentialFailure(
                    "generated-000: parser decision differs from expected",
                    DifferentialFailureSignature(
                        mismatch_kind="decision_mismatch",
                        stage="parse",
                        expected_decision="accepted",
                        rust_decision="accepted" if one_side else "rejected",
                        lean_decision="rejected",
                    ),
                )

            with (
                mock.patch(
                    "scripts.check_axi_contract_conformance.load_corpus",
                    return_value=(corpus, [case], {"version": 1}),
                ),
                mock.patch(
                    "scripts.check_axi_contract_conformance.evaluate_case",
                    side_effect=aliased_failure,
                ),
            ):
                with self.assertRaises(ConformanceError) as raised:
                    build_report(root, bins)
            message = str(raised.exception)
            self.assertIn('"source":"ONE_SIDE"', message)
            self.assertIn('"rust_decision":"accepted"', message)
            self.assertNotIn('"rust_decision":"rejected"', message)

    def test_shared_import_sorting_fails_ordered_sequence_golden(self) -> None:
        golden = {"SchemaV1Module.imports": ["Zed", "Alpha"]}
        shared_sorted = {"SchemaV1Module.imports": ["Alpha", "Zed"]}
        with self.assertRaisesRegex(ConformanceError, "ordered AST sequences differ"):
            require_ordered_sequence_golden(shared_sorted, golden, "hand-all-constructors")

    def test_all_constructor_golden_names_every_contract_ordered_array(self) -> None:
        _corpus, cases, contract = load_corpus(REPO)
        case = next(item for item in cases if item["id"] == "hand-all-constructors")
        self.assertEqual(
            set(case["ordered_sequence_golden"]), set(contract["ordered_sequences"])
        )

    def test_zero_generated_cases_rejects(self) -> None:
        generator = copy.deepcopy(self.corpus()["generator"])
        generator["case_count"] = 0
        with self.assertRaisesRegex(ConformanceError, "case_count"):
            generated_cases(generator)

    def test_generated_case_digest_drift_rejects(self) -> None:
        generator = copy.deepcopy(self.corpus()["generator"])
        generator["seed"] = "0x6a09e667f3bcc908"
        with self.assertRaisesRegex(ConformanceError, "generated case digest drift"):
            generated_cases(generator)

    def test_duplicate_case_ids_reject(self) -> None:
        _temporary, root, corpus = self.copied_contract()
        corpus["hand_cases"][1]["id"] = corpus["hand_cases"][0]["id"]
        target = root / CORPUS_PATH
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(corpus), encoding="utf-8")
        with self.assertRaisesRegex(ConformanceError, "case ids must be unique"):
            load_corpus(root)

    def test_unrecognized_rejection_class_rejects(self) -> None:
        _temporary, root, corpus = self.copied_contract()
        corpus["hand_cases"][2]["expected"]["outcome_class"] = "parse.other"
        target = root / CORPUS_PATH
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(corpus), encoding="utf-8")
        with self.assertRaisesRegex(ConformanceError, "unrecognized outcome class"):
            load_corpus(root)

    def test_contract_section_label_cannot_substitute_for_semantic_probe(self) -> None:
        _temporary, root, corpus = self.copied_contract()
        positive = corpus["hand_cases"][0]
        positive["coverage_probes"]["operational_bounds"] = "exact-n-and-runner-n-plus-one"
        target = root / CORPUS_PATH
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(corpus), encoding="utf-8")
        with self.assertRaisesRegex(ConformanceError, "exactly max_source_bytes"):
            load_corpus(root)

    def test_exact_n_fixture_and_n_plus_one_runner_rejection(self) -> None:
        _corpus, cases, _contract = load_corpus(REPO)
        bound = next(case for case in cases if case["id"] == "hand-bound-at-max")
        self.assertEqual(len(bound["source"].encode("utf-8")), 4096)
        self.assertEqual(
            bound["coverage_probes"]["operational_bounds"],
            "exact-n-and-runner-n-plus-one",
        )

    def assert_parse_envelope_rejects(self, value: dict[str, object], pattern: str) -> None:
        completed = subprocess.CompletedProcess([], 0, canonical_json(value), b"")
        with self.assertRaisesRegex(ConformanceError, pattern):
            parse_envelope(completed, "rust", "parse")

    def test_malformed_and_unknown_field_envelopes_reject(self) -> None:
        malformed = subprocess.CompletedProcess([], 0, b"not-json\n", b"")
        with self.assertRaisesRegex(ConformanceError, "malformed JSON"):
            parse_envelope(malformed, "rust", "parse")
        unknown = json.loads(self.envelope_bytes())
        unknown["extra"] = True
        self.assert_parse_envelope_rejects(unknown, "fields differ")

    def test_boolean_version_rejects(self) -> None:
        value = json.loads(self.envelope_bytes())
        value["version"] = True
        self.assert_parse_envelope_rejects(value, "envelope version")

    def test_normalized_ast_nested_missing_extra_and_wrong_types_reject(self) -> None:
        missing = json.loads(self.envelope_bytes())
        del missing["normalized_ast"]["imports"]
        self.assert_parse_envelope_rejects(missing, "fields differ")
        extra = json.loads(self.envelope_bytes())
        extra["normalized_ast"]["extra"] = []
        self.assert_parse_envelope_rejects(extra, "fields differ")
        wrong = json.loads(self.envelope_bytes())
        wrong["normalized_ast"]["schemas"] = {}
        self.assert_parse_envelope_rejects(wrong, "must be an array")

    def test_typecheck_summary_closed_shape_and_scalar_types_reject(self) -> None:
        base = json.loads(
            self.envelope_bytes(
                requested_stage="typecheck",
                observed_stage="typecheck",
                summary={
                    "module": "M",
                    "schemas": 0,
                    "theories": 0,
                    "instances": 0,
                    "assignments": 0,
                    "tuples": 0,
                },
            )
        )
        for mutation in (
            lambda value: value["summary"].pop("tuples"),
            lambda value: value["summary"].update({"extra": 0}),
            lambda value: value["summary"].update({"schemas": True}),
            lambda value: value["summary"].update({"module": {}}),
        ):
            value = copy.deepcopy(base)
            mutation(value)
            completed = subprocess.CompletedProcess([], 0, canonical_json(value), b"")
            with self.assertRaises(ConformanceError):
                parse_envelope(completed, "rust", "typecheck")

    def test_formation_summary_closed_shape_and_scalar_types_reject(self) -> None:
        base = json.loads(
            self.envelope_bytes(
                requested_stage="formation",
                observed_stage="formation",
                normalized_ast=None,
                summary={
                    "module": "M",
                    "import_closure": 1,
                    "schemas": 0,
                    "theories": 0,
                    "instances": 0,
                },
            )
        )
        for mutation in (
            lambda value: value["summary"].pop("instances"),
            lambda value: value["summary"].update({"extra": 0}),
            lambda value: value["summary"].update({"import_closure": True}),
            lambda value: value.update({"summary": []}),
        ):
            value = copy.deepcopy(base)
            mutation(value)
            completed = subprocess.CompletedProcess([], 0, canonical_json(value), b"")
            with self.assertRaises(ConformanceError):
                parse_envelope(completed, "rust", "formation")

    def test_equal_malformed_nested_summaries_cannot_pass(self) -> None:
        malformed = json.loads(
            self.envelope_bytes(
                requested_stage="typecheck",
                observed_stage="typecheck",
                summary={"module": "M", "schemas": True},
            )
        )
        completed = subprocess.CompletedProcess([], 0, canonical_json(malformed), b"")
        for implementation in ("rust", "lean"):
            value = copy.deepcopy(malformed)
            value["implementation"] = implementation
            completed = subprocess.CompletedProcess([], 0, canonical_json(value), b"")
            with self.assertRaises(ConformanceError):
                parse_envelope(completed, implementation, "typecheck")

    def test_envelope_exit_and_decision_disagreement_rejects(self) -> None:
        completed = subprocess.CompletedProcess([], 1, self.envelope_bytes(), b"")
        with self.assertRaisesRegex(ConformanceError, "decision and process exit"):
            parse_envelope(completed, "rust", "parse")

    def test_output_truncation_fails_closed(self) -> None:
        bounds = {
            "per_process_timeout_seconds": 1,
            "max_stdout_bytes": 8,
            "max_stderr_bytes": 8,
        }
        with self.assertRaisesRegex(ConformanceError, "stdout exceeded"):
            bounded_command(
                [sys.executable, "-c", "print('x' * 64)"],
                bounds,
                time.monotonic() + 2,
            )

    def test_timeout_fails_closed(self) -> None:
        bounds = {
            "per_process_timeout_seconds": 0.05,
            "max_stdout_bytes": 1024,
            "max_stderr_bytes": 1024,
        }
        with self.assertRaisesRegex(ConformanceError, "subprocess timeout"):
            bounded_command(
                [sys.executable, "-c", "import time; time.sleep(2)"],
                bounds,
                time.monotonic() + 2,
            )

    def test_equal_shared_bug_cannot_satisfy_hand_golden(self) -> None:
        with self.assertRaisesRegex(ConformanceError, "differs from hand golden"):
            require_hand_golden(
                {"module_name": "SharedWrong"},
                {"module_name": "HandPositive"},
                "hand-positive",
            )


if __name__ == "__main__":
    unittest.main()
