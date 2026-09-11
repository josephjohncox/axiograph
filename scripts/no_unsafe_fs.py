"""Linux type-first descriptor helpers for the no-unsafe policy tools.

This module is deliberately small and has no command-line surface. Production
callers use ``REAL_FILE_OPS``. Tests may pass private operation objects directly
to the same helpers to force otherwise non-portable failures.
"""

from __future__ import annotations

import ctypes
import errno
import os
import platform
import stat
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Protocol

O_PATH = getattr(os, "O_PATH", 0)
O_CLOEXEC = getattr(os, "O_CLOEXEC", 0)
O_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
O_NONBLOCK = getattr(os, "O_NONBLOCK", 0)
O_DIRECTORY = getattr(os, "O_DIRECTORY", 0)

RESOLVE_NO_XDEV = 0x01
RESOLVE_NO_MAGICLINKS = 0x02
RESOLVE_NO_SYMLINKS = 0x04
RESOLVE_BENEATH = 0x08
SYS_OPENAT2 = 437
SYS_GETDENTS64 = 61 if platform.machine() in {"aarch64", "arm64", "riscv64"} else 217
DIRECTORY_READ_CHUNK = 65_536


class PolicyFailure(RuntimeError):
    """One primary policy code plus optional close/residue diagnostics."""

    def __init__(self, code: str, detail: str, *, additional: tuple[str, ...] = ()):
        super().__init__(f"{code}: {detail}")
        self.code = code
        self.detail = detail
        self.additional = additional

    def with_additional(self, code: str) -> PolicyFailure:
        if code in self.additional:
            return self
        return PolicyFailure(self.code, self.detail, additional=(*self.additional, code))


class FileOps(Protocol):
    def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int: ...
    def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int: ...
    def open_portal(self, metadata_fd: int, flags: int) -> int: ...
    def fstat(self, fd: int) -> os.stat_result: ...
    def read(self, fd: int, count: int) -> bytes: ...
    def write(self, fd: int, data: bytes) -> int: ...
    def lseek(self, fd: int, offset: int, whence: int) -> int: ...
    def fsync(self, fd: int) -> None: ...
    def iter_directory(self, fd: int, *, max_bytes: int) -> Iterator[str]: ...
    def require_supported(
        self,
        family: str,
        *,
        require_openat2: bool = False,
        openat2_resolve: int | None = None,
    ) -> None: ...
    def close(self, fd: int) -> None: ...


class _OpenHow(ctypes.Structure):
    _fields_ = [
        ("flags", ctypes.c_uint64),
        ("mode", ctypes.c_uint64),
        ("resolve", ctypes.c_uint64),
    ]


class RealFileOps:
    """The only operation implementation reachable from production main paths."""

    def __init__(self) -> None:
        self._libc = ctypes.CDLL(None, use_errno=True)

    def require_supported(
        self,
        family: str,
        *,
        require_openat2: bool = False,
        openat2_resolve: int | None = None,
    ) -> None:
        """Probe every Linux primitive and exact resolve set used by the caller."""
        code = f"{family}_UNSUPPORTED"
        if not all((O_PATH, O_CLOEXEC, O_NOFOLLOW, O_NONBLOCK, O_DIRECTORY)):
            raise PolicyFailure(code, "required Linux open flags unavailable")
        metadata_fd: int | None = None
        data_fd: int | None = None
        probe_fd: int | None = None
        try:
            metadata_fd = os.open("/", O_PATH | O_CLOEXEC | O_NOFOLLOW)
            metadata = os.fstat(metadata_fd)
            data_fd = self.open_portal(
                metadata_fd,
                os.O_RDONLY | O_DIRECTORY | O_NONBLOCK | O_CLOEXEC,
            )
            if full_tuple(os.fstat(data_fd)) != full_tuple(metadata):
                raise OSError(errno.ENOTSUP, "descriptor portal changed identity")
            # Exercise bounded incremental directory enumeration as part of the
            # primitive set; failure must not be reclassified as an ordinary
            # traversal or output operation.
            next(self.iter_directory(data_fd, max_bytes=DIRECTORY_READ_CHUNK), None)
            if require_openat2 or openat2_resolve is not None:
                resolve = (
                    openat2_resolve
                    if openat2_resolve is not None
                    else RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS
                )
                probe_fd = self.openat2(
                    data_fd,
                    ".",
                    O_PATH | O_CLOEXEC | O_NOFOLLOW,
                    0,
                    resolve,
                )
                if not stat.S_ISDIR(os.fstat(probe_fd).st_mode):
                    raise OSError(errno.ENOTSUP, "openat2 directory probe changed type")
        except (OSError, AttributeError) as error:
            raise PolicyFailure(code, f"required Linux filesystem primitive unavailable: {error}") from error
        finally:
            for descriptor in (probe_fd, data_fd, metadata_fd):
                if descriptor is not None:
                    try:
                        os.close(descriptor)
                    except OSError as error:
                        raise PolicyFailure(code, f"filesystem primitive probe close failed: {error}") from error

    def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
        return os.open(name, flags, mode, dir_fd=parent_fd)

    def openat2(
        self, parent_fd: int, name: str, flags: int, mode: int, resolve: int
    ) -> int:
        encoded = os.fsencode(name)
        if b"\x00" in encoded:
            raise OSError(errno.EINVAL, "NUL in openat2 name")
        how = _OpenHow(flags=flags, mode=mode, resolve=resolve)
        result = self._libc.syscall(
            SYS_OPENAT2,
            ctypes.c_int(parent_fd),
            ctypes.c_char_p(encoded),
            ctypes.byref(how),
            ctypes.sizeof(how),
        )
        if result < 0:
            number = ctypes.get_errno()
            raise OSError(number, os.strerror(number), name)
        return int(result)

    def open_portal(self, metadata_fd: int, flags: int) -> int:
        return os.open(f"/proc/self/fd/{metadata_fd}", flags)

    def fstat(self, fd: int) -> os.stat_result:
        return os.fstat(fd)

    def read(self, fd: int, count: int) -> bytes:
        return os.read(fd, count)

    def write(self, fd: int, data: bytes) -> int:
        return os.write(fd, data)

    def lseek(self, fd: int, offset: int, whence: int) -> int:
        return os.lseek(fd, offset, whence)

    def fsync(self, fd: int) -> None:
        os.fsync(fd)

    def iter_directory(self, fd: int, *, max_bytes: int) -> Iterator[str]:
        """Yield Linux directory names under a charged raw getdents64 budget."""
        if max_bytes < 0:
            raise OSError(errno.EOVERFLOW, "negative directory byte budget")
        os.lseek(fd, 0, os.SEEK_SET)
        charged = 0
        while True:
            # The fixed allocation is bounded independently of max_bytes. A
            # full-size call is required to distinguish exact-budget EOF from
            # a next record that would exceed the remaining charged budget.
            buffer = ctypes.create_string_buffer(DIRECTORY_READ_CHUNK)
            count = self._libc.syscall(
                SYS_GETDENTS64,
                ctypes.c_int(fd),
                ctypes.byref(buffer),
                ctypes.c_uint(DIRECTORY_READ_CHUNK),
            )
            if count < 0:
                number = ctypes.get_errno()
                raise OSError(number, os.strerror(number))
            if count == 0:
                return
            if charged + int(count) > max_bytes:
                raise OSError(errno.EOVERFLOW, "directory enumeration byte budget exceeded")
            charged += int(count)
            raw = buffer.raw[:count]
            offset = 0
            while offset < count:
                if offset + 19 > count:
                    raise OSError(errno.EIO, "truncated getdents64 record")
                reclen = int.from_bytes(raw[offset + 16 : offset + 18], "little")
                if reclen < 19 or offset + reclen > count:
                    raise OSError(errno.EIO, "invalid getdents64 record length")
                field = raw[offset + 19 : offset + reclen]
                nul = field.find(b"\0")
                if nul < 0:
                    raise OSError(errno.EIO, "unterminated getdents64 name")
                name_raw = field[:nul]
                if name_raw not in (b".", b".."):
                    yield os.fsdecode(name_raw)
                offset += reclen

    def close(self, fd: int) -> None:
        os.close(fd)


REAL_FILE_OPS = RealFileOps()
REAL_IO_OPS = REAL_FILE_OPS


def full_tuple(value: os.stat_result) -> tuple[int, int, int, int, int, int]:
    return (
        value.st_dev,
        value.st_ino,
        value.st_mode,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
    )


def directory_tuple(value: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        value.st_dev,
        value.st_ino,
        value.st_mode,
        value.st_mtime_ns,
        value.st_ctime_ns,
    )


def _close(
    fd: int | None,
    ops: FileOps,
    close_code: str,
    primary: PolicyFailure | None,
) -> PolicyFailure | None:
    if fd is None:
        return primary
    try:
        ops.close(fd)
    except OSError as error:
        if primary is None:
            return PolicyFailure(close_code, f"descriptor close failed: {error}")
        return primary.with_additional(close_code)
    return primary


@dataclass(frozen=True)
class HeldDirectory:
    metadata_fd: int
    data_fd: int
    metadata: os.stat_result

    def close(self, ops: FileOps = REAL_FILE_OPS, code: str = "E_FS_CLOSE") -> None:
        active = sys.exc_info()[1]
        seed = active if isinstance(active, PolicyFailure) else None
        primary = _close(self.data_fd, ops, code, seed)
        primary = _close(self.metadata_fd, ops, code, primary)
        if primary is not None and primary is not active:
            raise primary


def close_descriptor(
    fd: int,
    *,
    family: str,
    ops: FileOps = REAL_FILE_OPS,
) -> None:
    """Close one descriptor while retaining an active policy failure."""
    active = sys.exc_info()[1]
    seed = active if isinstance(active, PolicyFailure) else None
    primary = _close(fd, ops, f"{family}_CLOSE", seed)
    if primary is not None and primary is not active:
        raise primary


@dataclass(frozen=True)
class StableBytes:
    data: bytes
    metadata: os.stat_result


def _metadata_open(
    parent_fd: int,
    name: str,
    *,
    ops: FileOps,
    open_code: str,
    link_code: str,
    use_openat2: bool,
    resolve: int,
) -> int:
    try:
        if use_openat2:
            return ops.openat2(
                parent_fd,
                name,
                O_PATH | O_CLOEXEC | O_NOFOLLOW,
                0,
                resolve,
            )
        return ops.openat(parent_fd, name, O_PATH | O_CLOEXEC | O_NOFOLLOW)
    except OSError as error:
        if error.errno == errno.ELOOP:
            raise PolicyFailure(link_code, f"final component is a link: {name}") from error
        raise PolicyFailure(open_code, f"metadata open failed for {name}: {error}") from error


def metadata_observe_at(
    parent_fd: int,
    name: str,
    *,
    family: str,
    ops: FileOps = REAL_FILE_OPS,
    expected: str = "regular",
    use_openat2: bool = False,
    resolve: int = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
) -> tuple[int, os.stat_result]:
    """Open and classify a final component without opening its data path."""
    open_code = f"{family}_OPEN"
    link_code = f"{family}_LINK"
    metadata_code = f"{family}_METADATA"
    nonregular_code = f"{family}_NONREGULAR"
    fd = _metadata_open(
        parent_fd,
        name,
        ops=ops,
        open_code=open_code,
        link_code=link_code,
        use_openat2=use_openat2,
        resolve=resolve,
    )
    try:
        try:
            value = ops.fstat(fd)
        except OSError as error:
            raise PolicyFailure(metadata_code, f"metadata fstat failed for {name}: {error}") from error
        if stat.S_ISLNK(value.st_mode):
            raise PolicyFailure(link_code, f"final component is a link: {name}")
        right_type = stat.S_ISREG(value.st_mode) if expected == "regular" else stat.S_ISDIR(value.st_mode)
        if not right_type:
            raise PolicyFailure(nonregular_code, f"final component is not {expected}: {name}")
        return fd, value
    except PolicyFailure as primary:
        closed = _close(fd, ops, f"{family}_CLOSE", primary)
        assert closed is not None
        raise closed


def upgrade_metadata_fd(
    metadata_fd: int,
    metadata: os.stat_result,
    *,
    family: str,
    ops: FileOps = REAL_FILE_OPS,
    expected: str = "regular",
) -> int:
    flags = os.O_RDONLY | O_NONBLOCK | O_CLOEXEC
    if expected == "directory":
        flags |= O_DIRECTORY
    try:
        data_fd = ops.open_portal(metadata_fd, flags)
    except OSError as error:
        raise PolicyFailure(f"{family}_DATA_OPEN", f"held-inode data upgrade failed: {error}") from error
    try:
        try:
            upgraded = ops.fstat(data_fd)
        except OSError as error:
            raise PolicyFailure(f"{family}_METADATA", f"upgraded descriptor fstat failed: {error}") from error
        if full_tuple(upgraded) != full_tuple(metadata):
            raise PolicyFailure(f"{family}_RACE", "metadata/data descriptor identity differs")
        right_type = stat.S_ISREG(upgraded.st_mode) if expected == "regular" else stat.S_ISDIR(upgraded.st_mode)
        if not right_type:
            raise PolicyFailure(f"{family}_RACE", "upgraded descriptor type differs")
        return data_fd
    except PolicyFailure as primary:
        closed = _close(data_fd, ops, f"{family}_CLOSE", primary)
        assert closed is not None
        raise closed


def open_directory_at(
    parent_fd: int,
    name: str,
    *,
    family: str = "E_FS",
    ops: FileOps = REAL_FILE_OPS,
    use_openat2: bool = False,
    resolve: int = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
) -> HeldDirectory:
    metadata_fd, metadata = metadata_observe_at(
        parent_fd,
        name,
        family=family,
        ops=ops,
        expected="directory",
        use_openat2=use_openat2,
        resolve=resolve,
    )
    try:
        data_fd = upgrade_metadata_fd(
            metadata_fd, metadata, family=family, ops=ops, expected="directory"
        )
    except PolicyFailure as primary:
        closed = _close(metadata_fd, ops, f"{family}_CLOSE", primary)
        assert closed is not None
        raise closed
    return HeldDirectory(metadata_fd, data_fd, metadata)


def open_root_directory(*, family: str = "E_FS", ops: FileOps = REAL_FILE_OPS) -> HeldDirectory:
    metadata_fd: int | None = None
    try:
        metadata_fd = ops.openat(
            getattr(os, "AT_FDCWD", -100), "/", O_PATH | O_CLOEXEC
        )
        metadata = ops.fstat(metadata_fd)
    except (OSError, PolicyFailure) as error:
        primary = PolicyFailure(f"{family}_UNSUPPORTED", f"cannot establish root descriptor: {error}")
        if metadata_fd is not None:
            closed = _close(metadata_fd, ops, f"{family}_CLOSE", primary)
            assert closed is not None
            primary = closed
        raise primary from error
    if not stat.S_ISDIR(metadata.st_mode):
        primary = PolicyFailure(f"{family}_UNSUPPORTED", "root descriptor is not a directory")
        closed = _close(metadata_fd, ops, f"{family}_CLOSE", primary)
        assert closed is not None
        raise closed
    try:
        data_fd = upgrade_metadata_fd(
            metadata_fd, metadata, family=family, ops=ops, expected="directory"
        )
    except PolicyFailure as primary:
        closed = _close(metadata_fd, ops, f"{family}_CLOSE", primary)
        assert closed is not None
        raise closed
    return HeldDirectory(metadata_fd, data_fd, metadata)


def read_held_regular(
    metadata_fd: int,
    metadata: os.stat_result,
    name: str,
    *,
    limit: int,
    family: str,
    ops: FileOps = REAL_FILE_OPS,
    allow_empty: bool = True,
    before_read: Callable[[os.stat_result], None] | None = None,
) -> StableBytes:
    """Bounded-read the inode held by ``metadata_fd`` without reopening its name.

    The caller retains ownership of the metadata descriptor. The optional
    budget callback runs only after the data descriptor has been proven to be
    the same inode, and before any content allocation or read.
    """
    if limit < 0:
        raise PolicyFailure(f"{family}_SIZE", "negative byte limit")
    data_fd: int | None = None
    primary: PolicyFailure | None = None
    result: StableBytes | None = None
    try:
        data_fd = upgrade_metadata_fd(metadata_fd, metadata, family=family, ops=ops)
        if metadata.st_size < (0 if allow_empty else 1) or metadata.st_size > limit:
            raise PolicyFailure(f"{family}_SIZE", f"size {metadata.st_size} exceeds {limit}")
        if before_read is not None:
            before_read(metadata)
        chunks: list[bytes] = []
        total = 0
        while total <= limit:
            try:
                chunk = ops.read(data_fd, min(65536, limit + 1 - total))
            except OSError as error:
                raise PolicyFailure(f"{family}_READ", f"read failed for {name}: {error}") from error
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > limit or total > metadata.st_size:
                raise PolicyFailure(f"{family}_RACE", f"file grew while reading {name}")
        try:
            after = ops.fstat(data_fd)
        except OSError as error:
            raise PolicyFailure(f"{family}_METADATA", f"post-read fstat failed: {error}") from error
        if full_tuple(after) != full_tuple(metadata):
            raise PolicyFailure(f"{family}_RACE", f"file changed while reading {name}")
        if total < metadata.st_size:
            raise PolicyFailure(f"{family}_SHORT_READ", f"early EOF while reading {name}")
        result = StableBytes(b"".join(chunks), metadata)
    except PolicyFailure as error:
        primary = error
    finally:
        primary = _close(data_fd, ops, f"{family}_CLOSE", primary)
    if primary is not None:
        raise primary
    assert result is not None
    return result


def read_regular_at(
    parent_fd: int,
    name: str,
    *,
    limit: int,
    family: str,
    ops: FileOps = REAL_FILE_OPS,
    use_openat2: bool = False,
    resolve: int = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    allow_empty: bool = True,
    before_upgrade: Callable[[], None] | None = None,
    before_read: Callable[[os.stat_result], None] | None = None,
) -> StableBytes:
    if limit < 0:
        raise PolicyFailure(f"{family}_SIZE", "negative byte limit")
    metadata_fd: int | None = None
    primary: PolicyFailure | None = None
    result: StableBytes | None = None
    try:
        metadata_fd, metadata = metadata_observe_at(
            parent_fd,
            name,
            family=family,
            ops=ops,
            use_openat2=use_openat2,
            resolve=resolve,
        )
        if before_upgrade is not None:
            before_upgrade()
        result = read_held_regular(
            metadata_fd,
            metadata,
            name,
            limit=limit,
            family=family,
            ops=ops,
            allow_empty=allow_empty,
            before_read=before_read,
        )
    except PolicyFailure as error:
        primary = error
    finally:
        primary = _close(metadata_fd, ops, f"{family}_CLOSE", primary)
    if primary is not None:
        raise primary
    assert result is not None
    return result


def walk_directory_components(
    components: list[str],
    *,
    family: str = "E_FS",
    ops: FileOps = REAL_FILE_OPS,
    use_openat2: bool = False,
    resolve: int = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
) -> list[HeldDirectory]:
    chain = [open_root_directory(family=family, ops=ops)]
    try:
        for component in components:
            chain.append(
                open_directory_at(
                    chain[-1].data_fd,
                    component,
                    family=family,
                    ops=ops,
                    use_openat2=use_openat2,
                    resolve=resolve,
                )
            )
        return chain
    except Exception:
        for item in reversed(chain):
            try:
                item.close(ops, f"{family}_CLOSE")
            except PolicyFailure:
                pass
        raise


def close_chain(chain: list[HeldDirectory], *, family: str, ops: FileOps = REAL_FILE_OPS) -> None:
    active = sys.exc_info()[1]
    primary = active if isinstance(active, PolicyFailure) else None
    for item in reversed(chain):
        try:
            item.close(ops, f"{family}_CLOSE")
        except PolicyFailure as error:
            if primary is None or error.code == getattr(active, "code", None):
                primary = error
            else:
                primary = primary.with_additional(error.code)
                for code in error.additional:
                    primary = primary.with_additional(code)
    if primary is not None and primary is not active:
        raise primary
