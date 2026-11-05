"""Offline fixtures for verified CI tool installation; no binary is executed."""

import hashlib
import http.client
import io
from pathlib import Path
import socket
import ssl
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import install_trunk


class Download(io.BytesIO):
    def geturl(self):
        return install_trunk.URL


class InterruptedDownload(Download):
    def __init__(self, content, error):
        super().__init__(content)
        self.error = error

    def read(self, size=-1):
        if self.tell() == 0:
            return super().read(8)
        raise self.error


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

    def test_transient_http_error_is_retried_with_checksum_verification(self):
        content = archive()
        error = urllib.error.HTTPError(install_trunk.URL, 504, "Gateway Timeout", {}, None)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "bin"
            with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()), \
                    patch.object(install_trunk.urllib.request, "urlopen",
                                 side_effect=[error, Download(content)]) as request, \
                    patch.object(install_trunk.time, "sleep") as sleep, \
                    patch("sys.stderr", new_callable=io.StringIO) as stderr:
                install_trunk.install(destination)
            self.assertEqual((destination / "trunk").read_bytes(), b"fixture")
            self.assertEqual(request.call_count, 2)
            sleep.assert_called_once_with(install_trunk.DOWNLOAD_RETRY_DELAYS[0])
            self.assertIn("504", stderr.getvalue())

    def test_interrupted_download_restarts_with_a_fresh_file_and_digest(self):
        content = archive()
        for error in [TimeoutError("fixture timeout"), ConnectionResetError("fixture reset"),
                      http.client.IncompleteRead(b"partial", 10)]:
            with self.subTest(error=error), tempfile.TemporaryDirectory() as temporary:
                destination = Path(temporary) / "bin"
                with patch.object(install_trunk, "SHA256", hashlib.sha256(content).hexdigest()), \
                        patch.object(install_trunk.urllib.request, "urlopen",
                                     side_effect=[InterruptedDownload(b"old-data", error),
                                                  Download(content)]) as request, \
                        patch.object(install_trunk.time, "sleep"):
                    install_trunk.install(destination)
                self.assertEqual(request.call_count, 2)
                self.assertEqual((destination / "trunk").read_bytes(), b"fixture")

    def test_checksum_failure_after_retry_does_not_replace_existing_binary(self):
        error = urllib.error.HTTPError(install_trunk.URL, 504, "Gateway Timeout", {}, None)
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            (destination / "trunk").write_bytes(b"existing")
            with patch.object(install_trunk.urllib.request, "urlopen",
                              side_effect=[error, Download(b"untrusted")]) as request, \
                    patch.object(install_trunk.time, "sleep") as sleep:
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    install_trunk.install(destination)
            self.assertEqual(request.call_count, 2)
            sleep.assert_called_once()
            self.assertEqual((destination / "trunk").read_bytes(), b"existing")

    def test_transient_errors_stop_after_bounded_retries(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            (destination / "trunk").write_bytes(b"existing")
            error = urllib.error.HTTPError(install_trunk.URL, 503, "Unavailable", {}, None)
            with patch.object(install_trunk.urllib.request, "urlopen", side_effect=error) as request, \
                    patch.object(install_trunk.time, "sleep") as sleep:
                with self.assertRaises(urllib.error.HTTPError) as raised:
                    install_trunk.install(destination)
            self.assertIs(raised.exception, error)
            self.assertEqual(request.call_count, len(install_trunk.DOWNLOAD_RETRY_DELAYS) + 1)
            self.assertEqual([call.args[0] for call in sleep.call_args_list],
                             list(install_trunk.DOWNLOAD_RETRY_DELAYS))
            self.assertEqual((destination / "trunk").read_bytes(), b"existing")

    def test_permanent_http_and_certificate_errors_are_not_retried(self):
        errors = [
            urllib.error.HTTPError(install_trunk.URL, 403, "Forbidden", {}, None),
            urllib.error.HTTPError(install_trunk.URL, 404, "Not Found", {}, None),
            urllib.error.URLError(ssl.SSLError("certificate verification failed")),
        ]
        for error in errors:
            with self.subTest(error=error), tempfile.TemporaryDirectory() as temporary:
                destination = Path(temporary) / "bin"
                with patch.object(install_trunk.urllib.request, "urlopen", side_effect=error) as request, \
                        patch.object(install_trunk.time, "sleep") as sleep:
                    with self.assertRaises(type(error)):
                        install_trunk.install(destination)
                request.assert_called_once()
                sleep.assert_not_called()
                self.assertFalse(destination.exists())

    def test_transient_wrapped_transport_errors_are_recognized(self):
        errors = [
            urllib.error.URLError(TimeoutError("fixture timeout")),
            urllib.error.URLError(ConnectionResetError("fixture reset")),
            urllib.error.URLError(socket.gaierror(socket.EAI_AGAIN, "temporary DNS failure")),
        ]
        for error in errors:
            with self.subTest(error=error):
                self.assertTrue(install_trunk.is_transient_download_error(error))
        self.assertFalse(install_trunk.is_transient_download_error(
            urllib.error.URLError(socket.gaierror(socket.EAI_NONAME, "unknown host"))
        ))

    def test_transient_http_statuses_are_recognized(self):
        for status in (408, 429, 500, 502, 503, 504):
            with self.subTest(status=status):
                error = urllib.error.HTTPError(install_trunk.URL, status, "fixture", {}, None)
                self.assertTrue(install_trunk.is_transient_download_error(error))
        error = urllib.error.HTTPError(install_trunk.URL, 501, "fixture", {}, None)
        self.assertFalse(install_trunk.is_transient_download_error(error))

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
