import hashlib
import os
from pathlib import Path
import tarfile
import tempfile
import unittest

from release_artifacts import collect


class ReleaseArtifactsTests(unittest.TestCase):
    def test_all_five_targets_have_distinct_downloads_and_matching_checksums(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            destination = root / "dist"
            for target, formats in [
                ("x86_64-pc-windows-msvc", ["nsis"]),
                ("aarch64-pc-windows-msvc", ["nsis"]),
                ("aarch64-apple-darwin", ["app", "dmg"]),
                ("x86_64-apple-darwin", ["app", "dmg"]),
                ("x86_64-unknown-linux-gnu", ["deb", "appimage"]),
            ]:
                source = root / target
                source.mkdir()
                for format_name in formats:
                    if format_name == "app":
                        bundle = source / "yMind.app" / "Contents" / "MacOS"
                        bundle.mkdir(parents=True)
                        executable = bundle / "ymind"
                        executable.write_bytes(b"binary")
                        executable.chmod(0o755)
                        if os.name != "nt":
                            (bundle / "ymind-link").symlink_to("ymind")
                    else:
                        suffix = {"nsis": ".exe", "dmg": ".dmg", "deb": ".deb",
                                  "appimage": ".AppImage"}[format_name]
                        (source / f"yMind{suffix}").write_bytes(target.encode())
                collect(source, destination, target, formats)
                manifest = destination / f"SHA256SUMS-{target}.txt"
                lines = manifest.read_text().splitlines()
                self.assertEqual(len(lines), len(formats))
                for line in lines:
                    digest, name = line.split("  ")
                    self.assertIn(target, name)
                    self.assertEqual(
                        digest, hashlib.sha256((destination / name).read_bytes()).hexdigest()
                    )
                if "app" in formats:
                    with tarfile.open(destination / f"yMind-{target}.app.tar.gz") as archive:
                        member = archive.getmember("yMind.app/Contents/MacOS/ymind")
                        self.assertEqual(archive.extractfile(member).read(), b"binary")
                        if os.name != "nt":
                            self.assertTrue(member.mode & 0o111)
                            self.assertTrue(
                                archive.getmember("yMind.app/Contents/MacOS/ymind-link").issym()
                            )
            self.assertEqual(len(list(destination.iterdir())), 13)

    def test_missing_requested_format_fails_before_creating_downloads(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "yMind.deb").write_bytes(b"deb")
            with self.assertRaisesRegex(ValueError, "Expected one appimage"):
                collect(root, root / "dist", "x86_64-unknown-linux-gnu", ["deb", "appimage"])
            self.assertFalse((root / "dist").exists())

    def test_empty_or_ambiguous_packages_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "first.exe").touch()
            with self.assertRaisesRegex(ValueError, "Invalid or empty"):
                collect(root, root / "dist", "x86_64-pc-windows-msvc", ["nsis"])
            (root / "second.exe").write_bytes(b"exe")
            with self.assertRaisesRegex(ValueError, "found 2"):
                collect(root, root / "dist", "x86_64-pc-windows-msvc", ["nsis"])

    def test_invalid_target_and_formats_fail(self):
        for target, formats in [
            ("../escape", ["nsis"]), ("x86_64-pc-windows-msvc", []),
            ("x86_64-pc-windows-msvc", ["nsis", "nsis"]),
            ("x86_64-pc-windows-msvc", ["unknown"]),
        ]:
            with self.subTest(target=target, formats=formats), self.assertRaises(ValueError):
                collect(Path("unused"), Path("unused"), target, formats)

    def test_existing_download_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "yMind.exe").write_bytes(b"installer")
            destination = root / "dist"
            collect(root, destination, "x86_64-pc-windows-msvc", ["nsis"])
            with self.assertRaises(FileExistsError):
                collect(root, destination, "x86_64-pc-windows-msvc", ["nsis"])
            self.assertEqual(
                (destination / "yMind-x86_64-pc-windows-msvc.exe").read_bytes(),
                b"installer",
            )


if __name__ == "__main__":
    unittest.main()
