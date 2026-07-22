#!/usr/bin/env python3
"""No-follow bounded regular-file helpers for release tooling."""

from __future__ import annotations

import os
import stat
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import BinaryIO


class BoundedIoError(RuntimeError):
    pass


def validate_json_nesting(raw: bytes, label: str, max_depth: int = 128) -> None:
    stack: list[int] = []
    in_string = False
    escaped = False
    for byte in raw:
        if in_string:
            if escaped:
                escaped = False
            elif byte == 0x5C:
                escaped = True
            elif byte == 0x22:
                in_string = False
            continue
        if byte == 0x22:
            in_string = True
        elif byte in (0x7B, 0x5B):
            stack.append(byte)
            if len(stack) > max_depth:
                raise BoundedIoError(f"{label} JSON nesting exceeds {max_depth}")
        elif byte == 0x7D:
            if not stack or stack.pop() != 0x7B:
                raise BoundedIoError(f"{label} JSON containers are unbalanced")
        elif byte == 0x5D and (not stack or stack.pop() != 0x5B):
            raise BoundedIoError(f"{label} JSON containers are unbalanced")
    if in_string or escaped or stack:
        raise BoundedIoError(f"{label} JSON is structurally unbalanced")


@contextmanager
def open_regular_bounded(
    path: Path, limit: int, label: str
) -> Iterator[tuple[BinaryIO, int]]:
    if limit <= 0:
        raise BoundedIoError(f"{label} byte limit must be positive")
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or path.is_symlink():
        raise BoundedIoError(f"{label} must be a regular non-symlink file")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode):
            raise BoundedIoError(f"opened {label} is not a regular file")
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise BoundedIoError(f"{label} changed while opening")
        if not 0 < opened.st_size <= limit:
            raise BoundedIoError(f"{label} size must be in 1..={limit} bytes")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            yield stream, opened.st_size
    finally:
        os.close(descriptor)


def read_regular_bounded(path: Path, limit: int, label: str) -> bytes:
    with open_regular_bounded(path, limit, label) as (stream, expected_size):
        data = bytearray()
        while chunk := stream.read(min(1024 * 1024, expected_size - len(data) + 1)):
            data.extend(chunk)
            if len(data) > expected_size:
                raise BoundedIoError(f"{label} grew while reading")
    if len(data) != expected_size:
        raise BoundedIoError(f"{label} size changed while reading")
    return bytes(data)


def sha256_regular_bounded(path: Path, limit: int, label: str) -> str:
    import hashlib

    with open_regular_bounded(path, limit, label) as (stream, expected_size):
        digest = hashlib.sha256()
        read_bytes = 0
        while chunk := stream.read(min(1024 * 1024, expected_size - read_bytes + 1)):
            read_bytes += len(chunk)
            if read_bytes > expected_size:
                raise BoundedIoError(f"{label} grew while hashing")
            digest.update(chunk)
    if read_bytes != expected_size:
        raise BoundedIoError(f"{label} size changed while hashing")
    return digest.hexdigest()
