from __future__ import annotations

import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

from scripts.bounded_io import (
    BoundedIoError,
    read_regular_bounded,
    validate_json_nesting,
)
from scripts.bounded_subprocess import BoundedProcessError, run_bounded


class BoundedSecurityTests(unittest.TestCase):
    def test_json_depth_rejects_before_parser(self) -> None:
        raw = b"[" * 129 + b"0" + b"]" * 129
        with self.assertRaises(BoundedIoError):
            validate_json_nesting(raw, "test")

    def test_regular_reader_rejects_symlink_and_oversize(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            target = directory / "target"
            target.write_bytes(b"abcd")
            with self.assertRaises(BoundedIoError):
                read_regular_bounded(target, 3, "test input")
            link = directory / "link"
            try:
                link.symlink_to(target)
            except (NotImplementedError, OSError):
                return
            with self.assertRaises(BoundedIoError):
                read_regular_bounded(link, 8, "test input")

    def test_process_output_and_timeout_are_bounded(self) -> None:
        with self.assertRaises(BoundedProcessError):
            run_bounded(
                [sys.executable, "-c", "print('x' * 100000)"],
                timeout_seconds=5,
                max_stdout_bytes=1024,
                max_stderr_bytes=1024,
            )
        with self.assertRaises(subprocess.TimeoutExpired):
            run_bounded(
                [sys.executable, "-c", "import time; time.sleep(10)"],
                timeout_seconds=0.05,
                max_stdout_bytes=1024,
                max_stderr_bytes=1024,
            )

    def test_timeout_kills_descendant(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            marker = Path(raw_directory) / "descendant-survived"
            child = (
                "import pathlib,time; time.sleep(0.4); "
                f"pathlib.Path({str(marker)!r}).write_text('bad')"
            )
            parent = (
                "import subprocess,sys,time; "
                f"subprocess.Popen([sys.executable,'-c',{child!r}]); "
                "time.sleep(10)"
            )
            with self.assertRaises(subprocess.TimeoutExpired):
                run_bounded(
                    [sys.executable, "-c", parent],
                    timeout_seconds=0.05,
                    max_stdout_bytes=1024,
                    max_stderr_bytes=1024,
                )
            time.sleep(0.6)
            self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
