from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from verify_macos_packages import ARCHITECTURES, run, verify, verify_app


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


if __name__ == "__main__":
    unittest.main()
