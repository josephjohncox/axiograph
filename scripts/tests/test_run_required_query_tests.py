from __future__ import annotations

import contextlib
import io
import subprocess
import unittest
from unittest.mock import patch

import scripts.run_required_query_tests as gate


class RequiredQueryTests(unittest.TestCase):
    def run_gate(self, output: str, status: int = 0, selector: str = "prepared_query"):
        stdout, stderr = io.StringIO(), io.StringIO()
        result = subprocess.CompletedProcess(
            [], status, output.encode(), b"diagnostic\n"
        )
        with (
            patch.object(gate, "run_bounded", return_value=result) as run,
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            code = gate.main(["--package", "axiograph-query", "--filter", selector])
        return code, stdout.getvalue(), stderr.getvalue(), run

    def test_nonempty_selection_runs_library_and_reports_actual_passes(self):
        code, stdout, stderr, run = self.run_gate(
            "running 2 tests\n"
            "test query_ir::tests::prepared_query_a ... ok\n"
            "test query_ir::tests::prepared_query_b ... ok\n"
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; "
            "109 filtered out; finished in 0.01s\n"
        )
        self.assertEqual(code, 0)
        self.assertIn("passed=2", stdout)
        self.assertIn("diagnostic", stderr)
        self.assertEqual(
            run.call_args.args[0],
            [
                "cargo",
                "test",
                "-p",
                "axiograph-query",
                "--lib",
                "prepared_query",
                "--",
                "--nocapture",
                "--format",
                "pretty",
                "--color",
                "never",
            ],
        )
        self.assertEqual(run.call_args.kwargs["timeout_seconds"], 600)
        self.assertEqual(run.call_args.kwargs["max_stdout_bytes"], 8 * 1024 * 1024)
        self.assertEqual(run.call_args.kwargs["max_stderr_bytes"], 8 * 1024 * 1024)

    def test_zero_and_all_ignored_selections_reject(self):
        for ignored in (0, 2):
            with self.subTest(ignored=ignored):
                code, _, stderr, _ = self.run_gate(
                    f"test result: ok. 0 passed; 0 failed; {ignored} ignored; "
                    "0 measured; 111 filtered out; finished in 0.00s\n"
                )
                self.assertEqual(code, 1)
                self.assertIn("no verified nonempty passing run", stderr)

    def test_missing_or_mismatched_output_rejects(self):
        summary = (
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
            "110 filtered out; finished in 0.00s\n"
        )
        for output in (
            "",
            summary,
            "test unrelated ... ok\n" + summary,
            "test prepared_query ... ok\n" + summary + summary,
        ):
            with self.subTest(output=output):
                self.assertEqual(self.run_gate(output)[0], 1)

    def test_wrong_filter_rejects_even_with_other_passes(self):
        output = (
            "test query_ir::tests::prepared_query_a ... ok\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
            "110 filtered out; finished in 0.00s\n"
        )
        self.assertEqual(self.run_gate(output, selector="obsolete_selector")[0], 1)

    def test_absent_or_blank_selector_rejects_before_spawning(self):
        for args in (
            ["--package", "axiograph-query"],
            ["--package", "axiograph-query", "--filter", " "],
            ["--filter", "prepared_query"],
        ):
            with (
                self.subTest(args=args),
                patch.object(gate, "run_bounded") as run,
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as error,
            ):
                gate.main(args)
            self.assertEqual(error.exception.code, 2)
            run.assert_not_called()

    def test_command_and_test_failures_preserve_status(self):
        for status in (101, 7, -15):
            with self.subTest(status=status):
                code, stdout, stderr, _ = self.run_gate("test failed\n", status)
                self.assertEqual(code, status if status > 0 else 128 - status)
                self.assertIn("test failed", stdout)
                self.assertIn("diagnostic", stderr)

    def test_missing_command_timeout_and_output_bounds_fail_closed(self):
        for error in (
            FileNotFoundError("missing cargo"),
            subprocess.TimeoutExpired(["cargo"], 600),
            gate.BoundedProcessError("process stdout exceeded limit"),
        ):
            with (
                self.subTest(error=error),
                patch.object(gate, "run_bounded", side_effect=error),
                contextlib.redirect_stderr(io.StringIO()),
            ):
                self.assertEqual(
                    gate.main(
                        [
                            "--package",
                            "axiograph-query",
                            "--filter",
                            "prepared_query",
                        ]
                    ),
                    1,
                )


if __name__ == "__main__":
    unittest.main()
