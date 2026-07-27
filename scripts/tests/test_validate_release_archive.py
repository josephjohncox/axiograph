from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import stat
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from scripts.build_release_bundle import build_bundle, canonical_json
from scripts.generate_release_source_manifest import (
    build_source_manifest,
    canonical_bytes,
)
from scripts.validate_release_archive import (
    ArchivedFile,
    ArchiveValidationError,
    open_regular_archive,
    snapshot_archive,
    validate_archive,
    validate_tar,
)

REPO_ROOT = Path(__file__).resolve().parents[2]

SOURCE_COMMIT = "1" * 40
SOURCE_TREE = "2" * 40


def write_source_manifest(path: Path) -> None:
    manifest = build_source_manifest(
        SOURCE_COMMIT,
        SOURCE_TREE,
        [("README.md", "100644", "3" * 40, b"synthetic clean source\n")],
    )
    path.write_bytes(canonical_bytes(manifest))


def build_valid_bundle(root: Path, host: str, directory_name: str) -> Path:
    runtime = root / f"runtime-{directory_name}"
    checker = root / f"checker-{directory_name}"
    source = root / f"source-{directory_name}.json"
    runtime.write_bytes(b"runtime bytes\n")
    checker.write_bytes(b"trusted checker bytes\n")
    write_source_manifest(source)
    output = root / directory_name
    archive, _, _ = build_bundle(
        REPO_ROOT,
        runtime,
        checker,
        source,
        REPO_ROOT / "release" / "fixtures.json",
        host,
        output,
    )
    return archive


def write_tar(path: Path, files: dict[str, tuple[int, bytes]]) -> None:
    with (
        path.open("wb") as raw,
        gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9
        ) as compressed,
        tarfile.open(
            fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT
        ) as archive,
    ):
        for name, (mode, data) in sorted(files.items()):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = mode
            info.uid = 0
            info.gid = 0
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))


def archived_files(entries: dict[str, ArchivedFile]) -> dict[str, tuple[int, bytes]]:
    return {name: (entry.mode, entry.data) for name, entry in entries.items()}


def read_valid_tar(path: Path) -> dict[str, ArchivedFile]:
    with open_regular_archive(path) as (stream, _):
        return validate_tar(stream, "")


class ReleaseArchiveValidationTests(unittest.TestCase):
    def test_reproducible_tar_bundle_validates_extracts_and_pins_external_inputs(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "first")
            second = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "second")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            self.assertEqual(
                first.with_name(first.name + ".sha256").read_bytes(),
                second.with_name(second.name + ".sha256").read_bytes(),
            )

            output = root / "extract"
            output.mkdir()
            report = validate_archive(
                first,
                output,
                expected_host="x86_64-unknown-linux-gnu",
                expected_version="0.6.0",
                expected_source_commit=SOURCE_COMMIT,
                expected_rust_toolchain="1.88.0",
            )
            self.assertTrue(report["validated"])
            self.assertEqual(report["source_tree"], SOURCE_TREE)
            self.assertEqual(
                {path.name for path in output.iterdir()},
                {
                    "axiograph",
                    "axiograph_verify",
                    "axiograph_verify.sha256",
                    "axiograph-source-manifest.json",
                    "axiograph-release.json",
                },
            )
            self.assertEqual((output / "axiograph").stat().st_mode & 0o777, 0o755)
            self.assertEqual(
                (output / "axiograph-release.json").stat().st_mode & 0o777,
                0o644,
            )

    def test_reproducible_stored_windows_zip_validates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = build_valid_bundle(root, "x86_64-pc-windows-msvc", "first")
            second = build_valid_bundle(root, "x86_64-pc-windows-msvc", "second")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            report = validate_archive(first, expected_host="x86_64-pc-windows-msvc")
            self.assertTrue(report["validated"])

    @unittest.skipIf(
        os.name == "nt", "Windows does not permit replacing this open fixture"
    )
    def test_open_archive_handle_survives_concurrent_path_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "race")
            original = archive.read_bytes()
            replacement = archive.with_name("replacement.tar.gz")
            replacement.write_bytes(b"attacker-controlled replacement")

            with (
                open_regular_archive(archive) as (source, archive_size),
                tempfile.TemporaryFile(mode="w+b") as snapshot,
            ):
                os.replace(replacement, archive)
                digest = snapshot_archive(source, snapshot, archive_size)
                entries = validate_tar(snapshot, "")

            self.assertEqual(digest, hashlib.sha256(original).hexdigest())
            self.assertEqual(len(entries), 5)
            self.assertEqual(archive.read_bytes(), b"attacker-controlled replacement")

    def test_rejects_path_traversal_symlink_and_duplicate_zip_entry(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            traversal = root / "axiograph-x86_64-unknown-linux-gnu.tar.gz"
            write_tar(traversal, {"../escape": (0o644, b"x")})
            with self.assertRaisesRegex(ArchiveValidationError, "top-level filename"):
                validate_archive(traversal)

            symlink = root / "symlink.tar.gz"
            with (
                symlink.open("wb") as raw,
                gzip.GzipFile(
                    filename="", mode="wb", fileobj=raw, mtime=0
                ) as compressed,
                tarfile.open(fileobj=compressed, mode="w") as archive,
            ):
                info = tarfile.TarInfo("axiograph")
                info.type = tarfile.SYMTYPE
                info.linkname = "../../outside"
                info.mtime = 0
                archive.addfile(info)
            canonical_name = root / "axiograph-x86_64-unknown-linux-gnu.tar.gz"
            traversal.unlink()
            symlink.rename(canonical_name)
            with self.assertRaisesRegex(ArchiveValidationError, "not a regular file"):
                validate_archive(canonical_name)

            duplicate = build_valid_bundle(root, "x86_64-pc-windows-msvc", "zip")
            with zipfile.ZipFile(duplicate, "a", zipfile.ZIP_STORED) as output:
                info = zipfile.ZipInfo("axiograph.exe", (1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.compress_type = zipfile.ZIP_STORED
                info.external_attr = (stat.S_IFREG | 0o755) << 16
                output.writestr(info, b"replacement")
            with self.assertRaisesRegex(ArchiveValidationError, "duplicate"):
                validate_archive(duplicate)

    def test_rejects_checker_corruption_and_release_manifest_substitution(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            valid = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "valid")
            files = archived_files(read_valid_tar(valid))

            checker_mode, checker = files["axiograph_verify"]
            files["axiograph_verify"] = (checker_mode, checker + b"tampered\n")
            corrupted = root / "axiograph-x86_64-unknown-linux-gnu.tar.gz"
            write_tar(corrupted, files)
            with self.assertRaisesRegex(
                ArchiveValidationError, "checksum does not match"
            ):
                validate_archive(corrupted)

            files = archived_files(read_valid_tar(valid))
            try:
                release = json.loads(files["axiograph-release.json"][1])
            except json.JSONDecodeError as error:
                self.fail(f"test fixture release manifest was invalid: {error}")
            release["host"] = "aarch64-apple-darwin"
            files["axiograph-release.json"] = (0o644, canonical_json(release))
            substituted = (
                root / "substituted" / "axiograph-x86_64-unknown-linux-gnu.tar.gz"
            )
            substituted.parent.mkdir()
            write_tar(substituted, files)
            with self.assertRaisesRegex(ArchiveValidationError, "host does not match"):
                validate_archive(substituted)

    def test_rejects_concatenated_gzip_and_nonempty_extraction_destination(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            valid = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "valid")
            concatenated = root / "concatenated" / valid.name
            concatenated.parent.mkdir()
            concatenated.write_bytes(
                valid.read_bytes() + gzip.compress(b"hidden second member", mtime=0)
            )
            with self.assertRaisesRegex(
                ArchiveValidationError, "exactly one gzip member"
            ):
                validate_archive(concatenated)

            destination = root / "nonempty"
            destination.mkdir()
            (destination / "sentinel").write_bytes(b"do not replace")
            with self.assertRaisesRegex(ArchiveValidationError, "must be empty"):
                validate_archive(valid, destination)
            self.assertEqual((destination / "sentinel").read_bytes(), b"do not replace")

    def test_rejects_noncanonical_gzip_zip_encoding_wrong_mode_and_truncation(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            valid = build_valid_bundle(root, "x86_64-unknown-linux-gnu", "valid")

            bad_header = root / "header" / valid.name
            bad_header.parent.mkdir()
            raw = bytearray(valid.read_bytes())
            raw[4] = 1
            bad_header.write_bytes(raw)
            with self.assertRaisesRegex(ArchiveValidationError, "zero mtime"):
                validate_archive(bad_header)

            files = archived_files(read_valid_tar(valid))
            mode, runtime = files["axiograph"]
            self.assertEqual(mode, 0o755)
            files["axiograph"] = (0o644, runtime)
            wrong_mode = root / "mode" / valid.name
            wrong_mode.parent.mkdir()
            write_tar(wrong_mode, files)
            with self.assertRaisesRegex(ArchiveValidationError, "mode"):
                validate_archive(wrong_mode)

            truncated = root / "truncated" / valid.name
            truncated.parent.mkdir()
            truncated.write_bytes(valid.read_bytes()[:100])
            with self.assertRaises(
                (ArchiveValidationError, tarfile.TarError, EOFError)
            ):
                validate_archive(truncated)

            windows = build_valid_bundle(root, "x86_64-pc-windows-msvc", "windows")
            compressed = root / "compressed" / windows.name
            compressed.parent.mkdir()
            with (
                zipfile.ZipFile(windows) as source,
                zipfile.ZipFile(compressed, "w", zipfile.ZIP_DEFLATED) as output,
            ):
                for old in source.infolist():
                    info = zipfile.ZipInfo(old.filename, old.date_time)
                    info.create_system = 3
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = old.external_attr
                    output.writestr(info, source.read(old))
            with self.assertRaisesRegex(ArchiveValidationError, "stored encoding"):
                validate_archive(compressed)


if __name__ == "__main__":
    unittest.main()
