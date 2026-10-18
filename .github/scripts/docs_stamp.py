"""Stamp Git-tracked Markdown from committed source history, never the wall clock.

Main-branch CI refreshes metadata automatically after source commits land.
Usage: python .github/scripts/docs_stamp.py [--check | --refresh-main]
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
STAMP = re.compile(
    r"_Last updated: [^\r\n]+ \(non-binding repository metadata\)\._"
    r" <!-- docs-stamp -->\r?\n(?:\r?\n)?"
)


def header_offset(text: str) -> int:
    """Keep a BOM and YAML frontmatter ahead of rendered Markdown."""
    offset = 1 if text.startswith("\ufeff") else 0
    lines = text[offset:].splitlines(keepends=True)
    if lines and lines[0].rstrip("\r\n") == "---":
        offset += len(lines[0])
        for line in lines[1:]:
            offset += len(line)
            if line.rstrip("\r\n") in ("---", "..."):
                if not line.endswith("\n"):
                    raise ValueError("YAML frontmatter must end with a newline")
                return offset
        raise ValueError("Unclosed YAML frontmatter")
    return offset


def strip_stamp(text: str) -> str:
    offset = header_offset(text)
    match = STAMP.match(text, offset)
    return text[:offset] + text[match.end():] if match else text


def with_stamp(text: str, timestamp: str) -> str:
    text = strip_stamp(text)
    offset = header_offset(text)
    newline = "\r\n" if "\r\n" in text else "\n"
    stamp = (
        f"_Last updated: {timestamp} (non-binding repository metadata)._"
        f" <!-- docs-stamp -->{newline}{newline}"
    )
    return text[:offset] + stamp + text[offset:]


def preserve_stamp(current: str, generated: str) -> str:
    """A generator owns the body; docs-stamp owns only the metadata."""
    offset = header_offset(current)
    match = STAMP.match(current, offset)
    if not match:
        return generated
    target = header_offset(generated)
    return generated[:target] + match[0] + generated[target:]


def git(root: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", *args], cwd=root, check=True, capture_output=True
    ).stdout


def source_body(text: str) -> str:
    return strip_stamp(text).replace("\r\n", "\n")


def historical_text(root: Path, commit: str, path: str) -> str | None:
    entry = git(root, "ls-tree", "--format=%(objectname)", commit, "--", f":(literal){path}")
    if not entry.strip():
        return None
    return git(root, "cat-file", "blob", entry.strip().decode("ascii")).decode("utf-8")


def source_timestamp(root: Path, path: str) -> tuple[str, str]:
    """Return the newest source-changing commit's date and the HEAD body.

    Git's default history simplification skips merges that merely integrate a
    file unchanged. A rename/re-add starts a new path history. Consecutive equal
    bodies (after removing our metadata) are stamp-only commits, not changes.
    """
    history = git(root, "log", "--format=%H %cI", "--", f":(literal){path}").decode("ascii").splitlines()
    if not history:
        raise ValueError(f"{path}: commit this new file before stamping")
    newest = historical_text(root, "HEAD", path)
    if newest is None:
        raise ValueError(f"{path}: commit this new file before stamping")
    body = source_body(newest)
    timestamp = None
    for entry in history:
        commit, date = entry.split(" ", 1)
        previous = historical_text(root, commit, path)
        if previous is None or source_body(previous) != body:
            break
        timestamp = date
    if timestamp is None:
        raise ValueError(f"{path}: could not identify its source commit")
    date = datetime.fromisoformat(timestamp).astimezone(timezone.utc)
    return date.strftime("%Y-%m-%d %H:%M:%S UTC"), body


def stamp_repository(root: Path, check: bool = False) -> list[str]:
    if git(root, "rev-parse", "--is-shallow-repository").strip() == b"true":
        raise ValueError("Full Git history is required; run git fetch --unshallow")
    paths = git(root, "ls-files", "-z", "--", "*.md").decode("utf-8").split("\0")
    changes = []
    for name in filter(None, paths):
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"{name}: expected a regular tracked Markdown file")
        current = path.read_bytes().decode("utf-8")
        timestamp, committed = source_timestamp(root, name)
        if source_body(current) != committed:
            raise ValueError(f"{name}: commit source edits before stamping")
        expected = with_stamp(current, timestamp)
        if current != expected:
            changes.append((name, expected))
    # Validate every source before writing any file.
    if not check:
        for name, expected in changes:
            (root / name).write_bytes(expected.encode("utf-8"))
    return [name for name, _ in changes]


def refresh_main(root: Path) -> list[str]:
    """Fast-forward metadata onto the latest main, retrying concurrent updates."""
    if git(root, "status", "--porcelain").strip():
        raise ValueError("Automatic refresh requires a clean checkout")
    for _ in range(3):
        git(root, "fetch", "origin", "main")
        git(root, "checkout", "--detach", "FETCH_HEAD")
        changed = stamp_repository(root)
        if not changed:
            return []
        git(root, "add", "--", *(f":(literal){name}" for name in changed))
        staged = git(root, "diff", "--cached", "--name-only", "-z").decode("utf-8").split("\0")
        if set(filter(None, staged)) != set(changed):
            raise ValueError("Refusing to commit unexpected staged files")
        for name in changed:
            indexed = git(root, "cat-file", "blob", f":{name}").decode("utf-8")
            previous = historical_text(root, "HEAD", name)
            if previous is None or source_body(indexed) != source_body(previous):
                raise ValueError(f"{name}: refusing to commit nonmetadata source edits")
        git(
            root,
            "-c", "user.name=github-actions[bot]",
            "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit", "-m",
            "chore(docs): refresh source timestamps\n\n"
            "Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>",
        )
        if stamp_repository(root, check=True):
            raise ValueError("Metadata commit changed source timestamps unexpectedly")
        try:
            git(root, "push", "--porcelain", "origin", "HEAD:refs/heads/main")
        except subprocess.CalledProcessError as error:
            if b"[rejected] (fetch first)" in error.stdout or b"[rejected] (non-fast-forward)" in error.stdout:
                continue
            raise
        return changed
    raise ValueError("Main advanced during all three refresh attempts; rerun the workflow")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="fail on stale metadata")
    mode.add_argument(
        "--refresh-main", action="store_true",
        help="CI only: fetch main, stamp, commit metadata and fast-forward push",
    )
    args = parser.parse_args()
    try:
        changed = refresh_main(ROOT) if args.refresh_main else stamp_repository(ROOT, check=args.check)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"docs-stamp: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr.decode("utf-8", errors="replace"), file=sys.stderr)
        return 1
    if changed:
        print("\n".join(changed))
        if args.check:
            print(
                "Stale Markdown timestamps. Commit source edits, run "
                "python .github/scripts/docs_stamp.py, then commit the stamps. "
                "Main-branch CI also refreshes automatically after merging.",
                file=sys.stderr,
            )
            return 1
        print(f"Stamped {len(changed)} Markdown files.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
