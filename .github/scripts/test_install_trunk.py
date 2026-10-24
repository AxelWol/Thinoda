"""Offline fixtures for verified CI tool installation; no binary is executed."""

import hashlib
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import install_trunk


class Download(io.BytesIO):
    def geturl(self):
        return install_trunk.URL


def archive(name="trunk", kind=tarfile.REGTYPE):
    content = io.BytesIO()
    with tarfile.open(fileobj=content, mode="w:gz") as output:
        member = tarfile.TarInfo(name)
        member.type = kind
        member.size = 7 if kind == tarfile.REGTYPE else 0
        member.linkname = "outside"
        output.addfile(member, io.BytesIO(b"fixture"))
    return content.getvalue()


class InstallTrunkTests(unittest.TestCase):
    def install_fixture(self, content, destination):
        with patch.object(install_trunk.urllib.request, "urlopen",
                          return_value=Download(content)) as request:
            install_trunk.install(destination)
        request.assert_called_once_with(install_trunk.URL, timeout=60)

    def test_pinned_release_identity(self):
        self.assertIn("/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz", install_trunk.URL)
        self.assertEqual(install_trunk.SHA256,
                         "f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b")

    def test_verified_regular_binary_installs(self):
        content = archive()
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "bin"
            with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()):
                self.install_fixture(content, destination)
            self.assertEqual((destination / "trunk").read_bytes(), b"fixture")
            self.assertEqual([p.name for p in destination.iterdir()], ["trunk"])

    def test_checksum_failure_precedes_archive_parsing_and_preserves_existing_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            (destination / "trunk").write_bytes(b"existing")
            with patch.object(install_trunk.tarfile, "open") as extract:
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    self.install_fixture(b"untrusted", destination)
                extract.assert_not_called()
            self.assertEqual((destination / "trunk").read_bytes(), b"existing")

    def test_bad_archive_members_are_never_installed(self):
        for name, kind in [("../trunk", tarfile.REGTYPE), ("/trunk", tarfile.REGTYPE),
                           ("trunk", tarfile.SYMTYPE), ("trunk", tarfile.LNKTYPE)]:
            with self.subTest(name=name, kind=kind), tempfile.TemporaryDirectory() as temporary:
                content = archive(name, kind)
                destination = Path(temporary) / "bin"
                with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()):
                    with self.assertRaisesRegex(ValueError, "archive contents"):
                        self.install_fixture(content, destination)
                self.assertFalse(destination.exists())

    def test_oversized_download_and_binary_are_rejected(self):
        for limit in ["MAX_DOWNLOAD", "MAX_BINARY"]:
            with self.subTest(limit=limit), tempfile.TemporaryDirectory() as temporary:
                content = archive()
                destination = Path(temporary) / "bin"
                with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()), \
                        patch.object(install_trunk, limit, 1):
                    with self.assertRaises(ValueError):
                        self.install_fixture(content, destination)
                self.assertFalse(destination.exists())

    def test_download_error_does_not_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "bin"
            with patch.object(install_trunk.urllib.request, "urlopen",
                              side_effect=urllib.error.URLError("fixture network failure")):
                with self.assertRaises(urllib.error.URLError):
                    install_trunk.install(destination)
            self.assertFalse(destination.exists())

    def test_non_https_final_download_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "bin"
            with patch.object(Download, "geturl", return_value="http://example.invalid/trunk"):
                with self.assertRaisesRegex(ValueError, "outside HTTPS"):
                    self.install_fixture(archive(), destination)
            self.assertFalse(destination.exists())

    def test_matching_digest_does_not_allow_malformed_archive(self):
        content = b"not a gzip archive"
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "bin"
            with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()):
                with self.assertRaises(tarfile.ReadError):
                    self.install_fixture(content, destination)
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
