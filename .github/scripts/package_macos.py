"""Build Developer ID-signed, notarized and stapled macOS release packages."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import tempfile
import tomllib

from verify_macos_packages import ARCHITECTURES, run, run_with_ticket_retry


ROOT = Path(__file__).resolve().parents[2]


def license_resources(license: Path) -> dict:
    def resource(data: bytes) -> list[dict]:
        return [{"Attributes": "0x0000", "ID": "5000", "Name": "English", "Data": data}]

    strings = [
        "English", "Agree", "Disagree", "Print", "Save...",
        "Choose Agree to access this volume under the displayed license, or Disagree to cancel.",
    ]
    encoded = [text.encode("mac_roman") for text in strings]
    buttons = struct.pack(">H", len(encoded)) + b"".join(
        bytes([len(text)]) + text for text in encoded
    )
    return {
        # LPic: default language, count, (system language, resource offset, two-byte flag).
        "LPic": resource(struct.pack(">5H", 0, 1, 0, 0, 0)),
        "STR#": resource(buttons),
        "TEXT": resource(license.read_bytes()),
    }


def create_dmg(app: Path, image: Path, license: Path) -> None:
    """Create from closed staging files, without a writable image/Finder session."""
    with tempfile.TemporaryDirectory(prefix="thinoda-dmg-stage-") as temporary:
        root = Path(temporary)
        staging = root / "volume"
        staging.mkdir()
        run("ditto", str(app), str(staging / app.name))
        # No file handles or subprocesses remain in the staging tree at image creation.
        (staging / "Applications").symlink_to("/Applications", target_is_directory=True)
        shutil.copy2(license, staging / "LICENSE.md")
        with (app / "Contents" / "Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        icon = app / "Contents" / "Resources" / info["CFBundleIconFile"]
        shutil.copy2(icon, staging / ".VolumeIcon.icns")
        run("SetFile", "-a", "C", str(staging))
        resources = root / "license.plist"
        resources.write_bytes(plistlib.dumps(license_resources(license)))
        run(
            "hdiutil", "create", "-ov", "-volname", app.stem, "-fs", "HFS+",
            "-format", "UDZO", "-srcfolder", str(staging), str(image), timeout=600,
        )
        run("hdiutil", "udifrez", "-xml", str(resources), "", "-quiet", str(image))


def package_config(root: Path, target: str, identity: str) -> dict:
    if target not in ARCHITECTURES:
        raise ValueError(f"Unsupported macOS target: {target}")
    if not identity.startswith("Developer ID Application: "):
        raise ValueError("MACOS_SIGN_IDENTITY must be a Developer ID Application identity")
    manifest = root / "crates" / "thinoda-app" / "Cargo.toml"
    app = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    config = app["metadata"]["packager"]
    config["version"] = workspace["package"]["version"]
    config["authors"] = workspace["package"]["authors"]
    config["out-dir"] = str(root / "target" / "packages")
    config["binaries-dir"] = str(root / "target" / target / "release")
    config["macos"]["signing-identity"] = identity
    return config


def notarize_dmg(image: Path) -> None:
    credentials = (
        "--key", os.environ["APPLE_API_KEY_PATH"],
        "--key-id", os.environ["APPLE_API_KEY"],
        "--issuer", os.environ["APPLE_API_ISSUER"],
    )
    response = json.loads(run(
        "xcrun", "notarytool", "submit", str(image), *credentials,
        "--wait", "--timeout", "30m", "--output-format", "json",
        timeout=2100,
    ))
    if response["status"] != "Accepted":
        log = run("xcrun", "notarytool", "log", response["id"], *credentials)
        raise RuntimeError(f"DMG notarization was not accepted: {response}\n{log}")
    run_with_ticket_retry(
        ["xcrun", "stapler", "staple", str(image)], timeout=120,
    )
    run("xcrun", "stapler", "validate", str(image))


def package(root: Path, target: str) -> None:
    required = (
        "MACOS_SIGN_IDENTITY", "APPLE_API_KEY", "APPLE_API_ISSUER", "APPLE_API_KEY_PATH",
    )
    for name in required:
        if not os.environ.get(name):
            raise ValueError(f"Missing required signing environment variable: {name}")
    if not Path(os.environ["APPLE_API_KEY_PATH"]).is_file():
        raise ValueError("APPLE_API_KEY_PATH must point to the decoded App Store Connect key")
    config = package_config(root, target, os.environ["MACOS_SIGN_IDENTITY"])
    subprocess.run([
        "cargo", "build", "--locked", "--release", "-p", "thinoda-app",
        "--bin", "thinoda", "--target", target,
    ], cwd=root, check=True)
    # Packager 0.11.8 signs Mach-O files and the bundle inside-out with hardened
    # runtime + timestamps, then notarizes/staples the app independently of DMG.
    run_with_ticket_retry([
        "cargo-packager", "--config", json.dumps(config),
        "--target", target, "--formats", "app",
    ], cwd=root / "crates" / "thinoda-app")
    output = root / "target" / "packages"
    apps = list(output.glob("*.app"))
    if len(apps) != 1:
        raise ValueError(f"Expected one app in {output}")
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(apps[0]))
    run_with_ticket_retry(["xcrun", "stapler", "validate", str(apps[0])], timeout=120)
    suffix = "x64" if ARCHITECTURES[target] == "x86_64" else "arm64"
    image = output / f"{config['product-name']}_{config['version']}_{suffix}.dmg"
    license = root / "crates" / "thinoda-app" / config["license-file"]
    create_dmg(apps[0], image, license)
    run("codesign", "--force", "--timestamp", "--sign", os.environ["MACOS_SIGN_IDENTITY"], str(image))
    run("codesign", "--verify", "--strict", "--verbose=2", str(image))
    notarize_dmg(image)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=ARCHITECTURES, required=True)
    args = parser.parse_args()
    package(ROOT, args.target)
