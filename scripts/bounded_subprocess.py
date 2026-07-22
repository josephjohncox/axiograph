#!/usr/bin/env python3
"""Small descendant-aware bounded subprocess helper for release scripts."""

from __future__ import annotations

import os
import signal
import subprocess
import threading
import time
from collections.abc import Mapping, Sequence
from contextlib import suppress
from pathlib import Path
from typing import BinaryIO


class BoundedProcessError(RuntimeError):
    pass


def _terminate_tree(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=5,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
            process.kill()
    else:
        with suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)


def run_bounded(
    argv: Sequence[str],
    *,
    input_bytes: bytes = b"",
    timeout_seconds: float,
    max_stdout_bytes: int,
    max_stderr_bytes: int,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
) -> subprocess.CompletedProcess[bytes]:
    if not argv:
        raise BoundedProcessError("process argv must not be empty")
    if not 0 < timeout_seconds <= 600:
        raise BoundedProcessError("process timeout must be in (0, 600]")
    if max_stdout_bytes <= 0 or max_stderr_bytes <= 0:
        raise BoundedProcessError("process output limits must be positive")
    if len(input_bytes) > 16 * 1024 * 1024:
        raise BoundedProcessError("process stdin exceeds 16777216 bytes")

    creationflags = 0
    popen_options: dict[str, object] = {}
    if os.name == "nt":
        creationflags = subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        popen_options["start_new_session"] = True

    process = subprocess.Popen(
        list(argv),
        cwd=cwd,
        env=None if env is None else dict(env),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        creationflags=creationflags,
        **popen_options,
    )
    stdout = bytearray()
    stderr = bytearray()
    overflow: list[str] = []
    overflow_event = threading.Event()

    def drain(stream: BinaryIO, target: bytearray, limit: int, label: str) -> None:
        while True:
            chunk = stream.read(64 * 1024)
            if not chunk:
                return
            remaining = limit - len(target)
            if remaining > 0:
                target.extend(chunk[:remaining])
            if len(chunk) > remaining:
                if not overflow:
                    overflow.append(label)
                overflow_event.set()

    def feed() -> None:
        assert process.stdin is not None
        try:
            with suppress(BrokenPipeError):
                process.stdin.write(input_bytes)
                process.stdin.flush()
        finally:
            process.stdin.close()

    assert process.stdout is not None
    assert process.stderr is not None
    threads = [
        threading.Thread(
            target=drain,
            args=(process.stdout, stdout, max_stdout_bytes, "stdout"),
            daemon=True,
        ),
        threading.Thread(
            target=drain,
            args=(process.stderr, stderr, max_stderr_bytes, "stderr"),
            daemon=True,
        ),
        threading.Thread(target=feed, daemon=True),
    ]
    for thread in threads:
        thread.start()

    deadline = time.monotonic() + timeout_seconds
    timed_out = False
    while process.poll() is None:
        if overflow_event.wait(timeout=0.02):
            _terminate_tree(process)
            break
        if time.monotonic() >= deadline:
            timed_out = True
            _terminate_tree(process)
            break
    try:
        returncode = process.wait(timeout=5)
    except subprocess.TimeoutExpired as timeout_error:
        del timeout_error
        _terminate_tree(process)
        returncode = process.wait(timeout=5)
    for thread in threads:
        thread.join(timeout=5)
    process.stdout.close()
    process.stderr.close()

    if timed_out:
        raise subprocess.TimeoutExpired(
            list(argv), timeout_seconds, bytes(stdout), bytes(stderr)
        )
    if overflow:
        raise BoundedProcessError(
            f"process {overflow[0]} exceeded "
            f"{max_stdout_bytes if overflow[0] == 'stdout' else max_stderr_bytes} bytes"
        )
    return subprocess.CompletedProcess(
        list(argv), returncode, bytes(stdout), bytes(stderr)
    )
