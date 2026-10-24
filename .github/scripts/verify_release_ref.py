"""Resolve release refs to commits already reachable from the fetched main branch."""

import argparse
import os
from pathlib import Path
import re
import subprocess


def git(*args: str, cwd: Path | None = None) -> str:
    return subprocess.check_output(
        ["git", *args], cwd=cwd, text=True, stderr=subprocess.PIPE
    ).strip()


def verified_commit(ref: str, cwd: Path | None = None) -> str:
    if not (ref.startswith("refs/tags/v") or ref == "refs/heads/main"
            or re.fullmatch(r"[0-9a-fA-F]{40}", ref)):
        raise ValueError("Release ref must be a v* tag, main, or a full commit SHA")
    object_id = ref if re.fullmatch(r"[0-9a-fA-F]{40}", ref) else git(
        "show-ref", "--verify", "--hash", ref, cwd=cwd
    )
    commit = git("rev-parse", "--verify", f"{object_id}^{{commit}}", cwd=cwd)
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, "refs/remotes/origin/main"],
        cwd=cwd, capture_output=True, text=True,
    )
    if result.returncode == 1:
        raise ValueError("Release commit is not reachable from origin/main")
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "Could not verify main ancestry")
    return commit


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", required=True)
    args = parser.parse_args()
    commit = verified_commit(args.ref)
    print(f"Verified release commit: {commit}")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"commit={commit}\n")


if __name__ == "__main__":
    main()
