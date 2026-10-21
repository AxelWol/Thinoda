"""Build Developer ID-signed, notarized and stapled macOS release packages."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib
import time
from urllib.parse import urlsplit

from verify_macos_packages import ARCHITECTURES, run


ROOT = Path(__file__).resolve().parents[2]
TICKET_RETRY_DELAYS = (15, 30, 60, 120)


def is_apple_cloudkit_url(value: str) -> bool:
    try:
        return urlsplit(value).hostname == "api.apple-cloudkit.com"
    except ValueError:
        return False


def run_with_ticket_retry(
    command: list[str], *, cwd: Path | None = None, timeout: int | None = None,
) -> None:
    for attempt, delay in enumerate((*TICKET_RETRY_DELAYS, None), start=1):
        result = subprocess.run(
            command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, check=False, timeout=timeout,
        )
        if result.stdout:
            print(
                result.stdout,
                end="" if result.stdout.endswith("\n") else "\n",
                flush=True,
            )
        if result.returncode == 0:
            return
        cloudkit_url = any(
            is_apple_cloudkit_url(url)
            for url in re.findall(r"https?://[^\s\"<>]+", result.stdout)
        )
        transient_stapling_failure = (
            cloudkit_url
            and ("Code=-1005" in result.stdout or "network connection was lost" in result.stdout)
        )
        if not transient_stapling_failure or delay is None:
            raise subprocess.CalledProcessError(result.returncode, command, output=result.stdout)
        print(
            f"Transient Apple ticket lookup failure; retrying {command[0]} "
            f"({attempt}/{len(TICKET_RETRY_DELAYS) + 1}) in {delay}s.",
            flush=True,
        )
        time.sleep(delay)


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
    # runtime + timestamps, then notarizes/staples the app BEFORE creating the DMG.
    run_with_ticket_retry([
        "cargo-packager", "--config", json.dumps(config),
        "--target", target, "--formats", "app,dmg",
    ], cwd=root / "crates" / "thinoda-app")
    output = root / "target" / "packages"
    apps = list(output.glob("*.app"))
    images = list(output.glob("*.dmg"))
    if len(apps) != 1 or len(images) != 1:
        raise ValueError(f"Expected one app and one DMG in {output}")
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(apps[0]))
    run("xcrun", "stapler", "validate", str(apps[0]))
    run("codesign", "--verify", "--strict", "--verbose=2", str(images[0]))
    # Packager signs the DMG but only notarizes/staples the app.
    notarize_dmg(images[0])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=ARCHITECTURES, required=True)
    args = parser.parse_args()
    package(ROOT, args.target)
