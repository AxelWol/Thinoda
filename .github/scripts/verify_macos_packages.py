"""Verify the downloadable macOS app archive and the app installed from its DMG."""

import argparse
from contextlib import contextmanager
from pathlib import Path
import plistlib
import re
import subprocess
import tarfile
import tempfile
import time
from urllib.parse import urlsplit


ARCHITECTURES = {
    "aarch64-apple-darwin": "arm64",
    "x86_64-apple-darwin": "x86_64",
}

LOCALIZATIONS = ["en", "de", "es", "fr", "da", "nl", "pt-PT", "pt-BR", "it", "pl"]
TICKET_RETRY_DELAYS = (15, 30, 60, 120)
DETACH_RETRY_DELAYS = (2, 4, 8, 16)


class CommandError(RuntimeError):
    def __init__(self, args: tuple[str, ...], result: subprocess.CompletedProcess):
        self.returncode = result.returncode
        super().__init__(
            f"Command failed ({result.returncode}): {' '.join(args)}\n"
            f"{result.stdout}{result.stderr}"
        )


def run(*args: str, input: str | None = None, timeout: int = 120) -> str:
    print(f"> {' '.join(args)}", flush=True)
    result = subprocess.run(args, input=input, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise CommandError(args, result)
    return result.stdout


def is_apple_cloudkit_url(value: str) -> bool:
    try:
        return urlsplit(value).hostname == "api.apple-cloudkit.com"
    except ValueError:
        return False


def run_with_ticket_retry(
    command: list[str], *, cwd: Path | None = None, timeout: int | None = None,
) -> str:
    for attempt, delay in enumerate((*TICKET_RETRY_DELAYS, None), start=1):
        result = subprocess.run(
            command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, check=False, timeout=timeout,
        )
        if result.stdout:
            print(result.stdout, end="" if result.stdout.endswith("\n") else "\n", flush=True)
        if result.returncode == 0:
            return result.stdout
        cloudkit_url = any(
            is_apple_cloudkit_url(url)
            for url in re.findall(r"https?://[^\s\"<>]+", result.stdout)
        )
        transient = cloudkit_url and (
            "Code=-1005" in result.stdout or "network connection was lost" in result.stdout
        )
        if not transient or delay is None:
            raise subprocess.CalledProcessError(result.returncode, command, output=result.stdout)
        print(
            f"Transient Apple ticket lookup failure; retrying {command[0]} "
            f"({attempt}/{len(TICKET_RETRY_DELAYS) + 1}) in {delay}s.",
            flush=True,
        )
        time.sleep(delay)


def attached_image(image: Path) -> dict | None:
    info = plistlib.loads(run("hdiutil", "info", "-plist").encode("utf-8"))
    matches = [
        entry for entry in info["images"]
        if Path(entry["image-path"]).resolve() == image.resolve()
    ]
    if len(matches) > 1:
        raise RuntimeError(f"Multiple attachments for {image}; refusing ambiguous cleanup")
    return matches[0] if matches else None


def detach_readonly_image(device: str, mount: Path) -> None:
    for delay in (*DETACH_RETRY_DELAYS, None):
        try:
            run("hdiutil", "detach", device)
            return
        except CommandError as error:
            print(error, flush=True)
            if error.returncode != 16:
                raise
            # lsof returns 1 when no handles match; it is diagnostic, not a gate.
            try:
                result = subprocess.run(
                    ["lsof", "+D", str(mount)], capture_output=True, text=True, timeout=30,
                )
                print(f"Open handles under {mount} (lsof {result.returncode}):\n"
                      f"{result.stdout}{result.stderr}", flush=True)
            except subprocess.TimeoutExpired as diagnostic:
                print(f"Open-handle diagnostic timed out: {diagnostic}", flush=True)
            if delay is None:
                # Only used for our explicitly read-only verification attachment.
                # No writable filesystem or unrelated disk is ever force-detached.
                run("hdiutil", "detach", "-force", device)
                return
            print(f"Read-only image busy; retrying {device} in {delay}s.", flush=True)
            time.sleep(delay)


@contextmanager
def readonly_image(image: Path, mount: Path):
    image = image.resolve()
    if attached_image(image) is not None:
        raise RuntimeError(f"Image already attached; refusing to take ownership: {image}")
    mount.mkdir()
    try:
        run(
            "hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen",
            "-mountpoint", str(mount), str(image), input="Y\n",
        )
        yield mount
    finally:
        # Also find devices left by a partially failed or timed-out attach.
        attachment = attached_image(image)
        if attachment is not None:
            entities = attachment["system-entities"]
            if any(
                Path(entity["mount-point"]).resolve() != mount.resolve()
                for entity in entities if entity.get("mount-point")
            ):
                raise RuntimeError(f"Unexpected mount for {image}; refusing unsafe cleanup")
            devices = [
                entity["dev-entry"] for entity in entities
                if re.fullmatch(r"/dev/disk\d+", entity.get("dev-entry", ""))
            ]
            if len(devices) != 1:
                raise RuntimeError(f"Cannot identify whole image device for {image}: {entities}")
            detach_readonly_image(devices[0], mount)
            if attached_image(image) is not None:
                raise RuntimeError(f"Image remains attached after detach: {image}")


def verify_app(app: Path, architecture: str, require_notarization: bool = False) -> None:
    run("plutil", "-lint", str(app / "Contents" / "Info.plist"))
    with (app / "Contents" / "Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info["CFBundleIdentifier"] != "io.github.axelwol.thinoda":
        raise ValueError(f"Unexpected bundle identifier in {app}")
    if (
        info.get("CFBundleLocalizations") != LOCALIZATIONS
        or info.get("CFBundleDevelopmentRegion") != "en"
    ):
        raise ValueError(f"Missing or incorrect bundle localizations in {app}")
    executable = app / "Contents" / "MacOS" / info["CFBundleExecutable"]
    run("lipo", str(executable), "-verify_arch", architecture)
    # Verify resources and Info.plist, not just the linker's Mach-O signature.
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app))
    if not (app / "Contents" / "_CodeSignature" / "CodeResources").is_file():
        raise ValueError(f"Missing app resource seal in {app}")
    if require_notarization:
        run_with_ticket_retry(["xcrun", "stapler", "validate", str(app)], timeout=120)
        run_with_ticket_retry(
            ["spctl", "--assess", "--type", "execute", "--verbose=2", str(app)], timeout=120,
        )
    print(f"Verified {architecture} app: {app}", flush=True)


def verify(source: Path, target: str, require_notarization: bool = False) -> None:
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
        verify_app(apps[0], architecture, require_notarization)

        run("hdiutil", "verify", str(images[0]))
        run("codesign", "--verify", "--strict", "--verbose=2", str(images[0]))
        if require_notarization:
            run_with_ticket_retry(["xcrun", "stapler", "validate", str(images[0])], timeout=120)
            run_with_ticket_retry([
                "spctl", "--assess", "--type", "open", "--context",
                "context:primary-signature", "--verbose=2", str(images[0]),
            ], timeout=120)
        mount = root / "mount"
        with readonly_image(images[0], mount):
            apps = list(mount.glob("*.app"))
            if len(apps) != 1:
                raise ValueError(f"Expected one app in {images[0]}")
            verify_app(apps[0], architecture, require_notarization)
            installed = root / "installed" / apps[0].name
            run("ditto", str(apps[0]), str(installed))
            verify_app(installed, architecture, require_notarization)
            # Both targets run on native runners; Intel must not require Rosetta on ARM.
            run(str(installed / "Contents" / "MacOS" / "thinoda"), "--version")
    print(f"Verified downloadable packages for {target}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("dist"))
    parser.add_argument("--target", choices=ARCHITECTURES, required=True)
    parser.add_argument("--require-notarization", action="store_true")
    args = parser.parse_args()
    verify(args.source, args.target, args.require_notarization)
