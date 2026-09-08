"""Run one filtered Cargo library gate; a successful empty selection is an error."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

if __package__:
    from .bounded_subprocess import BoundedProcessError, run_bounded
else:
    from bounded_subprocess import BoundedProcessError, run_bounded

ROOT = Path(__file__).resolve().parents[1]
SUMMARY = re.compile(
    r"^test result: ok\. (\d+) passed; 0 failed; \d+ ignored; "
    r"\d+ measured; \d+ filtered out; finished in .+$",
    re.MULTILINE,
)
PASSED = re.compile(r"^test (\S+) \.\.\. ok$", re.MULTILINE)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--package", required=True)
    parser.add_argument("--filter", required=True)
    args = parser.parse_args(argv)
    if not args.package.strip() or not args.filter.strip():
        parser.error("package and filter must be nonempty")

    command = [
        args.cargo,
        "test",
        "-p",
        args.package,
        "--lib",
        args.filter,
        "--",
        "--nocapture",
        "--format",
        "pretty",
        "--color",
        "never",
    ]
    try:
        result = run_bounded(
            command,
            cwd=ROOT / "rust",
            timeout_seconds=600,
            max_stdout_bytes=8 * 1024 * 1024,
            max_stderr_bytes=8 * 1024 * 1024,
        )
    except (OSError, BoundedProcessError, subprocess.TimeoutExpired) as error:
        print(
            f"error: required query tests could not complete: {error}", file=sys.stderr
        )
        return 1
    output = result.stdout.decode("utf-8", errors="replace")
    sys.stdout.write(output)
    sys.stderr.write(result.stderr.decode("utf-8", errors="replace"))
    if result.returncode != 0:
        # Preserve Cargo/test failures, including signal termination as a shell status.
        return result.returncode if result.returncode > 0 else 128 - result.returncode

    summaries = SUMMARY.findall(output)
    passed = PASSED.findall(output)
    # --lib selects a single libtest harness. Require actual named passes as well
    # as its summary; ignored tests and successful missing filters are not evidence.
    if (
        len(summaries) != 1
        or int(summaries[0]) == 0
        or int(summaries[0]) != len(passed)
        or any(args.filter not in name for name in passed)
    ):
        print(
            f"error: required query tests selected no verified nonempty passing run: "
            f"package={args.package} filter={args.filter}",
            file=sys.stderr,
        )
        return 1
    print(
        f"required query tests: package={args.package} filter={args.filter} "
        f"passed={len(passed)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
