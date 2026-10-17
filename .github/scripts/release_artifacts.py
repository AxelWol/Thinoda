"""Collect every requested package with collision-free names and SHA-256 hashes."""

import argparse
import hashlib
from pathlib import Path
import shutil
import tarfile


SUFFIXES = {"nsis": ".exe", "app": ".app", "dmg": ".dmg",
            "deb": ".deb", "appimage": ".AppImage"}


def collect(source: Path, destination: Path, target: str, formats: list[str]) -> None:
    if not target or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789_-" for c in target):
        raise ValueError("Invalid target triple")
    if not formats or len(set(formats)) != len(formats):
        raise ValueError("Package formats must be nonempty and unique")
    packages = []
    for format_name in formats:
        if format_name not in SUFFIXES:
            raise ValueError(f"Unsupported package format: {format_name}")
        suffix = SUFFIXES[format_name]
        matches = sorted(source.glob(f"*{suffix}"))
        if len(matches) != 1:
            raise ValueError(f"Expected one {format_name} package in {source}, found {len(matches)}")
        package = matches[0]
        if (format_name == "app" and not package.is_dir()) or (
            format_name != "app" and (not package.is_file() or package.stat().st_size == 0)
        ):
            raise ValueError(f"Invalid or empty package: {package}")
        packages.append((package, suffix))

    destination.mkdir(parents=True, exist_ok=True)
    checksums = []
    for package, suffix in packages:
        name = f"{package.stem}-{target}{suffix}"
        if suffix == ".app":
            name += ".tar.gz"
        output = destination / name
        if output.exists():
            raise FileExistsError(output)
        if suffix == ".app":
            # tar retains executable bits and bundle symlinks, unlike artifact ZIPs.
            with tarfile.open(output, "w:gz") as archive:
                archive.add(package, arcname=package.name)
        else:
            shutil.copy2(package, output)
        with output.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        checksums.append(f"{digest}  {name}\n")
    manifest = destination / f"SHA256SUMS-{target}.txt"
    with manifest.open("x", encoding="ascii", newline="\n") as stream:
        stream.writelines(checksums)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("target") / "packages")
    parser.add_argument("--destination", type=Path, default=Path("dist"))
    parser.add_argument("--target", required=True)
    parser.add_argument("--formats", required=True)
    args = parser.parse_args()
    collect(args.source, args.destination, args.target, args.formats.split(","))
