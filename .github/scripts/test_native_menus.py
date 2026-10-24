"""Compile real AppKit menus for both architectures and exercise their lifecycle."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / "crates" / "thinoda-platform" / "native"


@unittest.skipUnless(sys.platform == "darwin", "requires macOS and the real Apple SDK")
class MacNativeMenuTests(unittest.TestCase):
    def test_sdk_contracts_and_native_callback_ownership(self):
        sdk = subprocess.check_output(
            ["xcrun", "--sdk", "macosx", "--show-sdk-path"], text=True
        ).strip()
        with tempfile.TemporaryDirectory(prefix="thinoda-native-menu-") as directory:
            common = [
                "xcrun", "--sdk", "macosx", "clang", "-isysroot", sdk,
                "-mmacosx-version-min=11.0", "-fobjc-arc", "-Wall", "-Wextra",
                "-Werror", "-Werror=unguarded-availability-new",
            ]
            for architecture in ("x86_64", "arm64"):
                with self.subTest(architecture=architecture):
                    subprocess.run(
                        common + [
                            "-arch", architecture, "-dynamiclib",
                            str(NATIVE / "menu_macos.m"), "-framework", "AppKit",
                            "-o", str(Path(directory) / f"menu-{architecture}.dylib"),
                        ],
                        check=True, timeout=60,
                    )
            executable = Path(directory) / "native-menu-test"
            subprocess.run(
                common + [
                    str(NATIVE / "tests" / "menu_macos_test.m"), "-framework", "AppKit",
                    "-o", str(executable),
                ],
                check=True, timeout=60,
            )
            subprocess.run([str(executable)], check=True, timeout=30)
