from pathlib import Path
import plistlib
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import call, patch

from verify_macos_packages import (
    ARCHITECTURES, DETACH_RETRY_DELAYS, LOCALIZATIONS, CommandError,
    detach_readonly_image, readonly_image, run, verify, verify_app,
)


class CommandTests(unittest.TestCase):
    @patch("verify_macos_packages.subprocess.run")
    def test_failed_command_reports_both_output_streams(self, command):
        command.return_value = subprocess.CompletedProcess(
            ["codesign"], 1, stdout="invalid bundle\n", stderr="missing resource seal\n"
        )
        with self.assertRaisesRegex(RuntimeError, "invalid bundle\nmissing resource seal"):
            run("codesign", "--verify", "Thinoda.app")

    def test_missing_downloads_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "Expected one app archive and one DMG"):
                verify(Path(temporary), "aarch64-apple-darwin")

    @patch("verify_macos_packages.verify_app")
    @patch("verify_macos_packages.run_with_ticket_retry")
    @patch("verify_macos_packages.run")
    def test_release_checks_notarization_in_archive_dmg_and_installed_copy(
        self, command, ticket, app_check,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            app = source / "Thinoda.app"
            app.mkdir()
            target = "aarch64-apple-darwin"
            with tarfile.open(source / f"Thinoda-{target}.app.tar.gz", "w:gz") as archive:
                archive.add(app, arcname=app.name)
            image = source / f"Thinoda-{target}.dmg"
            image.touch()

            attached = False
            mount = None

            def run_command(*args, **kwargs):
                nonlocal attached, mount
                if args[:2] == ("hdiutil", "attach"):
                    mount = Path(args[args.index("-mountpoint") + 1])
                    (mount / app.name).mkdir()
                    attached = True
                if args[:2] == ("hdiutil", "info"):
                    return plistlib.dumps({"images": [{
                        "image-path": str(image.resolve()), "system-entities": [
                            {"dev-entry": "/dev/disk7"},
                            {"dev-entry": "/dev/disk7s1", "mount-point": str(mount)},
                        ],
                    }] if attached else []}).decode()
                if args[:2] == ("hdiutil", "detach"):
                    attached = False
                return ""

            command.side_effect = run_command
            verify(source, target, require_notarization=True)
            ticket.assert_any_call(["xcrun", "stapler", "validate", str(image)], timeout=120)
            ticket.assert_any_call([
                "spctl", "--assess", "--type", "open", "--context",
                "context:primary-signature", "--verbose=2", str(image),
            ], timeout=120)
            self.assertEqual(app_check.call_count, 3)
            for call in app_check.call_args_list:
                self.assertEqual(call.args[1:], ("arm64", True))
            command.assert_any_call("hdiutil", "detach", "/dev/disk7")
            self.assertFalse(attached)

    @patch("verify_macos_packages.run_with_ticket_retry")
    @patch("verify_macos_packages.run")
    def test_architecture_check_places_input_before_variadic_option(self, command, ticket):
        for architecture in ARCHITECTURES.values():
            with self.subTest(architecture=architecture), tempfile.TemporaryDirectory() as temporary:
                app = Path(temporary) / "Thinoda.app"
                contents = app / "Contents"
                (contents / "_CodeSignature").mkdir(parents=True)
                (contents / "_CodeSignature" / "CodeResources").touch()
                with (contents / "Info.plist").open("wb") as stream:
                    plistlib.dump({
                        "CFBundleIdentifier": "io.github.axelwol.thinoda",
                        "CFBundleExecutable": "thinoda",
                        "CFBundleDevelopmentRegion": "en",
                        "CFBundleLocalizations": LOCALIZATIONS,
                    }, stream)
                command.reset_mock()
                ticket.reset_mock()
                verify_app(app, architecture)
                command.assert_any_call(
                    "lipo", str(contents / "MacOS" / "thinoda"),
                    "-verify_arch", architecture,
                )
                ticket.assert_not_called()
                verify_app(app, architecture, require_notarization=True)
                ticket.assert_any_call(["xcrun", "stapler", "validate", str(app)], timeout=120)
                ticket.assert_any_call([
                    "spctl", "--assess", "--type", "execute", "--verbose=2", str(app),
                ], timeout=120)
                with (contents / "Info.plist").open("rb") as stream:
                    info = plistlib.load(stream)
                del info["CFBundleLocalizations"]
                with (contents / "Info.plist").open("wb") as stream:
                    plistlib.dump(info, stream)
                with self.assertRaisesRegex(ValueError, "bundle localizations"):
                    verify_app(app, architecture)


class ImageCleanupTests(unittest.TestCase):
    @staticmethod
    def error(code=16):
        return CommandError(("hdiutil", "detach"), subprocess.CompletedProcess(
            ["hdiutil"], code, stdout="", stderr="Resource busy" if code == 16 else "Not permitted",
        ))

    @patch("verify_macos_packages.time.sleep")
    @patch("verify_macos_packages.subprocess.run")
    @patch("verify_macos_packages.run")
    def test_busy_detach_recovers_without_force(self, command, diagnostic, sleep):
        command.side_effect = [self.error(), self.error(), ""]
        diagnostic.return_value = subprocess.CompletedProcess(["lsof"], 1, stdout="", stderr="")
        detach_readonly_image("/dev/disk7", Path("mount"))
        self.assertEqual(command.call_args_list, [call("hdiutil", "detach", "/dev/disk7")] * 3)
        self.assertEqual(sleep.call_args_list, [call(2), call(4)])
        self.assertEqual(diagnostic.call_args.args[0], ["lsof", "+D", "mount"])

    @patch("verify_macos_packages.time.sleep")
    @patch("verify_macos_packages.subprocess.run")
    @patch("verify_macos_packages.run")
    def test_exhausted_busy_retry_force_detaches_only_owned_readonly_device(
        self, command, diagnostic, sleep,
    ):
        command.side_effect = [self.error()] * 5 + [""]
        diagnostic.return_value = subprocess.CompletedProcess(["lsof"], 0, stdout="holder PID 42", stderr="")
        detach_readonly_image("/dev/disk7", Path("mount"))
        self.assertEqual(command.call_count, 6)
        self.assertEqual(command.call_args, call("hdiutil", "detach", "-force", "/dev/disk7"))
        self.assertEqual(sleep.call_args_list, [call(delay) for delay in DETACH_RETRY_DELAYS])
        self.assertEqual(diagnostic.call_count, 5)

    @patch("verify_macos_packages.time.sleep")
    @patch("verify_macos_packages.run")
    def test_nonbusy_error_is_not_retried_or_forced(self, command, sleep):
        command.side_effect = self.error(1)
        with self.assertRaisesRegex(CommandError, "Not permitted"):
            detach_readonly_image("/dev/disk7", Path("mount"))
        command.assert_called_once()
        sleep.assert_not_called()

    @patch("verify_macos_packages.attached_image")
    @patch("verify_macos_packages.detach_readonly_image")
    @patch("verify_macos_packages.run")
    def test_cleanup_on_body_or_partial_attach_error_and_timeout(self, command, detach, lookup):
        with tempfile.TemporaryDirectory() as temporary:
            image = Path(temporary) / "image.dmg"
            for stage in ("body", "attach", "timeout", "unmounted-partial"):
                with self.subTest(stage=stage):
                    mount = Path(temporary) / stage
                    entities = [{"dev-entry": "/dev/disk7"}]
                    if stage != "unmounted-partial":
                        entities.append({"dev-entry": "/dev/disk7s1", "mount-point": str(mount)})
                    lookup.side_effect = [None, {"system-entities": entities}, None]
                    command.reset_mock()
                    detach.reset_mock()
                    command.side_effect = (
                        None if stage == "body" else
                        subprocess.TimeoutExpired("hdiutil attach", 120) if stage == "timeout" else
                        RuntimeError("attach failed")
                    )
                    with self.assertRaises((ValueError, RuntimeError, subprocess.TimeoutExpired)):
                        with readonly_image(image, mount):
                            raise ValueError("verification failed")
                    detach.assert_called_once_with("/dev/disk7", mount)
                    command.assert_called_once_with(
                        "hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen",
                        "-mountpoint", str(mount), str(image.resolve()), input="Y\n",
                    )

    @patch("verify_macos_packages.attached_image")
    @patch("verify_macos_packages.detach_readonly_image")
    @patch("verify_macos_packages.run")
    def test_refuses_preexisting_foreign_or_ambiguous_mount(self, command, detach, lookup):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "image.dmg"
            lookup.return_value = {"system-entities": []}
            with self.assertRaisesRegex(RuntimeError, "already attached"):
                with readonly_image(image, root / "preexisting"):
                    self.fail("must not yield")
            command.assert_not_called()
            for name, entities in (
                ("foreign", [{"dev-entry": "/dev/disk7", "mount-point": str(root / "foreign-mount")}]),
                ("ambiguous", [{"dev-entry": "/dev/disk7"}, {"dev-entry": "/dev/disk8"}]),
            ):
                with self.subTest(name=name):
                    lookup.side_effect = [None, {"system-entities": entities}]
                    with self.assertRaisesRegex(RuntimeError, "refusing|Cannot identify"):
                        with readonly_image(image, root / name):
                            pass
            detach.assert_not_called()

    @patch("verify_macos_packages.attached_image")
    @patch("verify_macos_packages.detach_readonly_image")
    @patch("verify_macos_packages.run")
    def test_remaining_attachment_or_failed_force_is_not_success(self, command, detach, lookup):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            attachment = {"system-entities": [{"dev-entry": "/dev/disk7"}]}
            lookup.side_effect = [None, attachment, attachment]
            with self.assertRaisesRegex(RuntimeError, "remains attached"):
                with readonly_image(root / "image.dmg", root / "mount"):
                    pass
            lookup.side_effect = [None, attachment]
            detach.side_effect = self.error()
            with self.assertRaises(CommandError):
                with readonly_image(root / "image.dmg", root / "mount2"):
                    raise ValueError("original verification failure")


@unittest.skipUnless(sys.platform == "darwin", "Requires native macOS signing tools")
class NativeSignatureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.app = root / "Thinoda.app"
        contents = self.app / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        (contents / "Resources").mkdir()
        self.resource = contents / "Resources" / "sample.txt"
        self.resource.write_text("original resource")
        self.plist = contents / "Info.plist"
        with self.plist.open("wb") as stream:
            plistlib.dump({
                "CFBundleIdentifier": "io.github.axelwol.thinoda",
                "CFBundleExecutable": "thinoda",
                "CFBundleName": "Thinoda",
                "CFBundleVersion": "1",
                "CFBundlePackageType": "APPL",
                "CFBundleDevelopmentRegion": "en",
                "CFBundleLocalizations": LOCALIZATIONS,
            }, stream)
        source = root / "main.c"
        source.write_text("int main(void) { return 0; }\n")
        run("clang", str(source), "-o", str(contents / "MacOS" / "thinoda"))
        self.architecture = run("uname", "-m").strip()
        self.assertIn(self.architecture, ARCHITECTURES.values())

    def sign(self):
        run("codesign", "--force", "--sign", "-", str(self.app))

    def test_linker_only_or_unsigned_bundle_is_rejected(self):
        with self.assertRaises(RuntimeError):
            verify_app(self.app, self.architecture)

    def test_complete_signature_survives_install_copy(self):
        self.sign()
        installed = Path(self.temporary.name) / "installed" / "Thinoda.app"
        run("ditto", str(self.app), str(installed))
        verify_app(installed, self.architecture)

    def test_modified_resource_is_rejected(self):
        self.sign()
        self.resource.write_text("tampered resource")
        with self.assertRaises(RuntimeError):
            verify_app(self.app, self.architecture)

    def test_modified_info_plist_is_rejected(self):
        self.sign()
        with self.plist.open("rb") as stream:
            info = plistlib.load(stream)
        info["CFBundleVersion"] = "2"
        with self.plist.open("wb") as stream:
            plistlib.dump(info, stream)
        with self.assertRaises(RuntimeError):
            verify_app(self.app, self.architecture)

    def test_wrong_architecture_is_rejected(self):
        self.sign()
        other = "x86_64" if self.architecture == "arm64" else "arm64"
        with self.assertRaises(RuntimeError):
            verify_app(self.app, other)

    def test_compressed_dmg_preserves_license_signature_and_leaves_no_mount(self):
        from package_macos import create_dmg
        from verify_macos_packages import attached_image

        self.sign()
        root = Path(self.temporary.name)
        icon = self.app / "Contents" / "Resources" / "icon.icns"
        icon.write_bytes(b"test icon")
        info = plistlib.loads(self.plist.read_bytes())
        info["CFBundleIconFile"] = "icon.icns"
        self.plist.write_bytes(plistlib.dumps(info))
        self.sign()
        license = root / "LICENSE.md"
        license.write_text("Test license text", encoding="utf-8")
        image = root / "Thinoda.dmg"
        create_dmg(self.app, image, license)
        self.assertIsNone(attached_image(image))
        run("hdiutil", "verify", str(image))
        resources = plistlib.loads(run("hdiutil", "udifderez", "-xml", str(image)).encode())
        self.assertEqual(resources["TEXT"][0]["Data"], license.read_bytes())
        for name, fail in (("mount", False), ("failed-check-mount", True)):
            with self.subTest(fail=fail):
                try:
                    with readonly_image(image, root / name) as mount:
                        self.assertEqual((mount / "Applications").readlink(), Path("/Applications"))
                        self.assertEqual((mount / "LICENSE.md").read_bytes(), license.read_bytes())
                        self.assertEqual((mount / ".VolumeIcon.icns").read_bytes(), icon.read_bytes())
                        installed = root / f"installed-{name}" / self.app.name
                        run("ditto", str(mount / self.app.name), str(installed))
                        verify_app(installed, self.architecture)
                        if fail:
                            raise ValueError("deliberate verification failure")
                except ValueError:
                    if not fail:
                        raise
                self.assertIsNone(attached_image(image))


if __name__ == "__main__":
    unittest.main()
