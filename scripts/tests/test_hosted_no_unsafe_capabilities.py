from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.tests.run_hosted_no_unsafe_capability_tests import (
    CANDIDATE_HOMES,
    CLEANUP_REPORT_SCHEMA,
    CONTRACT_SCHEMA,
    EVIDENCE_MANIFEST_SCHEMA,
    PRIVILEGED_STATE_NAME,
    PRIVILEGED_STATE_SCHEMA,
    CapabilityFixtureError,
    RealPrivilegedFixtureOperations,
    _persist_privileged_state,
    _privileged_contract,
    _read_contract,
    cleanup_privileged,
    collect_evidence,
    generator_invocation,
    main,
    mount_observation,
    prepare,
    record_cleanup,
    require_archive_identity,
    scanner_invocation,
    setup_privileged,
    validate_bind_mount,
    validate_device,
)

REPO = Path(__file__).resolve().parents[2]


class FakePrivilegedOperations:
    def __init__(self, *, harden_error: str | None = None) -> None:
        self.events: list[str] = []
        self.harden_error = harden_error
        self.device = {
            "path": "/owned/device",
            "st_dev": 11,
            "st_ino": 12,
            "st_rdev": os.makedev(1, 3),
            "major": 1,
            "minor": 3,
            "mode": "0o0",
            "owner_uid": 0,
        }
        self.mount = {
            "mount_id": 81,
            "parent_id": 40,
            "device": "8:1",
            "root": "/backing/subdirectory",
            "target": "/owned/target",
            "options": ["rw"],
            "filesystem": "ext4",
            "source_identity": [os.makedev(8, 1), 42],
            "target_identity": [os.makedev(8, 1), 42],
            "target_pre_identity": [os.makedev(8, 1), 99],
        }

    def create_device(self, _contract: dict[str, object]) -> dict[str, object]:
        self.events.append("create-device")
        return dict(self.device)

    def create_mount(self, _contract: dict[str, object]) -> dict[str, object]:
        self.events.append("create-mount")
        return dict(self.mount)

    def harden_mount(
        self,
        _contract: dict[str, object],
        record: dict[str, object],
    ) -> dict[str, object]:
        self.events.append("harden-mount")
        if self.harden_error is not None:
            raise CapabilityFixtureError(self.harden_error)
        hardened = dict(record)
        hardened["options"] = ["nodev", "noexec", "nosuid", "ro"]
        return hardened

    def remove_device(
        self,
        _contract: dict[str, object],
        _record: dict[str, object],
    ) -> None:
        self.events.append("remove-device")

    def unmount(
        self,
        _contract: dict[str, object],
        _record: dict[str, object],
    ) -> None:
        self.events.append("unmount")


class HostedCapabilityFixtureContractTests(unittest.TestCase):
    def test_small_regular_archive_identity_helper_has_positive_and_negative_controls(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "archive"
            archive.write_bytes(b"fixed archive bytes")
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            require_archive_identity(
                archive,
                expected_bytes=len(archive.read_bytes()),
                expected_sha256=digest,
            )
            with self.assertRaisesRegex(
                CapabilityFixtureError, "bounded regular input"
            ):
                require_archive_identity(
                    archive,
                    expected_bytes=len(archive.read_bytes()) + 1,
                    expected_sha256=digest,
                )
            with self.assertRaisesRegex(CapabilityFixtureError, "SHA-256"):
                require_archive_identity(
                    archive,
                    expected_bytes=len(archive.read_bytes()),
                    expected_sha256="0" * 64,
                )

    def test_mount_observation_requires_one_exact_hardened_mount(self) -> None:
        target = Path("/tmp/owned fixture")
        mountinfo = (
            "81 40 8:1 /source /tmp/owned\\040fixture "
            "ro,nosuid,nodev,noexec,relatime - ext4 /dev/root rw\n"
        )
        observed = mount_observation(mountinfo, target)
        self.assertEqual(observed["mount_id"], 81)
        self.assertEqual(observed["target"], str(target))
        self.assertEqual(observed["filesystem"], "ext4")
        with self.assertRaisesRegex(CapabilityFixtureError, "missing options"):
            mount_observation(mountinfo.replace(",noexec", ""), target)
        with self.assertRaisesRegex(CapabilityFixtureError, "found 2"):
            mount_observation(mountinfo + mountinfo, target)

    def test_regular_file_cannot_substitute_for_real_device(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "not-a-device"
            path.write_bytes(b"")
            with self.assertRaisesRegex(
                CapabilityFixtureError, "real character device"
            ):
                validate_device(path)

    def test_contract_rejects_escape_and_root_user(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            contract = {
                "schema": CONTRACT_SCHEMA,
                "prepared_uid": os.getuid(),
                "prepared_gid": os.getgid(),
                "fixture_root": str(root),
                "device_repo": str(root / "device-repo"),
                "device_path": str(root / "device-repo/device"),
                "mount_repo": str(root / "mount-repo"),
                "mount_source": str(root / "mount-repo/scripts-source"),
                "mount_target": str(root / "mount-repo/scripts"),
                "archive": str(root / "archive"),
                "output": "/tmp/escape",
            }
            (root / "fixture-contract.json").write_text(
                json.dumps(contract), encoding="utf-8"
            )
            message = "tests must run" if os.getuid() == 0 else "escapes fixture root"
            with self.assertRaisesRegex(CapabilityFixtureError, message):
                _read_contract(root)

    def test_missing_privileged_fixture_fails_and_saves_bounded_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            fixture = root / "absent-fixture"
            report = root / "failure-report.json"
            self.assertEqual(
                main(
                    [
                        "verify-and-run",
                        "--fixture-root",
                        str(fixture),
                        "--report",
                        str(report),
                    ]
                ),
                1,
            )
            value = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(value["result"], "fail")
            self.assertIn("fixture-contract.json", value["failure"])
            self.assertLess(report.stat().st_size, 4096)

    def test_production_cli_invocations_are_bound_to_exact_fixture_roots(self) -> None:
        root = Path("/runner-temp/owned-fixture")
        contract = {
            "device_repo": str(root / "device-repo"),
            "mount_repo": str(root / "mount-repo"),
            "mount_target": str(root / "mount-repo/scripts"),
            "archive": str(root / "mount-repo/archive"),
            "output": str(
                root / "mount-repo/build/engineering-quality/hosted/run/output"
            ),
        }
        scanner_argv, scanner_cwd = scanner_invocation(contract)
        self.assertEqual(scanner_cwd, root / "device-repo")
        self.assertEqual(
            scanner_argv,
            [
                scanner_argv[0],
                "-B",
                str(root / "device-repo/scripts/check_no_unsafe.py"),
            ],
        )
        self.assertEqual(len(scanner_argv), 3)

        generator_argv, generator_cwd = generator_invocation(contract)
        self.assertEqual(generator_cwd, root / "mount-repo")
        self.assertEqual(
            generator_argv[2],
            str(
                root
                / "mount-repo/scripts/generate_no_unsafe_external_cache_manifest.py"
            ),
        )
        self.assertEqual(
            generator_argv[generator_argv.index("--check-manifest") + 1],
            str(root / "mount-repo/scripts/no_unsafe_external_cache_manifest_v1.json"),
        )
        self.assertNotIn("--expect-archive", generator_argv)
        for option in ("--archive", "--out", "--check-manifest"):
            self.assertTrue(
                Path(generator_argv[generator_argv.index(option) + 1]).is_relative_to(
                    generator_cwd
                )
            )

    def test_bind_validation_uses_mount_id_target_and_directory_identity(self) -> None:
        target = Path("/runner-temp/owned-fixture/mount-repo/scripts")
        source = Path("/runner-temp/owned-fixture/mount-repo/scripts-source")
        mountinfo = (
            f"81 40 8:1 /source-root {target} "
            "ro,nosuid,nodev,noexec,relatime - ext4 /dev/root rw\n"
        )
        helper = (
            "scripts.tests.run_hosted_no_unsafe_capability_tests._directory_identity"
        )
        device = os.makedev(8, 1)
        with mock.patch(helper, side_effect=[(device, 42), (device, 42)]):
            observed = validate_bind_mount(source, target, mountinfo)
        self.assertEqual(observed["mount_id"], 81)
        self.assertEqual(observed["root"], "/source-root")
        self.assertEqual(observed["source_identity"], [device, 42])
        self.assertEqual(observed["target_identity"], [device, 42])
        with (
            mock.patch(helper, side_effect=[(device, 42), (device, 43)]),
            self.assertRaisesRegex(CapabilityFixtureError, "identities differ"),
        ):
            validate_bind_mount(source, target, mountinfo)
        expected = dict(observed)
        expected["mount_id"] = 82
        with (
            mock.patch(helper, side_effect=[(device, 42), (device, 42)]),
            self.assertRaisesRegex(CapabilityFixtureError, "mount_id"),
        ):
            validate_bind_mount(source, target, mountinfo, expected=expected)
        changed_root = dict(observed)
        changed_root["root"] = "/different-backing-subdirectory"
        with (
            mock.patch(helper, side_effect=[(device, 42), (device, 42)]),
            self.assertRaisesRegex(CapabilityFixtureError, "root"),
        ):
            validate_bind_mount(source, target, mountinfo, expected=changed_root)
        wrong_device = mountinfo.replace("8:1", "8:2")
        with (
            mock.patch(helper, side_effect=[(device, 42), (device, 42)]),
            self.assertRaisesRegex(CapabilityFixtureError, "mountinfo device"),
        ):
            validate_bind_mount(source, target, wrong_device)

    def test_workflow_uses_one_scoped_setup_and_cleanup_operation(self) -> None:
        workflow = (
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ).read_text(encoding="utf-8")
        for forbidden in (
            "run_hosted_capability_tests.py",
            "--expect-archive",
            "findmnt",
            "sudo -- mknod",
            "sudo -- mount",
            "sudo -- umount",
            "sudo -- rm",
            "apt-get",
        ):
            self.assertNotIn(forbidden, workflow)
        self.assertEqual(workflow.count("setup-privileged"), 1)
        self.assertEqual(workflow.count("cleanup-privileged"), 1)
        self.assertEqual(workflow.count("verify-and-run"), 1)
        self.assertNotIn("matrix.arch ==", workflow)
        sudo_commands = re.findall(r"sudo -- ([^;\n]+)", workflow)
        self.assertEqual(len(sudo_commands), 2)
        self.assertTrue(
            all(
                command.startswith("python3 scripts/tests/")
                for command in sudo_commands
            )
        )

    def test_prebootstrap_failure_retains_evidence_without_masking_failure(
        self,
    ) -> None:
        workflow = (
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ).read_text(encoding="utf-8")
        checkout = workflow.index(
            "      - name: Check out exact source without credentials"
        )
        initialize = workflow.index(
            "      - name: Initialize evidence retention before fallible bootstrap"
        )
        rust_action = workflow.index(
            "      - name: Install the repository Rust metadata toolchain"
        )
        prepare_step = workflow.index(
            "      - name: Download and prepare bounded regular fixture inputs"
        )
        self.assertLess(checkout, initialize)
        self.assertLess(initialize, rust_action)
        self.assertLess(rust_action, prepare_step)
        self.assertNotIn("continue-on-error", workflow)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "evidence"
            source.mkdir(mode=0o700)
            (source / "prepare.log").write_text(
                "evidence_initialized_before_bootstrap=true\n", encoding="utf-8"
            )
            (source / "cleanup.log").write_text(
                "fixture contract absent after bootstrap failure\n", encoding="utf-8"
            )
            failed_bootstrap_exit = 1
            cleanup = record_cleanup(
                source / "cleanup.log", failed_bootstrap_exit, source / "cleanup.json"
            )
            output = root / "upload"
            manifest = collect_evidence(source, output)
            self.assertEqual(failed_bootstrap_exit, 1)
            self.assertEqual(cleanup["cleanup_status"], "failed")
            self.assertEqual(manifest["upload_safety"], "pass")
            self.assertEqual(
                {item["name"] for item in manifest["present"]},
                {"prepare.log", "cleanup.log", "cleanup.json"},
            )
            self.assertIn("capability.json", manifest["missing"])

    def test_workflow_trigger_runners_permissions_and_actions_are_closed(self) -> None:
        workflow = (
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ).read_text(encoding="utf-8")
        self.assertIn('      - "ci/no-unsafe-capability-tests-*"', workflow)
        self.assertIn("runner: ubuntu-24.04\n", workflow)
        self.assertIn("runner: ubuntu-24.04-arm\n", workflow)
        self.assertIn("permissions:\n  contents: read\n", workflow)
        self.assertNotIn("workflow_dispatch", workflow)
        self.assertNotIn("pull_request", workflow)
        self.assertNotIn("--privileged", workflow)
        self.assertNotIn("packages: write", workflow)
        self.assertNotIn("id-token: write", workflow)
        uses = re.findall(r"uses:\s*([^\s]+)", workflow)
        self.assertEqual(len(uses), 3)
        for use in uses:
            self.assertRegex(use, r"^[^@]+@[0-9a-f]{40}$")
        self.assertIn(
            "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772",
            uses,
        )
        self.assertIn("          toolchain: 1.98.0\n", workflow)
        self.assertIn("          persist-credentials: false\n", workflow)

    def test_workflow_bounds_archive_during_and_after_download(self) -> None:
        workflow = (
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ).read_text(encoding="utf-8")
        self.assertEqual(workflow.count("--max-filesize 137826806"), 1)
        self.assertEqual(
            workflow.count('test "$(stat -c %s "$archive_part")" = 137826806'), 1
        )
        self.assertEqual(
            workflow.count(
                "974128f44dd43618a06d21e5fe6d9ff67188de5986fe0bc57b534b0e4639efb9  $archive_part"
            ),
            1,
        )
        self.assertLess(
            workflow.index("--max-filesize 137826806"),
            workflow.index('--output "$archive_part"'),
        )

    def test_workflow_upload_safety_is_separate_from_job_semantics(self) -> None:
        workflow = (
            REPO / ".github/workflows/no-unsafe-capability-tests.yml"
        ).read_text(encoding="utf-8")
        validation = workflow.index(
            "      - name: Validate upload safety and retain bounded partial evidence"
        )
        upload = workflow.index("      - name: Upload bounded capability evidence")
        self.assertLess(validation, upload)
        validation_block = workflow[validation:upload]
        self.assertIn("        id: validate_evidence\n", validation_block)
        self.assertIn("        if: always()\n", validation_block)
        self.assertIn("collect-evidence", validation_block)
        upload_block = workflow[upload:]
        self.assertIn(
            "        if: ${{ always() && steps.validate_evidence.outcome == 'success' }}\n",
            upload_block,
        )
        self.assertIn("${{ runner.temp }}/${{ env.UPLOAD_DIR_NAME }}/", upload_block)
        self.assertNotIn("FIXTURE_NAME", upload_block)

    def test_prepare_builds_real_locked_git_fixture_and_scanner_control_flow(
        self,
    ) -> None:
        cache_library = (
            REPO
            / "build/engineering-quality/release-roadmap/complete-kani-with-verified-path-20260908T044738Z/tools/kani-home/kani-0.67.0/library"
        )
        self.assertTrue(
            cache_library.is_dir(), "local exact Kani cache fixture is required"
        )
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary).resolve()
            archive = parent / "small-parser-fixture"
            archive.write_bytes(b"local parser fixture only")
            root = parent / "owned-fixture"
            with mock.patch(
                "scripts.tests.run_hosted_no_unsafe_capability_tests.require_archive_identity"
            ):
                contract = prepare(root, archive)
            device_repo = Path(contract["device_repo"])
            self.assertTrue((device_repo / "rust/Cargo.lock").is_file())
            metadata = subprocess.run(
                [
                    "cargo",
                    "metadata",
                    "--manifest-path",
                    str(device_repo / "rust/Cargo.toml"),
                    "--format-version",
                    "1",
                    "--locked",
                    "--offline",
                    "--no-deps",
                ],
                cwd=device_repo,
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )
            self.assertEqual(metadata.returncode, 0, metadata.stderr)
            tracked = subprocess.run(
                ["git", "ls-files", "-z"],
                cwd=device_repo,
                capture_output=True,
                timeout=30,
                check=True,
            ).stdout.split(b"\0")
            self.assertIn(b"rust/Cargo.lock", tracked)
            candidate_prefix = CANDIDATE_HOMES[0].encode()
            self.assertFalse(any(candidate_prefix in path for path in tracked))
            destination = Path(contract["device_path"]).parent
            shutil.copytree(cache_library, destination, dirs_exist_ok=True)
            argv, cwd = scanner_invocation(contract)
            baseline = subprocess.run(
                argv,
                cwd=cwd,
                capture_output=True,
                text=True,
                timeout=120,
                check=False,
            )
            self.assertEqual(baseline.returncode, 0, baseline.stderr)
            self.assertEqual(json.loads(baseline.stdout)["external_cache_rust"], 28)
            fifo = destination / "local-fifo-control.rs"
            os.mkfifo(fifo)
            denied = subprocess.run(
                argv,
                cwd=cwd,
                capture_output=True,
                text=True,
                timeout=120,
                check=False,
            )
            self.assertEqual(denied.returncode, 1)
            self.assertIn("[E_FS_NONREGULAR]", denied.stderr)
            self.assertIn("local-fifo-control.rs", denied.stderr)
            self.assertNotIn("[E_FS_DATA_OPEN]", denied.stderr)
            self.assertNotIn("real device", denied.stderr)

    def test_nonprivileged_contract_cleanup_refuses_unrecorded_or_replaced_objects(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self._prepare_small_fixture(Path(temporary).resolve())
            operations = FakePrivilegedOperations()
            helper = "scripts.tests.run_hosted_no_unsafe_capability_tests"
            with (
                mock.patch(f"{helper}._target_has_mount", return_value=True),
                self.assertRaisesRegex(CapabilityFixtureError, "unrecorded"),
            ):
                cleanup_privileged(root, operations=operations)
            self.assertEqual(operations.events, [])

            contract = _privileged_contract(root)
            path = Path(contract["device_path"])
            path.write_bytes(b"replacement")
            parent = path.parent.stat()
            record = dict(operations.device)
            record["parent_identity"] = [parent.st_dev, parent.st_ino]
            real = RealPrivilegedFixtureOperations()
            with self.assertRaisesRegex(CapabilityFixtureError, "replaced"):
                real.remove_device(contract, record)
            self.assertTrue(stat.S_ISREG(path.stat().st_mode))
            self.assertEqual(path.read_bytes(), b"replacement")

    def test_nonprivileged_contract_rolls_back_device_when_first_record_write_fails(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self._prepare_small_fixture(Path(temporary).resolve())
            operations = FakePrivilegedOperations()

            def fail_persist(_path: Path, _state: dict[str, object]) -> None:
                raise OSError("injected record failure")

            with self.assertRaisesRegex(
                CapabilityFixtureError, "failure-state persistence"
            ):
                setup_privileged(root, operations=operations, persist=fail_persist)
            self.assertEqual(operations.events, ["create-device", "remove-device"])

    def test_nonprivileged_contract_rolls_back_mount_then_device_for_each_later_failure(
        self,
    ) -> None:
        for failure_point in ("mount-persist", "remount", "verify", "verified-persist"):
            with (
                self.subTest(failure_point=failure_point),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = self._prepare_small_fixture(Path(temporary).resolve())
                operations = FakePrivilegedOperations(
                    harden_error=f"injected {failure_point} failure"
                    if failure_point in ("remount", "verify")
                    else None
                )
                calls = 0

                def persist(
                    path: Path,
                    state: dict[str, object],
                    point: str = failure_point,
                ) -> None:
                    nonlocal calls
                    calls += 1
                    target = 2 if point == "mount-persist" else 3
                    if point not in ("remount", "verify") and calls == target:
                        raise OSError(f"injected {point}")
                    _persist_privileged_state(path, state)

                with self.assertRaisesRegex(
                    CapabilityFixtureError, "rollback complete"
                ):
                    setup_privileged(root, operations=operations, persist=persist)
                self.assertEqual(operations.events[-2:], ["unmount", "remove-device"])
                if failure_point in ("remount", "verify"):
                    self.assertIn("harden-mount", operations.events)

    def test_real_hardened_record_survives_successful_setup_to_cleanup(self) -> None:
        class RealShapeSuccessfulOperations(FakePrivilegedOperations):
            def __init__(self) -> None:
                super().__init__()
                self.real = RealPrivilegedFixtureOperations()

            def create_mount(self, contract: dict[str, object]) -> dict[str, object]:
                self.events.append("create-mount")
                source_identity = list(
                    contract["directory_identities"]["mount_source"]  # type: ignore[index]
                )
                target_pre_identity = list(
                    contract["directory_identities"]["mount_target"]  # type: ignore[index]
                )
                return {
                    "mount_id": 81,
                    "parent_id": 40,
                    "device": (
                        f"{os.major(source_identity[0])}:{os.minor(source_identity[0])}"
                    ),
                    "root": "/backing/subdirectory",
                    "target": str(contract["mount_target"]),
                    "options": ["rw"],
                    "filesystem": "ext4",
                    "source_identity": source_identity,
                    "target_identity": source_identity,
                    "target_pre_identity": target_pre_identity,
                }

            def harden_mount(
                self,
                contract: dict[str, object],
                record: dict[str, object],
            ) -> dict[str, object]:
                self.events.append("harden-mount")
                return self.real.harden_mount(contract, record)  # type: ignore[arg-type]

            def unmount(
                self,
                contract: dict[str, object],
                record: dict[str, object],
            ) -> None:
                self.events.append("unmount")
                self.real.unmount(contract, record)  # type: ignore[arg-type]

        with tempfile.TemporaryDirectory() as temporary:
            root = self._prepare_small_fixture(Path(temporary).resolve())
            operations = RealShapeSuccessfulOperations()
            contract = _privileged_contract(root)
            mount_record = operations.create_mount(contract)
            operations.events.clear()
            helper = "scripts.tests.run_hosted_no_unsafe_capability_tests"
            missing_pre_identity = {
                key: value
                for key, value in mount_record.items()
                if key != "target_pre_identity"
            }
            with (
                mock.patch(f"{helper}._run_privileged_command") as rejected_command,
                self.assertRaisesRegex(
                    CapabilityFixtureError, "target pre-identity differs"
                ),
            ):
                operations.real.harden_mount(contract, missing_pre_identity)
            rejected_command.assert_not_called()

            hardened_observation = {
                key: value
                for key, value in mount_record.items()
                if key != "target_pre_identity"
            }
            hardened_observation["options"] = ["nodev", "noexec", "nosuid", "ro"]
            target_pre_identity = tuple(mount_record["target_pre_identity"])
            with (
                mock.patch(f"{helper}._run_privileged_command") as command,
                mock.patch(
                    f"{helper}.validate_bind_mount",
                    side_effect=lambda *_args, **_kwargs: dict(hardened_observation),
                ),
                mock.patch(f"{helper}._mountinfo", return_value="fixture mountinfo"),
            ):
                state = setup_privileged(root, operations=operations)
                self.assertEqual(state["phase"], "ready")
                self.assertEqual(
                    state["mount"]["target_pre_identity"],
                    list(target_pre_identity),
                )
                persisted = json.loads(
                    (root / PRIVILEGED_STATE_NAME).read_text(encoding="utf-8")
                )
                self.assertEqual(
                    persisted["mount"]["target_pre_identity"],
                    list(target_pre_identity),
                )
                mounted_contract = _privileged_contract(root)
                mounted_contract["directory_identities"]["mount_target"] = list(
                    mount_record["source_identity"]
                )
                self.assertNotEqual(
                    mounted_contract["directory_identities"]["mount_target"],
                    list(target_pre_identity),
                )
                with (
                    mock.patch(
                        f"{helper}._privileged_contract",
                        return_value=mounted_contract,
                    ),
                    mock.patch(f"{helper}._target_has_mount", return_value=True),
                    mock.patch(f"{helper}.os.path.lexists", return_value=True),
                    mock.patch(
                        f"{helper}._directory_identity",
                        return_value=target_pre_identity,
                    ),
                ):
                    result = cleanup_privileged(root, operations=operations)
            self.assertEqual(result["status"], "clean")
            self.assertEqual(
                operations.events,
                [
                    "create-device",
                    "create-mount",
                    "harden-mount",
                    "unmount",
                    "remove-device",
                ],
            )
            self.assertEqual(command.call_count, 2)
            self.assertEqual(command.call_args_list[0].args[0][0], "/usr/bin/mount")
            self.assertEqual(command.call_args_list[1].args[0][0], "/usr/bin/umount")

    def test_nonprivileged_contract_cleanup_uses_mount_first_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self._prepare_small_fixture(Path(temporary).resolve())
            operations = FakePrivilegedOperations()
            state = {
                "schema": PRIVILEGED_STATE_SCHEMA,
                "fixture_root": str(root),
                "phase": "ready",
                "device": operations.device,
                "mount": operations.mount,
            }
            _persist_privileged_state(root / PRIVILEGED_STATE_NAME, state)
            helper = "scripts.tests.run_hosted_no_unsafe_capability_tests"
            with (
                mock.patch(f"{helper}._target_has_mount", return_value=True),
                mock.patch(f"{helper}.os.path.lexists", return_value=True),
            ):
                result = cleanup_privileged(root, operations=operations)
            self.assertEqual(result["status"], "clean")
            self.assertEqual(operations.events, ["unmount", "remove-device"])

    def test_nonprivileged_contract_bind_observation_failure_rolls_back(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = self._prepare_small_fixture(Path(temporary).resolve())
            contract = _privileged_contract(root)
            operations = RealPrivilegedFixtureOperations()
            observed = dict(FakePrivilegedOperations().mount)
            helper = "scripts.tests.run_hosted_no_unsafe_capability_tests"
            with (
                mock.patch(f"{helper}._run_privileged_command") as command,
                mock.patch(
                    f"{helper}.validate_bind_mount",
                    side_effect=[
                        CapabilityFixtureError("injected observation failure"),
                        observed,
                    ],
                ),
                mock.patch(f"{helper}._mountinfo", return_value="fixture mountinfo"),
                self.assertRaisesRegex(CapabilityFixtureError, "rolled back"),
            ):
                operations.create_mount(contract)
            self.assertEqual(command.call_count, 2)
            self.assertEqual(command.call_args_list[0].args[0][1], "--bind")
            self.assertEqual(command.call_args_list[1].args[0][0], "/usr/bin/umount")

    def test_partial_and_cleanup_failed_evidence_are_safely_retained(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "evidence"
            source.mkdir()
            (source / "prepare.log").write_text(
                "early setup failure\n", encoding="utf-8"
            )
            (source / "cleanup.log").write_text(
                "cleanup refused replacement\n", encoding="utf-8"
            )
            report = record_cleanup(source / "cleanup.log", 1, source / "cleanup.json")
            self.assertEqual(report["schema"], CLEANUP_REPORT_SCHEMA)
            self.assertEqual(report["cleanup_status"], "failed")
            output = root / "upload"
            manifest = collect_evidence(source, output)
            self.assertEqual(manifest["schema"], EVIDENCE_MANIFEST_SCHEMA)
            self.assertEqual(manifest["upload_safety"], "pass")
            self.assertIn("capability.json", manifest["missing"])
            self.assertTrue((output / "cleanup.json").is_file())
            self.assertTrue((output / "evidence-manifest.json").is_file())

    def test_evidence_collection_rejects_unsafe_and_unexpected_entries(self) -> None:
        cases = ("symlink", "fifo", "oversized", "unexpected")
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                source = root / "evidence"
                source.mkdir()
                entry = source / (
                    "prepare.log" if case != "unexpected" else "fixture-tree"
                )
                if case == "symlink":
                    entry.symlink_to(source / "absent")
                elif case == "fifo":
                    os.mkfifo(entry)
                elif case == "oversized":
                    with entry.open("wb") as target:
                        target.truncate(1024 * 1024 + 1)
                else:
                    entry.mkdir()
                output = root / "upload"
                with self.assertRaises(CapabilityFixtureError):
                    collect_evidence(source, output)
                self.assertFalse(output.exists())
        with self.assertRaisesRegex(CapabilityFixtureError, "absolute"):
            collect_evidence(Path("relative"), Path("also-relative"))

    def _prepare_small_fixture(self, parent: Path) -> Path:
        archive = parent / "small-parser-fixture"
        archive.write_bytes(b"local parser fixture only")
        root = parent / "owned-fixture"
        with mock.patch(
            "scripts.tests.run_hosted_no_unsafe_capability_tests.require_archive_identity"
        ):
            prepare(root, archive)
        return root

    def test_prepare_refuses_existing_root_without_touching_it(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            marker = root / "marker"
            marker.write_text("preserve\n", encoding="utf-8")
            with self.assertRaisesRegex(CapabilityFixtureError, "new absolute path"):
                prepare(root, root / "absent-archive")
            self.assertEqual(marker.read_text(encoding="utf-8"), "preserve\n")


if __name__ == "__main__":
    unittest.main()
