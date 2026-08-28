import subprocess
import unittest
from pathlib import Path
from unittest.mock import Mock

from scripts.check_soroban_sdk_pin import CheckError, check_repo, main

FIXTURES = Path(__file__).resolve().parent.parent / "testdata" / "check_soroban_sdk_pin"


def metadata_ok(*args, **kwargs):
    return subprocess.CompletedProcess(args=args[0], returncode=0, stdout="{}", stderr="")


class CheckSorobanSdkPinTests(unittest.TestCase):
    def test_happy_path_passes(self):
        manifest_version, lockfile_version = check_repo(
            FIXTURES / "happy",
            metadata_runner=metadata_ok,
        )

        self.assertEqual(manifest_version, "22.0")
        self.assertEqual(lockfile_version, "22.0.10")

    def test_manifest_drift_fails(self):
        with self.assertRaises(CheckError) as ctx:
            check_repo(FIXTURES / "manifest_drift", metadata_runner=metadata_ok)

        message = str(ctx.exception)
        self.assertIn("Workspace soroban-sdk version drift detected", message)
        self.assertIn("contracts/foo/Cargo.toml", message)
        self.assertIn("crates/bar/Cargo.toml", message)
        self.assertIn("22.0", message)
        self.assertIn("23.0", message)

    def test_missing_lockfile_stanza_fails(self):
        with self.assertRaises(CheckError) as ctx:
            check_repo(FIXTURES / "lock_missing", metadata_runner=metadata_ok)

        self.assertIn("Cargo.lock does not contain a soroban-sdk package stanza", str(ctx.exception))

    def test_duplicate_lockfile_stanza_fails(self):
        with self.assertRaises(CheckError) as ctx:
            check_repo(FIXTURES / "lock_duplicate", metadata_runner=metadata_ok)

        self.assertIn("should resolve exactly one soroban-sdk package stanza", str(ctx.exception))

    def test_locked_metadata_failure_is_reported(self):
        metadata_runner = Mock(
            side_effect=subprocess.CalledProcessError(
                101,
                ["cargo", "metadata", "--locked", "--format-version", "1"],
                stderr="error: the lock file needs to be updated but --locked was passed",
            )
        )

        with self.assertRaises(CheckError) as ctx:
            check_repo(FIXTURES / "happy", metadata_runner=metadata_runner)

        self.assertIn("Cargo.lock is out of sync with the manifests", str(ctx.exception))
        self.assertIn("--locked was passed", str(ctx.exception))

    def test_cli_skip_metadata_succeeds_on_fixture(self):
        exit_code = main(["--root", str(FIXTURES / "happy"), "--skip-metadata"])
        self.assertEqual(exit_code, 0)


if __name__ == "__main__":
    unittest.main()
