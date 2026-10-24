"""Install the checksum-pinned CI Linux Trunk binary without elevated privileges."""

import hashlib
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import urllib.request


URL = (
    "https://github.com/trunk-rs/trunk/releases/download/v0.21.14/"
    "trunk-x86_64-unknown-linux-gnu.tar.gz"
)
SHA256 = "f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b"
MAX_DOWNLOAD = 64 * 1024 * 1024
MAX_BINARY = 64 * 1024 * 1024


def install(destination: Path) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        archive = Path(temporary) / "trunk.tar.gz"
        digest = hashlib.sha256()
        total = 0
        with urllib.request.urlopen(URL, timeout=60) as response, archive.open("wb") as output:
            if not response.geturl().startswith("https://"):
                raise ValueError("Trunk download redirected outside HTTPS")
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                if total > MAX_DOWNLOAD:
                    raise ValueError("Trunk download exceeds size limit")
                digest.update(chunk)
                output.write(chunk)
        if digest.hexdigest() != SHA256:
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
