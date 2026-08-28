#!/usr/bin/env python3
"""Run a local fail-before-publish rehearsal over real bundle validators."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
from pathlib import Path

try:
    from .build_release_bundle import HOSTS, build_bundle
    from .generate_release_source_manifest import (
        build_source_manifest,
        canonical_bytes,
    )
    from .publish_release_local import (
        INJECTION_POINTS,
        InjectedPublicationFailure,
        LocalPublicationError,
        publish_local,
    )
    from .validate_release_archive import ArchiveValidationError
except ImportError:
    from build_release_bundle import HOSTS, build_bundle
    from generate_release_source_manifest import (
        build_source_manifest,
        canonical_bytes,
    )
    from publish_release_local import (
        INJECTION_POINTS,
        InjectedPublicationFailure,
        LocalPublicationError,
        publish_local,
    )
    from validate_release_archive import ArchiveValidationError

SOURCE_COMMIT = "a" * 40
SOURCE_TREE = "b" * 40


class RehearsalError(RuntimeError):
    pass


def _build_synthetic_assets(repo_root: Path, root: Path) -> tuple[Path, Path]:
    source_manifest = root / "source-manifest.json"
    source_manifest.write_bytes(
        canonical_bytes(
            build_source_manifest(
                SOURCE_COMMIT,
                SOURCE_TREE,
                [
                    (
                        "release/fixtures.json",
                        "100644",
                        "c" * 40,
                        (repo_root / "release" / "fixtures.json").read_bytes(),
                    )
                ],
            )
        )
    )
    runtime = root / "axiograph-input"
    checker = root / "axiograph-verify-input"
    runtime.write_bytes(b"synthetic deterministic runtime payload\n")
    checker.write_bytes(b"synthetic deterministic checker payload\n")
    dist = root / "dist"
    dist.mkdir()
    for index, host in enumerate(sorted(HOSTS)):
        built = root / f"built-{index}"
        archive, checksum, _ = build_bundle(
            repo_root,
            runtime,
            checker,
            source_manifest,
            repo_root / "release" / "fixtures.json",
            host,
            built,
        )
        shutil.copyfile(archive, dist / archive.name)
        shutil.copyfile(checksum, dist / checksum.name)
    image_digests = root / "image-digests"
    image_digests.mkdir()
    (image_digests / "amd64.digest").write_text("sha256:" + "1" * 64 + "\n")
    (image_digests / "arm64.digest").write_text("sha256:" + "2" * 64 + "\n")
    return dist, image_digests


def rehearse(repo_root: Path) -> dict[str, object]:
    with tempfile.TemporaryDirectory(
        prefix="axiograph-release-rehearsal-"
    ) as temporary:
        root = Path(temporary)
        dist, image_digests = _build_synthetic_assets(repo_root, root)
        injected: list[str] = []
        for point in sorted(INJECTION_POINTS):
            destination = root / f"published-{point}"
            try:
                publish_local(dist, image_digests, destination, point)
            except InjectedPublicationFailure:
                pass
            else:
                raise RehearsalError(f"injection point {point} unexpectedly published")
            if destination.exists() or destination.is_symlink():
                raise RehearsalError(
                    f"injection point {point} left a visible publication"
                )
            leaked = list(root.glob(".axiograph-release-stage-*"))
            if leaked:
                raise RehearsalError(
                    f"injection point {point} leaked staging directories: {leaked}"
                )
            injected.append(point)

        corrupt_dist = root / "corrupt-dist"
        shutil.copytree(dist, corrupt_dist)
        archive = sorted(
            path for path in corrupt_dist.iterdir() if not path.name.endswith(".sha256")
        )[0]
        with archive.open("ab") as output:
            output.write(b"corruption")
        corrupt_destination = root / "published-corrupt"
        try:
            publish_local(
                corrupt_dist,
                image_digests,
                corrupt_destination,
            )
        except (ArchiveValidationError, LocalPublicationError):
            pass
        else:
            raise RehearsalError("corrupted release assets unexpectedly published")
        if corrupt_destination.exists() or corrupt_destination.is_symlink():
            raise RehearsalError("corruption failure left a visible publication")

        destination = root / "published-success"
        report = publish_local(dist, image_digests, destination)
        publication_path = destination / "publication.json"
        if not publication_path.is_file():
            raise RehearsalError(
                "successful rehearsal did not publish publication.json"
            )
        try:
            published = json.loads(publication_path.read_bytes())
        except json.JSONDecodeError as error:
            raise RehearsalError(
                f"successful rehearsal wrote invalid publication JSON: {error}"
            ) from error
        if published != report:
            raise RehearsalError(
                "published rehearsal receipt differs from verified report"
            )
        expected_assets = sorted(path.name for path in dist.iterdir())
        actual_assets = sorted(path.name for path in (destination / "assets").iterdir())
        if actual_assets != expected_assets:
            raise RehearsalError("successful rehearsal published an inexact asset set")
        receipt_sha256 = hashlib.sha256(publication_path.read_bytes()).hexdigest()
        return {
            "schema": report["schema"],
            "failure_injections": injected,
            "corruption_rejected": True,
            "success_committed_once": True,
            "bundle_count": len(report["bundles"]),
            "image_digest_count": len(report["image_digests"]),
            "source_commit": report["source_commit"],
            "publication_receipt_sha256": receipt_sha256,
            "scope": report["scope"],
        }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo-root", type=Path, default=Path(__file__).resolve().parents[1]
    )
    args = parser.parse_args()
    try:
        report = rehearse(args.repo_root.resolve())
    except (
        ArchiveValidationError,
        LocalPublicationError,
        OSError,
        RehearsalError,
        json.JSONDecodeError,
    ) as error:
        print(f"release publication rehearsal failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
