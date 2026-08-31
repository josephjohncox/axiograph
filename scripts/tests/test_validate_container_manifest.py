from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from scripts.validate_container_manifest import (
    ATTESTATION_TYPE,
    OCI_INDEX,
    OCI_MANIFEST,
    AuditedInput,
    ManifestValidationError,
    validate_container_manifest,
)


def descriptor(
    digest_character: str,
    os_name: str,
    architecture: str,
    *,
    runtime_digest: str | None = None,
) -> dict[str, object]:
    result: dict[str, object] = {
        "mediaType": OCI_MANIFEST,
        "digest": f"sha256:{digest_character * 64}",
        "size": 123,
        "platform": {"architecture": architecture, "os": os_name},
    }
    if runtime_digest is not None:
        result["annotations"] = {
            "vnd.docker.reference.digest": runtime_digest,
            "vnd.docker.reference.type": ATTESTATION_TYPE,
        }
    return result


def write_index(path: Path, manifests: list[dict[str, object]]) -> None:
    path.write_text(
        json.dumps(
            {"schemaVersion": 2, "mediaType": OCI_INDEX, "manifests": manifests},
            sort_keys=True,
            separators=(",", ":"),
        ),
        encoding="utf-8",
    )


class ContainerManifestValidationTests(unittest.TestCase):
    def test_accepts_exact_union_of_audited_runtimes_and_attestations(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            amd_runtime = descriptor("1", "linux", "amd64")
            arm_runtime = descriptor("2", "linux", "arm64")
            amd_attestation = descriptor(
                "3",
                "unknown",
                "unknown",
                runtime_digest=str(amd_runtime["digest"]),
            )
            arm_attestation = descriptor(
                "4",
                "unknown",
                "unknown",
                runtime_digest=str(arm_runtime["digest"]),
            )
            write_index(root / "amd.json", [amd_runtime, amd_attestation])
            write_index(root / "arm.json", [arm_runtime, arm_attestation])
            write_index(
                root / "final.json",
                [amd_runtime, amd_attestation, arm_runtime, arm_attestation],
            )
            report = validate_container_manifest(
                root / "final.json",
                [
                    AuditedInput("linux", "amd64", root / "amd.json"),
                    AuditedInput("linux", "arm64", root / "arm.json"),
                ],
            )
            self.assertEqual(report["attestations"], 2)
            self.assertTrue(report["validated"])

    def test_rejects_unbound_attestation_and_unexpected_final_descriptor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = descriptor("1", "linux", "amd64")
            unbound = descriptor(
                "2",
                "unknown",
                "unknown",
                runtime_digest="sha256:" + "9" * 64,
            )
            write_index(root / "input.json", [runtime, unbound])
            write_index(root / "final.json", [runtime, unbound])
            with self.assertRaisesRegex(
                ManifestValidationError, "not bound to its audited runtime"
            ):
                validate_container_manifest(
                    root / "final.json",
                    [AuditedInput("linux", "amd64", root / "input.json")],
                )

            attestation = descriptor(
                "2",
                "unknown",
                "unknown",
                runtime_digest=str(runtime["digest"]),
            )
            unexpected = descriptor("3", "linux", "s390x")
            write_index(root / "input.json", [runtime, attestation])
            write_index(root / "final.json", [runtime, attestation, unexpected])
            with self.assertRaisesRegex(
                ManifestValidationError, "not the exact audited union"
            ):
                validate_container_manifest(
                    root / "final.json",
                    [AuditedInput("linux", "amd64", root / "input.json")],
                )


if __name__ == "__main__":
    unittest.main()
