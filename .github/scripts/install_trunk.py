"""Install the checksum-pinned CI Linux Trunk binary without elevated privileges."""

import hashlib
import http.client
from pathlib import Path
import shutil
import socket
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request


URL = (
    "https://github.com/trunk-rs/trunk/releases/download/v0.21.14/"
    "trunk-x86_64-unknown-linux-gnu.tar.gz"
)
SHA256 = "f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b"
MAX_DOWNLOAD = 64 * 1024 * 1024
MAX_BINARY = 64 * 1024 * 1024
DOWNLOAD_RETRY_DELAYS = (15, 30, 60, 120)
RETRYABLE_HTTP_STATUSES = {408, 429, 500, 502, 503, 504}


def is_transient_download_error(error: Exception) -> bool:
    if isinstance(error, urllib.error.HTTPError):
        return error.code in RETRYABLE_HTTP_STATUSES
    reason = error.reason if isinstance(error, urllib.error.URLError) else error
    return (
        isinstance(reason, (TimeoutError, ConnectionError, http.client.IncompleteRead))
        or (isinstance(reason, socket.gaierror) and reason.errno == socket.EAI_AGAIN)
    )


def download_archive(archive: Path) -> str:
    for attempt, delay in enumerate((*DOWNLOAD_RETRY_DELAYS, None), start=1):
        digest = hashlib.sha256()
        total = 0
        try:
            with urllib.request.urlopen(URL, timeout=60) as response, archive.open("wb") as output:
                if not response.geturl().startswith("https://"):
                    raise ValueError("Trunk download redirected outside HTTPS")
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    if total > MAX_DOWNLOAD:
                        raise ValueError("Trunk download exceeds size limit")
                    digest.update(chunk)
                    output.write(chunk)
            return digest.hexdigest()
        except (urllib.error.URLError, TimeoutError, ConnectionError,
                socket.gaierror, http.client.IncompleteRead) as error:
            if delay is None or not is_transient_download_error(error):
                raise
            print(
                f"Transient Trunk download failure ({error}); retrying in {delay}s "
                f"(attempt {attempt + 1}/{len(DOWNLOAD_RETRY_DELAYS) + 1}).",
                file=sys.stderr,
                flush=True,
            )
            time.sleep(delay)
    raise AssertionError("Trunk download retry loop ended unexpectedly")


def install(destination: Path) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        archive = Path(temporary) / "trunk.tar.gz"
        if download_archive(archive) != SHA256:
            raise ValueError("Trunk checksum mismatch; refusing to extract or install")
        binary = Path(temporary) / "trunk"
        with tarfile.open(archive, "r:gz") as source:
            members = source.getmembers()
            if (len(members) != 1 or members[0].name != "trunk"
                    or not members[0].isfile() or members[0].size > MAX_BINARY):
                raise ValueError("Unexpected Trunk archive contents")
            with source.extractfile(members[0]) as input_file, binary.open("wb") as output:
                shutil.copyfileobj(input_file, output)
        binary.chmod(0o755)
        destination.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary, destination / "trunk")
        (destination / "trunk").chmod(0o755)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("Usage: install_trunk.py DESTINATION")
    install(Path(sys.argv[1]))
