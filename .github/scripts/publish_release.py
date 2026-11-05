"""Verify and publish native release assets without rewriting existing release notes."""

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


TARGET_FORMATS = {
    "x86_64-pc-windows-msvc": (".exe",),
    "aarch64-pc-windows-msvc": (".exe",),
    "x86_64-apple-darwin": (".app.tar.gz", ".dmg"),
    "aarch64-apple-darwin": (".app.tar.gz", ".dmg"),
    "x86_64-unknown-linux-gnu": (".deb", ".AppImage"),
    "aarch64-unknown-linux-gnu": (".deb", ".AppImage"),
}


@dataclass(frozen=True)
class Asset:
    path: Path
    digest: str
    size: int


def describe_asset(path: Path) -> Asset:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"Missing, empty or unsafe release asset: {path}")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return Asset(path, f"sha256:{digest}", path.stat().st_size)


def verify_assets(directory: Path) -> list[Asset]:
    assets = []
    names = set()
    for target, suffixes in TARGET_FORMATS.items():
        manifest = directory / f"SHA256SUMS-{target}.txt"
        manifest_asset = describe_asset(manifest)
        lines = manifest.read_text(encoding="ascii").splitlines()
        if len(lines) != len(suffixes):
            raise ValueError(f"Incorrect number of checksums in {manifest}")
        formats = set()
        for line in lines:
            match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.-]+)", line)
            if not match:
                raise ValueError(f"Invalid checksum entry in {manifest}: {line}")
            digest, name = match.groups()
            suffix = next((s for s in suffixes if name.endswith(f"-{target}{s}")), None)
            if suffix is None or suffix in formats or name in names:
                raise ValueError(f"Unexpected or duplicate package in {manifest}: {name}")
            asset = describe_asset(directory / name)
            if asset.digest != f"sha256:{digest}":
                raise ValueError(f"Checksum mismatch: {asset.path}")
            formats.add(suffix)
            names.add(name)
            assets.append(asset)
        names.add(manifest.name)
        assets.append(manifest_asset)
    if {entry.name for entry in directory.iterdir()} != names:
        raise ValueError(f"Unexpected or missing files in release assets directory: {directory}")
    return sorted(assets, key=lambda asset: asset.path.name)


def gh(*args: str, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["gh", *args], text=True, check=check,
        stdout=subprocess.PIPE if capture else None,
    )


def pages(endpoint: str) -> list[dict]:
    response = gh("api", "--paginate", "--slurp", endpoint, capture=True)
    batches = json.loads(response.stdout)
    if not isinstance(batches, list) or not all(isinstance(batch, list) for batch in batches):
        raise ValueError(f"Unexpected GitHub API response for {endpoint}")
    return [item for batch in batches for item in batch]


def find_release(repository: str, tag: str) -> dict | None:
    matches = [
        release for release in pages(f"repos/{repository}/releases?per_page=100")
        if release["tag_name"] == tag
    ]
    if len(matches) > 1:
        raise ValueError(f"Multiple releases found for {tag}; refusing to choose one")
    return matches[0] if matches else None


def remote_assets(repository: str, release_id: int) -> dict[str, dict]:
    entries = pages(f"repos/{repository}/releases/{release_id}/assets?per_page=100")
    result = {}
    for entry in entries:
        name = entry["name"]
        if name in result:
            raise ValueError(f"Duplicate release asset: {name}")
        result[name] = entry
    return result


def matches(asset: Asset, remote: dict | None) -> bool:
    return bool(remote and remote.get("state") == "uploaded"
                and remote.get("digest") == asset.digest
                and remote.get("size") == asset.size)


def publish(tag: str, repository: str, directory: Path, replace_existing: bool = False) -> None:
    if not tag.startswith("v"):
        raise ValueError("Release tag must begin with v")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Repository must have owner/name format")
    assets = verify_assets(directory)
    release = find_release(repository, tag)
    if release is None:
        gh("release", "create", tag, "--repo", repository, "--verify-tag",
           "--draft", "--generate-notes")
        release = find_release(repository, tag)
        if release is None:
            raise RuntimeError(f"Created release {tag} could not be found")

    release_id = release["id"]
    existing = remote_assets(repository, release_id)
    conflicts = [
        asset.path.name for asset in assets
        if asset.path.name in existing and not matches(asset, existing[asset.path.name])
    ]
    if conflicts and not replace_existing:
        raise ValueError(
            "Existing release assets differ from the verified packages: "
            + ", ".join(conflicts)
            + ". To replace them intentionally, rerun with replace_assets enabled."
        )

    for asset in assets:
        current = existing.get(asset.path.name)
        if matches(asset, current):
            print(f"Already verified: {asset.path.name}", flush=True)
            continue
        args = ["release", "upload", tag, "--repo", repository, str(asset.path)]
        if current is not None:
            args.append("--clobber")
        result = gh(*args, check=False)
        if result.returncode:
            # An interrupted upload may have succeeded on GitHub despite a client error.
            uploaded = remote_assets(repository, release_id).get(asset.path.name)
            if not matches(asset, uploaded):
                raise RuntimeError(f"Upload failed for {asset.path.name} (exit {result.returncode})")
            print(f"Upload reported an error, but GitHub verified {asset.path.name}", flush=True)

    final = remote_assets(repository, release_id)
    missing = [asset.path.name for asset in assets if not matches(asset, final.get(asset.path.name))]
    if missing:
        raise RuntimeError("Release asset verification failed: " + ", ".join(missing))

    if release["draft"]:
        args = ["release", "edit", tag, "--repo", repository, "--draft=false"]
        if not release["prerelease"]:
            args.append("--latest")
        gh(*args)
        published = find_release(repository, tag)
        if published is None or published["id"] != release_id or published["draft"]:
            raise RuntimeError(f"Release {tag} was not published as expected")
    print(f"Verified {len(assets)} release assets for {tag}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    parser.add_argument("--assets-dir", type=Path, default=Path("dist"))
    parser.add_argument("--replace-existing", action="store_true")
    args = parser.parse_args()
    if not args.repository:
        parser.error("--repository or GITHUB_REPOSITORY is required")
    publish(args.tag, args.repository, args.assets_dir, args.replace_existing)


if __name__ == "__main__":
    main()
