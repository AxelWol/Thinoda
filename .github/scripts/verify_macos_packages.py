"""Verify the downloadable macOS app archive and the app installed from its DMG."""

import argparse
from pathlib import Path
import plistlib
import subprocess
import tarfile
import tempfile


ARCHITECTURES = {
    "aarch64-apple-darwin": "arm64",
    "x86_64-apple-darwin": "x86_64",
}


def run(*args: str, input: str | None = None) -> str:
    print(f"> {' '.join(args)}", flush=True)
    result = subprocess.run(args, input=input, capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {' '.join(args)}\n"
            f"{result.stdout}{result.stderr}"
        )
    return result.stdout


def verify_app(app: Path, architecture: str) -> None:
    run("plutil", "-lint", str(app / "Contents" / "Info.plist"))
    with (app / "Contents" / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info["CFBundleIdentifier"] != "io.github.axelwol.thinoda":
        raise ValueError(f"Unexpected bundle identifier in {app}")
    executable = app / "Contents" / "MacOS" / info["CFBundleExecutable"]
    run("lipo", "-verify_arch", architecture, str(executable))
    # Verify resources and Info.plist, not just the linker's Mach-O signature.
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app))
    if not (app / "Contents" / "_CodeSignature" / "CodeResources").is_file():
        raise ValueError(f"Missing app resource seal in {app}")
    print(f"Verified {architecture} app: {app}", flush=True)


def verify(source: Path, target: str) -> None:
    architecture = ARCHITECTURES[target]
    archives = list(source.glob(f"*-{target}.app.tar.gz"))
    images = list(source.glob(f"*-{target}.dmg"))
    if len(archives) != 1 or len(images) != 1:
        raise ValueError(f"Expected one app archive and one DMG for {target} in {source}")

    with tempfile.TemporaryDirectory(prefix="thinoda-package-check-") as temporary:
        root = Path(temporary)
        extracted = root / "archive"
        with tarfile.open(archives[0], "r:gz") as archive:
            archive.extractall(extracted, filter="data")
        apps = list(extracted.glob("*.app"))
        if len(apps) != 1:
            raise ValueError(f"Expected one app in {archives[0]}")
        verify_app(apps[0], architecture)

        run("hdiutil", "verify", str(images[0]))
        run("codesign", "--verify", "--strict", "--verbose=2", str(images[0]))
        mount = root / "mount"
        mount.mkdir()
        run(
            "hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen",
            "-mountpoint", str(mount), str(images[0].resolve()), input="Y\n",
        )
        try:
            apps = list(mount.glob("*.app"))
            if len(apps) != 1:
                raise ValueError(f"Expected one app in {images[0]}")
            verify_app(apps[0], architecture)
            installed = root / "installed" / apps[0].name
            run("ditto", str(apps[0]), str(installed))
            verify_app(installed, architecture)
            # Both targets run on native runners; Intel must not require Rosetta on ARM.
            run(str(installed / "Contents" / "MacOS" / "thinoda"), "--version")
        finally:
            run("hdiutil", "detach", str(mount))
    print(f"Verified downloadable packages for {target}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("dist"))
    parser.add_argument("--target", choices=ARCHITECTURES, required=True)
    args = parser.parse_args()
    verify(args.source, args.target)
