#!/usr/bin/env python3
"""Validate an exact multi-platform OCI index assembled from audited inputs."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

MAX_MANIFEST_BYTES = 1024 * 1024
OCI_INDEX = "application/vnd.oci.image.index.v1+json"
OCI_MANIFEST = "application/vnd.oci.image.manifest.v1+json"
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
ATTESTATION_TYPE = "attestation-manifest"


class ManifestValidationError(ValueError):
    """The registry manifest graph does not match the audited release inputs."""


@dataclass(frozen=True)
class AuditedInput:
    os: str
    architecture: str
    manifest: Path


def fail(message: str) -> ManifestValidationError:
    return ManifestValidationError(message)


def read_json(path: Path) -> dict[str, object]:
    raw = path.read_bytes()
    if not raw or len(raw) > MAX_MANIFEST_BYTES:
        raise fail(
            f"{path} size must be in 1..={MAX_MANIFEST_BYTES} bytes"
        )
    try:
        parsed = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise fail(f"{path} is not bounded valid JSON: {error}") from error
    if not isinstance(parsed, dict):
        raise fail(f"{path} must contain a JSON object")
    return parsed


def descriptor_digest(descriptor: object, label: str) -> str:
    if not isinstance(descriptor, dict):
        raise fail(f"{label} descriptor must be an object")
    digest = descriptor.get("digest")
    if not isinstance(digest, str) or DIGEST.fullmatch(digest) is None:
        raise fail(f"{label} descriptor has an invalid SHA-256 digest")
    size = descriptor.get("size")
    if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
        raise fail(f"{label} descriptor has an invalid byte count")
    if descriptor.get("mediaType") != OCI_MANIFEST:
        raise fail(f"{label} descriptor is not an OCI image manifest")
    return digest


def descriptor_platform(descriptor: object, label: str) -> tuple[str, str]:
    if not isinstance(descriptor, dict):
        raise fail(f"{label} descriptor must be an object")
    platform = descriptor.get("platform")
    if not isinstance(platform, dict):
        raise fail(f"{label} descriptor has no platform object")
    os_name = platform.get("os")
    architecture = platform.get("architecture")
    if not isinstance(os_name, str) or not isinstance(architecture, str):
        raise fail(f"{label} descriptor platform is invalid")
    return os_name, architecture


def index_descriptors(index: dict[str, object], label: str) -> list[dict[str, object]]:
    if index.get("schemaVersion") != 2 or index.get("mediaType") != OCI_INDEX:
        raise fail(f"{label} is not an OCI image index")
    manifests = index.get("manifests")
    if not isinstance(manifests, list) or not manifests:
        raise fail(f"{label} has no manifest descriptors")
    result: list[dict[str, object]] = []
    for position, descriptor in enumerate(manifests):
        descriptor_digest(descriptor, f"{label}[{position}]")
        descriptor_platform(descriptor, f"{label}[{position}]")
        if not isinstance(descriptor, dict):
            raise fail(f"{label}[{position}] descriptor must be an object")
        result.append(descriptor)
    return result


def validate_attestation(
    descriptor: dict[str, object], runtime_digest: str, label: str
) -> None:
    if descriptor_platform(descriptor, label) != ("unknown", "unknown"):
        raise fail(f"{label} does not use the OCI attestation platform")
    annotations = descriptor.get("annotations")
    if not isinstance(annotations, dict) or any(
        not isinstance(key, str) or not isinstance(value, str)
        for key, value in annotations.items()
    ):
        raise fail(f"{label} annotations are invalid")
    if annotations.get("vnd.docker.reference.type") != ATTESTATION_TYPE:
        raise fail(f"{label} is not marked as an attestation manifest")
    if annotations.get("vnd.docker.reference.digest") != runtime_digest:
        raise fail(f"{label} is not bound to its audited runtime manifest")


def validate_container_manifest(
    final_manifest: Path, audited_inputs: list[AuditedInput]
) -> dict[str, object]:
    if not audited_inputs:
        raise fail("at least one audited platform input is required")
    platforms = [(item.os, item.architecture) for item in audited_inputs]
    if len(set(platforms)) != len(platforms):
        raise fail("audited platform inputs contain a duplicate platform")

    expected_by_digest: dict[str, dict[str, object]] = {}
    runtime_platforms: dict[tuple[str, str], str] = {}
    for item in audited_inputs:
        label = f"audited input {item.os}/{item.architecture}"
        descriptors = index_descriptors(read_json(item.manifest), label)
        runtime = [
            descriptor
            for descriptor in descriptors
            if descriptor_platform(descriptor, label)
            == (item.os, item.architecture)
        ]
        attestations = [
            descriptor
            for descriptor in descriptors
            if descriptor_platform(descriptor, label) == ("unknown", "unknown")
        ]
        if len(runtime) != 1 or len(attestations) != 1 or len(descriptors) != 2:
            raise fail(
                f"{label} must contain exactly one runtime and one attestation"
            )
        runtime_digest = descriptor_digest(runtime[0], f"{label} runtime")
        validate_attestation(attestations[0], runtime_digest, f"{label} attestation")
        runtime_platforms[(item.os, item.architecture)] = runtime_digest
        for descriptor in descriptors:
            digest = descriptor_digest(descriptor, label)
            previous = expected_by_digest.setdefault(digest, descriptor)
            if previous != descriptor:
                raise fail(f"conflicting source descriptors for {digest}")

    final_descriptors = index_descriptors(read_json(final_manifest), "release index")
    actual_by_digest: dict[str, dict[str, object]] = {}
    for descriptor in final_descriptors:
        digest = descriptor_digest(descriptor, "release index")
        if digest in actual_by_digest:
            raise fail(f"release index repeats descriptor {digest}")
        actual_by_digest[digest] = descriptor
    if actual_by_digest != expected_by_digest:
        raise fail("release index descriptors are not the exact audited union")
    actual_runtime_platforms = {
        descriptor_platform(descriptor, "release runtime"): digest
        for digest, descriptor in actual_by_digest.items()
        if descriptor_platform(descriptor, "release descriptor")
        != ("unknown", "unknown")
    }
    if actual_runtime_platforms != runtime_platforms:
        raise fail(
            f"release runtime platforms are not exact: {actual_runtime_platforms!r}"
        )
    return {
        "attestations": len(expected_by_digest) - len(runtime_platforms),
        "platforms": [f"{os_name}/{architecture}" for os_name, architecture in platforms],
        "validated": True,
    }


def parse_input(raw: str) -> AuditedInput:
    try:
        platform, path = raw.split("=", 1)
        os_name, architecture = platform.split("/", 1)
    except ValueError as error:
        raise argparse.ArgumentTypeError(
            "input must have the form os/architecture=manifest.json"
        ) from error
    if not os_name or not architecture or not path:
        raise argparse.ArgumentTypeError(
            "input must have the form os/architecture=manifest.json"
        )
    return AuditedInput(os_name, architecture, Path(path))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--final", required=True, type=Path)
    parser.add_argument("--input", action="append", required=True, type=parse_input)
    args = parser.parse_args()
    try:
        report = validate_container_manifest(args.final, args.input)
    except (ManifestValidationError, OSError) as error:
        print(f"container manifest validation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
