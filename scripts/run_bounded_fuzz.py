#!/usr/bin/env python3
"""Run the committed libFuzzer targets under explicit resource bounds."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from bounded_subprocess import BoundedProcessError, run_bounded

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_ROOT = REPO_ROOT / "rust"
FUZZ_ROOT = RUST_ROOT / "fuzz"

TARGETS = (
    ("axi_parser", 256, 1_048_576),
    ("certificate_json", 256, 1_048_576),
    ("repl_command", 512, 65_536),
    ("proposal_adapter_json", 128, 8_388_608),
    ("axpd_bytes", 64, 262_144),
)


def decode_output(value: bytes | str | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def run_checked(
    command: list[str],
    *,
    env: dict[str, str],
    cwd: Path,
    timeout_seconds: int,
    max_output_bytes: int,
) -> str:
    try:
        result = run_bounded(
            command,
            timeout_seconds=timeout_seconds,
            max_stdout_bytes=max_output_bytes,
            max_stderr_bytes=max_output_bytes,
            cwd=cwd,
            env=env,
        )
    except subprocess.TimeoutExpired as error:
        output = decode_output(error.stdout) + decode_output(error.stderr)
        tail = "\n".join(output.splitlines()[-80:])
        raise RuntimeError(
            f"command exceeded {timeout_seconds}s: {' '.join(command)}\n{tail}"
        ) from error
    except BoundedProcessError as error:
        raise RuntimeError(
            f"bounded process rejected {' '.join(command)}: {error}"
        ) from error
    output = decode_output(result.stdout) + decode_output(result.stderr)
    if result.returncode != 0:
        tail = "\n".join(output.splitlines()[-80:])
        raise RuntimeError(
            f"command failed with exit {result.returncode}: {' '.join(command)}\n{tail}"
        )
    return output


def toolchain_environment(toolchain: str) -> tuple[dict[str, str], Path]:
    rustup = shutil.which("rustup")
    if rustup is None:
        raise RuntimeError("rustup is required for the pinned fuzz toolchain")
    try:
        result = run_bounded(
            [rustup, "which", "--toolchain", toolchain, "cargo"],
            timeout_seconds=30,
            max_stdout_bytes=64 * 1024,
            max_stderr_bytes=64 * 1024,
            cwd=REPO_ROOT,
        )
    except (BoundedProcessError, subprocess.TimeoutExpired) as error:
        raise RuntimeError(
            f"could not resolve required fuzz toolchain {toolchain!r}: {error}"
        ) from error
    if result.returncode != 0:
        detail = decode_output(result.stderr).strip()
        raise RuntimeError(
            f"required fuzz toolchain {toolchain!r} is unavailable: {detail}"
        )
    cargo = Path(decode_output(result.stdout).strip()).resolve()
    if not cargo.is_file():
        raise RuntimeError(f"rustup returned a missing cargo executable: {cargo}")
    env = os.environ.copy()
    env["PATH"] = f"{cargo.parent}{os.pathsep}{env.get('PATH', '')}"
    return env, cargo


def copy_seed_corpus(target: str, destination: Path) -> None:
    source = FUZZ_ROOT / "corpus" / target
    if not source.is_dir():
        raise RuntimeError(f"missing committed seed corpus: {source}")
    shutil.copytree(source, destination)
    if not any(destination.iterdir()):
        raise RuntimeError(f"seed corpus is empty: {source}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="nightly-2026-07-23")
    parser.add_argument("--cargo-fuzz-version", default="0.13.2")
    parser.add_argument("--subprocess-timeout-seconds", type=int, default=180)
    parser.add_argument("--case-timeout-seconds", type=int, default=5)
    parser.add_argument("--rss-limit-mb", type=int, default=2048)
    parser.add_argument("--max-total-time-seconds", type=int, default=30)
    parser.add_argument("--max-output-bytes", type=int, default=8 * 1024 * 1024)
    args = parser.parse_args()

    waitid_available = all(
        hasattr(os, attribute) for attribute in ("waitid", "P_PID", "WNOWAIT")
    )
    if os.name == "nt" or not waitid_available:
        parser.error(
            "bounded fuzz verification requires POSIX waitid/WNOWAIT process-group containment"
        )

    positive_values = (
        args.subprocess_timeout_seconds,
        args.case_timeout_seconds,
        args.rss_limit_mb,
        args.max_total_time_seconds,
        args.max_output_bytes,
    )
    if any(value <= 0 for value in positive_values):
        parser.error("all resource bounds must be positive")
    if args.subprocess_timeout_seconds > 600:
        parser.error("subprocess timeout must not exceed 600 seconds")
    if args.max_output_bytes > 64 * 1024 * 1024:
        parser.error("per-stream output limit must not exceed 67108864 bytes")

    try:
        env, cargo = toolchain_environment(args.toolchain)
        version = run_checked(
            [str(cargo), "fuzz", "--version"],
            env=env,
            cwd=RUST_ROOT,
            timeout_seconds=30,
            max_output_bytes=64 * 1024,
        ).strip()
        expected_version = f"cargo-fuzz {args.cargo_fuzz_version}"
        if version != expected_version:
            raise RuntimeError(
                f"expected {expected_version!r}, found {version!r}; "
                "install the exact cargo-fuzz version"
            )

        run_checked(
            [
                str(cargo),
                "metadata",
                "--locked",
                "--manifest-path",
                str(FUZZ_ROOT / "Cargo.toml"),
                "--format-version",
                "1",
                "--no-deps",
            ],
            env=env,
            cwd=RUST_ROOT,
            timeout_seconds=60,
            max_output_bytes=args.max_output_bytes,
        )
        run_checked(
            [
                str(cargo),
                "test",
                "--locked",
                "--manifest-path",
                str(FUZZ_ROOT / "Cargo.toml"),
                "--test",
                "seed_corpus",
            ],
            env=env,
            cwd=RUST_ROOT,
            timeout_seconds=args.subprocess_timeout_seconds,
            max_output_bytes=args.max_output_bytes,
        )

        with tempfile.TemporaryDirectory(prefix="axiograph-fuzz-") as temporary:
            temporary_root = Path(temporary)
            for target, runs, max_len in TARGETS:
                corpus = temporary_root / "corpus" / target
                artifacts = temporary_root / "artifacts" / target
                copy_seed_corpus(target, corpus)
                artifacts.mkdir(parents=True)
                command = [
                    str(cargo),
                    "fuzz",
                    "run",
                    target,
                    str(corpus),
                    "--",
                    f"-runs={runs}",
                    f"-max_len={max_len}",
                    f"-timeout={args.case_timeout_seconds}",
                    f"-rss_limit_mb={args.rss_limit_mb}",
                    f"-max_total_time={args.max_total_time_seconds}",
                    f"-exact_artifact_path={artifacts / 'failure-input'}",
                ]
                output = run_checked(
                    command,
                    env=env,
                    cwd=RUST_ROOT,
                    timeout_seconds=args.subprocess_timeout_seconds,
                    max_output_bytes=args.max_output_bytes,
                )
                completion = next(
                    (
                        line.strip()
                        for line in reversed(output.splitlines())
                        if "DONE" in line or "cov:" in line
                    ),
                    "completed",
                )
                print(f"PASS {target}: {completion}")
    except RuntimeError as error:
        print(f"bounded fuzz verification failed: {error}", file=sys.stderr)
        return 1

    print("bounded fuzz verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
