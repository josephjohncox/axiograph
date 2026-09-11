#!/usr/bin/env python3
"""Regenerate the pinned no-unsafe external-cache manifest from one archive.

The command is offline, fixed-location, descriptor-confined, bounded, and never
replaces or removes an output object. Production always uses module-constant
real operation objects and immutable archive limits.
"""

from __future__ import annotations

import errno
import hashlib
import json
import os
import re
import stat
import sys
import zlib
from dataclasses import dataclass
from typing import Any

try:
    from scripts.check_no_unsafe import (
        CANDIDATE_HOMES,
        MANIFEST_REL,
        bind_invocation,
        validate_closed_manifest,
    )
    from scripts.no_unsafe_fs import (
        O_CLOEXEC,
        O_NOFOLLOW,
        O_PATH,
        REAL_FILE_OPS,
        REAL_IO_OPS,
        RESOLVE_BENEATH,
        RESOLVE_NO_MAGICLINKS,
        RESOLVE_NO_SYMLINKS,
        RESOLVE_NO_XDEV,
        FileOps,
        HeldDirectory,
        PolicyFailure,
        directory_tuple,
        full_tuple,
        metadata_observe_at,
        open_directory_at,
        read_regular_at,
        upgrade_metadata_fd,
    )
except ModuleNotFoundError:  # Direct script execution.
    from check_no_unsafe import (
        CANDIDATE_HOMES,
        MANIFEST_REL,
        bind_invocation,
        validate_closed_manifest,
    )
    from no_unsafe_fs import (
        O_CLOEXEC,
        O_NOFOLLOW,
        O_PATH,
        REAL_FILE_OPS,
        REAL_IO_OPS,
        RESOLVE_BENEATH,
        RESOLVE_NO_MAGICLINKS,
        RESOLVE_NO_SYMLINKS,
        RESOLVE_NO_XDEV,
        FileOps,
        HeldDirectory,
        PolicyFailure,
        directory_tuple,
        full_tuple,
        metadata_observe_at,
        open_directory_at,
        read_regular_at,
        upgrade_metadata_fd,
    )

EXPECTED_BASENAME = "generate_no_unsafe_external_cache_manifest.py"
ARCHIVE_REL = "build/engineering-quality/release-roadmap/prepare-compatible-release-20260908T035930Z/tools/kani-home/kani-0.67.0-aarch64-unknown-linux-gnu.tar.gz"
ARCHIVE_NAME = "kani-0.67.0-aarch64-unknown-linux-gnu.tar.gz"
ARCHIVE_BYTES = 137_826_806
ARCHIVE_SHA256 = "974128f44dd43618a06d21e5fe6d9ff67188de5986fe0bc57b534b0e4639efb9"
ARCHIVE_ROOT = "kani-0.67.0"
LIBRARY_PREFIX = "kani-0.67.0/library/"
ORIGINAL_BYTES_DEFINITION = "For each regular member, SHA-256 is over exactly the decompressed tar payload bytes returned for that member, with no text decoding, newline conversion, metadata, path, padding, or other normalization."
OUTPUT_BASENAME = "regenerated-kani-library-inventory.json"
OUTPUT_RE = re.compile(r"build/engineering-quality/([A-Za-z0-9][A-Za-z0-9._-]{0,95})/([A-Za-z0-9][A-Za-z0-9._-]{0,95})/regenerated-kani-library-inventory\.json")
RESOLVE_INPUT = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV


@dataclass(frozen=True)
class ArchiveLimits:
    compressed: int = 160 * 1024 * 1024
    decompressed: int = 805_306_368
    members: int = 256
    payload: int = 640 * 1024 * 1024
    one_member: int = 64 * 1024 * 1024
    path_bytes: int = 256


PRODUCTION_ARCHIVE_LIMITS = ArchiveLimits()


@dataclass(frozen=True)
class ArchiveInventory:
    directories: tuple[dict[str, str], ...]
    files: tuple[dict[str, Any], ...]
    compressed_bytes: int
    decompressed_bytes: int
    member_count: int


class CountingInflater:
    """Bounded gzip inflater whose produced-byte counter never resets."""

    def __init__(
        self,
        fd: int,
        *,
        ops: FileOps,
        limits: ArchiveLimits,
        expected_compressed: int | None = None,
    ):
        self.fd = fd
        self.ops = ops
        self.limits = limits
        self.expected_compressed = expected_compressed
        self.compressed_bytes = 0
        self.decompressed_bytes = 0
        self._pending = b""
        self._output = bytearray()
        self._physical_eof = False
        self._finished = False
        self._decompressor = zlib.decompressobj(16 + zlib.MAX_WBITS)

    def _physical_read(self) -> bytes:
        if self._physical_eof:
            return b""
        try:
            chunk = self.ops.read(self.fd, min(65_536, self.limits.compressed + 1 - self.compressed_bytes))
        except OSError as error:
            raise PolicyFailure("E_REGEN_ARCHIVE_PARSER_READ", f"archive parser read failed: {error}") from error
        if not chunk:
            self._physical_eof = True
            if self.expected_compressed is not None and self.compressed_bytes < self.expected_compressed:
                raise PolicyFailure(
                    "E_REGEN_ARCHIVE_PARSER_SHORT_READ",
                    "archive parser physical input ended before stable declared size",
                )
            return b""
        self.compressed_bytes += len(chunk)
        if self.compressed_bytes > self.limits.compressed:
            raise PolicyFailure("E_ARCHIVE_COMPRESSED_LIMIT", "compressed archive limit exceeded")
        return chunk

    def _next_member_or_finish(self) -> None:
        pending = self._decompressor.unused_data
        if self._decompressor.unconsumed_tail:
            pending = self._decompressor.unconsumed_tail + pending
        if not pending:
            pending = self._physical_read()
        if not pending:
            self._finished = True
            return
        if not pending.startswith(b"\x1f\x8b"):
            raise PolicyFailure("E_ARCHIVE_GZIP", "non-gzip trailing compressed bytes")
        self._decompressor = zlib.decompressobj(16 + zlib.MAX_WBITS)
        self._pending = pending

    def _produce(self) -> None:
        while not self._output and not self._finished:
            if self._decompressor.eof:
                self._next_member_or_finish()
                continue
            chunk = self._pending
            self._pending = b""
            if not chunk:
                chunk = self._physical_read()
            if not chunk:
                try:
                    tail = self._decompressor.flush(65_536)
                except zlib.error as error:
                    raise PolicyFailure("E_ARCHIVE_GZIP", f"gzip flush rejected: {error}") from error
                if tail:
                    self._charge(tail)
                    self._output.extend(tail)
                    return
                if not self._decompressor.eof:
                    raise PolicyFailure("E_ARCHIVE_GZIP", "truncated gzip stream")
                continue
            remaining = self.limits.decompressed - self.decompressed_bytes
            try:
                output = self._decompressor.decompress(chunk, min(65_536, remaining + 1))
            except zlib.error as error:
                raise PolicyFailure("E_ARCHIVE_GZIP", f"gzip stream rejected: {error}") from error
            if self._decompressor.unconsumed_tail:
                self._pending = self._decompressor.unconsumed_tail
            self._charge(output)
            self._output.extend(output)

    def _charge(self, output: bytes) -> None:
        self.decompressed_bytes += len(output)
        if self.decompressed_bytes > self.limits.decompressed:
            raise PolicyFailure("E_ARCHIVE_DECOMPRESSED_LIMIT", "decompressed stream limit exceeded")

    def read(self, count: int) -> bytes:
        if count < 0:
            raise PolicyFailure("E_ARCHIVE_TAR", "unbounded inflater read rejected")
        result = bytearray()
        while len(result) < count:
            if not self._output:
                self._produce()
            if not self._output:
                break
            take = min(count - len(result), len(self._output))
            result.extend(self._output[:take])
            del self._output[:take]
        return bytes(result)

    def read_exact(self, count: int, *, code: str) -> bytes:
        data = self.read(count)
        if len(data) != count:
            raise PolicyFailure(code, f"needed {count} bytes, got {len(data)}")
        return data

    def drain_zeros(self) -> None:
        while True:
            chunk = self.read(65_536)
            if not chunk:
                return
            if any(chunk):
                raise PolicyFailure("E_ARCHIVE_TAR", "nonzero bytes after tar end marker")


def _tar_number(field: bytes, label: str) -> int:
    if field and field[0] & 0x80:
        raise PolicyFailure("E_ARCHIVE_TAR", f"base-256 {label} is unsupported")
    value = field.rstrip(b"\0 ").lstrip(b" ") or b"0"
    if not re.fullmatch(rb"[0-7]+", value):
        raise PolicyFailure("E_ARCHIVE_TAR", f"invalid tar {label}")
    return int(value, 8)


def _tar_text(field: bytes) -> bytes:
    """Decode one NUL-padded tar text field without hiding embedded data."""
    nul = field.find(b"\0")
    if nul < 0:
        return field
    if any(field[nul + 1 :]):
        raise PolicyFailure("E_ARCHIVE_PATH_NUL", "archive path field contains embedded NUL data")
    return field[:nul]


def _validate_archive_path(raw: bytes, limits: ArchiveLimits) -> str:
    if len(raw) > limits.path_bytes:
        raise PolicyFailure("E_ARCHIVE_PATH_SIZE", "archive path exceeds 256 bytes")
    try:
        path = raw.decode("ascii")
    except UnicodeDecodeError as error:
        raise PolicyFailure("E_ARCHIVE_PATH_ENCODING", "archive path is not ASCII") from error
    if "\x00" in path:
        raise PolicyFailure("E_ARCHIVE_PATH_NUL", "archive path contains NUL")
    if path.startswith("/"):
        raise PolicyFailure("E_ARCHIVE_PATH_ABSOLUTE", "archive path is absolute")
    if "\\" in path:
        raise PolicyFailure("E_ARCHIVE_PATH_BACKSLASH", "archive path contains backslash")
    stripped = path.removesuffix("/")
    parts = stripped.split("/")
    if not stripped or any(part == "" for part in parts):
        raise PolicyFailure("E_ARCHIVE_PATH_EMPTY", "archive path contains an empty component")
    if any(part == "." for part in parts):
        raise PolicyFailure("E_ARCHIVE_PATH_DOT", "archive path contains dot")
    if any(part == ".." for part in parts):
        raise PolicyFailure("E_ARCHIVE_PATH_ESCAPE", "archive path escapes")
    return path


def _validate_extension_header_path(
    raw: bytes,
    typeflag: bytes,
    limits: ArchiveLimits,
) -> str:
    """Validate a physical extension-header name under explicit GNU semantics."""
    # GNU tar uses this fixed metadata name for long-name records in the
    # official archive. It is control framing, not an extracted member path.
    control_names = {
        b"L": b"././@LongLink",
        b"x": b"././@PaxHeader",
        b"g": b"././@PaxHeader",
    }
    if raw == control_names.get(typeflag):
        if len(raw) > limits.path_bytes:
            raise PolicyFailure("E_ARCHIVE_PATH_SIZE", "archive extension path exceeds limit")
        return raw.decode("ascii")
    return _validate_archive_path(raw, limits)


def _parse_pax(payload: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    harmless_keys = {"path", "comment"}
    offset = 0
    while offset < len(payload):
        space = payload.find(b" ", offset)
        if space < 0:
            raise PolicyFailure("E_ARCHIVE_TAR", "malformed PAX length")
        try:
            length = int(payload[offset:space])
        except ValueError as error:
            raise PolicyFailure("E_ARCHIVE_TAR", "malformed PAX length") from error
        if length <= 0 or offset + length > len(payload):
            raise PolicyFailure("E_ARCHIVE_TAR", "PAX record length is out of range")
        record = payload[space + 1 : offset + length]
        if not record.endswith(b"\n") or b"=" not in record:
            raise PolicyFailure("E_ARCHIVE_TAR", "malformed PAX record")
        key, raw_value = record[:-1].split(b"=", 1)
        try:
            key_text = key.decode("ascii")
            value_text = raw_value.decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise PolicyFailure("E_ARCHIVE_PATH_ENCODING", "PAX text encoding rejected") from error
        if key_text in values:
            raise PolicyFailure("E_ARCHIVE_DUPLICATE", "duplicate PAX key")
        if key_text.startswith("GNU.sparse.") or (
            key_text == "SCHILY.filetype" and value_text == "sparse"
        ):
            raise PolicyFailure("E_ARCHIVE_KIND_SPARSE", "PAX sparse semantics are unsupported")
        if key_text not in harmless_keys:
            raise PolicyFailure(
                "E_ARCHIVE_KIND_UNSUPPORTED",
                f"unsupported PAX semantic key: {key_text[:128]}",
            )
        values[key_text] = value_text
        offset += length
    return values


def parse_archive_descriptor(
    fd: int,
    *,
    ops: FileOps = REAL_FILE_OPS,
    limits: ArchiveLimits = PRODUCTION_ARCHIVE_LIMITS,
    expected_compressed: int | None = None,
) -> ArchiveInventory:
    """Parse one already-open gzip/tar descriptor under immutable hard bounds."""
    inflater = CountingInflater(
        fd,
        ops=ops,
        limits=limits,
        expected_compressed=expected_compressed,
    )
    directories: dict[str, str] = {}
    files: dict[str, dict[str, Any]] = {}
    seen: set[str] = set()
    payload_sum = 0
    members = 0
    charged_headers = 0
    zero_blocks = 0
    global_pax: dict[str, str] = {}
    pending_pax: dict[str, str] = {}
    pending_long_name: str | None = None
    while True:
        block = inflater.read(512)
        if not block:
            raise PolicyFailure("E_ARCHIVE_TAR", "tar ended before two zero blocks")
        if len(block) != 512:
            raise PolicyFailure("E_ARCHIVE_TAR", "partial tar header")
        if block == bytes(512):
            zero_blocks += 1
            if zero_blocks == 2:
                inflater.drain_zeros()
                break
            continue
        if zero_blocks:
            raise PolicyFailure("E_ARCHIVE_TAR", "single tar zero block before a member")
        stored_checksum = _tar_number(block[148:156], "checksum")
        checksum_block = bytearray(block)
        checksum_block[148:156] = b"        "
        if sum(checksum_block) != stored_checksum:
            raise PolicyFailure("E_ARCHIVE_TAR", "tar checksum mismatch")
        # Charge a valid physical header before reading any path or size field.
        # This makes N+1 member/header precedence independent of malformed
        # path and size bytes in that header.
        charged_headers += 1
        if charged_headers > limits.members:
            raise PolicyFailure("E_ARCHIVE_MEMBER_COUNT", "archive member/header count exceeded")
        typeflag = block[156:157] or b"\0"
        extension = typeflag in (b"x", b"g", b"L")
        if not extension:
            members += 1
        name = _tar_text(block[0:100])
        prefix = _tar_text(block[345:500])
        raw_header_path = prefix + (b"/" if prefix and name else b"") + name
        effective_path = raw_header_path
        normalized = "<archive-extension>"
        if extension:
            normalized = _validate_extension_header_path(
                raw_header_path, typeflag, limits
            ).removesuffix("/")
        else:
            effective_pax = {**global_pax, **pending_pax}
            if "path" in effective_pax:
                effective_path = effective_pax["path"].encode("utf-8")
            if pending_long_name is not None:
                effective_path = pending_long_name.encode("ascii")
            path = _validate_archive_path(effective_path, limits)
            normalized = path.removesuffix("/")
            if normalized in seen:
                raise PolicyFailure("E_ARCHIVE_DUPLICATE", f"duplicate archive path: {normalized[:128]}")
            seen.add(normalized)
        kind_codes = {
            b"1": "E_ARCHIVE_KIND_HARDLINK",
            b"2": "E_ARCHIVE_KIND_SYMLINK",
            b"3": "E_ARCHIVE_KIND_DEVICE",
            b"4": "E_ARCHIVE_KIND_DEVICE",
            b"6": "E_ARCHIVE_KIND_FIFO",
            b"s": "E_ARCHIVE_KIND_SOCKET",
            b"S": "E_ARCHIVE_KIND_SPARSE",
            b"K": "E_ARCHIVE_KIND_UNSUPPORTED",
        }
        if typeflag in kind_codes:
            raise PolicyFailure(kind_codes[typeflag], f"unsupported archive member kind: {normalized[:128]}")
        if typeflag not in (b"0", b"\0", b"5", b"7", b"x", b"g", b"L"):
            raise PolicyFailure("E_ARCHIVE_KIND_UNSUPPORTED", f"unsupported archive typeflag {typeflag!r}")
        size = _tar_number(block[124:136], "size")
        if size > limits.one_member:
            raise PolicyFailure("E_ARCHIVE_MEMBER_SIZE", "archive member exceeds 64 MiB")
        payload_sum += size
        if payload_sum > limits.payload:
            raise PolicyFailure("E_ARCHIVE_EXPANSION", "archive declared payload exceeds 640 MiB")
        payload = inflater.read_exact(size, code="E_ARCHIVE_SHORT_MEMBER")
        padding = (-size) % 512
        if padding:
            padding_bytes = inflater.read_exact(padding, code="E_ARCHIVE_TAR")
            if any(padding_bytes):
                raise PolicyFailure("E_ARCHIVE_TAR", "tar member padding is not zero")
        if typeflag in (b"x", b"g"):
            parsed = _parse_pax(payload)
            if typeflag == b"x":
                pending_pax = parsed
            else:
                # POSIX global PAX values apply to each following ordinary
                # member until replaced by another global record; a local x
                # record overrides the same key for one member.
                global_pax.update(parsed)
            continue
        if typeflag == b"L":
            trimmed = payload.rstrip(b"\0")
            if b"\0" in trimmed:
                raise PolicyFailure("E_ARCHIVE_PATH_NUL", "GNU long name contains embedded NUL")
            try:
                pending_long_name = trimmed.decode("ascii")
            except UnicodeDecodeError as error:
                raise PolicyFailure("E_ARCHIVE_PATH_ENCODING", "GNU long name is not ASCII") from error
            continue
        pending_pax = {}
        pending_long_name = None
        if typeflag == b"5":
            if size != 0:
                raise PolicyFailure("E_ARCHIVE_TAR", "directory member has payload")
            if normalized.startswith(LIBRARY_PREFIX):
                relative = normalized[len(LIBRARY_PREFIX) :]
                if relative:
                    directories[relative] = normalized
            continue
        if normalized.startswith(LIBRARY_PREFIX):
            relative = normalized[len(LIBRARY_PREFIX) :]
            if relative:
                files[relative] = {
                    "archive_member": normalized,
                    "bytes": size,
                    "path": relative,
                    "sha256": hashlib.sha256(payload).hexdigest(),
                }
    if pending_pax or pending_long_name is not None:
        raise PolicyFailure("E_ARCHIVE_TAR", "orphan archive extension record")
    if len(directories) != 10 or len(files) != 32:
        raise PolicyFailure("E_ARCHIVE_LIBRARY_INVENTORY", "selected library inventory count differs")
    return ArchiveInventory(
        tuple({"archive_member": directories[path], "path": path} for path in sorted(directories)),
        tuple(files[path] for path in sorted(files)),
        inflater.compressed_bytes,
        inflater.decompressed_bytes,
        members,
    )


def _open_parent(
    root_fd: int,
    components: list[str],
    *,
    ops: FileOps,
) -> list[HeldDirectory]:
    chain: list[HeldDirectory] = []
    parent = root_fd
    try:
        for component in components:
            try:
                item = open_directory_at(
                    parent,
                    component,
                    family="E_REGEN_FS",
                    ops=ops,
                    use_openat2=True,
                    resolve=RESOLVE_INPUT,
                )
            except PolicyFailure as error:
                additional = tuple(
                    "E_REGEN_CLOSE" if code.endswith("_CLOSE") else code
                    for code in error.additional
                )
                raise PolicyFailure(
                    "E_REGEN_CONFINEMENT",
                    error.detail,
                    additional=additional,
                ) from error
            chain.append(item)
            parent = item.data_fd
        return chain
    except PolicyFailure as error:
        primary = error
        for item in reversed(chain):
            try:
                item.close(ops, "E_REGEN_CLOSE")
            except PolicyFailure as close_error:
                primary = primary.with_additional("E_REGEN_CLOSE")
                for code in close_error.additional:
                    primary = primary.with_additional(code)
        raise primary


def _close_parent(
    chain: list[HeldDirectory],
    *,
    ops: FileOps,
    close_code: str = "E_REGEN_CLOSE",
) -> None:
    active = sys.exc_info()[1]
    primary = active if isinstance(active, PolicyFailure) else None
    for item in reversed(chain):
        try:
            item.close(ops, close_code)
        except PolicyFailure as error:
            if primary is None or error.code == getattr(active, "code", None):
                primary = error
            else:
                primary = primary.with_additional(error.code)
                for code in error.additional:
                    primary = primary.with_additional(code)
    if primary is not None and primary is not active:
        raise primary


def _replay_parent(
    root_fd: int,
    components: list[str],
    expected: tuple[tuple[int, int, int, int, int], ...],
    *,
    race_code: str,
    ops: FileOps,
    close_code: str = "E_REGEN_CLOSE",
) -> None:
    try:
        replay = _open_parent(root_fd, components, ops=ops)
    except PolicyFailure as error:
        raise PolicyFailure(
            race_code, error.detail, additional=error.additional
        ) from error
    try:
        if tuple(directory_tuple(item.metadata) for item in replay) != expected:
            raise PolicyFailure(race_code, "parent chain identity changed")
    finally:
        _close_parent(replay, ops=ops, close_code=close_code)


def _replay_parent_identity(
    root_fd: int,
    components: list[str],
    expected: tuple[tuple[int, int, int], ...],
    *,
    race_code: str,
    ops: FileOps,
    close_code: str = "E_REGEN_CLOSE",
) -> None:
    try:
        replay = _open_parent(root_fd, components, ops=ops)
    except PolicyFailure as error:
        raise PolicyFailure(
            race_code, error.detail, additional=error.additional
        ) from error
    try:
        observed = tuple(
            (item.metadata.st_dev, item.metadata.st_ino, item.metadata.st_mode)
            for item in replay
        )
        if observed != expected:
            raise PolicyFailure(race_code, "parent chain identity changed")
    finally:
        _close_parent(replay, ops=ops, close_code=close_code)


def read_checked_manifest(
    root_fd: int,
    *,
    ops: FileOps = REAL_FILE_OPS,
) -> tuple[dict[str, Any], bytes]:
    parts = MANIFEST_REL.split("/")
    chain = _open_parent(root_fd, parts[:-1], ops=ops)
    parent = chain[-1]
    parent_snapshot = tuple(directory_tuple(item.metadata) for item in chain)
    parent_identity = tuple(
        (item.metadata.st_dev, item.metadata.st_ino, item.metadata.st_mode)
        for item in chain
    )
    d0 = directory_tuple(parent.metadata)
    try:
        stable = read_regular_at(
            parent.data_fd,
            parts[-1],
            limit=65_536,
            family="E_REGEN_CHECK",
            ops=ops,
            use_openat2=True,
            resolve=RESOLVE_INPUT,
            before_upgrade=lambda: _replay_parent_identity(
                root_fd,
                parts[:-1],
                parent_identity,
                race_code="E_REGEN_CONFINEMENT",
                ops=ops,
                close_code="E_REGEN_CHECK_CLOSE",
            ),
        )
        try:
            text = stable.data.decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise PolicyFailure("E_REGEN_CHECK_UTF8", "checked manifest is not UTF-8") from error
        duplicates: list[str] = []
        def pairs(values: list[tuple[str, Any]]) -> dict[str, Any]:
            result: dict[str, Any] = {}
            for key, value in values:
                if key in result:
                    duplicates.append(key)
                result[key] = value
            return result
        try:
            value = json.loads(
                text,
                object_pairs_hook=pairs,
                parse_constant=lambda token: (_ for _ in ()).throw(ValueError(token)),
            )
        except (ValueError, RecursionError) as error:
            raise PolicyFailure("E_REGEN_CHECK_JSON", f"checked manifest JSON rejected: {error}") from error
        if duplicates:
            raise PolicyFailure("E_REGEN_CHECK_JSON", "checked manifest has duplicate keys")
        value = validate_closed_manifest(value, code="E_REGEN_CHECK_SCHEMA")
        try:
            observation_fd, observation = metadata_observe_at(parent.data_fd, parts[-1], family="E_REGEN_CHECK", ops=ops, use_openat2=True, resolve=RESOLVE_INPUT)
        except PolicyFailure as error:
            raise PolicyFailure("E_REGEN_CHECK_RACE", error.detail, additional=error.additional) from error
        data_fd: int | None = None
        observation_primary: PolicyFailure | None = None
        try:
            if full_tuple(observation) != full_tuple(stable.metadata):
                raise PolicyFailure("E_REGEN_CHECK_RACE", "checked manifest name changed")
            data_fd = upgrade_metadata_fd(observation_fd, observation, family="E_REGEN_CHECK", ops=ops)
            observed = bytearray()
            while True:
                try:
                    chunk = ops.read(data_fd, min(65_536, len(stable.data) + 1 - len(observed)))
                except OSError as error:
                    raise PolicyFailure("E_REGEN_CHECK_READ", f"checked manifest observation read failed: {error}") from error
                if not chunk:
                    break
                observed.extend(chunk)
                if len(observed) > len(stable.data):
                    raise PolicyFailure("E_REGEN_CHECK_RACE", "checked manifest grew")
            try:
                after_observation = ops.fstat(data_fd)
            except OSError as error:
                raise PolicyFailure("E_REGEN_CHECK_RACE", f"checked manifest observation metadata failed: {error}") from error
            if full_tuple(after_observation) != full_tuple(observation):
                raise PolicyFailure("E_REGEN_CHECK_RACE", "checked manifest observation changed")
            if len(observed) < len(stable.data):
                raise PolicyFailure("E_REGEN_CHECK_SHORT_READ", "checked manifest observation ended early")
            if bytes(observed) != stable.data:
                raise PolicyFailure("E_REGEN_CHECK_RACE", "checked manifest bytes changed")
        except PolicyFailure as error:
            observation_primary = error
        finally:
            for descriptor in (data_fd, observation_fd):
                if descriptor is None:
                    continue
                try:
                    ops.close(descriptor)
                except OSError:
                    observation_primary = PolicyFailure("E_REGEN_CHECK_CLOSE", "checked manifest observation close failed") if observation_primary is None else observation_primary.with_additional("E_REGEN_CHECK_CLOSE")
        if observation_primary is not None:
            raise observation_primary
        try:
            parent_after = directory_tuple(ops.fstat(parent.data_fd))
        except OSError as error:
            raise PolicyFailure("E_REGEN_CHECK_RACE", f"checked manifest parent metadata failed: {error}") from error
        if parent_after != d0:
            raise PolicyFailure("E_REGEN_CHECK_RACE", "checked manifest parent changed")
        _replay_parent(
            root_fd,
            parts[:-1],
            parent_snapshot,
            race_code="E_REGEN_CHECK_RACE",
            ops=ops,
            close_code="E_REGEN_CHECK_CLOSE",
        )
        return value, stable.data
    finally:
        _close_parent(chain, ops=ops, close_code="E_REGEN_CHECK_CLOSE")


def read_archive_inventory(
    root_fd: int,
    *,
    ops: FileOps = REAL_FILE_OPS,
    limits: ArchiveLimits = PRODUCTION_ARCHIVE_LIMITS,
) -> ArchiveInventory:
    parts = ARCHIVE_REL.split("/")
    chain = _open_parent(root_fd, parts[:-1], ops=ops)
    parent = chain[-1]
    parent_snapshot = tuple(directory_tuple(item.metadata) for item in chain)
    parent_identity = tuple(
        (item.metadata.st_dev, item.metadata.st_ino, item.metadata.st_mode)
        for item in chain
    )
    b0 = directory_tuple(parent.metadata)
    metadata_fd: int | None = None
    data_fd: int | None = None
    primary: PolicyFailure | None = None
    inventory: ArchiveInventory | None = None
    initial: os.stat_result | None = None
    try:
        metadata_fd, initial = metadata_observe_at(parent.data_fd, parts[-1], family="E_REGEN_ARCHIVE", ops=ops, use_openat2=True, resolve=RESOLVE_INPUT)
        _replay_parent_identity(
            root_fd,
            parts[:-1],
            parent_identity,
            race_code="E_REGEN_CONFINEMENT",
            ops=ops,
            close_code="E_REGEN_ARCHIVE_CLOSE",
        )
        data_fd = upgrade_metadata_fd(metadata_fd, initial, family="E_REGEN_ARCHIVE", ops=ops)
        if initial.st_size != ARCHIVE_BYTES or initial.st_size > limits.compressed:
            raise PolicyFailure("E_REGEN_ARCHIVE_SIZE", "archive size differs from official identity")
        digest = hashlib.sha256()
        total = 0
        while total <= initial.st_size:
            try:
                chunk = ops.read(data_fd, min(65_536, initial.st_size + 1 - total))
            except OSError as error:
                raise PolicyFailure("E_REGEN_ARCHIVE_IDENTITY_READ", f"archive identity read failed: {error}") from error
            if not chunk:
                break
            total += len(chunk)
            digest.update(chunk)
            if total > initial.st_size:
                raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive grew during identity read")
        try:
            after_identity = ops.fstat(data_fd)
        except OSError as error:
            raise PolicyFailure("E_REGEN_ARCHIVE_METADATA", f"archive identity metadata failed: {error}") from error
        if full_tuple(after_identity) != full_tuple(initial):
            raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive changed during identity read")
        if total < initial.st_size:
            raise PolicyFailure("E_REGEN_ARCHIVE_IDENTITY_SHORT_READ", "archive identity read ended early")
        if digest.hexdigest() != ARCHIVE_SHA256:
            raise PolicyFailure("E_REGEN_ARCHIVE_HASH", "archive SHA-256 differs")
        try:
            offset = ops.lseek(data_fd, 0, os.SEEK_SET)
        except OSError as error:
            raise PolicyFailure("E_REGEN_ARCHIVE_SEEK", f"archive seek failed: {error}") from error
        if offset != 0:
            raise PolicyFailure("E_REGEN_ARCHIVE_SEEK", "archive seek returned a nonzero offset")
        try:
            inventory = parse_archive_descriptor(
                data_fd,
                ops=ops,
                limits=limits,
                expected_compressed=initial.st_size,
            )
        except PolicyFailure as error:
            if error.code != "E_REGEN_ARCHIVE_PARSER_SHORT_READ":
                raise
            try:
                short_metadata = ops.fstat(data_fd)
            except OSError as metadata_error:
                raise PolicyFailure("E_REGEN_ARCHIVE_METADATA", f"archive parser metadata failed: {metadata_error}") from metadata_error
            if full_tuple(short_metadata) != full_tuple(initial):
                raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive changed during parser short read") from error
            raise
        if inventory.compressed_bytes != initial.st_size:
            raise PolicyFailure("E_REGEN_ARCHIVE_PARSER_SHORT_READ", "parser did not consume exact physical archive bytes")
        try:
            after_parser = ops.fstat(data_fd)
        except OSError as error:
            raise PolicyFailure("E_REGEN_ARCHIVE_METADATA", f"archive parser metadata failed: {error}") from error
        if full_tuple(after_parser) != full_tuple(initial):
            raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive changed during parser pass")
    except PolicyFailure as error:
        primary = error
    finally:
        for fd in (data_fd, metadata_fd):
            if fd is None:
                continue
            try:
                ops.close(fd)
            except OSError:
                primary = PolicyFailure("E_REGEN_ARCHIVE_CLOSE", "archive descriptor close failed") if primary is None else primary.with_additional("E_REGEN_ARCHIVE_CLOSE")
    if primary is not None:
        try:
            _close_parent(chain, ops=ops, close_code="E_REGEN_ARCHIVE_CLOSE")
        except PolicyFailure:
            primary = primary.with_additional("E_REGEN_ARCHIVE_CLOSE")
        raise primary
    assert inventory is not None and initial is not None
    try:
        try:
            observation_fd, observation = metadata_observe_at(parent.data_fd, parts[-1], family="E_REGEN_ARCHIVE", ops=ops, use_openat2=True, resolve=RESOLVE_INPUT)
        except PolicyFailure as error:
            raise PolicyFailure(
                "E_REGEN_ARCHIVE_RACE",
                error.detail,
                additional=error.additional,
            ) from error
        observation_primary: PolicyFailure | None = None
        try:
            if full_tuple(observation) != full_tuple(initial):
                raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive final name changed")
        except PolicyFailure as error:
            observation_primary = error
        finally:
            try:
                ops.close(observation_fd)
            except OSError:
                observation_primary = PolicyFailure("E_REGEN_ARCHIVE_CLOSE", "archive observation close failed") if observation_primary is None else observation_primary.with_additional("E_REGEN_ARCHIVE_CLOSE")
        if observation_primary is not None:
            raise observation_primary
        try:
            parent_after = directory_tuple(ops.fstat(parent.data_fd))
        except OSError as error:
            raise PolicyFailure("E_REGEN_ARCHIVE_RACE", f"archive parent metadata failed: {error}") from error
        if parent_after != b0:
            raise PolicyFailure("E_REGEN_ARCHIVE_RACE", "archive parent changed")
        _replay_parent(
            root_fd,
            parts[:-1],
            parent_snapshot,
            race_code="E_REGEN_ARCHIVE_RACE",
            ops=ops,
            close_code="E_REGEN_ARCHIVE_CLOSE",
        )
        return inventory
    finally:
        _close_parent(chain, ops=ops, close_code="E_REGEN_ARCHIVE_CLOSE")


def manifest_object(inventory: ArchiveInventory) -> dict[str, Any]:
    return {
        "records": [
            {
                "cache_home_paths": list(CANDIDATE_HOMES),
                "directories": list(inventory.directories),
                "distribution": {
                    "archive_bytes": ARCHIVE_BYTES,
                    "archive_name": ARCHIVE_NAME,
                    "archive_root": ARCHIVE_ROOT,
                    "archive_sha256": ARCHIVE_SHA256,
                    "library_member_prefix": LIBRARY_PREFIX,
                    "original_bytes_definition": ORIGINAL_BYTES_DEFINITION,
                },
                "files": list(inventory.files),
                "library_relative_path": "library",
                "ownership_class": "upstream-tool-distribution-cache",
                "rationale": "Declared upstream tool library bytes are outside the first-party lexical no-unsafe policy; this is not a safety claim about the bytes.",
                "record_id": "kani-0.67.0-aarch64-unknown-linux-gnu-library",
                "source_owner": "model-checking/kani",
                "tool": {"name": "kani", "target": "aarch64-unknown-linux-gnu", "version": "0.67.0"},
            }
        ],
        "schema": "axiograph-no-unsafe-external-cache-manifest-v1",
    }


def deterministic_json(value: dict[str, Any]) -> bytes:
    data = (json.dumps(value, ensure_ascii=True, allow_nan=False, indent=2, sort_keys=True) + "\n").encode("utf-8")
    if len(data) > 65_536:
        raise PolicyFailure("E_REGEN_OUTPUT_SIZE", "generated manifest exceeds 65,536 bytes")
    return data


def _validate_absolute(path: str, root: str, expected_relative: str | None = None) -> str:
    try:
        encoded = path.encode("ascii")
    except UnicodeEncodeError as error:
        raise PolicyFailure("E_REGEN_PATH", "CLI path is not ASCII") from error
    if not 0 < len(encoded) <= 4096 or not path.startswith("/") or path.startswith("//") or path.endswith("/") or "//" in path or "\\" in path or "\x00" in path:
        raise PolicyFailure("E_REGEN_PATH", "CLI path grammar rejected")
    parts = path.split("/")[1:]
    if any(part in ("", ".", "..") for part in parts):
        raise PolicyFailure("E_REGEN_PATH", "CLI path component rejected")
    prefix = root + "/"
    if not path.startswith(prefix):
        raise PolicyFailure("E_REGEN_PATH", "CLI path is outside the bound root")
    relative = path[len(prefix) :]
    if expected_relative is not None and relative != expected_relative:
        raise PolicyFailure("E_REGEN_PATH", "CLI path differs from fixed location")
    return relative


def _parse_args(argv: list[str]) -> dict[str, str]:
    if len(argv) != 6:
        raise PolicyFailure("E_REGEN_ARGS", "exactly three option/value pairs are required")
    result: dict[str, str] = {}
    for index in range(0, len(argv), 2):
        option = argv[index]
        if option not in ("--archive", "--out", "--check-manifest") or option in result:
            raise PolicyFailure("E_REGEN_ARGS", "unknown, repeated, or positional argument")
        result[option] = argv[index + 1]
    if set(result) != {"--archive", "--out", "--check-manifest"}:
        raise PolicyFailure("E_REGEN_ARGS", "all fixed options are required")
    return result


def write_output(
    root_fd: int,
    relative: str,
    data: bytes,
    *,
    ops: FileOps = REAL_IO_OPS,
) -> None:
    parts = relative.split("/")
    chain = _open_parent(root_fd, parts[:-1], ops=ops)
    parent = chain[-1]
    name = parts[-1]
    parent_identity = tuple(
        (item.metadata.st_dev, item.metadata.st_ino, item.metadata.st_mode)
        for item in chain
    )
    created_fd: int | None = None
    post_create = False
    try:
        _replay_parent_identity(
            root_fd,
            parts[:-1],
            parent_identity,
            race_code="E_REGEN_CONFINEMENT",
            ops=ops,
        )
        try:
            probe = ops.openat2(parent.data_fd, name, O_PATH | O_CLOEXEC | O_NOFOLLOW, 0, RESOLVE_INPUT)
        except OSError as error:
            if error.errno == errno.ELOOP:
                raise PolicyFailure("E_REGEN_OUTPUT_EXISTS", "fixed output name already exists") from error
            if error.errno != errno.ENOENT:
                raise PolicyFailure("E_REGEN_OUTPUT_PROBE", f"output probe failed: {error}") from error
        else:
            try:
                ops.close(probe)
            except OSError as error:
                raise PolicyFailure(
                    "E_REGEN_OUTPUT_EXISTS",
                    "fixed output exists",
                    additional=("E_REGEN_OUTPUT_PROBE",),
                ) from error
            raise PolicyFailure("E_REGEN_OUTPUT_EXISTS", "fixed output name already exists")
        _replay_parent_identity(
            root_fd,
            parts[:-1],
            parent_identity,
            race_code="E_REGEN_CONFINEMENT",
            ops=ops,
        )
        try:
            entries = ops.iter_directory(parent.data_fd, max_bytes=65_536)
            first_entry = next(entries, None)
        except OSError as error:
            raise PolicyFailure("E_REGEN_CONFINEMENT", f"output parent enumeration failed: {error}") from error
        if first_entry is not None:
            raise PolicyFailure("E_REGEN_CONFINEMENT", "output parent contains another entry")
        _replay_parent_identity(
            root_fd,
            parts[:-1],
            parent_identity,
            race_code="E_REGEN_CONFINEMENT",
            ops=ops,
        )
        try:
            created_fd = ops.openat2(
                parent.data_fd,
                name,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL | O_CLOEXEC | O_NOFOLLOW,
                0o600,
                RESOLVE_INPUT,
            )
        except OSError as error:
            code = "E_REGEN_OUTPUT_EXISTS" if error.errno == errno.EEXIST else "E_REGEN_OUTPUT_CREATE"
            raise PolicyFailure(code, f"exclusive output create failed: {error}") from error
        post_create = True
        offset = 0
        while offset < len(data):
            try:
                count = ops.write(created_fd, data[offset:])
            except InterruptedError:
                continue
            except OSError as error:
                raise PolicyFailure("E_REGEN_WRITE", f"output write failed: {error}") from error
            if count <= 0 or count > len(data) - offset:
                raise PolicyFailure("E_REGEN_SHORT_WRITE", "output write made invalid progress")
            offset += count
        try:
            ops.fsync(created_fd)
            created = ops.fstat(created_fd)
        except OSError as error:
            raise PolicyFailure("E_REGEN_FSYNC", f"output fsync/fstat failed: {error}") from error
        if not stat.S_ISREG(created.st_mode) or created.st_size != len(data):
            raise PolicyFailure("E_REGEN_RACE", "created output metadata differs")
        try:
            ops.close(created_fd)
        except OSError as error:
            raise PolicyFailure("E_REGEN_CLOSE", f"created output close failed: {error}") from error
        created_fd = None
        try:
            stable = read_regular_at(parent.data_fd, name, limit=65_536, family="E_REGEN", ops=ops, use_openat2=True, resolve=RESOLVE_INPUT)
        except PolicyFailure as error:
            if error.code == "E_REGEN_CLOSE":
                raise
            raise PolicyFailure("E_REGEN_RACE", error.detail, additional=error.additional) from error
        if full_tuple(stable.metadata) != full_tuple(created) or stable.data != data:
            raise PolicyFailure("E_REGEN_RACE", "first output verification differs")
        try:
            ops.fsync(parent.data_fd)
        except OSError as error:
            raise PolicyFailure("E_REGEN_FSYNC", f"output parent fsync failed: {error}") from error
        try:
            parent_snapshot = tuple(directory_tuple(ops.fstat(item.data_fd)) for item in chain)
            p0 = directory_tuple(ops.fstat(parent.data_fd))
        except OSError as error:
            raise PolicyFailure("E_REGEN_RACE", f"output parent metadata failed: {error}") from error
        _replay_parent(
            root_fd,
            parts[:-1],
            parent_snapshot,
            race_code="E_REGEN_RACE",
            ops=ops,
        )
        try:
            final_fd, final_metadata = metadata_observe_at(parent.data_fd, name, family="E_REGEN", ops=ops, use_openat2=True, resolve=RESOLVE_INPUT)
        except PolicyFailure as error:
            if error.code == "E_REGEN_CLOSE":
                raise
            raise PolicyFailure("E_REGEN_RACE", error.detail, additional=error.additional) from error
        final_data_fd: int | None = None
        final_primary: PolicyFailure | None = None
        try:
            if full_tuple(final_metadata) != full_tuple(created):
                raise PolicyFailure("E_REGEN_RACE", "final output name differs")
            try:
                final_data_fd = upgrade_metadata_fd(final_fd, final_metadata, family="E_REGEN", ops=ops)
            except PolicyFailure as error:
                raise PolicyFailure("E_REGEN_RACE", error.detail, additional=error.additional) from error
            observed = bytearray()
            while len(observed) <= len(data):
                try:
                    chunk = ops.read(final_data_fd, min(65_536, len(data) + 1 - len(observed)))
                except OSError as error:
                    raise PolicyFailure("E_REGEN_RACE", f"final output read failed: {error}") from error
                if not chunk:
                    break
                observed.extend(chunk)
            try:
                final_after = ops.fstat(final_data_fd)
            except OSError as error:
                raise PolicyFailure("E_REGEN_RACE", f"final output metadata failed: {error}") from error
            if bytes(observed) != data or full_tuple(final_after) != full_tuple(created):
                raise PolicyFailure("E_REGEN_RACE", "final output bytes or metadata differ")
        except PolicyFailure as error:
            final_primary = error
        finally:
            for fd in (final_data_fd, final_fd):
                if fd is None:
                    continue
                try:
                    ops.close(fd)
                except OSError:
                    final_primary = PolicyFailure("E_REGEN_CLOSE", "final output close failed") if final_primary is None else final_primary.with_additional("E_REGEN_CLOSE")
        if final_primary is not None:
            raise final_primary
        try:
            final_entries = sorted(
                ops.iter_directory(parent.data_fd, max_bytes=65_536)
            )
            p1 = directory_tuple(ops.fstat(parent.data_fd))
        except OSError as error:
            raise PolicyFailure("E_REGEN_RACE", f"final output parent observation failed: {error}") from error
        if final_entries != [name]:
            raise PolicyFailure("E_REGEN_RACE", "output parent contains an unexpected entry")
        if p1 != p0:
            raise PolicyFailure("E_REGEN_RACE", "output parent changed")
        _replay_parent(
            root_fd,
            parts[:-1],
            parent_snapshot,
            race_code="E_REGEN_RACE",
            ops=ops,
        )
    except PolicyFailure as error:
        if created_fd is not None:
            try:
                ops.close(created_fd)
            except OSError:
                error = error.with_additional("E_REGEN_CLOSE")
        if post_create:
            error = error.with_additional("E_REGEN_RESIDUE")
        raise error  # noqa: TRY201 - raise the copy with residue/close causes
    finally:
        active = sys.exc_info()[1]
        try:
            _close_parent(chain, ops=ops)
        except PolicyFailure as close_error:
            if isinstance(active, PolicyFailure):
                active.additional = (*active.additional, "E_REGEN_CLOSE")
            else:
                if post_create:
                    close_error = close_error.with_additional("E_REGEN_RESIDUE")
                raise close_error  # noqa: TRY201 - raise the copy with residue


def generate(argv: list[str], *, file_ops: FileOps = REAL_FILE_OPS, io_ops: FileOps = REAL_IO_OPS) -> dict[str, Any]:
    arguments = _parse_args(argv)
    file_ops.require_supported(
        "E_REGEN_FS", require_openat2=True, openat2_resolve=RESOLVE_INPUT
    )
    if io_ops is not file_ops:
        io_ops.require_supported(
            "E_REGEN_FS", require_openat2=True, openat2_resolve=RESOLVE_INPUT
        )
    try:
        bound = bind_invocation(
            sys.argv[0],
            EXPECTED_BASENAME,
            ops=file_ops,
            unsupported_family="E_REGEN_FS",
        )
    except PolicyFailure as error:
        if error.code == "E_REGEN_FS_UNSUPPORTED":
            raise
        raise PolicyFailure("E_REGEN_ROOT_BINDING", error.detail, additional=error.additional) from error
    try:
        archive_rel = _validate_absolute(arguments["--archive"], bound.path, ARCHIVE_REL)
        check_rel = _validate_absolute(arguments["--check-manifest"], bound.path, MANIFEST_REL)
        del archive_rel, check_rel
        output_rel = _validate_absolute(arguments["--out"], bound.path)
        match = OUTPUT_RE.fullmatch(output_rel)
        if match is None or match.group(1) in (".", "..") or match.group(2) in (".", ".."):
            raise PolicyFailure("E_REGEN_PATH", "output path does not match fixed grammar")
        inventory = read_archive_inventory(bound.directory.data_fd, ops=file_ops)
        generated = manifest_object(inventory)
        checked, _ = read_checked_manifest(bound.directory.data_fd, ops=file_ops)
        if generated != checked:
            raise PolicyFailure("E_REGEN_COMPARE", "generated manifest differs from checked manifest")
        output = deterministic_json(generated)
        write_output(bound.directory.data_fd, output_rel, output, ops=io_ops)
        return {
            "schema": "axiograph-no-unsafe-regeneration-report-v1",
            "archive_bytes": inventory.compressed_bytes,
            "decompressed_bytes": inventory.decompressed_bytes,
            "members": inventory.member_count,
            "directories": len(inventory.directories),
            "files": len(inventory.files),
            "output_bytes": len(output),
            "output_sha256": hashlib.sha256(output).hexdigest(),
            "output": arguments["--out"],
        }
    finally:
        bound.close(file_ops, "E_REGEN_FS_CLOSE")


def main() -> int:
    try:
        report = generate(sys.argv[1:])
    except PolicyFailure as error:
        print(f"[{error.code}] {error.detail}", file=sys.stderr)
        for code in error.additional:
            print(f"[{code}] additional failure", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"[E_REGEN_FS_UNSUPPORTED] unclassified required filesystem operation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
