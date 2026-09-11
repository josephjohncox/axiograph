from __future__ import annotations

import ctypes
import errno
import gzip
import io
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.bounded_subprocess import BoundedProcessError
from scripts.check_no_unsafe import (
    CANDIDATE_HOMES,
    MANIFEST_REL,
    BoundRepository,
    ScanCounters,
    _cargo_metadata,
    _parse_index_paths,
    _parse_tree_paths,
    audit,
    bind_invocation,
    cargo_view,
    scan_candidate,
    scan_sources,
)
from scripts.generate_no_unsafe_external_cache_manifest import (
    ARCHIVE_REL,
    OUTPUT_BASENAME,
    RESOLVE_INPUT,
    ArchiveLimits,
    generate,
    parse_archive_descriptor,
    read_archive_inventory,
    read_checked_manifest,
    write_output,
)
from scripts.no_unsafe_fs import (
    DIRECTORY_READ_CHUNK,
    O_DIRECTORY,
    O_PATH,
    RESOLVE_NO_XDEV,
    SYS_GETDENTS64,
    PolicyFailure,
    RealFileOps,
    metadata_observe_at,
    open_directory_at,
    open_root_directory,
    read_regular_at,
)

REPO = Path(__file__).parents[2]
CANONICAL_MANIFEST = REPO / "scripts/no_unsafe_external_cache_manifest_v1.json"
CACHE_LIBRARY = REPO / CANDIDATE_HOMES[0] / "library"


class CountingOps(RealFileOps):
    def __init__(self) -> None:
        super().__init__()
        self.portal_opens = 0

    def open_portal(self, metadata_fd: int, flags: int) -> int:
        self.portal_opens += 1
        return super().open_portal(metadata_fd, flags)


class InjectedOps(RealFileOps):
    def __init__(self, operation: str) -> None:
        super().__init__()
        self.operation = operation
        self.read_calls = 0

    def open_portal(self, metadata_fd: int, flags: int) -> int:
        if self.operation == "data_open":
            raise OSError(5, "injected data open")
        return super().open_portal(metadata_fd, flags)

    def fstat(self, fd: int):  # type: ignore[no-untyped-def]
        if self.operation == "metadata":
            raise OSError(5, "injected metadata")
        return super().fstat(fd)

    def read(self, fd: int, count: int) -> bytes:
        self.read_calls += 1
        if self.operation == "read":
            raise OSError(5, "injected read")
        if self.operation == "short" and self.read_calls == 1:
            return b""
        return super().read(fd, count)

    def close(self, fd: int) -> None:
        if self.operation == "close":
            self.operation = "closed"
            raise OSError(5, "injected close")
        super().close(fd)


class TypeFirstTests(unittest.TestCase):
    def test_real_fifo_socket_and_device_are_never_data_opened(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = os.open(temporary, os.O_RDONLY | O_DIRECTORY)
            unix_socket = socket.socket(socket.AF_UNIX)
            try:
                os.mkfifo(Path(temporary) / "fifo")
                unix_socket.bind(str(Path(temporary) / "socket"))
                for name in ("fifo", "socket"):
                    ops = CountingOps()
                    with self.assertRaises(PolicyFailure) as caught:
                        metadata_observe_at(parent, name, family="E_SOURCE", ops=ops)
                    self.assertEqual(caught.exception.code, "E_SOURCE_NONREGULAR")
                    self.assertEqual(ops.portal_opens, 0)
            finally:
                unix_socket.close()
                os.close(parent)
        dev = os.open("/dev", os.O_RDONLY | O_DIRECTORY)
        try:
            ops = CountingOps()
            with self.assertRaises(PolicyFailure) as caught:
                metadata_observe_at(dev, "null", family="E_REGEN_ARCHIVE", ops=ops)
            self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_NONREGULAR")
            self.assertEqual(ops.portal_opens, 0)
        finally:
            os.close(dev)

    def test_candidate_device_entry_is_nonregular_before_data_open(self) -> None:
        class CandidateDeviceOps(CountingOps):
            def __init__(self) -> None:
                super().__init__()
                self.target_portals = 0

            def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                if name == "candidate-device.rs":
                    return os.open("/dev/null", O_PATH | os.O_CLOEXEC)
                return super().openat(parent_fd, name, flags, mode)

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                if os.readlink(f"/proc/self/fd/{metadata_fd}").endswith("/candidate-device.rs"):
                    self.target_portals += 1
                return super().open_portal(metadata_fd, flags)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "home/library"
            library.mkdir(parents=True)
            (library / "candidate-device.rs").write_bytes(b"placeholder")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = CandidateDeviceOps()
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    scan_candidate(
                        descriptor,
                        "home",
                        "home",
                        {"directories": [], "files": []},
                        ScanCounters(),
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_FS_NONREGULAR")
                self.assertEqual(ops.target_portals, 0)
                self.assertEqual(ops.portal_opens, 2)
            finally:
                os.close(descriptor)

    def test_link_is_metadata_classified_without_portal_open(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            Path(temporary, "regular").write_bytes(b"x")
            os.symlink("regular", Path(temporary, "link"))
            parent = os.open(temporary, os.O_RDONLY | O_DIRECTORY)
            try:
                ops = CountingOps()
                with self.assertRaises(PolicyFailure) as caught:
                    metadata_observe_at(parent, "link", family="E_FS", ops=ops)
                self.assertEqual(caught.exception.code, "E_FS_LINK")
                self.assertEqual(ops.portal_opens, 0)
            finally:
                os.close(parent)

    def test_operation_codes_are_exact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            Path(temporary, "file").write_bytes(b"abc")
            parent = os.open(temporary, os.O_RDONLY | O_DIRECTORY)
            try:
                for operation, code in (
                    ("metadata", "E_SOURCE_METADATA"),
                    ("data_open", "E_SOURCE_DATA_OPEN"),
                    ("read", "E_SOURCE_READ"),
                    ("short", "E_SOURCE_SHORT_READ"),
                    ("close", "E_SOURCE_CLOSE"),
                ):
                    with self.subTest(operation=operation):
                        with self.assertRaises(PolicyFailure) as caught:
                            read_regular_at(parent, "file", limit=3, family="E_SOURCE", ops=InjectedOps(operation))
                        self.assertEqual(caught.exception.code, code)
            finally:
                os.close(parent)

    def test_held_root_portal_has_exact_identity(self) -> None:
        root = open_root_directory()
        try:
            metadata = os.fstat(root.metadata_fd)
            data = os.fstat(root.data_fd)
            self.assertEqual(metadata, data)
            self.assertEqual(
                (metadata.st_dev, metadata.st_ino, metadata.st_mode, metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns),
                (data.st_dev, data.st_ino, data.st_mode, data.st_size, data.st_mtime_ns, data.st_ctime_ns),
            )
        finally:
            root.close()

        class UnsupportedRootOps(RealFileOps):
            def open_portal(self, metadata_fd: int, flags: int) -> int:
                raise OSError(errno.ENOSYS, "descriptor portal unavailable")

        with self.assertRaises(PolicyFailure) as caught:
            open_root_directory(ops=UnsupportedRootOps())
        self.assertEqual(caught.exception.code, "E_FS_DATA_OPEN")


class TypeFirstOperationTests(unittest.TestCase):
    class FaultOps(RealFileOps):
        def __init__(self, fault: str, other: Path) -> None:
            super().__init__()
            self.fault = fault
            self.other = other
            self.metadata_fd: int | None = None
            self.data_fd: int | None = None
            self.closed_fault = False

        def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
            if name == "input" and self.fault == "open":
                raise OSError(errno.EACCES, "open")
            result = super().openat(parent_fd, name, flags, mode)
            if name == "input":
                self.metadata_fd = result
            return result

        def fstat(self, fd: int) -> os.stat_result:
            if self.fault == "metadata" and fd == self.metadata_fd:
                raise OSError(errno.EIO, "metadata")
            return super().fstat(fd)

        def open_portal(self, metadata_fd: int, flags: int) -> int:
            if self.fault == "data_open":
                raise OSError(errno.EIO, "portal")
            if self.fault == "race":
                return os.open(self.other, os.O_RDONLY | os.O_NONBLOCK)
            self.data_fd = super().open_portal(metadata_fd, flags)
            return self.data_fd

        def read(self, fd: int, count: int) -> bytes:
            if fd == self.data_fd and self.fault == "read":
                raise OSError(errno.EIO, "read")
            if fd == self.data_fd and self.fault == "short":
                return b""
            return super().read(fd, count)

        def close(self, fd: int) -> None:
            if self.fault == "close" and fd == self.data_fd and not self.closed_fault:
                self.closed_fault = True
                super().close(fd)
                raise OSError(errno.EIO, "close")
            super().close(fd)

    def test_regular_reader_operation_causes_are_exact(self) -> None:
        expected = {
            "open": "E_TEST_OPEN",
            "metadata": "E_TEST_METADATA",
            "data_open": "E_TEST_DATA_OPEN",
            "race": "E_TEST_RACE",
            "read": "E_TEST_READ",
            "short": "E_TEST_SHORT_READ",
            "close": "E_TEST_CLOSE",
        }
        for fault, code in expected.items():
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "input").write_bytes(b"1234")
                other = root / "other"
                other.write_bytes(b"5678")
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        read_regular_at(fd, "input", limit=4, family="E_TEST", ops=self.FaultOps(fault, other))
                    self.assertEqual(caught.exception.code, code)
                finally:
                    os.close(fd)


class RootBindingTests(unittest.TestCase):
    def test_stable_real_invocation_copy_binds_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / "scripts"
            scripts.mkdir()
            script = scripts / "check_no_unsafe.py"
            script.write_bytes(b"# stable\n")
            bound = bind_invocation(str(script), "check_no_unsafe.py")
            try:
                self.assertEqual(bound.path, str(root))
            finally:
                bound.close()

    def test_second_name_observation_detects_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / "scripts"
            scripts.mkdir()
            script = scripts / "check_no_unsafe.py"
            script.write_bytes(b"# original\n")

            class ReplaceSecond(RealFileOps):
                def __init__(self) -> None:
                    super().__init__()
                    self.opens = 0

                def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                    if name == "check_no_unsafe.py":
                        self.opens += 1
                        if self.opens == 2:
                            os.rename(name, "old.py", src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
                            fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=parent_fd)
                            os.write(fd, b"# replacement\n")
                            os.close(fd)
                    return super().openat(parent_fd, name, flags, mode)

            with self.assertRaises(PolicyFailure) as caught:
                bind_invocation(str(script), "check_no_unsafe.py", ops=ReplaceSecond())
            self.assertEqual(caught.exception.code, "E_ROOT_BINDING")


class RootCorrectionTests(unittest.TestCase):
    def test_linked_and_malformed_invocation_names_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / "scripts"
            scripts.mkdir()
            actual = scripts / "actual.py"
            shutil.copyfile(REPO / "scripts/check_no_unsafe.py", actual)
            for dependency in ("no_unsafe_fs.py", "bounded_subprocess.py"):
                shutil.copyfile(REPO / "scripts" / dependency, scripts / dependency)
            os.symlink("actual.py", scripts / "check_no_unsafe.py")
            result = subprocess.run(
                ["python3", str(scripts / "check_no_unsafe.py")],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("[E_ROOT_BINDING]", result.stderr)
            self.assertIn("exemptions=0", result.stderr)

            moved = root / "scripts-real"
            os.rename(scripts, moved)
            os.symlink(moved.name, scripts)
            result = subprocess.run(
                ["python3", str(scripts / "check_no_unsafe.py")], cwd=root,
                capture_output=True, text=True, timeout=30, check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("[E_ROOT_BINDING]", result.stderr)
            self.assertIn("exemptions=0", result.stderr)

            launcher = (
                "import scripts.check_no_unsafe as scanner,sys; "
                "sys.argv=[sys.argv[1]]; raise SystemExit(scanner.main())"
            )
            for kind in ("fifo", "directory"):
                with self.subTest(nonregular=kind), tempfile.TemporaryDirectory() as nonregular_temp:
                    candidate = Path(nonregular_temp) / "scripts/check_no_unsafe.py"
                    candidate.parent.mkdir()
                    if kind == "fifo":
                        os.mkfifo(candidate)
                    else:
                        candidate.mkdir()
                    result = subprocess.run(
                        [sys.executable, "-c", launcher, str(candidate)], cwd=REPO,
                        capture_output=True, text=True, timeout=30, check=False,
                    )
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("[E_ROOT_BINDING]", result.stderr)
                    self.assertIn("not regular", result.stderr)
                    self.assertIn("exemptions=0", result.stderr)
        malformed_launcher = (
            "import scripts.check_no_unsafe as scanner,sys; "
            "sys.argv=[sys.argv[1]]; raise SystemExit(scanner.main())"
        )
        for argv0 in ("", "./scripts/check_no_unsafe.py", "scripts/../scripts/check_no_unsafe.py", "bad\\name"):
            with self.subTest(argv0=argv0), self.assertRaises(PolicyFailure) as caught:
                bind_invocation(argv0, "check_no_unsafe.py")
            self.assertEqual(caught.exception.code, "E_ROOT_BINDING")
            result = subprocess.run(
                [sys.executable, "-c", malformed_launcher, argv0], cwd=REPO,
                capture_output=True, text=True, timeout=30, check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("[E_ROOT_BINDING]", result.stderr)
            self.assertIn("exemptions=0", result.stderr)

    def test_scripts_parent_mutation_before_s1_is_root_binding_failure(self) -> None:
        for mutation in ("touch", "replace"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                scripts = root / "scripts"
                scripts.mkdir()
                script = scripts / "check_no_unsafe.py"
                script.write_text("# stable\n")

                class MutateBeforeReplay(RealFileOps):
                    def __init__(self, selected: str, selected_root: Path, selected_scripts: Path) -> None:
                        super().__init__()
                        self.selected = selected
                        self.root = selected_root
                        self.scripts = selected_scripts
                        self.script_fds: list[int] = []
                        self.mutated = False

                    def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                        descriptor = super().openat(parent_fd, name, flags, mode)
                        if name == "check_no_unsafe.py":
                            self.script_fds.append(descriptor)
                        return descriptor

                    def close(self, fd: int) -> None:
                        super().close(fd)
                        if len(self.script_fds) == 2 and fd == self.script_fds[0] and not self.mutated:
                            if self.selected == "touch":
                                os.utime(self.scripts)
                            else:
                                moved = self.root / "scripts-original"
                                os.rename(self.scripts, moved)
                                self.scripts.mkdir()
                                (self.scripts / "check_no_unsafe.py").write_text("# replacement\n")
                            self.mutated = True

                ops = MutateBeforeReplay(mutation, root, scripts)
                with self.assertRaises(PolicyFailure) as caught:
                    bind_invocation(str(script), "check_no_unsafe.py", ops=ops)
                self.assertEqual(caught.exception.code, "E_ROOT_BINDING")
                self.assertTrue(ops.mutated)

    def test_scripts_parent_mutation_after_replay_is_caught_by_final_held_s1(self) -> None:
        for mutation in ("touch", "replace"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                scripts = root / "scripts"
                scripts.mkdir()
                script = scripts / "check_no_unsafe.py"
                script.write_text("# stable\n")

                class MutateAfterReplay(RealFileOps):
                    def __init__(self, selected: str) -> None:
                        super().__init__()
                        self.selected = selected
                        self.scripts_metadata: list[int] = []
                        self.replay_data_fd: int | None = None
                        self.mutated = False

                    def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                        descriptor = super().openat(parent_fd, name, flags, mode)
                        if name == "scripts":
                            self.scripts_metadata.append(descriptor)
                        return descriptor

                    def open_portal(self, metadata_fd: int, flags: int) -> int:
                        descriptor = super().open_portal(metadata_fd, flags)
                        if len(self.scripts_metadata) == 2 and metadata_fd == self.scripts_metadata[1]:
                            self.replay_data_fd = descriptor
                        return descriptor

                    def close(self, fd: int) -> None:
                        super().close(fd)
                        if fd == self.replay_data_fd and not self.mutated:
                            if self.selected == "touch":
                                os.utime(scripts)
                            else:
                                moved = root / "scripts-original"
                                os.rename(scripts, moved)
                                scripts.mkdir()
                                (scripts / "check_no_unsafe.py").write_text("# replacement\n")
                            self.mutated = True

                ops = MutateAfterReplay(mutation)
                with self.assertRaises(PolicyFailure) as caught:
                    bind_invocation(str(script), "check_no_unsafe.py", ops=ops)
                self.assertEqual(caught.exception.code, "E_ROOT_BINDING")
                self.assertIn("held scripts directory changed after parent replay", caught.exception.detail)
                self.assertTrue(ops.mutated)

    def test_runtime_operation_seams_are_not_cli_authority(self) -> None:
        import inspect

        from scripts import check_no_unsafe as scanner
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        self.assertIs(scanner.audit.__kwdefaults__["ops"], scanner.REAL_FILE_OPS)
        self.assertIs(generator.generate.__kwdefaults__["file_ops"], generator.REAL_FILE_OPS)
        self.assertIs(generator.generate.__kwdefaults__["io_ops"], generator.REAL_IO_OPS)
        scanner_main = inspect.getsource(scanner.main)
        generator_main = inspect.getsource(generator.main)
        for forbidden in ("--ops", "FILE_OPS", "IO_OPS", "plugin", "__loader__"):
            self.assertNotIn(forbidden, scanner_main)
            self.assertNotIn(forbidden, generator_main)
        self.assertIn("audit(sys.argv[0])", scanner_main)
        self.assertIn("generate(sys.argv[1:])", generator_main)
        binding_source = inspect.getsource(scanner.bind_invocation)
        for forbidden in ("getsource", "__loader__", "read_text", "read_bytes"):
            self.assertNotIn(forbidden, binding_source)
        portal_source = inspect.getsource(RealFileOps.open_portal)
        self.assertIn('os.open(f"/proc/self/fd/{metadata_fd}", flags)', portal_source)
        self.assertNotIn("dir_fd=", portal_source)
        self.assertNotIn("resolve", portal_source)


class ArchiveParserTests(unittest.TestCase):
    @staticmethod
    def archive_bytes(*, special: str | None = None, duplicate: bool = False) -> tuple[bytes, bytes]:
        tar_buffer = io.BytesIO()
        with tarfile.open(fileobj=tar_buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for index in range(10):
                info = tarfile.TarInfo(f"kani-0.67.0/library/d{index}")
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                archive.addfile(info)
            for index in range(32):
                name = f"kani-0.67.0/library/f{index}.rs"
                info = tarfile.TarInfo(name)
                payload = f"file-{index}\n".encode()
                info.size = len(payload)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(payload))
            if duplicate:
                info = tarfile.TarInfo("kani-0.67.0/library/f0.rs")
                info.size = 1
                archive.addfile(info, io.BytesIO(b"x"))
            if special is not None:
                info = tarfile.TarInfo("bad")
                info.type = {
                    "symlink": tarfile.SYMTYPE,
                    "fifo": tarfile.FIFOTYPE,
                }[special]
                archive.addfile(info)
        raw_tar = tar_buffer.getvalue()
        return gzip.compress(raw_tar, mtime=0), raw_tar

    def parse(self, compressed: bytes, limits: ArchiveLimits) -> object:
        with tempfile.NamedTemporaryFile() as archive:
            archive.write(compressed)
            archive.flush()
            fd = os.open(archive.name, os.O_RDONLY)
            try:
                return parse_archive_descriptor(fd, limits=limits)
            finally:
                os.close(fd)

    def test_synthetic_exact_inventory_uses_production_parser(self) -> None:
        compressed, _raw = self.archive_bytes()
        decompressed = gzip.decompress(compressed)
        result = self.parse(compressed, ArchiveLimits(compressed=len(compressed), decompressed=len(decompressed), members=42, payload=1024, one_member=64, path_bytes=256))
        self.assertEqual(len(result.directories), 10)  # type: ignore[attr-defined]
        self.assertEqual(len(result.files), 32)  # type: ignore[attr-defined]
        self.assertEqual(result.member_count, 42)  # type: ignore[attr-defined]
        self.assertEqual(result.decompressed_bytes, len(decompressed))  # type: ignore[attr-defined]

    def test_compressed_and_decompressed_n_plus_one(self) -> None:
        compressed, raw_tar = self.archive_bytes()
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(compressed, ArchiveLimits(compressed=len(compressed) - 1, decompressed=len(raw_tar), members=42, payload=1024, one_member=64, path_bytes=256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_COMPRESSED_LIMIT")
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(compressed, ArchiveLimits(compressed=len(compressed), decompressed=len(raw_tar) - 1, members=42, payload=1024, one_member=64, path_bytes=256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_DECOMPRESSED_LIMIT")

    def test_member_count_duplicate_and_special_causes(self) -> None:
        compressed, raw_tar = self.archive_bytes()
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(compressed, ArchiveLimits(compressed=len(compressed), decompressed=len(raw_tar), members=41, payload=1024, one_member=64, path_bytes=256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_MEMBER_COUNT")
        compressed, raw_tar = self.archive_bytes(duplicate=True)
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(compressed, ArchiveLimits(compressed=len(compressed), decompressed=len(raw_tar), members=100, payload=1024, one_member=64, path_bytes=256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_DUPLICATE")
        for kind, code in (("symlink", "E_ARCHIVE_KIND_SYMLINK"), ("fifo", "E_ARCHIVE_KIND_FIFO")):
            compressed, raw_tar = self.archive_bytes(special=kind)
            with self.assertRaises(PolicyFailure) as caught:
                self.parse(compressed, ArchiveLimits(compressed=len(compressed), decompressed=len(raw_tar), members=100, payload=1024, one_member=64, path_bytes=256))
            self.assertEqual(caught.exception.code, code)

    @staticmethod
    def one_member_archive(name: str, data: bytes = b"x", *, kind: bytes = tarfile.REGTYPE) -> tuple[bytes, bytes]:
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            info = tarfile.TarInfo(name)
            info.type = kind
            info.size = len(data) if kind == tarfile.REGTYPE else 0
            archive.addfile(info, io.BytesIO(data) if kind == tarfile.REGTYPE else None)
        raw = buffer.getvalue()
        return gzip.compress(raw, mtime=0), raw

    def test_path_payload_member_and_path_limits_are_specific(self) -> None:
        for name, code in (
            ("/absolute", "E_ARCHIVE_PATH_ABSOLUTE"),
            ("../escape", "E_ARCHIVE_PATH_ESCAPE"),
            ("bad\\name", "E_ARCHIVE_PATH_BACKSLASH"),
        ):
            with self.subTest(name=name):
                compressed, raw = self.one_member_archive(name)
                with self.assertRaises(PolicyFailure) as caught:
                    self.parse(compressed, ArchiveLimits(len(compressed), len(raw), 8, 8, 8, 256))
                self.assertEqual(caught.exception.code, code)
        compressed, raw = self.one_member_archive("file", b"1234")
        for limits, code in (
            (ArchiveLimits(len(compressed), len(raw), 8, 3, 4, 256), "E_ARCHIVE_EXPANSION"),
            (ArchiveLimits(len(compressed), len(raw), 8, 8, 3, 256), "E_ARCHIVE_MEMBER_SIZE"),
            (ArchiveLimits(len(compressed), len(raw), 8, 8, 4, 3), "E_ARCHIVE_PATH_SIZE"),
        ):
            with self.subTest(code=code), self.assertRaises(PolicyFailure) as caught:
                self.parse(compressed, limits)
            self.assertEqual(caught.exception.code, code)

    def test_pax_semantic_overrides_and_sparse_records_fail_closed(self) -> None:
        for key, value, code in (
            ("size", "0", "E_ARCHIVE_KIND_UNSUPPORTED"),
            ("linkpath", "target", "E_ARCHIVE_KIND_UNSUPPORTED"),
            ("vendor.semantic", "value", "E_ARCHIVE_KIND_UNSUPPORTED"),
            ("GNU.sparse.size", "1", "E_ARCHIVE_KIND_SPARSE"),
            ("GNU.sparse.map", "0,1", "E_ARCHIVE_KIND_SPARSE"),
            ("SCHILY.filetype", "sparse", "E_ARCHIVE_KIND_SPARSE"),
        ):
            with self.subTest(key=key, value=value, code=code):
                buffer = io.BytesIO()
                with tarfile.open(
                    fileobj=buffer,
                    mode="w",
                    format=tarfile.PAX_FORMAT,
                    pax_headers={key: value},
                ) as archive:
                    info = tarfile.TarInfo("kani-0.67.0/library/value.rs")
                    info.size = 1
                    archive.addfile(info, io.BytesIO(b"x"))
                raw = buffer.getvalue()
                compressed = gzip.compress(raw, mtime=0)
                with self.assertRaises(PolicyFailure) as caught:
                    self.parse(
                        compressed,
                        ArchiveLimits(len(compressed), len(raw), 8, 1024, 1024, 256),
                    )
                self.assertEqual(caught.exception.code, code)

    def test_malformed_checksum_truncation_and_nonzero_padding_reject(self) -> None:
        _compressed, raw = self.one_member_archive("file", b"x")
        malformed = bytearray(raw)
        malformed[0] ^= 1
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(gzip.compress(bytes(malformed), mtime=0), ArchiveLimits(1024, len(raw), 8, 8, 8, 256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_TAR")
        truncated = raw[:512]
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(gzip.compress(truncated, mtime=0), ArchiveLimits(1024, len(raw), 8, 8, 8, 256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_SHORT_MEMBER")
        padded = bytearray(raw)
        padded[513] = 1
        with self.assertRaises(PolicyFailure) as caught:
            self.parse(gzip.compress(bytes(padded), mtime=0), ArchiveLimits(1024, len(raw), 8, 8, 8, 256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_TAR")


class GeneratorInputOutputTests(unittest.TestCase):
    def _root_fd(self, root: Path) -> int:
        return os.open(root, os.O_RDONLY | O_DIRECTORY)

    def test_checked_manifest_real_success_and_fifo_rejection(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            shutil.copyfile(CANONICAL_MANIFEST, root / MANIFEST_REL)
            fd = self._root_fd(root)
            try:
                value, raw = read_checked_manifest(fd)
                self.assertEqual(value["schema"], "axiograph-no-unsafe-external-cache-manifest-v1")
                self.assertEqual(raw, CANONICAL_MANIFEST.read_bytes())
            finally:
                os.close(fd)
            (root / MANIFEST_REL).unlink()
            os.mkfifo(root / MANIFEST_REL)
            fd = self._root_fd(root)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    read_checked_manifest(fd)
                self.assertEqual(caught.exception.code, "E_REGEN_CHECK_NONREGULAR")
            finally:
                os.close(fd)

    def test_check_and_archive_specials_are_classified_before_data_open(self) -> None:
        for relative, reader, code in (
            (MANIFEST_REL, read_checked_manifest, "E_REGEN_CHECK"),
            (ARCHIVE_REL, read_archive_inventory, "E_REGEN_ARCHIVE"),
        ):
            for kind in ("link", "fifo", "socket", "device"):
                with self.subTest(relative=relative, kind=kind), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    target = root / relative
                    target.parent.mkdir(parents=True)
                    unix_socket: socket.socket | None = None
                    if kind == "link":
                        os.symlink("missing", target)
                    elif kind == "fifo":
                        os.mkfifo(target)
                    elif kind == "socket":
                        unix_socket = socket.socket(socket.AF_UNIX)
                        parent_fd = os.open(target.parent, os.O_RDONLY | O_DIRECTORY)
                        try:
                            unix_socket.bind(f"/proc/self/fd/{parent_fd}/{target.name}")
                        finally:
                            os.close(parent_fd)
                    class TargetCountingOps(CountingOps):
                        def __init__(self, target_name: str) -> None:
                            super().__init__()
                            self.target_name = target_name
                            self.target_portals = 0

                        def open_portal(self, metadata_fd: int, flags: int) -> int:
                            if os.readlink(f"/proc/self/fd/{metadata_fd}").endswith("/" + self.target_name):
                                self.target_portals += 1
                            return super().open_portal(metadata_fd, flags)

                    ops: TargetCountingOps
                    if kind == "device":
                        class DeviceOps(TargetCountingOps):
                            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                                if name == self.target_name:
                                    return os.open("/dev/null", O_PATH | os.O_CLOEXEC)
                                return super().openat2(parent_fd, name, flags, mode, resolve)

                        ops = DeviceOps(target.name)
                    else:
                        ops = TargetCountingOps(target.name)
                    descriptor = self._root_fd(root)
                    try:
                        with self.assertRaises(PolicyFailure) as caught:
                            reader(descriptor, ops=ops)
                        expected = code + ("_LINK" if kind == "link" else "_NONREGULAR")
                        self.assertEqual(caught.exception.code, expected)
                        self.assertEqual(ops.target_portals, 0)
                    finally:
                        os.close(descriptor)
                        if unix_socket is not None:
                            unix_socket.close()

    def test_checked_manifest_n_and_n_plus_one_sizes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / MANIFEST_REL
            target.parent.mkdir()
            canonical = CANONICAL_MANIFEST.read_bytes()
            target.write_bytes(canonical + b" " * (65_536 - len(canonical)))
            fd = self._root_fd(root)
            try:
                value, raw = read_checked_manifest(fd)
                self.assertEqual(len(raw), 65_536)
                self.assertEqual(value["schema"], "axiograph-no-unsafe-external-cache-manifest-v1")
            finally:
                os.close(fd)
            target.write_bytes(canonical + b" " * (65_537 - len(canonical)))
            fd = self._root_fd(root)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    read_checked_manifest(fd)
                self.assertEqual(caught.exception.code, "E_REGEN_CHECK_SIZE")
            finally:
                os.close(fd)

    def test_archive_fifo_is_rejected_before_data_open(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / ARCHIVE_REL
            archive.parent.mkdir(parents=True)
            os.mkfifo(archive)
            fd = self._root_fd(root)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    read_archive_inventory(fd)
                self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_NONREGULAR")
            finally:
                os.close(fd)

    def test_output_success_and_existing_fixed_precedence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent = root / "build/engineering-quality/ns/run"
            parent.mkdir(parents=True)
            fd = self._root_fd(root)
            relative = f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}"
            try:
                write_output(fd, relative, b"{}\n")
                self.assertEqual((parent / OUTPUT_BASENAME).read_bytes(), b"{}\n")
                (parent / "other").write_bytes(b"x")
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(fd, relative, b"{}\n")
                self.assertEqual(caught.exception.code, "E_REGEN_OUTPUT_EXISTS")
            finally:
                os.close(fd)

    def test_other_parent_entry_is_confinement_and_fifo_is_exists(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent = root / "build/engineering-quality/ns/run"
            parent.mkdir(parents=True)
            relative = f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}"
            fd = self._root_fd(root)
            try:
                (parent / "other").write_bytes(b"x")
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(fd, relative, b"{}\n")
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
                (parent / "other").unlink()
                os.mkfifo(parent / OUTPUT_BASENAME)
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(fd, relative, b"{}\n")
                self.assertEqual(caught.exception.code, "E_REGEN_OUTPUT_EXISTS")
            finally:
                os.close(fd)


class ScannerCliTests(unittest.TestCase):
    def test_live_relative_and_absolute_invocation_are_green(self) -> None:
        relative = subprocess.run(["python3", "scripts/check_no_unsafe.py"], cwd=REPO, capture_output=True, text=True, timeout=180, check=False)
        self.assertEqual(relative.returncode, 0, relative.stderr)
        report = json.loads(relative.stdout)
        self.assertEqual(report["external_cache_rust"], 56)
        self.assertEqual(len(report["active_homes"]), 2)
        decoy = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, decoy)
        environment = dict(os.environ)
        environment.update(
            {
                "GIT_DIR": decoy,
                "GIT_WORK_TREE": decoy,
                "GIT_INDEX_FILE": str(Path(decoy) / "index"),
                "GIT_COMMON_DIR": decoy,
                "GIT_CONFIG": str(Path(decoy) / "config"),
                "GIT_CONFIG_SYSTEM": str(Path(decoy) / "system-config"),
                "GIT_CONFIG_GLOBAL": str(Path(decoy) / "global-config"),
                "GIT_CONFIG_COUNT": "1",
                "GIT_CONFIG_KEY_0": "core.bare",
                "GIT_CONFIG_VALUE_0": "true",
                "GIT_OBJECT_DIRECTORY": str(Path(decoy) / "objects"),
                "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(Path(decoy) / "alternates"),
                "GIT_CEILING_DIRECTORIES": str(REPO.parent),
                "GIT_DISCOVERY_ACROSS_FILESYSTEM": "0",
                "HOME": decoy,
                "XDG_CONFIG_HOME": decoy,
            }
        )
        absolute = subprocess.run(
            ["python3", str(REPO / "scripts/check_no_unsafe.py")],
            cwd=REPO.parent,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
            env=environment,
        )
        self.assertEqual(absolute.returncode, 0, absolute.stderr)
        self.assertEqual(json.loads(absolute.stdout)["bound_root"], str(REPO))


class IsolatedScannerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        scripts = self.root / "scripts"
        scripts.mkdir()
        for name in ("check_no_unsafe.py", "no_unsafe_fs.py", "bounded_subprocess.py"):
            shutil.copyfile(REPO / "scripts" / name, scripts / name)
        shutil.copyfile(CANONICAL_MANIFEST, self.root / MANIFEST_REL)
        crate = self.root / "rust/crate/src"
        crate.mkdir(parents=True)
        (self.root / "rust/Cargo.toml").write_text(
            '[workspace]\nmembers=["crate"]\nresolver="2"\n'
            '[workspace.lints.rust]\nunsafe_code="forbid"\n'
        )
        (self.root / "rust/crate/Cargo.toml").write_text(
            '[package]\nname="fixture"\nversion="0.1.0"\nedition="2024"\n'
            '[lints]\nworkspace=true\n'
        )
        (crate / "lib.rs").write_text("pub fn safe() {}\n")
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.email", "fixture@example.invalid"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.name", "Fixture"], cwd=self.root, check=True)
        subprocess.run(["git", "add", "scripts", "rust"], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "fixture"], cwd=self.root, check=True)
        self.home = self.root / CANDIDATE_HOMES[0]
        self.home.parent.mkdir(parents=True)
        shutil.copytree(CACHE_LIBRARY, self.home / "library")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_scanner(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", "scripts/check_no_unsafe.py"],
            cwd=self.root,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )

    def test_exact_untracked_home_is_ownership_exempt(self) -> None:
        result = self.run_scanner()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["external_cache_rust"], 28)
        self.assertEqual(report["lexically_scanned_rust"], 1)

    def test_one_byte_tamper_is_hash_denial_and_lexical_evidence(self) -> None:
        target = self.home / "library/kani/src/arbitrary.rs"
        target.write_bytes(target.read_bytes() + b"\nunsafe { changed(); }\n")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_CACHE_SIZE]", result.stderr)
        self.assertIn("[E_UNSAFE]", result.stderr)
        self.assertNotIn('"external_cache_rust": 28', result.stderr)

    def test_extra_under_all_prune_names_is_not_hidden(self) -> None:
        for name in (".codebase-index", ".git", ".lake", ".pi-subagents", "node_modules", "target"):
            directory = self.home / "library" / name
            directory.mkdir()
            (directory / "evil.rs").write_text("unsafe fn hidden() {}\n")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_CACHE_EXTRA]", result.stderr)
        for name in (".codebase-index", ".git", ".lake", ".pi-subagents", "node_modules", "target"):
            self.assertIn(f"/library/{name}/evil.rs", result.stderr)

    def test_tracked_candidate_file_forces_first_party(self) -> None:
        relative = Path(CANDIDATE_HOMES[0]) / "library/kani/src/arbitrary.rs"
        subprocess.run(["git", "add", str(relative)], cwd=self.root, check=True)
        staged = self.run_scanner()
        self.assertEqual(staged.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", staged.stderr)
        self.assertIn("[E_UNSAFE]", staged.stderr)
        subprocess.run(["git", "commit", "-qm", "track candidate"], cwd=self.root, check=True)
        committed = self.run_scanner()
        self.assertEqual(committed.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", committed.stderr)
        self.assertIn("[E_UNSAFE]", committed.stderr)

    def test_ordinary_untracked_cfg_disabled_and_invalid_utf8_are_scanned(self) -> None:
        baseline_result = self.run_scanner()
        self.assertEqual(baseline_result.returncode, 0, baseline_result.stderr)
        baseline = json.loads(baseline_result.stdout)
        path = self.root / "unattached.rs"
        raw = b"\xff#[cfg(any())]\nunsafe fn hidden() {}\xfe\n"
        path.write_bytes(raw)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_UNSAFE] unattached.rs:2", result.stderr)
        self.assertNotIn("UTF8", result.stderr)
        charged = json.loads(result.stderr.splitlines()[-1])
        self.assertEqual(charged["rust_bytes"], baseline["rust_bytes"] + len(raw))
        self.assertEqual(charged["discovered_rust"], baseline["discovered_rust"] + 1)
        (self.root / ".git/info/exclude").write_text("ignored-unsafe.rs\n")
        ignored = self.root / "ignored-unsafe.rs"
        ignored.write_text("#[cfg(any())]\nunsafe fn ignored() {}\n")
        check = subprocess.run(["git", "check-ignore", ignored.name], cwd=self.root, capture_output=True, check=False)
        self.assertEqual(check.returncode, 0)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_UNSAFE] ignored-unsafe.rs:2", result.stderr)
        self.assertIn("[E_UNSAFE] unattached.rs:2", result.stderr)

    def test_candidate_symlink_and_ordinary_fifo_fail_without_hang(self) -> None:
        os.symlink("kani/src/arbitrary.rs", self.home / "library/linked.rs")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_FS_LINK]", result.stderr)
        (self.home / "library/linked.rs").unlink()
        exclude = self.root / ".git/info/exclude"
        exclude.write_text("ignored-fifo.rs\nignored-socket.rs\n")
        for name, ignored in (("ordinary.rs", False), ("ignored-fifo.rs", True)):
            with self.subTest(kind="fifo", ignored=ignored):
                target = self.root / name
                os.mkfifo(target)
                if ignored:
                    check = subprocess.run(["git", "check-ignore", name], cwd=self.root, capture_output=True, check=False)
                    self.assertEqual(check.returncode, 0)
                result = self.run_scanner()
                self.assertEqual(result.returncode, 1)
                self.assertIn("[E_SOURCE_NONREGULAR]", result.stderr)
                self.assertIn('"external_cache_rust": 28', result.stderr)
                target.unlink()
        for name, ignored in (("ordinary-socket.rs", False), ("ignored-socket.rs", True)):
            with self.subTest(kind="socket", ignored=ignored):
                unix_socket = socket.socket(socket.AF_UNIX)
                root_fd = os.open(self.root, os.O_RDONLY | O_DIRECTORY)
                try:
                    unix_socket.bind(f"/proc/self/fd/{root_fd}/{name}")
                    if ignored:
                        check = subprocess.run(["git", "check-ignore", name], cwd=self.root, capture_output=True, check=False)
                        self.assertEqual(check.returncode, 0)
                    result = self.run_scanner()
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("[E_SOURCE_NONREGULAR]", result.stderr)
                    self.assertIn('"external_cache_rust": 28', result.stderr)
                finally:
                    unix_socket.close()
                    os.close(root_fd)

    def test_candidate_fifo_and_socket_are_nonregular_without_data_open(self) -> None:
        fifo = self.home / "library/candidate-fifo"
        os.mkfifo(fifo)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_FS_NONREGULAR]", result.stderr)
        self.assertIn("exemptions=0", result.stderr)
        fifo.unlink()
        unix_socket = socket.socket(socket.AF_UNIX)
        library_fd = os.open(self.home / "library", os.O_RDONLY | O_DIRECTORY)
        try:
            unix_socket.bind(f"/proc/self/fd/{library_fd}/candidate-socket.rs")
            result = self.run_scanner()
            self.assertEqual(result.returncode, 1)
            self.assertIn("[E_FS_NONREGULAR]", result.stderr)
            self.assertIn("exemptions=0", result.stderr)
        finally:
            unix_socket.close()
            os.close(library_fd)

    def test_git_index_link_fifo_and_unsupported_control_fail_closed(self) -> None:
        index = self.root / ".git/index"
        original = index.read_bytes()
        index.unlink()
        os.symlink("missing-index", index)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_GIT_INDEX_LINK]", result.stderr)
        index.unlink()
        os.mkfifo(index)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_GIT_INDEX_NONREGULAR]", result.stderr)
        index.unlink()
        index.write_bytes(original)
        (self.root / ".git/commondir").write_text("../.git\n")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_GIT_BINDING]", result.stderr)

    def test_blob_and_gitlink_candidate_ancestors_force_first_party(self) -> None:
        shutil.rmtree(self.root / "build")
        (self.root / "build").write_text("tracked ancestor\n")
        subprocess.run(["git", "add", "-f", "build"], cwd=self.root, check=True, capture_output=True)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", result.stderr)
        subprocess.run(["git", "commit", "-qm", "blob candidate ancestor"], cwd=self.root, check=True)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", result.stderr)

        subprocess.run(["git", "rm", "-qf", "build"], cwd=self.root, check=True)
        (self.root / "build").mkdir()
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        subprocess.run(
            ["git", "update-index", "--add", "--cacheinfo", f"160000,{head},build"],
            cwd=self.root,
            check=True,
            capture_output=True,
        )
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", result.stderr)
        subprocess.run(["git", "commit", "-qm", "gitlink candidate ancestor"], cwd=self.root, check=True)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_TRACKED_OVERRIDE]", result.stderr)

    def test_head_change_between_git_views_is_a_race(self) -> None:
        branch = subprocess.run(
            ["git", "symbolic-ref", "HEAD"],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        replacement = subprocess.run(
            ["git", "commit-tree", "HEAD^{tree}", "-p", "HEAD"],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
            input="race commit\n",
        ).stdout.strip()

        class HeadMutationOps(RealFileOps):
            def __init__(self, root: Path) -> None:
                super().__init__()
                self.root = root
                self.mutated = False

            def iter_directory(self, fd: int, *, max_bytes: int):  # type: ignore[no-untyped-def]
                target = os.readlink(f"/proc/self/fd/{fd}")
                if target == str(self.root) and not self.mutated:
                    (self.root / ".git" / branch).write_text(replacement + "\n")
                    self.mutated = True
                yield from super().iter_directory(fd, max_bytes=max_bytes)

        ops = HeadMutationOps(self.root)
        with self.assertRaises(PolicyFailure) as caught:
            audit(str(self.root / "scripts/check_no_unsafe.py"), ops=ops)
        self.assertEqual(caught.exception.code, "E_GIT_RACE")
        self.assertTrue(ops.mutated)

    def test_root_plus_511_manifests_accepts_and_next_manifest_rejects(self) -> None:
        many = self.root / "rust/many"
        many.mkdir()
        for index in range(510):
            package = many / f"p{index:03d}"
            (package / "src").mkdir(parents=True)
            (package / "src/lib.rs").write_text("pub fn safe() {}\n")
            (package / "Cargo.toml").write_text(
                f'[package]\nname="p{index:03d}"\nversion="0.1.0"\nedition="2024"\n'
                '[lints]\nworkspace=true\n'
            )
        root_manifest = self.root / "rust/Cargo.toml"
        root_manifest.write_text(
            '[workspace]\nmembers=["crate","many/*"]\nresolver="2"\n'
            '[workspace.lints.rust]\nunsafe_code="forbid"\n'
        )
        result = self.run_scanner()
        self.assertEqual(result.returncode, 0, result.stderr)
        package = many / "p510"
        (package / "src").mkdir(parents=True)
        (package / "src/lib.rs").write_text("pub fn safe() {}\n")
        (package / "Cargo.toml").write_text(
            '[package]\nname="p510"\nversion="0.1.0"\nedition="2024"\n'
            '[lints]\nworkspace=true\n'
        )
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_CARGO_MANIFEST_COUNT]", result.stderr)

    def test_manifest_tamper_fails_closed(self) -> None:
        manifest = self.root / MANIFEST_REL
        value = json.loads(manifest.read_text())
        value["records"][0]["rationale"] = "identity means safe"
        manifest.write_text(json.dumps(value))
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_MANIFEST_SCHEMA]", result.stderr)
        self.assertIn("exemptions=0", result.stderr)

    def test_semantically_identical_manifest_format_is_accepted(self) -> None:
        manifest = self.root / MANIFEST_REL
        value = json.loads(manifest.read_text())
        manifest.write_text(json.dumps(value, separators=(",", ":")))
        result = self.run_scanner()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_candidate_home_ancestor_symlink_is_not_silently_inactive(self) -> None:
        shutil.rmtree(self.root / "build")
        os.symlink("rust", self.root / "build")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_FS_LINK]", result.stderr)
        self.assertIn("exemptions=0", result.stderr)

    def test_candidate_home_ancestor_non_directory_is_not_silently_inactive(self) -> None:
        stage = self.root / "build/engineering-quality/release-roadmap/complete-kani-with-verified-path-20260908T044738Z"
        shutil.rmtree(stage)
        stage.write_text("not a directory\n")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_FS_NONREGULAR]", result.stderr)
        self.assertIn("exemptions=0", result.stderr)

    def test_extra_rust_and_nonrust_files_deny_home_and_rust_is_scanned(self) -> None:
        for name, contents, unsafe_expected in (
            ("extra.rs", b"unsafe fn extra() {}\n", True),
            ("extra.txt", b"non-Rust extra\n", False),
        ):
            with self.subTest(name=name):
                target = self.home / "library" / name
                target.write_bytes(contents)
                result = self.run_scanner()
                self.assertEqual(result.returncode, 1)
                self.assertIn("[E_CACHE_EXTRA]", result.stderr)
                self.assertIn('"external_cache_rust": 0', result.stderr)
                if unsafe_expected:
                    self.assertIn(f"[E_UNSAFE] {target.relative_to(self.root)}:1", result.stderr)
                target.unlink()

    def test_same_size_tamper_and_missing_file_are_denied(self) -> None:
        target = self.home / "library/kani/src/arbitrary.rs"
        original = target.read_bytes()
        target.write_bytes(bytes([original[0] ^ 1]) + original[1:])
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_CACHE_HASH]", result.stderr)
        target.unlink()
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_CACHE_MISSING]", result.stderr)

    def test_duplicate_and_invalid_utf8_manifest_causes(self) -> None:
        manifest = self.root / MANIFEST_REL
        manifest.write_text('{"schema":"x","schema":"y","records":[]}')
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_MANIFEST_DUPLICATE]", result.stderr)
        manifest.write_bytes(b"\xff")
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_MANIFEST_UTF8]", result.stderr)

    def test_cargo_package_root_around_candidate_forces_workspace_override(self) -> None:
        (self.home / "src").mkdir()
        (self.home / "src/lib.rs").write_text("pub fn cache_package() {}\n")
        workspace_path = os.path.relpath(self.root / "rust", self.home)
        (self.home / "Cargo.toml").write_text(
            '[package]\nname="cache_fixture"\nversion="0.1.0"\nedition="2024"\n'
            f'workspace="{workspace_path}"\n'
            '[lints]\nworkspace=true\n'
        )
        root_manifest = self.root / "rust/Cargo.toml"
        member = "../" + CANDIDATE_HOMES[0]
        root_manifest.write_text(
            f'[workspace]\nmembers=["crate","{member}"]\nresolver="2"\n'
            '[workspace.lints.rust]\nunsafe_code="forbid"\n'
        )
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_WORKSPACE_OVERRIDE]", result.stderr)

    def test_cargo_target_source_below_candidate_forces_workspace_override(self) -> None:
        target = self.home / "library/kani/src/arbitrary.rs"
        relative_target = str(target)
        (self.root / "rust/crate/Cargo.toml").write_text(
            '[package]\nname="fixture"\nversion="0.1.0"\nedition="2024"\n'
            f'[lib]\npath="{relative_target}"\n'
            '[lints]\nworkspace=true\n'
        )
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_WORKSPACE_OVERRIDE]", result.stderr)
        self.assertIn('"external_cache_rust": 0', result.stderr)

    def test_candidate_directory_n_plus_one_marks_coverage_incomplete(self) -> None:
        # The pinned inventory has ten descendant directories. Add 22 to reach
        # the exact production ceiling N=32: traversal completes but inventory
        # comparison denies the home. The next directory is N+1 and stops
        # before any exemption can be issued.
        for index in range(22):
            (self.home / "library" / f"extra-{index}").mkdir()
        at_n = self.run_scanner()
        self.assertEqual(at_n.returncode, 1)
        self.assertIn("[E_CACHE_EXTRA]", at_n.stderr)
        self.assertIn('"coverage_complete": true', at_n.stderr)
        self.assertIn('"external_cache_rust": 0', at_n.stderr)
        (self.home / "library/extra-22").mkdir()
        above_n = self.run_scanner()
        self.assertEqual(above_n.returncode, 1)
        self.assertIn("[E_LIMIT_CANDIDATE_DIRECTORIES]", above_n.stderr)
        self.assertIn("coverage_complete=false", above_n.stderr)
        self.assertIn("exemptions=0", above_n.stderr)

    def test_ordinary_source_size_n_plus_one_fails_closed(self) -> None:
        with (self.root / "large.rs").open("wb") as stream:
            stream.truncate(4 * 1024 * 1024 + 1)
        result = self.run_scanner()
        self.assertEqual(result.returncode, 1)
        self.assertIn("[E_SOURCE_SIZE]", result.stderr)
        self.assertIn("exemptions=0", result.stderr)

    def test_all_repository_and_candidate_ceilings_cli_n_and_n_plus_one(self) -> None:
        launcher = (
            "import scripts.check_no_unsafe as scanner,sys; "
            "setattr(scanner,sys.argv[1],int(sys.argv[2])); "
            "sys.argv=['scripts/check_no_unsafe.py']; raise SystemExit(scanner.main())"
        )
        cases = (
            ("MAX_REPOSITORY_DIRECTORIES", 32_768, "E_LIMIT_REPOSITORY_DIRECTORIES"),
            ("MAX_REPOSITORY_ENTRIES", 262_144, "E_LIMIT_REPOSITORY_ENTRIES"),
            ("MAX_REPOSITORY_DEPTH", 64, "E_LIMIT_REPOSITORY_DEPTH"),
            ("MAX_RUST_FILES", 8_192, "E_LIMIT_REPOSITORY_RUST"),
            ("MAX_SOURCE_BYTES", 256 * 1024 * 1024, "E_LIMIT_REPOSITORY_RUST"),
            ("MAX_SOURCE_FILE_BYTES", 4 * 1024 * 1024, "E_SOURCE_SIZE"),
            ("MAX_CANDIDATE_DIRECTORIES", 32, "E_LIMIT_CANDIDATE_DIRECTORIES"),
            ("MAX_CANDIDATE_FILES", 64, "E_LIMIT_CANDIDATE_FILES"),
            ("MAX_CANDIDATE_ENTRIES", 96, "E_LIMIT_CANDIDATE_ENTRIES"),
            ("MAX_CANDIDATE_FILE_BYTES", 131_072, "E_FS_SIZE"),
            ("MAX_CANDIDATE_BYTES", 1024 * 1024, "E_LIMIT_CANDIDATE_BYTES"),
            ("MAX_ALL_CANDIDATE_BYTES", 8 * 1024 * 1024, "E_LIMIT_CANDIDATE_BYTES"),
        )

        def invoke(name: str, limit: int) -> subprocess.CompletedProcess[str]:
            return subprocess.run(
                [sys.executable, "-c", launcher, name, str(limit)],
                cwd=self.root,
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )

        for name, upper, code in cases:
            with self.subTest(limit=name):
                high = upper
                accepted = invoke(name, high)
                self.assertEqual(accepted.returncode, 0, (name, high, accepted.stderr))
                low = 0
                while low < high:
                    middle = (low + high) // 2
                    result = invoke(name, middle)
                    if result.returncode == 0:
                        high = middle
                    else:
                        self.assertIn(f"[{code}]", result.stderr, (name, middle, result.stderr))
                        low = middle + 1
                exact_n = invoke(name, low)
                self.assertEqual(exact_n.returncode, 0, (name, low, exact_n.stderr))
                self.assertGreater(low, 0)
                above_limit = invoke(name, low - 1)
                self.assertEqual(above_limit.returncode, 1)
                self.assertIn(f"[{code}]", above_limit.stderr)
                self.assertIn("exemptions=0", above_limit.stderr)


class ArchiveOperationTests(unittest.TestCase):
    class FaultOps(RealFileOps):
        def __init__(self, fault: str, other: Path) -> None:
            super().__init__()
            self.fault = fault
            self.other = other
            self.metadata_fd: int | None = None
            self.data_fd: int | None = None
            self.data_fstats = 0
            self.phase = "identity"
            self.changed_hash_byte = False
            self.close_failed = False

        def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
            if name == Path(ARCHIVE_REL).name and self.fault == "open":
                raise OSError(errno.EACCES, "open")
            result = super().openat2(parent_fd, name, flags, mode, resolve)
            if name == Path(ARCHIVE_REL).name:
                self.metadata_fd = result
            return result

        @staticmethod
        def larger(value: os.stat_result) -> os.stat_result:
            fields = list(value)
            fields[6] += 1
            return os.stat_result(fields)

        def fstat(self, fd: int) -> os.stat_result:
            if fd == self.metadata_fd and self.fault in ("metadata", "size"):
                if self.fault == "metadata":
                    raise OSError(errno.EIO, "metadata")
                return self.larger(super().fstat(fd))
            if fd == self.data_fd:
                self.data_fstats += 1
                if self.fault == "size" and self.data_fstats == 1:
                    return self.larger(super().fstat(fd))
                if self.fault == "identity_metadata" and self.data_fstats == 2:
                    raise OSError(errno.EIO, "identity metadata")
                if self.fault == "parser_metadata" and self.data_fstats == 3:
                    raise OSError(errno.EIO, "parser metadata")
            return super().fstat(fd)

        def open_portal(self, metadata_fd: int, flags: int) -> int:
            target = os.readlink(f"/proc/self/fd/{metadata_fd}")
            if target.endswith("/" + Path(ARCHIVE_REL).name):
                if self.fault == "data_open":
                    raise OSError(errno.EIO, "data open")
                if self.fault == "upgrade_race":
                    self.data_fd = os.open(self.other, os.O_RDONLY | os.O_NONBLOCK)
                    return self.data_fd
                self.data_fd = super().open_portal(metadata_fd, flags)
                return self.data_fd
            return super().open_portal(metadata_fd, flags)

        def read(self, fd: int, count: int) -> bytes:
            if fd == self.data_fd:
                if self.phase == "identity" and self.fault == "identity_read":
                    raise OSError(errno.EIO, "identity read")
                if self.phase == "identity" and self.fault == "identity_short":
                    return b""
                if self.phase == "parser" and self.fault == "parser_read":
                    raise OSError(errno.EIO, "parser read")
                if self.phase == "parser" and self.fault == "parser_short":
                    return b""
                chunk = super().read(fd, count)
                if self.phase == "identity" and self.fault == "hash" and chunk and not self.changed_hash_byte:
                    self.changed_hash_byte = True
                    return bytes([chunk[0] ^ 1]) + chunk[1:]
                return chunk
            return super().read(fd, count)

        def lseek(self, fd: int, offset: int, whence: int) -> int:
            if fd == self.data_fd and self.fault == "seek":
                raise OSError(errno.EIO, "seek")
            result = super().lseek(fd, offset, whence)
            self.phase = "parser"
            if fd == self.data_fd and self.fault == "seek_offset":
                return 1
            return result

        def close(self, fd: int) -> None:
            if self.fault == "close" and fd in (self.data_fd, self.metadata_fd) and not self.close_failed:
                self.close_failed = True
                super().close(fd)
                raise OSError(errno.EIO, "close")
            super().close(fd)

    def test_archive_total_operation_causes(self) -> None:
        cases = {
            "open": "E_REGEN_ARCHIVE_OPEN",
            "metadata": "E_REGEN_ARCHIVE_METADATA",
            "data_open": "E_REGEN_ARCHIVE_DATA_OPEN",
            "upgrade_race": "E_REGEN_ARCHIVE_RACE",
            "size": "E_REGEN_ARCHIVE_SIZE",
            "identity_read": "E_REGEN_ARCHIVE_IDENTITY_READ",
            "identity_short": "E_REGEN_ARCHIVE_IDENTITY_SHORT_READ",
            "identity_metadata": "E_REGEN_ARCHIVE_METADATA",
            "hash": "E_REGEN_ARCHIVE_HASH",
            "seek": "E_REGEN_ARCHIVE_SEEK",
            "seek_offset": "E_REGEN_ARCHIVE_SEEK",
            "parser_read": "E_REGEN_ARCHIVE_PARSER_READ",
            "parser_short": "E_REGEN_ARCHIVE_PARSER_SHORT_READ",
            "parser_metadata": "E_REGEN_ARCHIVE_METADATA",
            "close": "E_REGEN_ARCHIVE_CLOSE",
        }
        root_fd = os.open(REPO, os.O_RDONLY | O_DIRECTORY)
        other = REPO / "scripts/README.md"
        try:
            for fault, code in cases.items():
                with self.subTest(fault=fault), self.assertRaises(PolicyFailure) as caught:
                    read_archive_inventory(root_fd, ops=self.FaultOps(fault, other))
                self.assertEqual(caught.exception.code, code)
        finally:
            os.close(root_fd)


class ScannerExactRaceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = IsolatedScannerTests()
        self.fixture.setUp()
        self.root = self.fixture.root

    def tearDown(self) -> None:
        self.fixture.tearDown()

    class IndexMutationOps(RealFileOps):
        def __init__(self, root: Path, target_open: int) -> None:
            super().__init__()
            self.root = root
            self.target_open = target_open
            self.index_opens = 0
            self.mutated = False

        def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
            if name == "index" and os.readlink(f"/proc/self/fd/{parent_fd}") == str(self.root / ".git"):
                self.index_opens += 1
                if self.index_opens == self.target_open:
                    staged = self.root / f"staged-{self.target_open}.rs"
                    staged.write_text("pub fn staged() {}\n")
                    subprocess.run(["git", "add", staged.name], cwd=self.root, check=True, capture_output=True)
                    self.mutated = True
            return super().openat(parent_fd, name, flags, mode)

    def test_real_index_mutation_at_each_post_i0_snapshot_is_a_race(self) -> None:
        # Open 1 is the metadata-only special-file preflight; I0 is open 2.
        # Mutate at each later byte snapshot I1 through I4.
        for target_open in (3, 4, 5, 6):
            with self.subTest(target_open=target_open), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "repo"
                shutil.copytree(self.root, root)
                ops = self.IndexMutationOps(root, target_open)
                with self.assertRaises(PolicyFailure) as caught:
                    audit(str(root / "scripts/check_no_unsafe.py"), ops=ops)
                self.assertEqual(caught.exception.code, "E_GIT_RACE")
                self.assertTrue(ops.mutated)

    class CargoMutationOps(RealFileOps):
        def __init__(self, root: Path, mode: str) -> None:
            super().__init__()
            self.root = root
            self.mode = mode
            self.opens = 0
            self.final_fd: int | None = None
            self.mutated = False
            self.reads_after_mutation = 0

        def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
            is_root_manifest = name == "Cargo.toml" and os.readlink(f"/proc/self/fd/{parent_fd}") == str(self.root / "rust")
            if is_root_manifest:
                self.opens += 1
                # Open 1 is the root-lint read. Open 2 is declaration.
                # Open 3 is the held-inode aggregate authorization. Open 4 is
                # the post-parse final-name observation.
                if self.mode == "replace_before_authorization" and self.opens == 3:
                    target = self.root / "rust/Cargo.toml"
                    replacement = target.with_name("Cargo.replacement")
                    replacement.write_bytes(target.read_bytes() + b"# larger replacement\n")
                    os.replace(replacement, target)
                    self.mutated = True
                elif self.mode == "replace_final" and self.opens == 4:
                    target = self.root / "rust/Cargo.toml"
                    replacement = target.with_name("Cargo.replacement")
                    replacement.write_bytes(b"not = [toml")
                    os.replace(replacement, target)
                    self.mutated = True
            result = super().openat(parent_fd, name, flags, mode)
            if is_root_manifest and self.opens == 4:
                self.final_fd = result
            return result

        def read(self, fd: int, count: int) -> bytes:
            if self.mutated:
                self.reads_after_mutation += 1
            return super().read(fd, count)

        def close(self, fd: int) -> None:
            super().close(fd)
            if fd == self.final_fd and not self.mutated:
                if self.mode == "touch_parent":
                    os.utime(self.root / "rust")
                    self.mutated = True
                elif self.mode == "replace_parent":
                    parent = self.root / "rust"
                    moved = self.root / "rust-original"
                    os.rename(parent, moved)
                    parent.mkdir()
                    self.mutated = True

    def test_cargo_replacement_before_actual_aggregate_authorization_reads_zero_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "repo"
            shutil.copytree(self.root, root)
            ops = self.CargoMutationOps(root, "replace_before_authorization")
            manifest_total = sum(path.stat().st_size for path in root.glob("rust/**/Cargo.toml"))
            with mock.patch("scripts.check_no_unsafe.MAX_ALL_CARGO_MANIFEST_BYTES", manifest_total), self.assertRaises(PolicyFailure) as caught:
                audit(str(root / "scripts/check_no_unsafe.py"), ops=ops)
            self.assertEqual(caught.exception.code, "E_CARGO_MANIFEST_RACE")
            self.assertIn("before held-inode authorization", caught.exception.detail)
            self.assertTrue(ops.mutated)
            self.assertEqual(ops.reads_after_mutation, 0)

    def test_real_cargo_final_name_and_parent_windows_are_races(self) -> None:
        for mode in ("replace_final", "touch_parent", "replace_parent"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "repo"
                shutil.copytree(self.root, root)
                ops = self.CargoMutationOps(root, mode)
                with self.assertRaises(PolicyFailure) as caught:
                    audit(str(root / "scripts/check_no_unsafe.py"), ops=ops)
                self.assertEqual(caught.exception.code, "E_CARGO_MANIFEST_RACE", (caught.exception.detail, ops.opens))
                self.assertNotEqual(caught.exception.code, "E_CARGO_MANIFEST_TOML")
                self.assertEqual(ops.opens, 4)
                self.assertTrue(ops.mutated)


class ArchiveExactRaceTests(unittest.TestCase):
    class MutationOps(RealFileOps):
        def __init__(self, archive: Path, mode: str) -> None:
            super().__init__()
            self.archive = archive
            self.mode = mode
            self.opens = 0
            self.final_fd: int | None = None
            self.data_fd: int | None = None
            self.data_fstats = 0
            self.mutated = False

        def replace(self) -> None:
            replacement = self.archive.with_name("replacement.tar.gz")
            shutil.copyfile(self.archive, replacement)
            os.replace(replacement, self.archive)
            self.mutated = True

        def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
            if name == self.archive.name:
                self.opens += 1
                if self.mode == "replace_final" and self.opens == 2:
                    self.replace()
            result = super().openat2(parent_fd, name, flags, mode, resolve)
            if name == self.archive.name and self.opens == 2:
                self.final_fd = result
            return result

        def open_portal(self, metadata_fd: int, flags: int) -> int:
            result = super().open_portal(metadata_fd, flags)
            if self.opens == 1:
                self.data_fd = result
            return result

        def fstat(self, fd: int):  # type: ignore[no-untyped-def]
            if fd == self.data_fd:
                self.data_fstats += 1
                if self.mode == "post_parser_tuple" and self.data_fstats == 3 and not self.mutated:
                    value = os.stat(self.archive)
                    os.utime(self.archive, ns=(value.st_atime_ns, value.st_mtime_ns + 1_000_000_000))
                    self.mutated = True
            return super().fstat(fd)

        def close(self, fd: int) -> None:
            super().close(fd)
            if fd == self.final_fd and not self.mutated:
                if self.mode == "touch_parent":
                    os.utime(self.archive.parent)
                    self.mutated = True
                elif self.mode == "replace_parent":
                    parent = self.archive.parent
                    moved = parent.with_name(parent.name + "-original")
                    os.rename(parent, moved)
                    parent.mkdir()
                    self.mutated = True

    def test_archive_final_name_and_parent_windows_are_real_races(self) -> None:
        official = REPO / ARCHIVE_REL
        for mode in ("post_parser_tuple", "replace_final", "touch_parent", "replace_parent"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(
                dir=REPO / "build/engineering-quality/roadmap-wave-01-full-loop"
            ) as temporary:
                root = Path(temporary)
                archive = root / ARCHIVE_REL
                archive.parent.mkdir(parents=True)
                shutil.copyfile(official, archive)
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                ops = self.MutationOps(archive, mode)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        read_archive_inventory(fd, ops=ops)
                    self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_RACE")
                    self.assertTrue(ops.mutated)
                finally:
                    os.close(fd)


class OutputOperationTests(unittest.TestCase):
    class FaultOps(RealFileOps):
        def __init__(self, fault: str) -> None:
            super().__init__()
            self.fault = fault
            self.created = False
            self.created_fd: int | None = None
            self.interrupted = False
            self.fsyncs = 0

        def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
            if self.fault == "probe" and name == OUTPUT_BASENAME and not (flags & os.O_CREAT):
                raise OSError(errno.EACCES, "probe")
            if self.fault == "create" and flags & os.O_CREAT:
                raise OSError(errno.EACCES, "create")
            if self.fault == "create_exists" and flags & os.O_CREAT:
                raise OSError(errno.EEXIST, "create")
            result = super().openat2(parent_fd, name, flags, mode, resolve)
            if flags & os.O_CREAT:
                self.created = True
                self.created_fd = result
            return result

        def write(self, fd: int, data: bytes) -> int:
            if self.fault == "zero_write":
                return 0
            if self.fault == "write":
                raise OSError(errno.EIO, "write")
            if self.fault == "interrupt" and not self.interrupted:
                self.interrupted = True
                raise InterruptedError(errno.EINTR, "interrupt")
            if self.fault == "partial":
                return super().write(fd, data[:2])
            return super().write(fd, data)

        def fsync(self, fd: int) -> None:
            self.fsyncs += 1
            if self.fault == "fsync" and self.created:
                raise OSError(errno.EIO, "fsync")
            if self.fault == "parent_fsync" and self.fsyncs == 2:
                raise OSError(errno.EIO, "parent fsync")
            super().fsync(fd)

        def close(self, fd: int) -> None:
            if self.fault == "close" and fd == self.created_fd:
                self.created_fd = None
                super().close(fd)
                raise OSError(errno.EIO, "close")
            super().close(fd)

    def invoke(self, fault: str) -> PolicyFailure:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "build/engineering-quality/ns/run").mkdir(parents=True)
            root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(
                        root_fd,
                        f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                        b"{}\n",
                        ops=self.FaultOps(fault),
                    )
                return caught.exception
            finally:
                os.close(root_fd)

    def test_probe_and_create_failures_are_disjoint(self) -> None:
        self.assertEqual(self.invoke("probe").code, "E_REGEN_OUTPUT_PROBE")
        self.assertEqual(self.invoke("create").code, "E_REGEN_OUTPUT_CREATE")
        self.assertEqual(self.invoke("create_exists").code, "E_REGEN_OUTPUT_EXISTS")

    def test_write_and_fsync_failures_leave_residue(self) -> None:
        for fault, code in (("zero_write", "E_REGEN_SHORT_WRITE"), ("write", "E_REGEN_WRITE"), ("fsync", "E_REGEN_FSYNC")):
            with self.subTest(fault=fault):
                error = self.invoke(fault)
                self.assertEqual(error.code, code)
                self.assertIn("E_REGEN_RESIDUE", error.additional)

    def test_parent_fsync_and_close_failures_leave_residue(self) -> None:
        for fault, code in (("parent_fsync", "E_REGEN_FSYNC"), ("close", "E_REGEN_CLOSE")):
            with self.subTest(fault=fault):
                error = self.invoke(fault)
                self.assertEqual(error.code, code)
                self.assertIn("E_REGEN_RESIDUE", error.additional)

    def test_partial_and_interrupted_writes_finish_successfully(self) -> None:
        for fault in ("partial", "interrupt"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "build/engineering-quality/ns/run").mkdir(parents=True)
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    write_output(fd, f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}", b"{}\n", ops=self.FaultOps(fault))
                    self.assertEqual((root / f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}").read_bytes(), b"{}\n")
                finally:
                    os.close(fd)

    def test_output_implementation_has_no_deletion_primitive(self) -> None:
        import inspect

        from scripts import generate_no_unsafe_external_cache_manifest as generator

        source = inspect.getsource(generator.write_output)
        self.assertNotIn("unlink", source)
        self.assertNotIn("unlinkat", source)
        self.assertNotIn("rename", source)
        for operation in ("unlink", "unlinkat", "rename", "replace"):
            self.assertFalse(hasattr(generator.REAL_IO_OPS, operation))
            self.assertFalse(hasattr(generator.REAL_FILE_OPS, operation))


class DirectRealRaceTests(unittest.TestCase):
    class ReplaceOps(RealFileOps):
        def __init__(self, mode: str, parent: Path) -> None:
            super().__init__()
            self.mode = mode
            self.parent = parent
            self.output_opens = 0
            self.output_portals = 0
            self.listings = 0
            self.replaced = False

        def replace(self) -> None:
            if self.replaced:
                return
            replacement = self.parent / "replacement"
            replacement.write_bytes(b"replacement-bytes\n")
            os.replace(replacement, self.parent / OUTPUT_BASENAME)
            self.replaced = True

        def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
            if name == OUTPUT_BASENAME:
                self.output_opens += 1
                if self.mode == "before_second_metadata" and self.output_opens == 4:
                    self.replace()
            return super().openat2(parent_fd, name, flags, mode, resolve)

        def open_portal(self, metadata_fd: int, flags: int) -> int:
            target = os.readlink(f"/proc/self/fd/{metadata_fd}")
            if target.endswith("/" + OUTPUT_BASENAME):
                self.output_portals += 1
                if self.mode == "after_second_metadata" and self.output_portals == 2:
                    self.replace()
            return super().open_portal(metadata_fd, flags)

        def iter_directory(self, fd: int, *, max_bytes: int):  # type: ignore[no-untyped-def]
            target = os.readlink(f"/proc/self/fd/{fd}")
            if target == str(self.parent):
                self.listings += 1
                if self.mode == "after_final_read" and self.listings == 2:
                    self.replace()
            yield from super().iter_directory(fd, max_bytes=max_bytes)

    def test_output_replacements_at_three_exact_windows_are_residue_races(self) -> None:
        for mode in ("before_second_metadata", "after_second_metadata", "after_final_read"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                parent = root / "build/engineering-quality/ns/run"
                parent.mkdir(parents=True)
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                ops = self.ReplaceOps(mode, parent)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        write_output(
                            fd,
                            f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                            b"{}\n",
                            ops=ops,
                        )
                    self.assertEqual(caught.exception.code, "E_REGEN_RACE")
                    self.assertIn("E_REGEN_RESIDUE", caught.exception.additional)
                    self.assertTrue(ops.replaced)
                    self.assertEqual((parent / OUTPUT_BASENAME).read_bytes(), b"replacement-bytes\n")
                finally:
                    os.close(fd)

    class CheckReplaceOps(RealFileOps):
        def __init__(self, target: Path, mode: str) -> None:
            super().__init__()
            self.target = target
            self.mode = mode
            self.opens = 0
            self.portals = 0

        def replace(self) -> None:
            replacement = self.target.with_name("replacement")
            replacement.write_bytes(self.target.read_bytes().replace(b"external", b"changedx", 1))
            os.replace(replacement, self.target)

        def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
            if name == self.target.name:
                self.opens += 1
                if self.mode == "before_observation" and self.opens == 2:
                    self.replace()
            return super().openat2(parent_fd, name, flags, mode, resolve)

        def open_portal(self, metadata_fd: int, flags: int) -> int:
            target = os.readlink(f"/proc/self/fd/{metadata_fd}")
            if target.endswith("/" + self.target.name):
                self.portals += 1
                if self.mode == "after_observation" and self.portals == 2:
                    self.replace()
            return super().open_portal(metadata_fd, flags)

    def test_checked_manifest_replacements_hit_both_exact_windows(self) -> None:
        for mode in ("before_observation", "after_observation"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = root / MANIFEST_REL
                target.parent.mkdir()
                shutil.copyfile(CANONICAL_MANIFEST, target)
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        read_checked_manifest(fd, ops=self.CheckReplaceOps(target, mode))
                    self.assertEqual(caught.exception.code, "E_REGEN_CHECK_RACE")
                finally:
                    os.close(fd)


class GeneratorPreflightTests(unittest.TestCase):
    def run_generator(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", "scripts/generate_no_unsafe_external_cache_manifest.py", *arguments],
            cwd=REPO,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )

    def test_argument_and_fixed_path_errors_precede_archive_reads(self) -> None:
        for arguments in (
            (),
            ("--archive", "x"),
            ("positional",),
            ("--unknown", "x"),
            ("--archive", "x", "--archive", "x", "--out", "y", "--check-manifest", "z"),
        ):
            with self.subTest(arguments=arguments):
                result = self.run_generator(*arguments)
                self.assertEqual(result.returncode, 1)
                self.assertIn("[E_REGEN_ARGS]", result.stderr)
        output = REPO / "build/engineering-quality/ns/run" / OUTPUT_BASENAME
        valid = {
            "--archive": str(REPO / ARCHIVE_REL),
            "--out": str(output),
            "--check-manifest": str(CANONICAL_MANIFEST),
        }
        for option, bad in (
            ("--archive", "/wrong/archive"),
            ("--check-manifest", "/wrong/manifest"),
            ("--out", str(REPO / "wrong/output")),
        ):
            arguments: list[str] = []
            for name in ("--archive", "--out", "--check-manifest"):
                arguments.extend((name, bad if name == option else valid[name]))
            with self.subTest(option=option):
                result = self.run_generator(*arguments)
                self.assertEqual(result.returncode, 1)
                self.assertIn("[E_REGEN_PATH]", result.stderr)

    def test_checked_manifest_json_utf8_and_schema_causes(self) -> None:
        for raw, code in (
            (b"\xff", "E_REGEN_CHECK_UTF8"),
            (b"{", "E_REGEN_CHECK_JSON"),
            (b'{"schema":"x","records":[]}', "E_REGEN_CHECK_SCHEMA"),
            (b'{"schema":"x","schema":"y","records":[]}', "E_REGEN_CHECK_JSON"),
        ):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "scripts").mkdir()
                (root / MANIFEST_REL).write_bytes(raw)
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        read_checked_manifest(fd)
                    self.assertEqual(caught.exception.code, code)
                finally:
                    os.close(fd)

    def test_existing_directory_link_and_socket_all_map_to_output_exists(self) -> None:
        for kind in ("directory", "link", "socket"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                parent = root / "build/engineering-quality/ns/run"
                parent.mkdir(parents=True)
                target = parent / OUTPUT_BASENAME
                unix_socket: socket.socket | None = None
                if kind == "directory":
                    target.mkdir()
                elif kind == "link":
                    os.symlink("missing", target)
                else:
                    unix_socket = socket.socket(socket.AF_UNIX)
                    unix_socket.bind(str(target))
                fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        write_output(fd, f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}", b"{}\n")
                    self.assertEqual(caught.exception.code, "E_REGEN_OUTPUT_EXISTS")
                finally:
                    os.close(fd)
                    if unix_socket is not None:
                        unix_socket.close()


class CorrectionRegressionTests(unittest.TestCase):
    @staticmethod
    def _rewrite_header(raw_tar: bytes, mutate) -> bytes:  # type: ignore[no-untyped-def]
        raw = bytearray(raw_tar)
        block = bytearray(raw[:512])
        mutate(block)
        block[148:156] = b"        "
        checksum = sum(block)
        block[148:156] = f"{checksum:06o}".encode("ascii") + b"\0 "
        raw[:512] = block
        return gzip.compress(bytes(raw), mtime=0)

    @staticmethod
    def _parse_bytes(compressed: bytes, raw_size: int, *, members: int = 64, payload: int = 4096, one_member: int = 4096) -> object:
        with tempfile.NamedTemporaryFile() as archive:
            archive.write(compressed)
            archive.flush()
            descriptor = os.open(archive.name, os.O_RDONLY)
            try:
                return parse_archive_descriptor(
                    descriptor,
                    limits=ArchiveLimits(len(compressed), raw_size, members, payload, one_member, 256),
                )
            finally:
                os.close(descriptor)

    def test_archive_all_limits_accept_n_and_reject_n_plus_one(self) -> None:
        compressed, raw = ArchiveParserTests.archive_bytes()
        generous = ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, 256)
        inventory = self._parse_bytes(compressed, len(raw), members=42, payload=4096, one_member=4096)
        payload_n = sum(item["bytes"] for item in inventory.files)  # type: ignore[attr-defined]
        member_n = max(item["bytes"] for item in inventory.files)  # type: ignore[attr-defined]
        path_n = max(len(item["archive_member"].encode("ascii")) for item in (*inventory.directories, *inventory.files))  # type: ignore[attr-defined]
        cases = (
            ("compressed", ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, 256), ArchiveLimits(len(compressed) - 1, len(raw), 42, 4096, 4096, 256), "E_ARCHIVE_COMPRESSED_LIMIT"),
            ("decompressed", ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, 256), ArchiveLimits(len(compressed), len(raw) - 1, 42, 4096, 4096, 256), "E_ARCHIVE_DECOMPRESSED_LIMIT"),
            ("count", ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, 256), ArchiveLimits(len(compressed), len(raw), 41, 4096, 4096, 256), "E_ARCHIVE_MEMBER_COUNT"),
            ("payload", ArchiveLimits(len(compressed), len(raw), 42, payload_n, 4096, 256), ArchiveLimits(len(compressed), len(raw), 42, payload_n - 1, 4096, 256), "E_ARCHIVE_EXPANSION"),
            ("member", ArchiveLimits(len(compressed), len(raw), 42, 4096, member_n, 256), ArchiveLimits(len(compressed), len(raw), 42, 4096, member_n - 1, 256), "E_ARCHIVE_MEMBER_SIZE"),
            ("path", ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, path_n), ArchiveLimits(len(compressed), len(raw), 42, 4096, 4096, path_n - 1), "E_ARCHIVE_PATH_SIZE"),
        )
        self.assertEqual(generous.members, inventory.member_count)  # type: ignore[attr-defined]
        for boundary, accepted, rejected, code in cases:
            with self.subTest(boundary=boundary, value="N"):
                parsed = self._parse_with_limits(compressed, accepted)
                self.assertEqual(len(parsed.directories), 10)  # type: ignore[attr-defined]
                self.assertEqual(len(parsed.files), 32)  # type: ignore[attr-defined]
            with self.subTest(boundary=boundary, value="N+1"), self.assertRaises(PolicyFailure) as caught:
                self._parse_with_limits(compressed, rejected)
            self.assertEqual(caught.exception.code, code)

    @staticmethod
    def _parse_with_limits(compressed: bytes, limits: ArchiveLimits) -> object:
        with tempfile.NamedTemporaryFile() as archive:
            archive.write(compressed)
            archive.flush()
            descriptor = os.open(archive.name, os.O_RDONLY)
            try:
                return parse_archive_descriptor(descriptor, limits=limits)
            finally:
                os.close(descriptor)

    def test_archive_total_stream_charges_framing_extensions_and_concatenation(self) -> None:
        buffer = io.BytesIO()
        with tarfile.open(
            fileobj=buffer,
            mode="w",
            format=tarfile.PAX_FORMAT,
            pax_headers={"comment": "global control framing"},
        ) as archive:
            for index in range(10):
                info = tarfile.TarInfo(f"kani-0.67.0/library/d{index}")
                info.type = tarfile.DIRTYPE
                archive.addfile(info)
            for index in range(32):
                info = tarfile.TarInfo(f"kani-0.67.0/library/f{index}.rs")
                info.size = 1
                if index == 0:
                    info.pax_headers = {"path": "kani-0.67.0/library/f0.rs"}
                archive.addfile(info, io.BytesIO(b"x"))
        raw = buffer.getvalue()
        second_output = bytes(1024)
        compressed = gzip.compress(raw, mtime=0) + gzip.compress(second_output, mtime=0)
        total_output = len(raw) + len(second_output)
        limits = ArchiveLimits(len(compressed), total_output, 64, 4096, 4096, 256)
        parsed = self._parse_with_limits(compressed, limits)
        self.assertEqual(parsed.compressed_bytes, len(compressed))  # type: ignore[attr-defined]
        self.assertEqual(parsed.decompressed_bytes, total_output)  # type: ignore[attr-defined]
        self.assertEqual(len(parsed.directories), 10)  # type: ignore[attr-defined]
        self.assertEqual(len(parsed.files), 32)  # type: ignore[attr-defined]
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_with_limits(
                compressed,
                ArchiveLimits(len(compressed), total_output - 1, 64, 4096, 4096, 256),
            )
        self.assertEqual(caught.exception.code, "E_ARCHIVE_DECOMPRESSED_LIMIT")
        trailing = gzip.compress(raw, mtime=0) + gzip.compress(bytes(1024) + b"x", mtime=0)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_with_limits(
                trailing,
                ArchiveLimits(len(trailing), len(raw) + 1025, 64, 4096, 4096, 256),
            )
        self.assertEqual(caught.exception.code, "E_ARCHIVE_TAR")
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_with_limits(
                trailing,
                ArchiveLimits(len(trailing), len(raw) + 1024, 64, 4096, 4096, 256),
            )
        self.assertEqual(caught.exception.code, "E_ARCHIVE_DECOMPRESSED_LIMIT")

    def test_archive_exhaustive_path_kind_and_terminal_causes(self) -> None:
        _compressed, raw = ArchiveParserTests.one_member_archive("plain")
        path_cases = (
            (b"", "E_ARCHIVE_PATH_EMPTY"),
            (b".", "E_ARCHIVE_PATH_DOT"),
            (b"a//b", "E_ARCHIVE_PATH_EMPTY"),
            (b"../escape", "E_ARCHIVE_PATH_ESCAPE"),
            (b"/absolute", "E_ARCHIVE_PATH_ABSOLUTE"),
            (b"bad\\name", "E_ARCHIVE_PATH_BACKSLASH"),
            (b"a\0b", "E_ARCHIVE_PATH_NUL"),
            (b"\xff", "E_ARCHIVE_PATH_ENCODING"),
        )
        for path, code in path_cases:
            def replace_path(block: bytearray, value: bytes = path) -> None:
                block[0:100] = bytes(100)
                block[0:len(value)] = value
            with self.subTest(category="path", path=path, code=code), self.assertRaises(PolicyFailure) as caught:
                self._parse_bytes(self._rewrite_header(raw, replace_path), len(raw))
            self.assertEqual(caught.exception.code, code)
        for typeflag, code in (
            (b"1", "E_ARCHIVE_KIND_HARDLINK"),
            (b"2", "E_ARCHIVE_KIND_SYMLINK"),
            (b"3", "E_ARCHIVE_KIND_DEVICE"),
            (b"4", "E_ARCHIVE_KIND_DEVICE"),
            (b"6", "E_ARCHIVE_KIND_FIFO"),
            (b"s", "E_ARCHIVE_KIND_SOCKET"),
            (b"S", "E_ARCHIVE_KIND_SPARSE"),
            (b"K", "E_ARCHIVE_KIND_UNSUPPORTED"),
            (b"Z", "E_ARCHIVE_KIND_UNSUPPORTED"),
        ):
            with self.subTest(category="kind", typeflag=typeflag, code=code), self.assertRaises(PolicyFailure) as caught:
                self._parse_bytes(
                    self._rewrite_header(raw, lambda block, value=typeflag: block.__setitem__(156, value[0])),
                    len(raw),
                )
            self.assertEqual(caught.exception.code, code)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_with_limits(b"not gzip", ArchiveLimits(64, 64, 8, 8, 8, 256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_GZIP")
        compressed, one_raw = ArchiveParserTests.one_member_archive("plain")
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_with_limits(compressed, ArchiveLimits(len(compressed), len(one_raw), 8, 8, 8, 256))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_LIBRARY_INVENTORY")

    def test_archive_extension_headers_charge_member_and_payload_limits(self) -> None:
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=tarfile.GNU_FORMAT) as archive:
            name = "kani-0.67.0/library/" + ("long-name-" * 16) + ".rs"
            info = tarfile.TarInfo(name)
            info.size = 1
            archive.addfile(info, io.BytesIO(b"x"))
        raw = buffer.getvalue()
        compressed = gzip.compress(raw, mtime=0)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(compressed, len(raw), members=1)
        self.assertEqual(caught.exception.code, "E_ARCHIVE_MEMBER_COUNT")
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(compressed, len(raw), payload=1)
        self.assertEqual(caught.exception.code, "E_ARCHIVE_EXPANSION")

    def test_archive_path_kind_size_precedence_and_socket_nul_branches(self) -> None:
        _compressed, raw = ArchiveParserTests.one_member_archive("plain")

        socket_archive = self._rewrite_header(
            raw,
            lambda block: block.__setitem__(156, ord("s")),
        )
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(socket_archive, len(raw))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_KIND_SOCKET")

        def embedded_nul(block: bytearray) -> None:
            block[0:100] = bytes(100)
            block[0:3] = b"a\0b"

        nul_archive = self._rewrite_header(raw, embedded_nul)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(nul_archive, len(raw))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_PATH_NUL")

        def link_with_large_size(block: bytearray) -> None:
            block[156] = ord("2")
            block[124:136] = b"00000010000\0"

        precedence_archive = self._rewrite_header(raw, link_with_large_size)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(precedence_archive, len(raw), one_member=8)
        self.assertEqual(caught.exception.code, "E_ARCHIVE_KIND_SYMLINK")

    def test_extension_path_global_pax_and_cross_cause_precedence(self) -> None:
        _compressed, raw = ArchiveParserTests.one_member_archive("plain")

        def malformed_n_plus_one(block: bytearray) -> None:
            block[0:4] = b"a\0b\0"
            block[124:136] = b"not-octal!!!"

        malformed = self._rewrite_header(raw, malformed_n_plus_one)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(malformed, len(raw), members=0)
        self.assertEqual(caught.exception.code, "E_ARCHIVE_MEMBER_COUNT")

        def malformed_extension_path(block: bytearray) -> None:
            block[0:100] = bytes(100)
            block[0:7] = b"../bad\0"
            block[156] = ord("x")

        malformed = self._rewrite_header(raw, malformed_extension_path)
        with self.assertRaises(PolicyFailure) as caught:
            self._parse_bytes(malformed, len(raw))
        self.assertEqual(caught.exception.code, "E_ARCHIVE_PATH_ESCAPE")

        buffer = io.BytesIO()
        with tarfile.open(
            fileobj=buffer,
            mode="w",
            format=tarfile.PAX_FORMAT,
            pax_headers={"comment": "global metadata is retained by parser state"},
        ) as archive:
            for index in range(10):
                info = tarfile.TarInfo(f"kani-0.67.0/library/d{index}")
                info.type = tarfile.DIRTYPE
                archive.addfile(info)
            for index in range(32):
                info = tarfile.TarInfo(f"kani-0.67.0/library/f{index}.rs")
                info.size = 1
                archive.addfile(info, io.BytesIO(b"x"))
        raw = buffer.getvalue()
        compressed = gzip.compress(raw, mtime=0)
        parsed = self._parse_bytes(compressed, len(raw), members=64, payload=4096)
        self.assertEqual(parsed.member_count, 42)  # type: ignore[attr-defined]

    def test_declared_aggregates_precede_reads_and_exact_directory_eof(self) -> None:
        class NoReadOps(RealFileOps):
            def __init__(self) -> None:
                super().__init__()
                self.read_calls = 0

            def read(self, fd: int, count: int) -> bytes:
                self.read_calls += 1
                raise AssertionError("data read occurred after declared aggregate overflow")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "home"
            (home / "library").mkdir(parents=True)
            (home / "library/a").write_bytes(b"x")
            root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = NoReadOps()
            try:
                with mock.patch(
                    "scripts.check_no_unsafe.MAX_CANDIDATE_BYTES", 0
                ), self.assertRaises(PolicyFailure) as caught:
                    scan_candidate(
                            root_fd,
                            "home",
                            "home",
                            {"directories": [], "files": []},
                            ScanCounters(),
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_LIMIT_CANDIDATE_BYTES")
                self.assertEqual(ops.read_calls, 0)
            finally:
                os.close(root_fd)

        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            repository.mkdir()
            (repository / "one.rs").write_bytes(b"x")
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(outer_fd, "repository")
            ops = NoReadOps()
            try:
                with mock.patch(
                    "scripts.check_no_unsafe.MAX_SOURCE_BYTES", 0
                ), self.assertRaises(PolicyFailure) as caught:
                    scan_sources(
                            BoundRepository(str(repository), held),
                            {"cache_home_paths": CANDIDATE_HOMES, "directories": [], "files": []},
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_LIMIT_REPOSITORY_RUST")
                self.assertEqual(ops.read_calls, 0)
            finally:
                held.close()
                os.close(outer_fd)

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / ("n" * 255)).write_bytes(b"")
            descriptor = os.open(directory, os.O_RDONLY | O_DIRECTORY)
            libc = ctypes.CDLL(None, use_errno=True)
            charged = 0
            try:
                os.lseek(descriptor, 0, os.SEEK_SET)
                while True:
                    buffer = ctypes.create_string_buffer(DIRECTORY_READ_CHUNK)
                    count = libc.syscall(
                        SYS_GETDENTS64,
                        ctypes.c_int(descriptor),
                        ctypes.byref(buffer),
                        ctypes.c_uint(DIRECTORY_READ_CHUNK),
                    )
                    self.assertGreaterEqual(count, 0)
                    if count == 0:
                        break
                    charged += int(count)
                self.assertEqual(
                    list(RealFileOps().iter_directory(descriptor, max_bytes=charged)),
                    ["n" * 255],
                )
                with self.assertRaises(OSError) as caught:
                    list(RealFileOps().iter_directory(descriptor, max_bytes=charged - 1))
                self.assertEqual(caught.exception.errno, errno.EOVERFLOW)
            finally:
                os.close(descriptor)

    def test_repository_and_candidate_entry_n_and_n_plus_one(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            repository.mkdir()
            (repository / "a").write_bytes(b"")
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(outer_fd, "repository")
            record = {"cache_home_paths": CANDIDATE_HOMES, "directories": [], "files": []}
            try:
                with mock.patch("scripts.check_no_unsafe.MAX_REPOSITORY_ENTRIES", 1):
                    _diagnostics, _candidates, counters = scan_sources(
                        BoundRepository(str(repository), held), record
                    )
                    self.assertEqual(counters.entries, 1)
                    (repository / "b").write_bytes(b"")
                    with self.assertRaises(PolicyFailure) as caught:
                        scan_sources(BoundRepository(str(repository), held), record)
                    self.assertEqual(caught.exception.code, "E_LIMIT_REPOSITORY_ENTRIES")
            finally:
                held.close()
                os.close(outer_fd)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "home/library"
            library.mkdir(parents=True)
            (library / "a").write_bytes(b"")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with mock.patch("scripts.check_no_unsafe.MAX_CANDIDATE_ENTRIES", 1), mock.patch(
                    "scripts.check_no_unsafe.MAX_CANDIDATE_FILES", 1
                ):
                    result = scan_candidate(
                        descriptor,
                        "home",
                        "home",
                        {"directories": [], "files": []},
                        ScanCounters(),
                    )
                    self.assertEqual(result.candidate_bytes, 0)
                    (library / "b").write_bytes(b"")
                    with self.assertRaises(PolicyFailure) as caught:
                        scan_candidate(
                            descriptor,
                            "home",
                            "home",
                            {"directories": [], "files": []},
                            ScanCounters(),
                        )
                    self.assertEqual(caught.exception.code, "E_LIMIT_CANDIDATE_ENTRIES")
                    with mock.patch("scripts.check_no_unsafe.MAX_CANDIDATE_ENTRIES", 2), mock.patch(
                        "scripts.check_no_unsafe.MAX_CANDIDATE_FILES", 1
                    ):
                        with self.assertRaises(PolicyFailure) as caught:
                            scan_candidate(
                                descriptor,
                                "home",
                                "home",
                                {"directories": [], "files": []},
                                ScanCounters(),
                            )
                        self.assertEqual(caught.exception.code, "E_LIMIT_CANDIDATE_FILES")
            finally:
                os.close(descriptor)

    def test_close_causes_retain_primary_and_generator_family(self) -> None:
        class FailCloseOps(RealFileOps):
            def require_supported(
                self,
                family: str,
                *,
                require_openat2: bool = False,
                openat2_resolve: int | None = None,
            ) -> None:
                del family, require_openat2, openat2_resolve

            def close(self, fd: int) -> None:
                del fd
                raise OSError(errno.EIO, "injected close")

        fake_stat = os.stat_result((0o040755, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        held = open_root_directory()
        try:
            held_for_failure = type(held)(101, 102, fake_stat)
            with self.assertRaises(PolicyFailure) as caught:
                try:
                    raise PolicyFailure("E_PRIMARY", "primary")
                finally:
                    held_for_failure.close(FailCloseOps(), "E_TEST_CLOSE")
            self.assertEqual(caught.exception.code, "E_PRIMARY")
            self.assertIn("E_TEST_CLOSE", caught.exception.additional)
        finally:
            held.close()

        bound = BoundRepository("/repo", type(held)(201, 202, fake_stat))
        arguments = [
            "--archive", f"/repo/{ARCHIVE_REL}",
            "--out", f"/repo/build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
            "--check-manifest", f"/repo/{MANIFEST_REL}",
        ]
        with mock.patch(
            "scripts.generate_no_unsafe_external_cache_manifest.bind_invocation",
            return_value=bound,
        ), mock.patch(
            "scripts.generate_no_unsafe_external_cache_manifest.read_archive_inventory",
            side_effect=PolicyFailure("E_REGEN_ARCHIVE_HASH", "primary"),
        ), self.assertRaises(PolicyFailure) as caught:
            generate(arguments, file_ops=FailCloseOps(), io_ops=FailCloseOps())
        self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_HASH")
        self.assertIn("E_REGEN_FS_CLOSE", caught.exception.additional)

    def test_closed_manifest_accepts_format_only_and_rejects_nested_excess(self) -> None:
        canonical = json.loads(CANONICAL_MANIFEST.read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / MANIFEST_REL
            target.parent.mkdir()
            target.write_text(json.dumps(canonical, separators=(",", ":")))
            root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                value, _raw = read_checked_manifest(root_fd)
                self.assertEqual(value, canonical)
                malformed = json.loads(CANONICAL_MANIFEST.read_text())
                malformed["records"][0]["tool"]["unknown"] = "rejected"
                target.write_text(json.dumps(malformed))
                with self.assertRaises(PolicyFailure) as caught:
                    read_checked_manifest(root_fd)
                self.assertEqual(caught.exception.code, "E_REGEN_CHECK_SCHEMA")
            finally:
                os.close(root_fd)

    def test_git_authority_parsers_reject_duplicate_path_and_mode_type_defects(self) -> None:
        home = CANDIDATE_HOMES[0]
        closure = {
            "/".join(home.split("/")[:index])
            for index in range(1, len(home.split("/")) + 1)
        }
        oid = b"0" * 40
        valid_index = b"100644 " + oid + b" 0\t" + home.encode() + b"/library/file\0"
        self.assertEqual(_parse_index_paths(valid_index, closure), {home + "/library/file"})
        for raw in (
            valid_index + valid_index,
            b"100644 " + oid + b" 0\toutside/path\0",
            b"100644 " + oid + b" 0\t" + home.encode() + b"/bad\npath\0",
            b"040000 " + oid + b" 0\t" + home.encode() + b"\0",
        ):
            with self.subTest(raw=raw), self.assertRaises(PolicyFailure) as caught:
                _parse_index_paths(raw, closure)
            self.assertEqual(caught.exception.code, "E_GIT_SCHEMA")
        malformed_tree = b"100644 tree " + oid + b"\t" + home.encode() + b"\0"
        with self.assertRaises(PolicyFailure) as caught:
            _parse_tree_paths(malformed_tree, closure)
        self.assertEqual(caught.exception.code, "E_GIT_SCHEMA")

    def test_cargo_metadata_duplicate_and_type_errors_are_policy_failures(self) -> None:
        malformed_values = (
            {"packages": [], "workspace_members": [["unhashable"]]},
            {"packages": [{"id": "x"}, {"id": "x"}], "workspace_members": []},
            {"packages": [], "workspace_members": ["missing"]},
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / "scripts"
            scripts.mkdir()
            script = scripts / "check_no_unsafe.py"
            script.write_text("# fixture\n")
            bound = bind_invocation(str(script), "check_no_unsafe.py")
            try:
                for value in malformed_values:
                    with self.subTest(value=value), mock.patch(
                        "scripts.check_no_unsafe._cargo_metadata", return_value=value
                    ):
                        with self.assertRaises(PolicyFailure) as caught:
                            cargo_view(bound)
                        self.assertEqual(caught.exception.code, "E_METADATA_SCHEMA")
            finally:
                bound.close()

    def test_cargo_metadata_output_overflow_has_output_cause(self) -> None:
        with mock.patch(
            "scripts.check_no_unsafe.run_bounded",
            side_effect=BoundedProcessError("process stdout exceeded 16777216 bytes"),
        ), self.assertRaises(PolicyFailure) as caught:
            _cargo_metadata(mock.Mock(path="/tmp/repository"))
        self.assertEqual(caught.exception.code, "E_METADATA_OUTPUT")

    def test_required_primitive_unavailability_has_family_specific_cause(self) -> None:
        class UnsupportedOps(RealFileOps):
            def require_supported(
                self,
                family: str,
                *,
                require_openat2: bool = False,
                openat2_resolve: int | None = None,
            ) -> None:
                del require_openat2, openat2_resolve
                raise PolicyFailure(f"{family}_UNSUPPORTED", "injected unsupported primitive")

        with self.assertRaises(PolicyFailure) as caught:
            bind_invocation("scripts/check_no_unsafe.py", "check_no_unsafe.py", ops=UnsupportedOps())
        self.assertEqual(caught.exception.code, "E_FS_UNSUPPORTED")
        with self.assertRaises(PolicyFailure) as caught:
            generate(
                ["--archive", "x", "--out", "y", "--check-manifest", "z"],
                file_ops=UnsupportedOps(),
                io_ops=UnsupportedOps(),
            )
        self.assertEqual(caught.exception.code, "E_REGEN_FS_UNSUPPORTED")

    def test_real_primitive_preflight_probes_every_production_flag_and_resolve_bit(self) -> None:
        from scripts import no_unsafe_fs

        for missing_flag in ("O_PATH", "O_NONBLOCK"):
            with self.subTest(missing_flag=missing_flag), mock.patch.object(no_unsafe_fs, missing_flag, 0), self.assertRaises(PolicyFailure) as caught:
                RealFileOps().require_supported(
                    "E_REGEN_FS", require_openat2=True, openat2_resolve=RESOLVE_INPUT
                )
            self.assertEqual(caught.exception.code, "E_REGEN_FS_UNSUPPORTED")

        class ProbeFailureOps(RealFileOps):
            def __init__(self, fault: str) -> None:
                super().__init__()
                self.fault = fault
                self.seen_resolve: int | None = None

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                if self.fault == "portal":
                    raise OSError(errno.ENOTSUP, "portal unsupported")
                if self.fault == "nonblock" and flags & os.O_NONBLOCK:
                    raise OSError(errno.ENOTSUP, "nonblock unsupported")
                return super().open_portal(metadata_fd, flags)

            def iter_directory(self, fd: int, *, max_bytes: int):  # type: ignore[no-untyped-def]
                if self.fault == "getdents":
                    raise OSError(errno.ENOTSUP, "getdents unsupported")
                return super().iter_directory(fd, max_bytes=max_bytes)

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                self.seen_resolve = resolve
                if self.fault == "openat2" or self.fault == "no_xdev" and resolve & RESOLVE_NO_XDEV:
                    raise OSError(errno.ENOTSUP, f"{self.fault} unsupported")
                return super().openat2(parent_fd, name, flags, mode, resolve)

        for fault in ("portal", "nonblock", "getdents", "openat2", "no_xdev"):
            with self.subTest(fault=fault):
                ops = ProbeFailureOps(fault)
                with self.assertRaises(PolicyFailure) as caught:
                    ops.require_supported(
                        "E_REGEN_FS", require_openat2=True, openat2_resolve=RESOLVE_INPUT
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_FS_UNSUPPORTED")
                if fault in ("openat2", "no_xdev"):
                    self.assertEqual(ops.seen_resolve, RESOLVE_INPUT)
                    self.assertTrue(ops.seen_resolve & RESOLVE_NO_XDEV)

    def test_directory_enumeration_is_incremental_and_byte_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index in range(600):
                (root / f"entry-{index:04d}").write_bytes(b"")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                iterator = RealFileOps().iter_directory(descriptor, max_bytes=4096)
                with self.assertRaises(OSError) as caught:
                    list(iterator)
                self.assertEqual(caught.exception.errno, errno.EOVERFLOW)
            finally:
                os.close(descriptor)

        class FirstOnlyOps(RealFileOps):
            def iter_directory(self, fd: int, *, max_bytes: int):  # type: ignore[no-untyped-def]
                del fd, max_bytes
                yield "other"
                raise AssertionError("precreate enumeration continued after first entry")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "build/engineering-quality/ns/run").mkdir(parents=True)
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(
                        descriptor,
                        f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                        b"{}\n",
                        ops=FirstOnlyOps(),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
            finally:
                os.close(descriptor)

    def test_initial_checked_manifest_parent_replay_precedes_data_upgrade(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / MANIFEST_REL
            target.parent.mkdir()
            shutil.copyfile(CANONICAL_MANIFEST, target)

            class MoveScriptsOps(CountingOps):
                def __init__(self) -> None:
                    super().__init__()
                    self.moved = False
                    self.manifest_portals = 0

                def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                    result = super().openat2(parent_fd, name, flags, mode, resolve)
                    if name == target.name and not self.moved:
                        os.rename(root / "scripts", root / "scripts-old")
                        (root / "scripts").mkdir()
                        self.moved = True
                    return result

                def open_portal(self, metadata_fd: int, flags: int) -> int:
                    if os.readlink(f"/proc/self/fd/{metadata_fd}").endswith("/" + target.name):
                        self.manifest_portals += 1
                    return super().open_portal(metadata_fd, flags)

            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = MoveScriptsOps()
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    read_checked_manifest(descriptor, ops=ops)
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
                self.assertEqual(ops.manifest_portals, 0)
            finally:
                os.close(descriptor)

    def test_output_parent_replay_precedes_probe(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent = root / "build/engineering-quality/ns/run"
            parent.mkdir(parents=True)

            class MoveOutputParent(RealFileOps):
                def __init__(self) -> None:
                    super().__init__()
                    self.moved = False
                    self.output_probes = 0

                def open_portal(self, metadata_fd: int, flags: int) -> int:
                    result = super().open_portal(metadata_fd, flags)
                    if os.readlink(f"/proc/self/fd/{metadata_fd}") == str(parent) and not self.moved:
                        os.rename(parent, parent.with_name("run-old"))
                        parent.mkdir()
                        self.moved = True
                    return result

                def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                    if name == OUTPUT_BASENAME:
                        self.output_probes += 1
                    return super().openat2(parent_fd, name, flags, mode, resolve)

            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = MoveOutputParent()
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(
                        descriptor,
                        f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                        b"{}\n",
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
                self.assertEqual(ops.output_probes, 0)
            finally:
                os.close(descriptor)

    def test_held_inode_reads_precharge_global_rust_and_use_source_causes(self) -> None:
        from scripts import check_no_unsafe as scanner

        original = b"fn original() {}\n"
        replacement = b"unsafe fn replacement() {}\n"

        class ReplaceAfterMetadataOps(RealFileOps):
            def __init__(self, target: Path, pending: Path) -> None:
                super().__init__()
                self.target = target
                self.pending = pending
                self.replaced = False
                self.read_payloads: list[bytes] = []

            def fstat(self, fd: int):  # type: ignore[no-untyped-def]
                value = super().fstat(fd)
                try:
                    path = os.readlink(f"/proc/self/fd/{fd}")
                except OSError:
                    return value
                if path == str(self.target) and not self.replaced:
                    os.replace(self.pending, self.target)
                    self.replaced = True
                return value

            def read(self, fd: int, count: int) -> bytes:
                value = super().read(fd, count)
                self.read_payloads.append(value)
                return value

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "home/library"
            library.mkdir(parents=True)
            target = library / "a.rs"
            pending = library / "pending"
            target.write_bytes(original)
            pending.write_bytes(replacement)
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = ReplaceAfterMetadataOps(target, pending)
            record = {
                "directories": [],
                "files": [{
                    "path": "a.rs",
                    "bytes": len(original),
                    "sha256": __import__("hashlib").sha256(original).hexdigest(),
                }],
            }
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    scan_candidate(
                        descriptor,
                        "home",
                        "home",
                        record,
                        ScanCounters(),
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_FS_RACE")
                self.assertTrue(ops.replaced)
                consumed = b"".join(ops.read_payloads)
                self.assertNotIn(replacement, consumed)
                self.assertIn(consumed, (b"", original))
            finally:
                os.close(descriptor)

        class NoReadOps(RealFileOps):
            def __init__(self) -> None:
                super().__init__()
                self.read_calls = 0

            def read(self, fd: int, count: int) -> bytes:
                del fd, count
                self.read_calls += 1
                raise AssertionError("candidate bytes were read after global Rust overflow")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "home/library"
            library.mkdir(parents=True)
            (library / "a.rs").write_bytes(b"x")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = NoReadOps()
            try:
                with mock.patch.object(scanner, "MAX_RUST_FILES", 0), self.assertRaises(
                    PolicyFailure
                ) as caught:
                    scan_candidate(
                        descriptor,
                        "home",
                        "home",
                        {"directories": [], "files": []},
                        ScanCounters(),
                        ops=ops,
                    )
                self.assertEqual(caught.exception.code, "E_LIMIT_REPOSITORY_RUST")
                self.assertEqual(ops.read_calls, 0)
            finally:
                os.close(descriptor)

        class SourceInitialFaultOps(RealFileOps):
            def __init__(self, fault: str) -> None:
                super().__init__()
                self.fault = fault
                self.initial_fd: int | None = None

            def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                if name == "one.rs" and self.fault == "open":
                    raise OSError(errno.EIO, "injected source open")
                descriptor = super().openat(parent_fd, name, flags, mode)
                if name == "one.rs" and self.initial_fd is None:
                    self.initial_fd = descriptor
                return descriptor

            def fstat(self, fd: int):  # type: ignore[no-untyped-def]
                if fd == self.initial_fd and self.fault == "metadata":
                    raise OSError(errno.EIO, "injected source metadata")
                return super().fstat(fd)

            def close(self, fd: int) -> None:
                if fd == self.initial_fd and self.fault == "close":
                    super().close(fd)
                    self.initial_fd = None
                    raise OSError(errno.EIO, "injected source close")
                super().close(fd)

        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            repository.mkdir()
            (repository / "one.rs").write_bytes(b"fn main() {}\n")
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(outer_fd, "repository")
            try:
                for fault, code in (
                    ("open", "E_SOURCE_OPEN"),
                    ("metadata", "E_SOURCE_METADATA"),
                    ("close", "E_SOURCE_CLOSE"),
                ):
                    with self.subTest(fault=fault), self.assertRaises(
                        PolicyFailure
                    ) as caught:
                        scan_sources(
                            BoundRepository(str(repository), held),
                            {"cache_home_paths": CANDIDATE_HOMES},
                            ops=SourceInitialFaultOps(fault),
                        )
                    self.assertEqual(caught.exception.code, code)
            finally:
                held.close()
                os.close(outer_fd)

    def test_directory_budget_derivation_covers_dot_records_exact_names_and_eof(self) -> None:
        from scripts import check_no_unsafe as scanner

        self.assertEqual(
            scanner._directory_byte_budget(1, 1),
            (1 + 2) * scanner.MAX_DIRENT64_BYTES,
        )
        self.assertEqual(
            scanner._candidate_directory_byte_budget(1, 0),
            (1 + 2) * scanner.MAX_DIRENT64_BYTES,
        )
        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            repository.mkdir()
            exact_name = "n" * 255
            (repository / exact_name).write_bytes(b"")
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(outer_fd, "repository")
            try:
                with mock.patch.object(scanner, "MAX_REPOSITORY_ENTRIES", 1), mock.patch.object(
                    scanner, "MAX_REPOSITORY_DIRECTORIES", 1
                ):
                    _diagnostics, _candidates, counters = scan_sources(
                        BoundRepository(str(repository), held),
                        {"cache_home_paths": CANDIDATE_HOMES},
                    )
                    self.assertEqual((counters.entries, counters.directories), (1, 1))
                    (repository / ("m" * 255)).write_bytes(b"")
                    with self.assertRaises(PolicyFailure) as caught:
                        scan_sources(
                            BoundRepository(str(repository), held),
                            {"cache_home_paths": CANDIDATE_HOMES},
                        )
                    self.assertEqual(caught.exception.code, "E_LIMIT_REPOSITORY_ENTRIES")
            finally:
                held.close()
                os.close(outer_fd)

        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            repository.mkdir()
            (repository / ("d" * 255)).mkdir()
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(outer_fd, "repository")
            try:
                with mock.patch.object(scanner, "MAX_REPOSITORY_ENTRIES", 1), mock.patch.object(
                    scanner, "MAX_REPOSITORY_DIRECTORIES", 2
                ):
                    _diagnostics, _candidates, counters = scan_sources(
                        BoundRepository(str(repository), held),
                        {"cache_home_paths": CANDIDATE_HOMES},
                    )
                    self.assertEqual((counters.entries, counters.directories), (1, 2))
            finally:
                held.close()
                os.close(outer_fd)

    def test_git_reported_paths_bind_descriptors_and_c0_precedes_i0(self) -> None:
        from scripts import check_no_unsafe as scanner

        for replace_git in (False, True):
            with self.subTest(replace_git=replace_git), tempfile.TemporaryDirectory() as temporary:
                outer = Path(temporary)
                repository = outer / "repository"
                (repository / ".git").mkdir(parents=True)
                outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
                bound_dir = open_directory_at(outer_fd, "repository")
                git_dir = open_directory_at(bound_dir.data_fd, ".git")
                bound = BoundRepository(str(repository), bound_dir)
                expected_path = repository / ".git" if replace_git else repository
                held = git_dir if replace_git else bound_dir
                if replace_git:
                    os.rename(repository / ".git", repository / ".git-old")
                    (repository / ".git").mkdir()
                else:
                    os.rename(repository, outer / "repository-old")
                    repository.mkdir()
                try:
                    with mock.patch.object(
                        scanner,
                        "_git",
                        return_value=mock.Mock(returncode=0, stderr=b"", stdout=str(expected_path).encode("ascii") + b"\n"),
                    ), self.assertRaises(PolicyFailure) as caught:
                        scanner._git_exact(
                            bound,
                            ["rev-parse", "--path-format=absolute", "--show-toplevel"],
                            str(expected_path).encode("ascii") + b"\n",
                            identity=held,
                            ops=RealFileOps(),
                        )
                    self.assertEqual(caught.exception.code, "E_GIT_BINDING")
                finally:
                    git_dir.close()
                    bound.close()
                    os.close(outer_fd)

        with tempfile.TemporaryDirectory() as temporary:
            outer = Path(temporary)
            repository = outer / "repository"
            git_path = repository / ".git"
            (git_path / "objects/info").mkdir(parents=True)
            (git_path / "index").write_bytes(b"index")
            outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
            bound_dir = open_directory_at(outer_fd, "repository")
            bound = BoundRepository(str(repository), bound_dir)
            events: list[str] = []
            commit = b"1" * 40

            def fake_git(_bound, suffix, **_kwargs):  # type: ignore[no-untyped-def]
                if suffix == ["rev-parse", "--verify", "HEAD^{commit}"]:
                    events.append("C0")
                    return mock.Mock(returncode=0, stderr=b"", stdout=commit + b"\n")
                if suffix and suffix[0] == "ls-files":
                    events.append("X0")
                    return mock.Mock(returncode=0, stderr=b"", stdout=b"")
                if suffix and suffix[0] == "ls-tree":
                    events.append("T0")
                    return mock.Mock(returncode=0, stderr=b"", stdout=b"")
                expected = {
                    ("rev-parse", "--path-format=absolute", "--show-toplevel"): str(repository).encode() + b"\n",
                    ("rev-parse", "--path-format=absolute", "--absolute-git-dir"): str(git_path).encode() + b"\n",
                    ("rev-parse", "--path-format=absolute", "--git-common-dir"): str(git_path).encode() + b"\n",
                    ("rev-parse", "--show-object-format=storage"): b"sha1\n",
                    ("rev-parse", "--is-bare-repository"): b"false\n",
                    ("rev-parse", "--is-inside-work-tree"): b"true\n",
                    ("rev-parse", "--path-format=absolute", "--git-path", "index"): str(git_path / "index").encode() + b"\n",
                    ("rev-parse", "--shared-index-path"): b"",
                }
                key = tuple(suffix)
                if key in expected:
                    return mock.Mock(returncode=0, stderr=b"", stdout=expected[key])
                if suffix[0] == "config":
                    value = b"false\0" if suffix[-1] == "core.bare" else b""
                    code = 0 if suffix[-1] == "core.bare" else 1
                    return mock.Mock(returncode=code, stderr=b"", stdout=value)
                raise AssertionError(suffix)

            snapshots = iter(((1, 2, 3, 4, 5, 6), "hash") for _ in range(2))

            def fake_snapshot(*_args, **_kwargs):  # type: ignore[no-untyped-def]
                events.append("I0" if events.count("I0") == 0 else "I1")
                return next(snapshots)

            git_dir: object | None = None
            try:
                with mock.patch.object(scanner, "_git", side_effect=fake_git), mock.patch.object(
                    scanner, "_index_snapshot", side_effect=fake_snapshot
                ):
                    _view, git_dir = scanner.git_view_initial(
                        bound,
                        {"records": [{"cache_home_paths": [], "directories": [], "files": []}]},
                    )
                self.assertLess(events.index("C0"), events.index("I0"))
                self.assertLess(events.index("I0"), events.index("X0"))
                self.assertLess(events.index("X0"), events.index("I1"))
                self.assertLess(events.index("I1"), events.index("T0"))
            finally:
                if git_dir is not None:
                    git_dir.close()  # type: ignore[attr-defined]
                bound.close()
                os.close(outer_fd)

    def test_parent_mount_escape_errno_maps_to_generator_confinement(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        class MountEscapeOps(RealFileOps):
            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                if name == "mount-boundary":
                    raise OSError(errno.EXDEV, "mount or escape rejected")
                return super().openat2(parent_fd, name, flags, mode, resolve)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "safe/mount-boundary").mkdir(parents=True)
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    generator._open_parent(
                        descriptor,
                        ["safe", "mount-boundary"],
                        ops=MountEscapeOps(),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
                self.assertIn("mount or escape rejected", caught.exception.detail)
            finally:
                os.close(descriptor)

    def test_specific_close_causes_and_parent_cleanup_are_retained(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        class FailNamedCloseOps(RealFileOps):
            def __init__(self, suffix: str) -> None:
                super().__init__()
                self.suffix = suffix
                self.failed = False

            def close(self, fd: int) -> None:
                try:
                    path = os.readlink(f"/proc/self/fd/{fd}")
                except OSError:
                    path = ""
                super().close(fd)
                if path.endswith(self.suffix) and not self.failed:
                    self.failed = True
                    raise OSError(errno.EIO, "injected named close")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent = root / "build/engineering-quality/ns/run"
            parent.mkdir(parents=True)
            (parent / OUTPUT_BASENAME).write_bytes(b"existing")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(
                        descriptor,
                        f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                        b"{}\n",
                        ops=FailNamedCloseOps(OUTPUT_BASENAME),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_OUTPUT_EXISTS")
                self.assertIn("E_REGEN_OUTPUT_PROBE", caught.exception.additional)
            finally:
                os.close(descriptor)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "a").mkdir()
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    generator._open_parent(
                        descriptor,
                        ["a", "missing"],
                        ops=FailNamedCloseOps("/a"),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_CONFINEMENT")
                self.assertIn("E_REGEN_CLOSE", caught.exception.additional)
            finally:
                os.close(descriptor)

    def test_checked_manifest_original_and_observation_closes_are_specific(self) -> None:
        class CheckedCloseOps(RealFileOps):
            def __init__(self, target: Path, phase: str) -> None:
                super().__init__()
                self.target = target
                self.phase = phase
                self.target_metadata: list[int] = []
                self.fail_fd: int | None = None
                self.failed = False

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                descriptor = super().openat2(parent_fd, name, flags, mode, resolve)
                if name == self.target.name:
                    self.target_metadata.append(descriptor)
                    if (
                        (self.phase == "original_metadata" and len(self.target_metadata) == 1)
                        or (self.phase == "observation_metadata" and len(self.target_metadata) == 2)
                    ):
                        self.fail_fd = descriptor
                return descriptor

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                descriptor = super().open_portal(metadata_fd, flags)
                if (
                    self.target_metadata
                    and metadata_fd == self.target_metadata[-1]
                    and (
                        (self.phase == "original_data" and len(self.target_metadata) == 1)
                        or (
                            self.phase == "observation_data"
                            and len(self.target_metadata) == 2
                        )
                    )
                ):
                    self.fail_fd = descriptor
                return descriptor

            def close(self, fd: int) -> None:
                if fd == self.fail_fd and not self.failed:
                    super().close(fd)
                    self.failed = True
                    raise OSError(errno.EIO, "injected checked-manifest close")
                super().close(fd)

        for phase in ("original_metadata", "original_data", "observation_metadata", "observation_data"):
            with self.subTest(phase=phase), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = root / MANIFEST_REL
                target.parent.mkdir()
                shutil.copyfile(CANONICAL_MANIFEST, target)
                descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        read_checked_manifest(
                            descriptor, ops=CheckedCloseOps(target, phase)
                        )
                    self.assertEqual(caught.exception.code, "E_REGEN_CHECK_CLOSE")
                finally:
                    os.close(descriptor)

        class ReadAndCloseFailureOps(RealFileOps):
            def __init__(self, target: Path) -> None:
                super().__init__()
                self.target = target
                self.metadata_fd: int | None = None
                self.data_fd: int | None = None
                self.read_failed = False
                self.close_failed = False

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                descriptor = super().openat2(parent_fd, name, flags, mode, resolve)
                if name == self.target.name:
                    self.metadata_fd = descriptor
                return descriptor

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                descriptor = super().open_portal(metadata_fd, flags)
                if metadata_fd == self.metadata_fd:
                    self.data_fd = descriptor
                return descriptor

            def read(self, fd: int, count: int) -> bytes:
                if fd == self.data_fd and not self.read_failed:
                    self.read_failed = True
                    raise OSError(errno.EIO, "injected checked read")
                return super().read(fd, count)

            def close(self, fd: int) -> None:
                if fd == self.data_fd and not self.close_failed:
                    super().close(fd)
                    self.close_failed = True
                    raise OSError(errno.EIO, "injected checked close")
                super().close(fd)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / MANIFEST_REL
            target.parent.mkdir()
            shutil.copyfile(CANONICAL_MANIFEST, target)
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            ops = ReadAndCloseFailureOps(target)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    read_checked_manifest(descriptor, ops=ops)
                self.assertEqual(caught.exception.code, "E_REGEN_CHECK_READ")
                self.assertIn("E_REGEN_CHECK_CLOSE", caught.exception.additional)
                self.assertTrue(ops.read_failed)
                self.assertTrue(ops.close_failed)
            finally:
                os.close(descriptor)

    def test_output_verification_close_is_specific_and_retains_residue(self) -> None:
        class OutputVerificationCloseOps(RealFileOps):
            def __init__(self) -> None:
                super().__init__()
                self.verification_metadata: int | None = None
                self.fail_fd: int | None = None
                self.failed = False

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                descriptor = super().openat2(parent_fd, name, flags, mode, resolve)
                if (
                    name == OUTPUT_BASENAME
                    and flags & O_PATH
                    and self.verification_metadata is None
                ):
                    self.verification_metadata = descriptor
                return descriptor

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                descriptor = super().open_portal(metadata_fd, flags)
                if metadata_fd == self.verification_metadata:
                    self.fail_fd = descriptor
                return descriptor

            def close(self, fd: int) -> None:
                if fd == self.fail_fd and not self.failed:
                    super().close(fd)
                    self.failed = True
                    raise OSError(errno.EIO, "injected output verification close")
                super().close(fd)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "build/engineering-quality/ns/run").mkdir(parents=True)
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    write_output(
                        descriptor,
                        f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",
                        b"{}\n",
                        ops=OutputVerificationCloseOps(),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_CLOSE")
                self.assertIn("E_REGEN_RESIDUE", caught.exception.additional)
            finally:
                os.close(descriptor)

    def test_archive_original_and_observation_closes_are_specific(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        class ArchiveCloseOps(RealFileOps):
            def __init__(self, target: Path, phase: str) -> None:
                super().__init__()
                self.target = target
                self.phase = phase
                self.target_metadata: list[int] = []
                self.fail_fd: int | None = None
                self.failed = False

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                descriptor = super().openat2(parent_fd, name, flags, mode, resolve)
                if name == self.target.name:
                    self.target_metadata.append(descriptor)
                    if self.phase == "metadata" and len(self.target_metadata) == 1:
                        self.fail_fd = descriptor
                    if self.phase == "observation" and len(self.target_metadata) == 2:
                        self.fail_fd = descriptor
                return descriptor

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                descriptor = super().open_portal(metadata_fd, flags)
                if (
                    self.phase == "original"
                    and self.target_metadata
                    and metadata_fd == self.target_metadata[0]
                ):
                    self.fail_fd = descriptor
                return descriptor

            def close(self, fd: int) -> None:
                if fd == self.fail_fd and not self.failed:
                    super().close(fd)
                    self.failed = True
                    raise OSError(errno.EIO, "injected archive close")
                super().close(fd)

        for phase in ("metadata", "original", "observation"):
            with self.subTest(phase=phase), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = root / ARCHIVE_REL
                target.parent.mkdir(parents=True)
                target.write_bytes(b"x")
                descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
                inventory = generator.ArchiveInventory((), (), 1, 0, 0)
                try:
                    with mock.patch.object(generator, "ARCHIVE_BYTES", 1), mock.patch.object(
                        generator,
                        "ARCHIVE_SHA256",
                        __import__("hashlib").sha256(b"x").hexdigest(),
                    ), mock.patch.object(
                        generator, "parse_archive_descriptor", return_value=inventory
                    ), self.assertRaises(PolicyFailure) as caught:
                        read_archive_inventory(
                            descriptor,
                            ops=ArchiveCloseOps(target, phase),
                            limits=ArchiveLimits(1, 1, 1, 1, 1, 256),
                        )
                    self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_CLOSE")
                finally:
                    os.close(descriptor)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / ARCHIVE_REL
            target.parent.mkdir(parents=True)
            target.write_bytes(b"x")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            try:
                with mock.patch.object(generator, "ARCHIVE_BYTES", 1), mock.patch.object(
                    generator, "ARCHIVE_SHA256", __import__("hashlib").sha256(b"x").hexdigest()
                ), mock.patch.object(
                    generator,
                    "parse_archive_descriptor",
                    side_effect=PolicyFailure("E_ARCHIVE_TAR", "injected parser primary"),
                ), self.assertRaises(PolicyFailure) as caught:
                    read_archive_inventory(
                        descriptor,
                        ops=ArchiveCloseOps(target, "original"),
                        limits=ArchiveLimits(1, 1, 1, 1, 1, 256),
                    )
                self.assertEqual(caught.exception.code, "E_ARCHIVE_TAR")
                self.assertIn("E_REGEN_ARCHIVE_CLOSE", caught.exception.additional)
            finally:
                os.close(descriptor)

    def test_archive_observation_race_keeps_observation_close_cause(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        class ObservationFailureOps(RealFileOps):
            def __init__(self, target: Path) -> None:
                super().__init__()
                self.target = target
                self.archive_opens = 0
                self.observation_fd: int | None = None

            def openat2(self, parent_fd: int, name: str, flags: int, mode: int, resolve: int) -> int:
                descriptor = super().openat2(parent_fd, name, flags, mode, resolve)
                if name == self.target.name:
                    self.archive_opens += 1
                    if self.archive_opens == 2:
                        self.observation_fd = descriptor
                return descriptor

            def fstat(self, fd: int):  # type: ignore[no-untyped-def]
                if fd == self.observation_fd:
                    raise OSError(errno.EIO, "injected observation metadata")
                return super().fstat(fd)

            def close(self, fd: int) -> None:
                if fd == self.observation_fd:
                    super().close(fd)
                    self.observation_fd = None
                    raise OSError(errno.EIO, "injected observation close")
                super().close(fd)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / ARCHIVE_REL
            target.parent.mkdir(parents=True)
            target.write_bytes(b"x")
            descriptor = os.open(root, os.O_RDONLY | O_DIRECTORY)
            inventory = generator.ArchiveInventory((), (), 1, 0, 0)
            try:
                with mock.patch.object(generator, "ARCHIVE_BYTES", 1), mock.patch.object(
                    generator, "ARCHIVE_SHA256", __import__("hashlib").sha256(b"x").hexdigest()
                ), mock.patch.object(
                    generator, "parse_archive_descriptor", return_value=inventory
                ), self.assertRaises(PolicyFailure) as caught:
                    read_archive_inventory(
                        descriptor,
                        ops=ObservationFailureOps(target),
                        limits=ArchiveLimits(1, 1, 1, 1, 1, 256),
                    )
                self.assertEqual(caught.exception.code, "E_REGEN_ARCHIVE_RACE")
                self.assertIn("E_REGEN_ARCHIVE_CLOSE", caught.exception.additional)
            finally:
                os.close(descriptor)

    def test_row_case_map_rejects_absent_unexecuted_unknown_and_wrong_level(self) -> None:
        from scripts.run_no_unsafe_row_evidence import (
            EVIDENCE_SCHEMA,
            CaseMapFailure,
            validate_case_map,
            validate_executed_evidence,
        )

        original = json.loads((REPO / "scripts/no_unsafe_row_cases_v1.json").read_bytes())
        self.assertEqual(len(validate_case_map(original)), 81)

        mutations = {}
        absent = json.loads(json.dumps(original))
        absent["rows"].pop()
        mutations["absent"] = absent
        unexecuted = json.loads(json.dumps(original))
        unexecuted["rows"][0]["runners"] = []
        mutations["unexecuted"] = unexecuted
        unknown = json.loads(json.dumps(original))
        unknown["rows"][0]["runners"][0]["selector"] = (
            "scripts.tests.test_no_unsafe_policy.MissingTests.test_absent"
        )
        mutations["unknown"] = unknown
        wrong_level = json.loads(json.dumps(original))
        wrong_level["rows"][0]["runners"][0]["level"] = "direct"
        mutations["wrong_level"] = wrong_level
        for case, malformed in mutations.items():
            with self.subTest(kind="case-map", case=case), self.assertRaises(CaseMapFailure):
                validate_case_map(malformed)

        case_rows = validate_case_map(original)
        executions = {}
        for row in case_rows:
            for runner in row["runners"]:
                key = (row["id"], runner["selector"], runner["level"], runner["official_archive"])
                assertion = {
                    "row_id": row["id"], "method": "assertEqual",
                    "test_function": "test_observed_fixture",
                    "source_file": "scripts/tests/test_no_unsafe_policy.py",
                    "source_line": 1,
                    "arguments": ["'actual'", "'actual'"], "keywords": {}, "passed": True,
                }
                outcome = {"successful": True, "tests_run": 1, "skips": 0, "assertions": [assertion]}
                executions[key] = {
                    "row_id": key[0], "selector": key[1], "level": key[2], "official_archive": key[3],
                    "argv": ["python3", "runner", key[1], "--row-id", key[0]], "cwd": str(REPO),
                    "allowlisted_environment": {}, "exit": 0, "stdout": "", "stderr": "", "outcome": outcome,
                }
        evidence_rows=[]
        for row in case_rows:
            references=[]
            observations=[]
            for runner in row["runners"]:
                key=(row["id"],runner["selector"],runner["level"],runner["official_archive"])
                execution=executions[key]
                references.append({"selector":key[1],"level":key[2],"official_archive":key[3],"exit":0,"outcome":execution["outcome"]})
                observations.append({"selector":key[1],"level":key[2],"official_archive":key[3],"assertions":execution["outcome"]["assertions"]})
            evidence_rows.append({"id":row["id"],"required_level":row["required_level"],"stimulus":row["stimulus"],"required_evidence":row["required_evidence"],"executed":True,"passed":True,"observations":observations,"executions":references})
        valid_evidence={"schema":EVIDENCE_SCHEMA,"case_map":"case-map.json","cwd":str(REPO),"row_count":81,"execution_count":len(executions),"all_executed":True,"all_passed":True,"rows":evidence_rows,"executions":list(executions.values())}
        validate_executed_evidence(valid_evidence,case_rows)
        evidence_mutations={}
        absent_evidence=json.loads(json.dumps(valid_evidence)); absent_evidence["rows"].pop(); evidence_mutations["absent"]=absent_evidence
        unexecuted_evidence=json.loads(json.dumps(valid_evidence)); unexecuted_evidence["rows"][0]["executed"]=False; evidence_mutations["unexecuted"]=unexecuted_evidence
        unknown_evidence=json.loads(json.dumps(valid_evidence)); unknown_evidence["executions"][0]["selector"]="scripts.tests.test_no_unsafe_policy.MissingTests.test_absent"; evidence_mutations["unknown"]=unknown_evidence
        wrong_level_evidence=json.loads(json.dumps(valid_evidence)); wrong_level_evidence["rows"][0]["executions"][0]["level"]="direct"; evidence_mutations["wrong_level"]=wrong_level_evidence
        no_observation=json.loads(json.dumps(valid_evidence)); no_observation["executions"][0]["outcome"]["assertions"]=[]; evidence_mutations["no_observation"]=no_observation
        copied_claim=json.loads(json.dumps(valid_evidence)); copied_claim["rows"][0]["observations"]=[{"selector":"copied","level":"CLI","official_archive":False,"assertions":[{"passed":True}]}]; evidence_mutations["copied_claim"]=copied_claim
        for case,malformed in evidence_mutations.items():
            with self.subTest(kind="executed-evidence",case=case), self.assertRaises(CaseMapFailure):
                validate_executed_evidence(malformed,case_rows)


class ExecutedRowCaseTests(unittest.TestCase):
    """Previously missing row stimuli, each executed through production code."""

    @staticmethod
    def _scanner_fixture() -> IsolatedScannerTests:
        fixture = IsolatedScannerTests(methodName="runTest")
        fixture.setUp()
        return fixture

    @staticmethod
    def _run_scanner(fixture: IsolatedScannerTests, *, path: str | None = None) -> subprocess.CompletedProcess[str]:
        environment = dict(os.environ)
        if path is not None:
            environment["PATH"] = path + os.pathsep + environment.get("PATH", os.defpath)
        return subprocess.run(
            ["python3", "scripts/check_no_unsafe.py"],
            cwd=fixture.root,
            env=environment,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )

    @staticmethod
    def _install_fake_git(root: Path, mode: str) -> Path:
        real_git = shutil.which("git")
        if real_git is None:
            raise AssertionError("git is required")
        directory = root / "fake-bin"
        directory.mkdir()
        state = root / ".git/fake-git-state"
        script = directory / "git"
        script.write_text(
            "#!/usr/bin/env python3\n"
            "import os, pathlib, subprocess, sys\n"
            f"MODE={mode!r}\nREAL={real_git!r}\nSTATE=pathlib.Path({str(state)!r})\n"
            "args=sys.argv[1:]\n"
            "work=next((x.split('=',1)[1] for x in args if x.startswith('--work-tree=')), '')\n"
            "gitdir=next((x.split('=',1)[1] for x in args if x.startswith('--git-dir=')), '')\n"
            "def count(tag):\n"
            " p=STATE.with_name(STATE.name+'-'+tag); n=int(p.read_text())+1 if p.exists() else 1; p.write_text(str(n)); return n\n"
            "if MODE=='binding' and '--show-toplevel' in args:\n"
            " sys.stdout.buffer.write(work.encode()+b'\\n\\n'); raise SystemExit(0)\n"
            "if MODE=='binding_nul' and '--show-toplevel' in args:\n"
            " sys.stdout.buffer.write(work.encode()+b'\\0\\n'); raise SystemExit(0)\n"
            "if MODE=='gitdir' and '--absolute-git-dir' in args:\n"
            " sys.stdout.buffer.write(work.encode()+b'\\n'); raise SystemExit(0)\n"
            "if MODE=='gitdir_extra' and '--absolute-git-dir' in args:\n"
            " sys.stdout.buffer.write(gitdir.encode()+b'\\n\\n'); raise SystemExit(0)\n"
            "if MODE=='common' and '--git-common-dir' in args:\n"
            " sys.stdout.buffer.write(gitdir.encode()+b'\\n\\n'); raise SystemExit(0)\n"
            "if MODE=='common_nul' and '--git-common-dir' in args:\n"
            " sys.stdout.buffer.write(gitdir.encode()+b'\\0\\n'); raise SystemExit(0)\n"
            "if MODE=='inside' and '--is-inside-work-tree' in args:\n"
            " sys.stdout.buffer.write(b'TRUE\\n'); raise SystemExit(0)\n"
            "if MODE=='config' and args[-1:] == ['core.bare']:\n"
            " sys.stdout.buffer.write(b'true\\0'); raise SystemExit(0)\n"
            "if MODE=='config_extra_nul' and args[-1:] == ['core.bare']:\n"
            " sys.stdout.buffer.write(b'false\\0\\0'); raise SystemExit(0)\n"
            "if MODE=='config_worktree' and args[-1:] == ['core.worktree']:\n"
            " sys.stdout.buffer.write(b'/decoy\\0'); raise SystemExit(0)\n"
            "if MODE=='config_extension' and args[-1:] == ['extensions.worktreeConfig']:\n"
            " sys.stdout.buffer.write(b'true\\0'); raise SystemExit(0)\n"
            "if MODE=='config_include' and '--get-regexp' in args:\n"
            " sys.stdout.buffer.write(b'include.path\\nelsewhere\\0'); raise SystemExit(0)\n"
            "if MODE=='head_extra' and 'HEAD^{commit}' in args:\n"
            " result=subprocess.run([REAL,*args],capture_output=True); sys.stdout.buffer.write(result.stdout+b'\\n'); sys.stderr.buffer.write(result.stderr); raise SystemExit(result.returncode)\n"
            "if MODE in ('head_c1','head_c2') and 'HEAD^{commit}' in args:\n"
            " n=count('head'); target=2 if MODE=='head_c1' else 3\n"
            " if n==target: sys.stdout.write('0'*40+'\\n'); raise SystemExit(0)\n"
            "if MODE=='tree_t1' and 'ls-tree' in args and count('tree') > 1:\n"
            " sys.stdout.buffer.write(b'changed'); raise SystemExit(0)\n"
            "if MODE in ('status_lf','status_partial') and 'ls-files' in args:\n"
            " sys.stdout.buffer.write((b'100644 '+b'0'*40+b' 0\\tbad\\nname\\0') if MODE=='status_lf' else b'partial'); raise SystemExit(0)\n"
            "if MODE=='x1' and 'ls-files' in args and count('lsfiles')==2:\n"
            " sys.stdout.buffer.write(b'changed'); raise SystemExit(0)\n"
            "mutate=False\n"
            "if MODE=='index_replace_i1' and 'ls-files' in args and count('phase')==1:\n"
            " p=pathlib.Path(gitdir)/'index'; replacement=p.with_name('index.replacement'); replacement.write_bytes(p.read_bytes()); os.replace(replacement,p)\n"
            "if MODE=='index_i1' and 'ls-files' in args and count('phase')==1: mutate=True\n"
            "if MODE=='index_i2' and 'ls-tree' in args and count('phase')==1: mutate=True\n"
            "if MODE=='index_i3' and 'ls-files' in args and count('phase')==2: mutate=True\n"
            "if MODE=='index_i4' and 'HEAD^{commit}' in args and count('headphase')==2: mutate=True\n"
            "if mutate:\n"
            " p=pathlib.Path(work)/'index-race.rs'; p.write_text('pub fn race() {}\\n')\n"
            " subprocess.run([REAL, '--git-dir='+gitdir, '--work-tree='+work, 'add', '-f', 'index-race.rs'], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\n"
            "os.execv(REAL,[REAL,*args])\n"
        )
        script.chmod(0o755)
        return directory

    @staticmethod
    def _generator_fixture(root: Path) -> tuple[Path, Path, Path]:
        scripts = root / "scripts"
        scripts.mkdir()
        for name in (
            "generate_no_unsafe_external_cache_manifest.py",
            "check_no_unsafe.py",
            "no_unsafe_fs.py",
            "bounded_subprocess.py",
        ):
            shutil.copyfile(REPO / "scripts" / name, scripts / name)
        shutil.copyfile(CANONICAL_MANIFEST, root / MANIFEST_REL)
        archive = root / ARCHIVE_REL
        archive.parent.mkdir(parents=True)
        os.link(REPO / ARCHIVE_REL, archive)
        output_parent = root / "build/engineering-quality/ns/run"
        output_parent.mkdir(parents=True)
        return archive, root / MANIFEST_REL, output_parent / OUTPUT_BASENAME

    @staticmethod
    def _generator_command(root: Path, archive: Path, checked: Path, output: Path) -> list[str]:
        return [
            "python3",
            "scripts/generate_no_unsafe_external_cache_manifest.py",
            "--archive",
            str(archive),
            "--out",
            str(output),
            "--check-manifest",
            str(checked),
        ]

    def test_base01_frozen_original_cli_exact_red(self) -> None:
        frozen = REPO / "build/engineering-quality/roadmap-wave-01-policy-v2/policy-design-20260908T171348Z/frozen/scripts/check_no_unsafe.py.txt"
        launcher = (
            "from pathlib import Path; import sys; "
            "source=Path(sys.argv[1]).read_text(); "
            "scope={'__file__':sys.argv[2],'__name__':'__main__'}; "
            "exec(compile(source,sys.argv[1],'exec'),scope)"
        )
        result = subprocess.run(
            [sys.executable, "-c", launcher, str(frozen), str(REPO / "scripts/check_no_unsafe.py")],
            cwd=REPO,
            capture_output=True,
            text=True,
            timeout=600,
            check=False,
        )
        findings = [line for line in result.stderr.splitlines() if "Rust `unsafe` keyword is forbidden" in line]
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(findings), 54)
        for home in CANDIDATE_HOMES:
            self.assertEqual(sum(home in line for line in findings), 27)

    def test_base02_make_recipe_retains_compiler_forbid(self) -> None:
        result = subprocess.run(
            ["make", "check-no-unsafe"],
            cwd=REPO,
            capture_output=True,
            text=True,
            timeout=600,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("python3 scripts/check_no_unsafe.py", result.stdout)
        self.assertIn('"external_cache_rust": 56', result.stdout)
        self.assertIn("cargo check --workspace --all-targets --all-features --locked", result.stdout)

    def test_root_replacement_cli_smoke(self) -> None:
        import threading
        import time

        fixture = self._scanner_fixture()
        try:
            target = fixture.root / "scripts/check_no_unsafe.py"
            original = target.read_bytes()
            def mutate() -> None:
                time.sleep(0.0005)
                replacement = target.with_name("check_no_unsafe.pending")
                replacement.write_bytes(original)
                os.replace(replacement, target)

            thread = threading.Thread(target=mutate)
            thread.start()
            result = self._run_scanner(fixture)
            thread.join()
            self.assertIn(result.returncode, (0, 1))
            if result.returncode:
                self.assertIn("[E_ROOT_BINDING]", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
        finally:
            fixture.tearDown()

    def test_git_cli_binding_config_malformed_outputs(self) -> None:
        stable_fixture = self._scanner_fixture()
        try:
            stable = self._run_scanner(stable_fixture)
            self.assertEqual(stable.returncode, 0, stable.stderr)
            self.assertEqual(json.loads(stable.stdout)["external_cache_rust"], 28)
        finally:
            stable_fixture.tearDown()
        for mode, code in (
            ("binding", "E_GIT_BINDING"),
            ("binding_nul", "E_GIT_BINDING"),
            ("gitdir", "E_GIT_BINDING"),
            ("gitdir_extra", "E_GIT_BINDING"),
            ("common", "E_GIT_BINDING"),
            ("common_nul", "E_GIT_BINDING"),
            ("inside", "E_GIT_BINDING"),
            ("config", "E_GIT_CONFIG"),
            ("config_extra_nul", "E_GIT_CONFIG"),
            ("config_worktree", "E_GIT_CONFIG"),
            ("config_extension", "E_GIT_CONFIG"),
            ("config_include", "E_GIT_CONFIG"),
            ("head_extra", "E_GIT_HEAD"),
            ("status_lf", "E_GIT_SCHEMA"),
            ("status_partial", "E_GIT_SCHEMA"),
        ):
            with self.subTest(mode=mode):
                fixture = self._scanner_fixture()
                try:
                    fake = self._install_fake_git(fixture.root, mode)
                    result = self._run_scanner(fixture, path=str(fake))
                    self.assertEqual(result.returncode, 1)
                    self.assertIn(f"[{code}]", result.stderr)
                    self.assertIn("exemptions=0", result.stderr)
                finally:
                    fixture.tearDown()

    def test_git_cli_head_and_tree_changes_are_races(self) -> None:
        for mode in ("head_c1", "head_c2", "tree_t1"):
            with self.subTest(mode=mode):
                fixture = self._scanner_fixture()
                try:
                    fake = self._install_fake_git(fixture.root, mode)
                    result = self._run_scanner(fixture, path=str(fake))
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("[E_GIT_RACE]", result.stderr)
                    self.assertIn("exemptions=0", result.stderr)
                finally:
                    fixture.tearDown()

    def test_git_cli_real_index_mutation_is_a_race(self) -> None:
        for mode in ("index_i1", "index_replace_i1", "index_i2", "index_i3", "index_i4", "x1"):
            with self.subTest(mode=mode):
                fixture = self._scanner_fixture()
                try:
                    fake = self._install_fake_git(fixture.root, mode)
                    result = self._run_scanner(fixture, path=str(fake))
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("[E_GIT_RACE]", result.stderr)
                    self.assertIn("exemptions=0", result.stderr)
                    if mode.startswith("index_") and mode != "index_replace_i1":
                        self.assertTrue((fixture.root / "index-race.rs").exists())
                    if mode == "index_replace_i1":
                        self.assertFalse((fixture.root / ".git/index.replacement").exists())
                finally:
                    fixture.tearDown()

    def test_git_index_all_operation_causes_and_specials(self) -> None:
        from scripts import check_no_unsafe as scanner

        class FaultOps(RealFileOps):
            def __init__(self, fault: str, other: Path) -> None:
                super().__init__()
                self.fault = fault
                self.other = other
                self.metadata_fd: int | None = None
                self.data_fd: int | None = None
                self.portal_opens = 0
                self.failed_close = False

            def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                if name == "index" and self.fault == "open":
                    raise OSError(errno.EIO, "index open")
                descriptor = super().openat(parent_fd, name, flags, mode)
                if name == "index":
                    self.metadata_fd = descriptor
                return descriptor

            def fstat(self, fd: int):  # type: ignore[no-untyped-def]
                if fd == self.metadata_fd and self.fault == "metadata":
                    raise OSError(errno.EIO, "index metadata")
                return super().fstat(fd)

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                if metadata_fd == self.metadata_fd:
                    self.portal_opens += 1
                    if self.fault == "data_open":
                        raise OSError(errno.EIO, "index portal")
                    if self.fault == "race":
                        self.data_fd = os.open(self.other, os.O_RDONLY)
                        return self.data_fd
                self.data_fd = super().open_portal(metadata_fd, flags)
                return self.data_fd

            def read(self, fd: int, count: int) -> bytes:
                if fd == self.data_fd and self.fault == "read":
                    raise OSError(errno.EIO, "index read")
                if fd == self.data_fd and self.fault == "short":
                    return b""
                return super().read(fd, count)

            def close(self, fd: int) -> None:
                if fd == self.data_fd and self.fault == "close" and not self.failed_close:
                    self.failed_close = True
                    super().close(fd)
                    raise OSError(errno.EIO, "index close")
                super().close(fd)

        expected = {
            "open": "E_GIT_INDEX_OPEN",
            "metadata": "E_GIT_INDEX_METADATA",
            "data_open": "E_GIT_INDEX_DATA_OPEN",
            "race": "E_GIT_INDEX_RACE",
            "read": "E_GIT_INDEX_READ",
            "short": "E_GIT_INDEX_SHORT_READ",
            "close": "E_GIT_INDEX_CLOSE",
        }
        for fault, code in expected.items():
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                git = root / ".git"
                git.mkdir()
                (git / "index").write_bytes(b"index")
                other = root / "other"
                other.write_bytes(b"other")
                root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                held = open_directory_at(root_fd, ".git")
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        scanner._index_snapshot(mock.Mock(path=str(root)), held, ops=FaultOps(fault, other))
                    self.assertEqual(caught.exception.code, code)
                finally:
                    held.close()
                    os.close(root_fd)
        for kind in ("link", "fifo", "socket", "device"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                git = root / ".git"
                git.mkdir()
                target = git / "index"
                unix_socket: socket.socket | None = None
                if kind == "link":
                    os.symlink("missing", target)
                elif kind == "fifo":
                    os.mkfifo(target)
                elif kind == "socket":
                    unix_socket = socket.socket(socket.AF_UNIX)
                    unix_socket.bind(str(target))
                else:
                    target.write_bytes(b"placeholder")
                root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
                held = open_directory_at(root_fd, ".git")
                ops = FaultOps("none", root / "other")
                if kind == "device":
                    original_openat = ops.openat

                    def device_open(
                        parent_fd: int,
                        name: str,
                        flags: int,
                        mode: int = 0,
                        fallback=original_openat,  # type: ignore[no-untyped-def]
                    ) -> int:
                        if name == "index":
                            return os.open("/dev/null", O_PATH | os.O_CLOEXEC)
                        return fallback(parent_fd, name, flags, mode)

                    ops.openat = device_open  # type: ignore[method-assign]
                try:
                    with self.assertRaises(PolicyFailure) as caught:
                        scanner._index_snapshot(mock.Mock(path=str(root)), held, ops=ops)
                    suffix = "LINK" if kind == "link" else "NONREGULAR"
                    self.assertEqual(caught.exception.code, f"E_GIT_INDEX_{suffix}")
                    self.assertEqual(ops.portal_opens, 0)
                finally:
                    held.close()
                    os.close(root_fd)
                    if unix_socket is not None:
                        unix_socket.close()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            git = root / ".git"
            git.mkdir()
            with (git / "index").open("wb") as stream:
                stream.truncate(64 * 1024 * 1024 + 1)
            root_fd = os.open(root, os.O_RDONLY | O_DIRECTORY)
            held = open_directory_at(root_fd, ".git")
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    scanner._index_snapshot(mock.Mock(path=str(root)), held)
                self.assertEqual(caught.exception.code, "E_GIT_INDEX_SIZE")
            finally:
                held.close()
                os.close(root_fd)

    def test_cargo_manifest_all_operation_causes_and_specials(self) -> None:
        from scripts import check_no_unsafe as scanner

        metadata = {"packages": [], "workspace_members": []}

        class CargoFaultOps(RealFileOps):
            def __init__(self, fault: str, other: Path) -> None:
                super().__init__()
                self.fault = fault
                self.other = other
                self.target_opens = 0
                self.metadata_fd: int | None = None
                self.data_fd: int | None = None
                self.portal_opens = 0
                self.failed_close = False

            def openat(self, parent_fd: int, name: str, flags: int, mode: int = 0) -> int:
                if name == "Cargo.toml" and self.fault == "open":
                    raise OSError(errno.EIO, "Cargo open")
                descriptor = super().openat(parent_fd, name, flags, mode)
                if name == "Cargo.toml":
                    self.target_opens += 1
                    self.metadata_fd = descriptor
                return descriptor

            def fstat(self, fd: int):  # type: ignore[no-untyped-def]
                if fd == self.metadata_fd and self.fault == "metadata":
                    raise OSError(errno.EIO, "Cargo metadata")
                return super().fstat(fd)

            def open_portal(self, metadata_fd: int, flags: int) -> int:
                if metadata_fd == self.metadata_fd:
                    self.portal_opens += 1
                    if self.fault == "data_open":
                        raise OSError(errno.EIO, "Cargo data open")
                    if self.fault == "race":
                        self.data_fd = os.open(self.other, os.O_RDONLY)
                        return self.data_fd
                self.data_fd = super().open_portal(metadata_fd, flags)
                return self.data_fd

            def read(self, fd: int, count: int) -> bytes:
                if fd == self.data_fd and self.fault == "read":
                    raise OSError(errno.EIO, "Cargo read")
                if fd == self.data_fd and self.fault == "short":
                    return b""
                return super().read(fd, count)

            def close(self, fd: int) -> None:
                if fd == self.data_fd and self.fault == "close" and not self.failed_close:
                    self.failed_close = True
                    super().close(fd)
                    raise OSError(errno.EIO, "Cargo close")
                super().close(fd)

        cases = {
            "open": "E_CARGO_MANIFEST_OPEN",
            "metadata": "E_CARGO_MANIFEST_METADATA",
            "data_open": "E_CARGO_MANIFEST_DATA_OPEN",
            "race": "E_CARGO_MANIFEST_RACE",
            "read": "E_CARGO_MANIFEST_READ",
            "short": "E_CARGO_MANIFEST_SHORT_READ",
            "close": "E_CARGO_MANIFEST_CLOSE",
        }
        for fault, code in cases.items():
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                outer = Path(temporary)
                repository = outer / "repository"
                (repository / "rust").mkdir(parents=True)
                (repository / "rust/Cargo.toml").write_text('[workspace]\n[workspace.lints.rust]\nunsafe_code="forbid"\n')
                other = repository / "other"
                other.write_text("other")
                outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
                held = open_directory_at(outer_fd, "repository")
                try:
                    with mock.patch.object(scanner, "_cargo_metadata", return_value=metadata), self.assertRaises(PolicyFailure) as caught:
                        cargo_view(BoundRepository(str(repository), held), ops=CargoFaultOps(fault, other))
                    self.assertEqual(caught.exception.code, code)
                finally:
                    held.close()
                    os.close(outer_fd)
        for raw, code in ((b"\xff", "E_CARGO_MANIFEST_UTF8"), (b"not = [toml", "E_CARGO_MANIFEST_TOML")):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                outer = Path(temporary)
                repository = outer / "repository"
                (repository / "rust").mkdir(parents=True)
                (repository / "rust/Cargo.toml").write_bytes(raw)
                outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
                held = open_directory_at(outer_fd, "repository")
                try:
                    with mock.patch.object(scanner, "_cargo_metadata", return_value=metadata), self.assertRaises(PolicyFailure) as caught:
                        cargo_view(BoundRepository(str(repository), held))
                    self.assertEqual(caught.exception.code, code)
                finally:
                    held.close()
                    os.close(outer_fd)
        for kind in ("link", "fifo", "socket", "device"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                outer = Path(temporary)
                repository = outer / "repository"
                (repository / "rust").mkdir(parents=True)
                target = repository / "rust/Cargo.toml"
                unix_socket: socket.socket | None = None
                if kind == "link": os.symlink("missing", target)
                elif kind == "fifo": os.mkfifo(target)
                elif kind == "socket":
                    unix_socket = socket.socket(socket.AF_UNIX); unix_socket.bind(str(target))
                else: target.write_bytes(b"placeholder")
                outer_fd = os.open(outer, os.O_RDONLY | O_DIRECTORY)
                held = open_directory_at(outer_fd, "repository")
                ops = CargoFaultOps("none", repository / "other")
                if kind == "device":
                    original_openat = ops.openat
                    def device_open(
                        parent_fd: int,
                        name: str,
                        flags: int,
                        mode: int = 0,
                        fallback=original_openat,  # type: ignore[no-untyped-def]
                    ) -> int:
                        if name == "Cargo.toml":
                            return os.open("/dev/null", O_PATH | os.O_CLOEXEC)
                        return fallback(parent_fd, name, flags, mode)
                    ops.openat = device_open  # type: ignore[method-assign]
                try:
                    with mock.patch.object(scanner, "_cargo_metadata", return_value=metadata), self.assertRaises(PolicyFailure) as caught:
                        cargo_view(BoundRepository(str(repository), held), ops=ops)
                    suffix = "LINK" if kind == "link" else "NONREGULAR"
                    self.assertEqual(caught.exception.code, f"E_CARGO_MANIFEST_{suffix}")
                    self.assertEqual(ops.portal_opens, 0)
                finally:
                    held.close(); os.close(outer_fd)
                    if unix_socket is not None: unix_socket.close()
        for limit_name, code in (("MAX_CARGO_MANIFEST_BYTES", "E_CARGO_MANIFEST_SIZE"), ("MAX_ALL_CARGO_MANIFEST_BYTES", "E_CARGO_MANIFEST_AGGREGATE")):
            with self.subTest(limit=limit_name), tempfile.TemporaryDirectory() as temporary:
                outer=Path(temporary); repository=outer/"repository"; (repository/"rust").mkdir(parents=True); (repository/"rust/Cargo.toml").write_text("x")
                outer_fd=os.open(outer,os.O_RDONLY|O_DIRECTORY); held=open_directory_at(outer_fd,"repository")
                try:
                    with mock.patch.object(scanner,"_cargo_metadata",return_value=metadata), mock.patch.object(scanner,limit_name,0), self.assertRaises(PolicyFailure) as caught:
                        cargo_view(BoundRepository(str(repository),held))
                    self.assertEqual(caught.exception.code,code)
                finally: held.close(); os.close(outer_fd)

    def test_checked_manifest_all_operation_causes(self) -> None:
        class CheckFaultOps(RealFileOps):
            def __init__(self, fault: str, other: Path, target: Path) -> None:
                super().__init__(); self.fault=fault; self.other=other; self.target=target; self.opens=0; self.metadata_fd=None; self.data_fd=None; self.failed=False; self.grew=False
            def openat2(self,parent_fd:int,name:str,flags:int,mode:int,resolve:int)->int:
                if name==Path(MANIFEST_REL).name and self.fault=="open": raise OSError(errno.EIO,"check open")
                fd=super().openat2(parent_fd,name,flags,mode,resolve)
                if name==Path(MANIFEST_REL).name: self.opens+=1; self.metadata_fd=fd
                return fd
            def fstat(self,fd:int):  # type: ignore[no-untyped-def]
                if fd==self.metadata_fd and self.fault=="metadata": raise OSError(errno.EIO,"check metadata")
                return super().fstat(fd)
            def open_portal(self,metadata_fd:int,flags:int)->int:
                if metadata_fd==self.metadata_fd:
                    if self.fault=="data_open": raise OSError(errno.EIO,"check portal")
                    if self.fault=="race": self.data_fd=os.open(self.other,os.O_RDONLY); return self.data_fd
                self.data_fd=super().open_portal(metadata_fd,flags); return self.data_fd
            def read(self,fd:int,count:int)->bytes:
                if fd==self.data_fd and self.fault=="read": raise OSError(errno.EIO,"check read")
                if fd==self.data_fd and self.fault=="short": return b""
                if fd==self.data_fd and self.fault=="growth" and not self.grew:
                    self.grew=True
                    with self.target.open("ab") as stream: stream.write(b"x")
                return super().read(fd,count)
            def close(self,fd:int)->None:
                if fd==self.data_fd and self.fault=="close" and not self.failed:
                    self.failed=True; super().close(fd); raise OSError(errno.EIO,"check close")
                super().close(fd)
        expected={"open":"E_REGEN_CHECK_OPEN","metadata":"E_REGEN_CHECK_METADATA","data_open":"E_REGEN_CHECK_DATA_OPEN","size":"E_REGEN_CHECK_SIZE","race":"E_REGEN_CHECK_RACE","read":"E_REGEN_CHECK_READ","short":"E_REGEN_CHECK_SHORT_READ","growth":"E_REGEN_CHECK_RACE","close":"E_REGEN_CHECK_CLOSE"}
        for fault,code in expected.items():
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary); target=root/MANIFEST_REL; target.parent.mkdir(); shutil.copyfile(CANONICAL_MANIFEST,target); other=root/"other"; other.write_bytes(b"other")
                if fault == "size":
                    with target.open("ab") as stream:
                        stream.truncate(65_537)
                fd=os.open(root,os.O_RDONLY|O_DIRECTORY)
                try:
                    with self.assertRaises(PolicyFailure) as caught: read_checked_manifest(fd,ops=CheckFaultOps(fault,other,target))
                    self.assertEqual(caught.exception.code,code)
                finally: os.close(fd)

    def test_persistent_global_pax_path_and_local_override(self) -> None:
        buffer=io.BytesIO()
        global_path="kani-0.67.0/library/global.rs"
        local_path="kani-0.67.0/library/local.rs"
        with tarfile.open(fileobj=buffer,mode="w",format=tarfile.PAX_FORMAT,pax_headers={"path":global_path}) as archive:
            first=tarfile.TarInfo("raw-first"); first.size=1; archive.addfile(first,io.BytesIO(b"a"))
            second=tarfile.TarInfo("raw-second"); second.pax_headers={"path":local_path}; second.size=1; archive.addfile(second,io.BytesIO(b"b"))
            third=tarfile.TarInfo("raw-third"); third.size=1; archive.addfile(third,io.BytesIO(b"c"))
        raw=buffer.getvalue(); compressed=gzip.compress(raw,mtime=0)
        with tempfile.NamedTemporaryFile() as archive_file:
            archive_file.write(compressed); archive_file.flush(); fd=os.open(archive_file.name,os.O_RDONLY)
            try:
                with self.assertRaises(PolicyFailure) as caught:
                    parse_archive_descriptor(fd,limits=ArchiveLimits(len(compressed),len(raw),16,4096,4096,256))
                self.assertEqual(caught.exception.code,"E_ARCHIVE_DUPLICATE")
                self.assertIn(global_path,caught.exception.detail)
                self.assertNotIn(local_path,caught.exception.detail)
            finally: os.close(fd)

    def test_static_checked_cause_partition(self) -> None:
        import inspect

        from scripts import generate_no_unsafe_external_cache_manifest as generator
        source=inspect.getsource(generator)
        self.assertNotIn("E_REGEN_CHECK_PATH",source)
        for marker in (
            'PolicyFailure("E_REGEN_PATH"',
            'PolicyFailure("E_REGEN_CONFINEMENT"',
            'family="E_REGEN_CHECK"',
            'PolicyFailure("E_REGEN_CHECK_RACE"',
        ):
            self.assertIn(marker,source)
        generate_source=inspect.getsource(generator.generate)
        self.assertLess(generate_source.index("_validate_absolute"),generate_source.index("read_checked_manifest"))

    def test_special_replacements_at_final_observations(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator
        class ReplaceWithFifo(RealFileOps):
            def __init__(self,target:Path,trigger:int) -> None:
                super().__init__(); self.target=target; self.trigger=trigger; self.opens=0; self.replaced=False; self.target_portals_after=0
            def openat2(self,parent_fd:int,name:str,flags:int,mode:int,resolve:int)->int:
                if name==self.target.name:
                    self.opens+=1
                    if self.opens==self.trigger and not self.replaced:
                        self.target.unlink(); os.mkfifo(self.target); self.replaced=True
                return super().openat2(parent_fd,name,flags,mode,resolve)
            def open_portal(self,metadata_fd:int,flags:int)->int:
                if self.replaced and os.readlink(f"/proc/self/fd/{metadata_fd}").endswith("/"+self.target.name): self.target_portals_after+=1
                return super().open_portal(metadata_fd,flags)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); target=root/MANIFEST_REL; target.parent.mkdir(); shutil.copyfile(CANONICAL_MANIFEST,target); fd=os.open(root,os.O_RDONLY|O_DIRECTORY); ops=ReplaceWithFifo(target,2)
            try:
                with self.assertRaises(PolicyFailure) as caught: read_checked_manifest(fd,ops=ops)
                self.assertEqual(caught.exception.code,"E_REGEN_CHECK_RACE"); self.assertEqual(ops.target_portals_after,0)
            finally: os.close(fd)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); parent=root/"build/engineering-quality/ns/run"; parent.mkdir(parents=True); target=parent/OUTPUT_BASENAME; fd=os.open(root,os.O_RDONLY|O_DIRECTORY); ops=ReplaceWithFifo(target,4)
            try:
                with self.assertRaises(PolicyFailure) as caught: write_output(fd,f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}",b"{}\n",ops=ops)
                self.assertEqual(caught.exception.code,"E_REGEN_RACE"); self.assertIn("E_REGEN_RESIDUE",caught.exception.additional); self.assertEqual(ops.target_portals_after,0)
            finally: os.close(fd)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); target=root/ARCHIVE_REL; target.parent.mkdir(parents=True); target.write_bytes(b"x"); fd=os.open(root,os.O_RDONLY|O_DIRECTORY); ops=ReplaceWithFifo(target,2); inventory=generator.ArchiveInventory((),(),1,0,0)
            try:
                with mock.patch.object(generator,"ARCHIVE_BYTES",1), mock.patch.object(generator,"ARCHIVE_SHA256",__import__("hashlib").sha256(b"x").hexdigest()), mock.patch.object(generator,"parse_archive_descriptor",return_value=inventory), self.assertRaises(PolicyFailure) as caught:
                    read_archive_inventory(fd,ops=ops,limits=ArchiveLimits(1,1,1,1,1,256))
                self.assertEqual(caught.exception.code,"E_REGEN_ARCHIVE_RACE"); self.assertEqual(ops.target_portals_after,0)
            finally: os.close(fd)

    def test_generator_root_binding_direct_real(self) -> None:
        from scripts import generate_no_unsafe_external_cache_manifest as generator

        for mutation in ("final", "touch_parent", "replace_parent"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary); scripts=root/"scripts"; scripts.mkdir(); target=scripts/"generate_no_unsafe_external_cache_manifest.py"; target.write_text("# original\n")
                class MutateBinding(RealFileOps):
                    def __init__(self, selected: str, selected_root: Path, selected_scripts: Path, selected_name: str) -> None:
                        super().__init__(); self.selected=selected; self.root=selected_root; self.scripts=selected_scripts; self.name=selected_name; self.opens=0; self.fds=[]; self.mutated=False
                    def openat(self,parent_fd:int,name:str,flags:int,mode:int=0)->int:
                        if name==self.name:
                            self.opens+=1
                            if self.selected=="final" and self.opens==2:
                                os.rename(name,"old",src_dir_fd=parent_fd,dst_dir_fd=parent_fd); fd=os.open(name,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600,dir_fd=parent_fd); os.write(fd,b"# replacement\n"); os.close(fd); self.mutated=True
                        descriptor=super().openat(parent_fd,name,flags,mode)
                        if name==self.name: self.fds.append(descriptor)
                        return descriptor
                    def close(self,fd:int)->None:
                        super().close(fd)
                        if self.selected!="final" and len(self.fds)==2 and fd==self.fds[0] and not self.mutated:
                            if self.selected=="touch_parent": os.utime(self.scripts)
                            else:
                                moved=self.root/"scripts-original"; os.rename(self.scripts,moved); self.scripts.mkdir(); (self.scripts/self.name).write_text("# replacement\n")
                            self.mutated=True
                ops=MutateBinding(mutation,root,scripts,target.name)
                arguments = [
                    "--archive", str(root / ARCHIVE_REL),
                    "--out", str(root / f"build/engineering-quality/ns/run/{OUTPUT_BASENAME}"),
                    "--check-manifest", str(root / MANIFEST_REL),
                ]
                with mock.patch.object(sys, "argv", [str(target)]), self.assertRaises(PolicyFailure) as caught:
                    generator.generate(arguments, file_ops=ops, io_ops=ops)
                self.assertEqual(caught.exception.code,"E_REGEN_ROOT_BINDING")
                self.assertTrue(ops.mutated)

    def test_generator_cli_confinement_and_identity_failures(self) -> None:
        for case,code in (
            ("size", "E_REGEN_ARCHIVE_SIZE"),
            ("hash", "E_REGEN_ARCHIVE_HASH"),
            ("archive_parent_link", "E_REGEN_CONFINEMENT"),
            ("archive_parent_non_directory", "E_REGEN_CONFINEMENT"),
            ("check_fifo", "E_REGEN_CHECK_NONREGULAR"),
        ):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary); archive,checked,output=self._generator_fixture(root)
                if case=="size": archive.unlink(); archive.write_bytes(b"x")
                elif case=="hash":
                    archive.unlink()
                    with archive.open("wb") as stream:
                        stream.write(b"x")
                        stream.truncate(137_826_806)
                elif case == "archive_parent_link":
                    stage=archive.parents[2]; moved=stage.with_name(stage.name+"-real"); os.rename(stage,moved); os.symlink(moved.name,stage)
                elif case == "archive_parent_non_directory":
                    stage=archive.parents[2]; shutil.rmtree(stage); stage.write_bytes(b"not a directory")
                else:
                    checked.unlink()
                    os.mkfifo(checked)
                result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=600,check=False)
                if case.startswith("archive_parent_") and "[E_REGEN_ROOT_BINDING]" in result.stderr:
                    result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=600,check=False)
                self.assertEqual(result.returncode,1); self.assertIn(f"[{code}]",result.stderr); self.assertFalse(output.exists())

        import ctypes
        import select

        for parent_kind in ("link", "non_directory"):
            with self.subTest(check_parent=parent_kind), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary); archive,checked,output=self._generator_fixture(root)
                libc=ctypes.CDLL(None,use_errno=True); inotify=libc.inotify_init1(os.O_CLOEXEC); self.assertGreaterEqual(inotify,0)
                self.assertGreaterEqual(libc.inotify_add_watch(inotify,os.fsencode(archive.parent),0x20),0)
                process=subprocess.Popen(self._generator_command(root,archive,checked,output),cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                try:
                    readable,_,_=select.select([inotify],[],[],30); self.assertTrue(readable); os.read(inotify,4096)
                    scripts=root/"scripts"; moved=root/"scripts-real"; os.rename(scripts,moved)
                    if parent_kind == "link": os.symlink(moved.name,scripts)
                    else: scripts.write_bytes(b"not a directory")
                    stdout,stderr=process.communicate(timeout=600)
                finally:
                    if process.poll() is None: process.kill(); process.wait()
                    os.close(inotify)
                self.assertEqual(process.returncode,1,(stdout,stderr)); self.assertIn("[E_REGEN_CONFINEMENT]",stderr); self.assertFalse(output.exists())

    def test_checked_cli_four_disjoint_precedence_categories(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root)
            unknown=subprocess.run(
                ["python3","scripts/generate_no_unsafe_external_cache_manifest.py","--unknown"],
                cwd=root,capture_output=True,text=True,timeout=60,check=False,
            )
            self.assertEqual(unknown.returncode,1); self.assertIn("[E_REGEN_ARGS]",unknown.stderr)
            bad_path=self._generator_command(root,archive,checked,output); bad_path[bad_path.index(str(checked))]="relative.json"
            result=subprocess.run(bad_path,cwd=root,capture_output=True,text=True,timeout=60,check=False)
            self.assertEqual(result.returncode,1); self.assertIn("[E_REGEN_PATH]",result.stderr); self.assertNotIn("[E_REGEN_CHECK_OPEN]",result.stderr)

        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root)
            stage=archive.parents[2]; moved=stage.with_name(stage.name+"-real"); os.rename(stage,moved); os.symlink(moved.name,stage)
            result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=60,check=False)
            self.assertEqual(result.returncode,1); self.assertIn("[E_REGEN_CONFINEMENT]",result.stderr); self.assertNotIn("[E_REGEN_CHECK_OPEN]",result.stderr)

        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root); checked.unlink()
            result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=600,check=False)
            self.assertEqual(result.returncode,1); self.assertIn("[E_REGEN_CHECK_OPEN]",result.stderr)
            self.assertNotIn("[E_REGEN_PATH]",result.stderr); self.assertNotIn("[E_REGEN_CONFINEMENT]",result.stderr)

        import time

        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root)
            pending=checked.with_name("manifest.pending"); pending.write_bytes(checked.read_bytes())
            checked_stat=checked.stat()
            ptrace=ctypes.CDLL(None,use_errno=True).ptrace
            ptrace.argtypes=[ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p,ctypes.c_void_p]
            ptrace.restype=ctypes.c_long
            trace_syscall=24; trace_detach=17; trace_set_options=0x4200
            trace_sysgood=1
            launcher=(
                "import ctypes,os,signal,sys; "
                "p=ctypes.CDLL(None,use_errno=True).ptrace; "
                "p.argtypes=[ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p,ctypes.c_void_p]; "
                "p.restype=ctypes.c_long; "
                "assert p(0,0,None,None)==0; os.kill(os.getpid(),signal.SIGSTOP); "
                "os.execvp(sys.argv[1],sys.argv[1:])"
            )
            process=subprocess.Popen(
                [sys.executable,"-B","-c",launcher,*self._generator_command(root,archive,checked,output)],
                cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,
            )
            traced=False; stopped=False; synchronized=False; stdout=""; stderr=""
            deadline=time.monotonic()+60.0
            try:
                waited,status=os.waitpid(process.pid,0)
                self.assertEqual(waited,process.pid); self.assertTrue(os.WIFSTOPPED(status))
                stopped=True
                self.assertEqual(ptrace(trace_set_options,process.pid,None,ctypes.c_void_p(trace_sysgood)),0)
                traced=True
                while time.monotonic()<deadline:
                    self.assertEqual(ptrace(trace_syscall,process.pid,None,None),0)
                    stopped=False
                    waited,status=os.waitpid(process.pid,0)
                    self.assertEqual(waited,process.pid)
                    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                        break
                    self.assertTrue(os.WIFSTOPPED(status)); stopped=True
                    matching=0
                    for descriptor in Path(f"/proc/{process.pid}/fd").iterdir():
                        try:
                            observed=os.stat(descriptor)
                        except FileNotFoundError:
                            continue
                        if (observed.st_dev,observed.st_ino)==(checked_stat.st_dev,checked_stat.st_ino):
                            matching+=1
                    if matching>=2:
                        synchronized=True
                        break
                self.assertTrue(synchronized,"generator did not hold checked-manifest metadata and data descriptors")
                os.replace(pending,checked)
                self.assertEqual(ptrace(trace_detach,process.pid,None,None),0)
                traced=False; stopped=False
                stdout,stderr=process.communicate(timeout=60)
            finally:
                if traced and stopped:
                    ptrace(trace_detach,process.pid,None,None)
                if process.poll() is None:
                    process.kill(); process.wait()
            self.assertEqual(process.returncode,1,(stdout,stderr))
            self.assertIn("[E_REGEN_CHECK_RACE]",stderr)
            self.assertNotIn("[E_REGEN_PATH]",stderr)
            self.assertNotIn("[E_REGEN_CONFINEMENT]",stderr)
            self.assertNotIn("[E_REGEN_CHECK_OPEN]",stderr)

    def test_generator_cli_output_race_and_precedence(self) -> None:
        kinds=("regular","link","fifo","socket","other","fixed_plus_other")
        for kind in kinds:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary); archive,checked,output=self._generator_fixture(root); unix_socket:socket.socket|None=None
                if kind=="regular": output.write_bytes(b"existing")
                elif kind=="link": os.symlink("missing",output)
                elif kind=="fifo": os.mkfifo(output)
                elif kind=="socket": unix_socket=socket.socket(socket.AF_UNIX); unix_socket.bind(str(output))
                elif kind == "other": (output.parent/"other").write_bytes(b"other")
                else: output.write_bytes(b"existing"); (output.parent/"other").write_bytes(b"other")
                try:
                    result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=600,check=False)
                    self.assertEqual(result.returncode,1)
                    expected="E_REGEN_CONFINEMENT" if kind=="other" else "E_REGEN_OUTPUT_EXISTS"
                    self.assertIn(f"[{expected}]",result.stderr)
                finally:
                    if unix_socket is not None: unix_socket.close()

        import ctypes
        import select

        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root)
            libc=ctypes.CDLL(None,use_errno=True); inotify=libc.inotify_init1(os.O_CLOEXEC); self.assertGreaterEqual(inotify,0)
            self.assertGreaterEqual(libc.inotify_add_watch(inotify,os.fsencode(output.parent),0x100),0)
            process=subprocess.Popen(self._generator_command(root,archive,checked,output),cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                readable,_,_=select.select([inotify],[],[],30); self.assertTrue(readable); os.read(inotify,4096)
                replacement=output.with_name("replacement"); replacement.write_bytes(b"replacement"); os.replace(replacement,output)
                stdout,stderr=process.communicate(timeout=600)
            finally:
                if process.poll() is None: process.kill(); process.wait()
                os.close(inotify)
            self.assertEqual(process.returncode,1,(stdout,stderr)); self.assertIn("[E_REGEN_RACE]",stderr); self.assertIn("first output verification differs",stderr); self.assertIn("[E_REGEN_RESIDUE]",stderr); self.assertTrue(output.exists())

    def _replacement_smoke(self,target_kind:str)->None:
        import threading
        import time
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); archive,checked,output=self._generator_fixture(root)
            if target_kind == "archive":
                import ctypes
                import select
                import struct
                libc=ctypes.CDLL(None,use_errno=True); inotify=libc.inotify_init1(os.O_CLOEXEC); self.assertGreaterEqual(inotify,0)
                self.assertGreaterEqual(libc.inotify_add_watch(inotify,os.fsencode(archive.parent),0x20),0)
                process=subprocess.Popen(self._generator_command(root,archive,checked,output),cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                opens=0
                try:
                    deadline=time.monotonic()+30
                    while opens<1 and time.monotonic()<deadline:
                        readable,_,_=select.select([inotify],[],[],1)
                        if not readable: continue
                        data=os.read(inotify,4096); offset=0
                        while offset<len(data):
                            _,mask,_,length=struct.unpack_from("iIII",data,offset); name=data[offset+16:offset+16+length].rstrip(b"\0"); offset+=16+length
                            if mask&0x20 and name==os.fsencode(archive.name): opens+=1
                    self.assertGreaterEqual(opens,1)
                    time.sleep(0.05)
                    replacement=archive.with_name("replacement"); os.link(archive,replacement); os.replace(replacement,archive)
                    stdout,stderr=process.communicate(timeout=600)
                finally:
                    if process.poll() is None: process.kill(); process.wait()
                    os.close(inotify)
                self.assertEqual(process.returncode,1,(stdout,stderr)); self.assertIn("[E_REGEN_ARCHIVE_RACE]",stderr); self.assertNotIn("Traceback",stderr)
                return
            if target_kind == "check":
                import ctypes
                import select
                libc=ctypes.CDLL(None,use_errno=True); inotify=libc.inotify_init1(os.O_CLOEXEC); self.assertGreaterEqual(inotify,0)
                self.assertGreaterEqual(libc.inotify_add_watch(inotify,os.fsencode(archive.parent),0x20),0)
                process=subprocess.Popen(self._generator_command(root,archive,checked,output),cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                stop=threading.Event(); original=checked.read_bytes()
                def mutate_check()->None:
                    while not stop.is_set():
                        replacement=checked.with_name("replacement"); replacement.write_bytes(original); os.replace(replacement,checked); time.sleep(0.001)
                try:
                    readable,_,_=select.select([inotify],[],[],30); self.assertTrue(readable); os.read(inotify,4096)
                    thread=threading.Thread(target=mutate_check); thread.start()
                    try: stdout,stderr=process.communicate(timeout=600)
                    finally: stop.set(); thread.join()
                finally:
                    if process.poll() is None: process.kill(); process.wait()
                    os.close(inotify)
                self.assertIn(process.returncode,(0,1)); self.assertNotIn("Traceback",stderr)
                if process.returncode: self.assertIn("[E_REGEN_CHECK_RACE]",stderr)
                return
            def mutate(stop:threading.Event)->None:
                while not stop.is_set() and not output.exists(): time.sleep(0.0005)
                if not stop.is_set() and output.exists():
                    replacement=output.with_name("replacement"); replacement.write_bytes(b"replacement"); os.replace(replacement,output)
            for _ in range(3):
                stop=threading.Event(); thread=threading.Thread(target=mutate,args=(stop,)); thread.start()
                try: result=subprocess.run(self._generator_command(root,archive,checked,output),cwd=root,capture_output=True,text=True,timeout=600,check=False)
                finally: stop.set(); thread.join()
                if "[E_REGEN_ROOT_BINDING]" not in result.stderr: break
            self.assertIn(result.returncode,(0,1)); self.assertNotIn("Traceback",result.stderr)
            if result.returncode:
                expected={"check":"E_REGEN_CHECK_RACE","archive":"E_REGEN_ARCHIVE_RACE","output":"E_REGEN_RACE"}[target_kind]
                self.assertIn(f"[{expected}]",result.stderr)
    def test_checked_manifest_replacement_cli_smoke(self)->None: self._replacement_smoke("check")
    def test_archive_replacement_cli_smoke(self)->None: self._replacement_smoke("archive")
    def test_generator_output_replacement_cli_smoke(self)->None: self._replacement_smoke("output")

    def test_cargo_replacement_cli_smoke(self) -> None:
        import ctypes
        import select
        import time

        fixture=self._scanner_fixture()
        try:
            real_cargo=shutil.which("cargo"); self.assertIsNotNone(real_cargo)
            fake=fixture.root/"fake-cargo-bin"; fake.mkdir(); marker=fixture.root/".git/cargo-ready"; go=fixture.root/".git/cargo-go"
            wrapper=fake/"cargo"
            wrapper.write_text(
                "#!/usr/bin/env python3\nimport pathlib,subprocess,sys,time\n"
                f"REAL={real_cargo!r}; MARKER=pathlib.Path({str(marker)!r}); GO=pathlib.Path({str(go)!r})\n"
                "result=subprocess.run([REAL,*sys.argv[1:]],capture_output=True)\n"
                "MARKER.write_text('ready')\n"
                "deadline=time.monotonic()+30\n"
                "while not GO.exists() and time.monotonic()<deadline: time.sleep(0.0005)\n"
                "sys.stdout.buffer.write(result.stdout); sys.stderr.buffer.write(result.stderr); raise SystemExit(result.returncode)\n"
            ); wrapper.chmod(0o755)
            target=fixture.root/"rust/Cargo.toml"
            members=["crate",*[f"delay{index:03d}" for index in range(510)]]
            target.write_text(
                "[workspace]\nmembers="+json.dumps(members)+"\nresolver=\"2\"\n"
                "[workspace.lints.rust]\nunsafe_code=\"forbid\"\n"
            )
            for member in members[1:]:
                package=fixture.root/"rust"/member; (package/"src").mkdir(parents=True)
                (package/"Cargo.toml").write_text(f'[package]\nname="{member}"\nversion="0.1.0"\nedition="2024"\n[lints]\nworkspace=true\n')
                (package/"src/lib.rs").write_text("pub fn safe() {}\n")
            original=target.read_bytes(); watched=fixture.root/"rust/delay010"
            libc=ctypes.CDLL(None,use_errno=True); inotify=libc.inotify_init1(os.O_CLOEXEC|os.O_NONBLOCK); self.assertGreaterEqual(inotify,0)
            self.assertGreaterEqual(libc.inotify_add_watch(inotify,os.fsencode(watched),0x20),0)
            environment=dict(os.environ); environment["PATH"]=str(fake)+os.pathsep+environment.get("PATH",os.defpath)
            process=subprocess.Popen(["python3","scripts/check_no_unsafe.py"],cwd=fixture.root,env=environment,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                deadline=time.monotonic()+30
                while not marker.exists() and time.monotonic()<deadline: time.sleep(0.001)
                self.assertTrue(marker.exists())
                try:
                    while os.read(inotify,4096): pass
                except BlockingIOError:
                    pass
                go.write_text("go")
                readable,_,_=select.select([inotify],[],[],30); self.assertTrue(readable); os.read(inotify,4096)
                replacement=target.with_name("Cargo.pending"); replacement.write_bytes(original); os.replace(replacement,target)
                stdout,stderr=process.communicate(timeout=180)
            finally:
                if process.poll() is None: process.kill(); process.wait()
                os.close(inotify)
            self.assertEqual(process.returncode,1,(stdout,stderr)); self.assertIn("[E_CARGO_MANIFEST_RACE]",stderr); self.assertNotIn("Traceback",stderr)
        finally: fixture.tearDown()

    def test_cargo_member_replacement_cli_smoke(self) -> None:
        import threading
        import time

        fixture = self._scanner_fixture()
        try:
            target = fixture.root / "rust/crate/Cargo.toml"
            original = target.read_bytes()

            def mutate() -> None:
                time.sleep(0.01)
                replacement = target.with_name("Cargo.pending")
                replacement.write_bytes(original)
                os.replace(replacement, target)

            thread = threading.Thread(target=mutate)
            thread.start()
            result = self._run_scanner(fixture)
            thread.join()
            self.assertIn(result.returncode, (0, 1))
            if result.returncode:
                self.assertRegex(
                    result.stderr,
                    r"\[E_(?:CARGO_MANIFEST_RACE|METADATA_EXIT|METADATA_JSON|METADATA_SCHEMA)\]",
                )
                self.assertIn("exemptions=0", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
        finally:
            fixture.tearDown()


class OfficialGeneratorCliTests(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get("AXIOGRAPH_RUN_OFFICIAL_KANI_ARCHIVE") == "1",
        "official 138-MiB archive integration is an explicit production lane",
    )
    def test_official_archive_real_cli_matches_pinned_manifest(self) -> None:
        archive = REPO / ARCHIVE_REL
        invocations = (
            ("scripts/generate_no_unsafe_external_cache_manifest.py", REPO),
            (str(REPO / "scripts/generate_no_unsafe_external_cache_manifest.py"), REPO.parent),
        )
        for script, cwd in invocations:
            with self.subTest(script=script), tempfile.TemporaryDirectory(
                dir=REPO / "build/engineering-quality/roadmap-wave-01-full-loop"
            ) as temporary:
                run = Path(temporary)
                # tempfile creates the run directory itself; its basename satisfies
                # the fixed two-component output grammar below the fixed namespace.
                output = run / OUTPUT_BASENAME
                result = subprocess.run(
                    [
                        "python3",
                        script,
                        "--archive",
                        str(archive),
                        "--out",
                        str(output),
                        "--check-manifest",
                        str(CANONICAL_MANIFEST),
                    ],
                    cwd=cwd,
                    capture_output=True,
                    text=True,
                    timeout=600,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                report = json.loads(result.stdout)
                self.assertEqual(report["members"], 146)
                self.assertEqual(report["files"], 32)
                self.assertEqual(output.read_bytes(), CANONICAL_MANIFEST.read_bytes())


if __name__ == "__main__":
    unittest.main()
