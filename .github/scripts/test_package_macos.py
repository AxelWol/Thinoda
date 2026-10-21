import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import call, patch

from package_macos import (
    ROOT, TICKET_RETRY_DELAYS, notarize_dmg, package, package_config,
    run_with_ticket_retry,
)
from verify_macos_packages import ARCHITECTURES


IDENTITY = "Developer ID Application: Example Developer (EXAMPLE123)"
TICKET_FAILURE = (
    'Error Domain=NSURLErrorDomain Code=-1005 "The network connection was lost." '
    "NSErrorFailingURLStringKey=https://api.apple-cloudkit.com/database/1/"
    "com.apple.gk.ticket-delivery/production/public/records/lookup\n"
    "The staple and validate action failed! Error 68.\n"
)


class PackageConfigTests(unittest.TestCase):
    def test_both_targets_reuse_metadata_without_changing_local_signing(self):
        for target in ARCHITECTURES:
            with self.subTest(target=target):
                config = package_config(ROOT, target, IDENTITY)
                self.assertEqual(config["macos"]["signing-identity"], IDENTITY)
                self.assertEqual(config["identifier"], "io.github.axelwol.thinoda")
                self.assertEqual(config["macos"]["info-plist-path"], "../../assets/installer/Info.plist")
                self.assertTrue(config["version"])
                self.assertTrue(config["authors"])
                self.assertEqual(Path(config["binaries-dir"]), ROOT / "target" / target / "release")
                self.assertEqual(Path(config["out-dir"]), ROOT / "target" / "packages")
                self.assertEqual(config["binaries"], [{"path": "thinoda", "main": True}])
                self.assertTrue(config["file-associations"])
        manifest = ROOT / "crates" / "thinoda-app" / "Cargo.toml"
        self.assertIn('signing-identity = "-"', manifest.read_text(encoding="utf-8"))

    def test_ad_hoc_and_non_application_identities_are_rejected(self):
        for identity in ("", "-", "Developer ID Installer: Example"):
            with self.subTest(identity=identity), self.assertRaisesRegex(ValueError, "Developer ID Application"):
                package_config(ROOT, "aarch64-apple-darwin", identity)

    def test_non_macos_target_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Unsupported macOS target"):
            package_config(ROOT, "x86_64-pc-windows-msvc", IDENTITY)


class PackageTests(unittest.TestCase):
    @patch("package_macos.time.sleep")
    def test_stderr_ticket_failure_is_seen_from_a_real_subprocess(self, sleep):
        with tempfile.TemporaryDirectory() as temporary:
            marker = Path(temporary) / "attempted"
            script = (
                "import pathlib,sys; "
                f"marker=pathlib.Path({str(marker)!r}); "
                "failed=not marker.exists(); marker.touch(); "
                f"print({TICKET_FAILURE!r} if failed else 'Recovered', file=sys.stderr); "
                "sys.exit(68 if failed else 0)"
            )
            run_with_ticket_retry([sys.executable, "-c", script], timeout=120)
        sleep.assert_called_once_with(15)

    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    def test_retries_cargo_packager_after_transient_ticket_lookup_failure(
        self, command, sleep,
    ):
        command.side_effect = [
            subprocess.CompletedProcess(
                ["cargo-packager"], 1,
                stdout="https://api.apple-cloudkit.com/database/lookup: "
                "NSURLErrorDomain Code=-1005",
            ),
            subprocess.CompletedProcess(["cargo-packager"], 0, stdout="Packaged\n"),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            run_with_ticket_retry(["cargo-packager"], cwd=Path(temporary))

        self.assertEqual(command.call_count, 2)
        self.assertEqual(command.call_args_list[0], call(
            ["cargo-packager"], cwd=Path(temporary), stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, check=False, timeout=None,
        ))
        sleep.assert_called_once_with(15)

    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    def test_does_not_retry_non_transient_packager_failure(self, command, sleep):
        for output in (
            "https://notapi.apple-cloudkit.com/lookup: NSURLErrorDomain Code=-1005",
            "https://api.apple-cloudkit.com.example.com/lookup: network connection was lost",
            "https://api.apple-cloudkit.com/lookup: invalid signature",
            "No ticket found. The staple and validate action failed! Error 68.",
            "NSURLErrorDomain Code=-1005",
            "Notarization status Invalid: missing secure timestamp",
        ):
            with self.subTest(output=output):
                command.reset_mock()
                command.return_value = subprocess.CompletedProcess(
                    ["cargo-packager"], 1, stdout=output,
                )
                with self.assertRaises(subprocess.CalledProcessError):
                    run_with_ticket_retry(["cargo-packager"], cwd=Path("."))
                command.assert_called_once()
                sleep.assert_not_called()

    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    def test_transient_failure_budget_is_bounded_and_preserves_final_error(self, command, sleep):
        command.return_value = subprocess.CompletedProcess(
            ["cargo-packager"], 68, stdout=TICKET_FAILURE,
        )
        with self.assertRaises(subprocess.CalledProcessError) as error:
            run_with_ticket_retry(["cargo-packager"], cwd=ROOT)
        self.assertEqual(command.call_count, 5)
        self.assertEqual(sleep.call_args_list, [call(delay) for delay in TICKET_RETRY_DELAYS])
        self.assertEqual(error.exception.returncode, 68)
        self.assertEqual(error.exception.output, TICKET_FAILURE)

    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    def test_timeout_is_not_treated_as_confirmed_ticket_failure(self, command, sleep):
        command.side_effect = subprocess.TimeoutExpired(["xcrun"], 120)
        with self.assertRaises(subprocess.TimeoutExpired):
            run_with_ticket_retry(["xcrun", "stapler", "staple", "Thinoda.dmg"], timeout=120)
        command.assert_called_once()
        sleep.assert_not_called()

    @patch.dict("os.environ", {}, clear=True)
    @patch("package_macos.subprocess.run")
    def test_missing_credentials_fail_before_building(self, command):
        with self.assertRaisesRegex(ValueError, "MACOS_SIGN_IDENTITY"):
            package(ROOT, "aarch64-apple-darwin")
        command.assert_not_called()

    def test_builds_signed_app_and_dmg_then_checks_and_notarizes(self):
        for target in ARCHITECTURES:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                key = root / "AuthKey.p8"
                key.touch()
                output = root / "target" / "packages"
                output.mkdir(parents=True)
                app = output / "Thinoda.app"
                app.mkdir()
                image = output / "Thinoda.dmg"
                image.touch()
                env = {
                    "MACOS_SIGN_IDENTITY": IDENTITY, "APPLE_API_KEY": "key-id",
                    "APPLE_API_ISSUER": "issuer", "APPLE_API_KEY_PATH": str(key),
                }
                with (
                    patch.dict("os.environ", env, clear=True),
                    patch("package_macos.package_config", return_value={"macos": {"signing-identity": IDENTITY}}),
                    patch("package_macos.subprocess.run") as command,
                    patch("package_macos.run_with_ticket_retry") as pack,
                    patch("package_macos.run") as check,
                    patch("package_macos.notarize_dmg") as notarize,
                ):
                    package(root, target)
                    build = command.call_args
                    self.assertIn("--locked", build.args[0])
                    self.assertEqual(build.args[0][-1], target)
                    self.assertEqual(pack.call_args.kwargs, {"cwd": root / "crates" / "thinoda-app"})
                    args = pack.call_args.args[0]
                    self.assertEqual(json.loads(args[2])["macos"]["signing-identity"], IDENTITY)
                    self.assertEqual(args[-2:], ["--formats", "app,dmg"])
                    check.assert_any_call("xcrun", "stapler", "validate", str(app))
                    notarize.assert_called_once_with(image)

    @patch("package_macos.subprocess.run")
    def test_missing_decoded_key_fails_before_building(self, command):
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {
            "MACOS_SIGN_IDENTITY": IDENTITY, "APPLE_API_KEY": "key-id",
            "APPLE_API_ISSUER": "issuer",
            "APPLE_API_KEY_PATH": str(Path(temporary) / "missing.p8"),
        }, clear=True):
            with self.assertRaisesRegex(ValueError, "decoded App Store Connect key"):
                package(ROOT, "aarch64-apple-darwin")
        command.assert_not_called()


@patch.dict("os.environ", {
    "APPLE_API_KEY_PATH": "AuthKey.p8", "APPLE_API_KEY": "key-id",
    "APPLE_API_ISSUER": "issuer",
}, clear=True)
class NotarizationTests(unittest.TestCase):
    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    @patch("package_macos.run")
    def test_dmg_ticket_recovery_does_not_resubmit_or_skip_validation(self, command, staple, sleep):
        command.return_value = '{"id": "submission", "status": "Accepted"}'
        staple.side_effect = [
            subprocess.CompletedProcess(["xcrun"], 68, stdout=TICKET_FAILURE),
            subprocess.CompletedProcess(["xcrun"], 68, stdout=TICKET_FAILURE),
            subprocess.CompletedProcess(["xcrun"], 0, stdout="The staple action worked!\n"),
        ]
        notarize_dmg(Path("Thinoda.dmg"))
        self.assertEqual(command.call_count, 2)
        self.assertEqual(command.call_args_list[0].args[:3], ("xcrun", "notarytool", "submit"))
        self.assertEqual(
            command.call_args_list[1].args,
            ("xcrun", "stapler", "validate", "Thinoda.dmg"),
        )
        self.assertEqual(staple.call_count, 3)
        for invocation in staple.call_args_list:
            self.assertEqual(invocation, call(
                ["xcrun", "stapler", "staple", "Thinoda.dmg"],
                cwd=None, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, check=False, timeout=120,
            ))
        self.assertEqual(sleep.call_args_list, [call(15), call(30)])

    @patch("package_macos.time.sleep")
    @patch("package_macos.subprocess.run")
    @patch("package_macos.run")
    def test_dmg_ticket_exhaustion_fails_without_validating(self, command, staple, sleep):
        command.return_value = '{"id": "submission", "status": "Accepted"}'
        staple.return_value = subprocess.CompletedProcess(["xcrun"], 68, stdout=TICKET_FAILURE)
        with self.assertRaises(subprocess.CalledProcessError):
            notarize_dmg(Path("Thinoda.dmg"))
        command.assert_called_once()
        self.assertEqual(staple.call_count, 5)
        self.assertEqual(sleep.call_count, 4)

    @patch("package_macos.run_with_ticket_retry")
    @patch("package_macos.run")
    def test_accepted_image_is_stapled_and_validated(self, command, staple):
        command.return_value = '{"id": "submission", "status": "Accepted"}'
        image = Path("Thinoda.dmg")
        notarize_dmg(image)
        submit, validate = command.call_args_list
        self.assertEqual(submit.args[:4], ("xcrun", "notarytool", "submit", str(image)))
        self.assertIn("--wait", submit.args)
        self.assertIn("--output-format", submit.args)
        self.assertIn("AuthKey.p8", submit.args)
        self.assertEqual(submit.kwargs, {"timeout": 2100})
        staple.assert_called_once_with(
            ["xcrun", "stapler", "staple", str(image)], timeout=120,
        )
        self.assertEqual(validate.args, ("xcrun", "stapler", "validate", str(image)))

    @patch("package_macos.run_with_ticket_retry")
    @patch("package_macos.run")
    def test_rejected_image_reports_apple_log_and_is_never_stapled(self, command, staple):
        command.side_effect = [
            '{"id": "submission", "status": "Invalid"}', "Missing secure timestamp",
        ]
        with self.assertRaisesRegex(RuntimeError, "Missing secure timestamp"):
            notarize_dmg(Path("Thinoda.dmg"))
        self.assertEqual(command.call_count, 2)
        self.assertEqual(command.call_args.args[:4], ("xcrun", "notarytool", "log", "submission"))
        staple.assert_not_called()

    @patch("package_macos.run_with_ticket_retry")
    @patch("package_macos.run")
    def test_timeout_or_malformed_response_never_staples(self, command, staple):
        for failure in (subprocess.TimeoutExpired("notarytool", 2100), "not json"):
            with self.subTest(failure=failure):
                command.reset_mock()
                command.side_effect = [failure]
                with self.assertRaises((subprocess.TimeoutExpired, json.JSONDecodeError)):
                    notarize_dmg(Path("Thinoda.dmg"))
                self.assertEqual(command.call_count, 1)
                staple.assert_not_called()


if __name__ == "__main__":
    unittest.main()
