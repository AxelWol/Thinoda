import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from docs_stamp import preserve_stamp, refresh_main, source_timestamp, stamp_repository, strip_stamp, with_stamp
import docs_stamp
import third_party_notices


FIRST = "2025-01-02T03:04:05+02:00"
FIRST_UTC = "2025-01-02 01:04:05 UTC"
SECOND = "2025-02-03T04:05:06-03:00"
SECOND_UTC = "2025-02-03 07:05:06 UTC"
THIRD = "2025-03-04T05:06:07+00:00"


class RenderingTests(unittest.TestCase):
    def test_preserves_full_body_bom_frontmatter_and_line_endings(self):
        for text in [
            "", "# Title\n\nFull legal terms.\n", "\ufeff# Title\n",
            "---\ntitle: Legal terms\n---\n# Terms\n\nAll original text.",
            "\ufeff---\r\ntitle: Legal terms\r\n...\r\n\r\n# Terms\r\n",
        ]:
            with self.subTest(text=text):
                stamped = with_stamp(text, FIRST_UTC)
                self.assertEqual(strip_stamp(stamped), text)
                self.assertEqual(with_stamp(stamped, FIRST_UTC), stamped)
                self.assertIn(f"_Last updated: {FIRST_UTC}", stamped)
                self.assertIn("(non-binding repository metadata)", stamped)
                if text.startswith("\ufeff---"):
                    self.assertTrue(stamped.startswith("\ufeff---\r\ntitle: Legal terms\r\n...\r\n"))
                    self.assertNotIn("\n", stamped.replace("\r\n", ""))

    def test_only_our_header_is_removed(self):
        text = "# Example\n\n" + with_stamp("Example body", FIRST_UTC)
        self.assertEqual(strip_stamp(text), text)
        ordinary = "_Last updated: an author's own text._\n\n# Terms\n"
        self.assertEqual(strip_stamp(ordinary), ordinary)

    def test_invalid_frontmatter_is_an_explicit_error(self):
        for text in ["---\ntitle: Unclosed\n", "---\ntitle: No newline\n---"]:
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "frontmatter"):
                with_stamp(text, FIRST_UTC)

    def test_generator_preserves_only_metadata_not_previous_content(self):
        current = with_stamp("old content\n", FIRST_UTC)
        generated = "# New notices\n\nFull new generated content.\n"
        self.assertEqual(preserve_stamp(current, generated), with_stamp(generated, FIRST_UTC))
        self.assertEqual(preserve_stamp("no metadata", generated), generated)


class HistoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Docs stamp tests")
        self.git("config", "user.email", "docs-stamp@example.invalid")
        self.git("config", "core.autocrlf", "false")
        self.git("config", "commit.gpgsign", "false")
        self.write("README.md", "# First\n")
        self.commit(FIRST)

    def git(self, *args, date=FIRST):
        env = dict(os.environ, GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date)
        # Do not inherit an enclosing hook's alternate index or repository.
        for key in ("GIT_INDEX_FILE", "GIT_DIR", "GIT_WORK_TREE"):
            env.pop(key, None)
        return subprocess.run(
            ["git", *args], cwd=self.root, env=env, check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        ).stdout

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode("utf-8"))

    def read(self, name="README.md"):
        return (self.root / name).read_bytes().decode("utf-8")

    def commit(self, date):
        self.git("add", "--all")
        self.git("commit", "-m", "test fixture", date=date)

    def test_check_is_read_only_and_stamping_is_idempotent(self):
        self.assertEqual(stamp_repository(self.root, check=True), ["README.md"])
        self.assertEqual(self.read(), "# First\n")
        self.assertEqual(stamp_repository(self.root), ["README.md"])
        self.assertIn(FIRST_UTC, self.read())
        self.assertEqual(stamp_repository(self.root), [])
        self.assertEqual(stamp_repository(self.root, check=True), [])

    def test_stamp_only_commits_do_not_advance_source_date(self):
        stamp_repository(self.root)
        self.commit(SECOND)
        self.write("README.md", with_stamp(self.read(), "2099-01-01 00:00:00 UTC"))
        self.commit(THIRD)
        self.assertEqual(source_timestamp(self.root, "README.md"), (FIRST_UTC, "# First\n"))
        self.assertEqual(stamp_repository(self.root), ["README.md"])
        self.assertIn(FIRST_UTC, self.read())
        self.commit(THIRD)
        self.assertEqual(stamp_repository(self.root, check=True), [])

    def test_source_changes_get_their_own_date_not_stamp_commit_date(self):
        stamp_repository(self.root)
        self.commit(SECOND)
        self.write("README.md", with_stamp("# Changed\n", FIRST_UTC))
        self.commit(THIRD)
        stamp_repository(self.root)
        self.assertIn("2025-03-04 05:06:07 UTC", self.read())

    def test_all_tracked_markdown_only_including_special_paths(self):
        names = ["docs/nested guide.md", "docs/[literal].md", "docs/caf\u00e9.md", "-option.md"]
        for name in names:
            self.write(name, "# Guide\n")
        self.write("other.txt", "untouched")
        self.commit(SECOND)
        self.write("untracked.md", "# Do not stamp\n")
        self.assertEqual(set(stamp_repository(self.root)), {"README.md", *names})
        for name in names:
            self.assertIn(SECOND_UTC, self.read(name))
        self.assertEqual(self.read("untracked.md"), "# Do not stamp\n")
        self.assertEqual(self.read("other.txt"), "untouched")

    def test_uncommitted_sources_fail_without_any_partial_writes(self):
        self.write("Z.md", "# New staged file\n")
        self.git("add", "Z.md")
        with self.assertRaisesRegex(ValueError, "commit this new file"):
            stamp_repository(self.root)
        self.assertEqual(self.read(), "# First\n")
        self.git("commit", "-m", "new file", date=SECOND)
        self.write("Z.md", "# Dirty source\n")
        with self.assertRaisesRegex(ValueError, "commit source edits"):
            stamp_repository(self.root)
        self.assertEqual(self.read(), "# First\n")

    def test_normal_merge_keeps_source_date(self):
        self.git("checkout", "-b", "feature")
        self.write("README.md", "# Feature\n")
        self.commit(SECOND)
        stamp_repository(self.root)
        self.commit(THIRD)
        self.git("checkout", "main")
        self.git("merge", "--no-ff", "feature", "-m", "merge", date=THIRD)
        self.assertEqual(stamp_repository(self.root, check=True), [])
        self.assertIn(SECOND_UTC, self.read())

    def test_squash_detects_changed_source_date(self):
        self.git("checkout", "-b", "feature")
        self.write("README.md", "# Feature\n")
        self.commit(SECOND)
        stamp_repository(self.root)
        self.commit(THIRD)
        self.git("checkout", "main")
        self.git("merge", "--squash", "feature")
        self.git("commit", "-m", "squashed feature", date=THIRD)
        self.assertEqual(stamp_repository(self.root, check=True), ["README.md"])

    def test_rewritten_committer_date_is_refreshed(self):
        self.write("README.md", "# Feature\n")
        self.commit(SECOND)
        stamp_repository(self.root)
        self.commit(THIRD)
        # A squash/rebase can combine the source and its old metadata under a
        # different committer date; refresh must use the rewritten history.
        self.git("reset", "--soft", "HEAD~2")
        self.git("commit", "-m", "rewritten source and metadata", date=THIRD)
        stamp_repository(self.root)
        self.assertIn("2025-03-04 05:06:07 UTC", self.read())

    def test_rename_and_readd_start_new_path_history(self):
        stamp_repository(self.root)
        self.commit(FIRST)
        self.git("mv", "README.md", "RENAMED.md")
        self.commit(SECOND)
        stamp_repository(self.root)
        self.assertIn(SECOND_UTC, self.read("RENAMED.md"))
        self.commit(SECOND)
        self.git("rm", "RENAMED.md")
        self.commit(SECOND)
        self.write("RENAMED.md", "# First\n")
        self.commit(THIRD)
        stamp_repository(self.root)
        self.assertIn("2025-03-04 05:06:07 UTC", self.read("RENAMED.md"))

    def test_shallow_history_fails_instead_of_inventing_a_date(self):
        self.write("README.md", "# Second\n")
        self.commit(SECOND)
        with tempfile.TemporaryDirectory() as destination:
            self.git("clone", "--depth=1", self.root.as_uri(), destination)
            with self.assertRaisesRegex(ValueError, "Full Git history"):
                stamp_repository(Path(destination))

    def test_crlf_checkout_does_not_count_as_source_edit(self):
        self.write("README.md", "# First\r\n")
        stamp_repository(self.root)
        self.assertEqual(strip_stamp(self.read()), "# First\r\n")
        self.assertIn(FIRST_UTC, self.read())

    def test_deleted_tracked_file_fails_explicitly(self):
        (self.root / "README.md").unlink()
        with self.assertRaisesRegex(ValueError, "regular tracked"):
            stamp_repository(self.root)

    def make_remote(self):
        remote = tempfile.TemporaryDirectory()
        self.addCleanup(remote.cleanup)
        self.git("init", "--bare", remote.name)
        self.git("remote", "add", "origin", remote.name)
        self.git("push", "origin", "main")
        return Path(remote.name)

    def test_automatic_main_refresh_persists_and_does_not_loop(self):
        remote = self.make_remote()
        self.assertEqual(refresh_main(self.root), ["README.md"])
        first_head = self.git("rev-parse", "HEAD")
        self.assertEqual(
            first_head, self.git("--git-dir", str(remote), "rev-parse", "refs/heads/main")
        )
        self.assertIn(FIRST_UTC, self.read())
        self.assertEqual(stamp_repository(self.root, check=True), [])
        self.assertEqual(refresh_main(self.root), [])
        self.assertEqual(first_head, self.git("rev-parse", "HEAD"))

    def racing_refresh(self, body):
        remote = self.make_remote()
        original_git = docs_stamp.git
        raced = False
        with tempfile.TemporaryDirectory() as writer:
            self.git("clone", "--branch", "main", str(remote), writer)

            def race(root, *args):
                nonlocal raced
                if args[0] == "push" and not raced:
                    raced = True
                    path = Path(writer)
                    (path / "README.md").write_text(body, encoding="utf-8")
                    env = dict(os.environ, GIT_AUTHOR_DATE=SECOND, GIT_COMMITTER_DATE=SECOND)
                    for key in ("GIT_INDEX_FILE", "GIT_DIR", "GIT_WORK_TREE"):
                        env.pop(key, None)
                    for command in [
                        ["add", "README.md"],
                        ["-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                         "-c", "commit.gpgsign=false", "commit", "-m", "concurrent source"],
                        ["push", "origin", "main"],
                    ]:
                        subprocess.run(
                            ["git", *command], cwd=path, env=env,
                            check=True, capture_output=True,
                        )
                return original_git(root, *args)

            with patch.object(docs_stamp, "git", side_effect=race):
                self.assertEqual(refresh_main(self.root), ["README.md"])
        self.assertTrue(raced)
        self.assertEqual(stamp_repository(self.root, check=True), [])
        self.assertEqual(
            self.git("rev-parse", "HEAD"),
            self.git("--git-dir", str(remote), "rev-parse", "refs/heads/main"),
        )

    def test_automatic_refresh_retries_concurrent_main_update(self):
        self.racing_refresh("# Concurrent source\n")
        self.assertEqual(strip_stamp(self.read()), "# Concurrent source\n")
        self.assertIn(SECOND_UTC, self.read())

    def test_metadata_only_race_keeps_original_source_date(self):
        self.racing_refresh(with_stamp("# First\n", "2099-01-01 00:00:00 UTC"))
        self.assertEqual(strip_stamp(self.read()), "# First\n")
        self.assertIn(FIRST_UTC, self.read())

    def test_refresh_rejects_nonmetadata_edits_in_index(self):
        self.make_remote()
        original_git = docs_stamp.git

        def tamper(root, *args):
            if args[0] == "add":
                self.write("README.md", with_stamp("# Unexpected source edit\n", FIRST_UTC))
            return original_git(root, *args)

        with patch.object(docs_stamp, "git", side_effect=tamper):
            with self.assertRaisesRegex(ValueError, "nonmetadata source edits"):
                refresh_main(self.root)
        self.assertEqual(self.git("rev-parse", "HEAD"), self.git("rev-parse", "origin/main"))

    def test_release_please_changelog_regeneration_is_refreshed_automatically(self):
        self.write("CHANGELOG.md", "# Changelog\n\n## 0.1.0\n\nInitial release.\n")
        self.commit(FIRST)
        stamp_repository(self.root)
        self.commit(FIRST)
        generated = "# Changelog\n\n## 0.2.0\n\nNew feature.\n\n## 0.1.0\n\nInitial release.\n"
        self.write("CHANGELOG.md", generated)
        self.commit(SECOND)
        self.make_remote()
        self.assertEqual(refresh_main(self.root), ["CHANGELOG.md"])
        self.assertEqual(strip_stamp(self.read("CHANGELOG.md")), generated)
        self.assertIn(SECOND_UTC, self.read("CHANGELOG.md"))
        self.assertEqual(refresh_main(self.root), [])

    def test_refresh_refuses_dirty_checkout(self):
        self.write("README.md", "# Dirty\n")
        with self.assertRaisesRegex(ValueError, "clean checkout"):
            refresh_main(self.root)

    def test_permission_failure_is_reported_without_retry_or_force_push(self):
        self.make_remote()
        original_git = docs_stamp.git
        pushes = []

        def denied(root, *args):
            if args[0] == "push":
                pushes.append(args)
                raise subprocess.CalledProcessError(
                    1, ["git", *args], output=b"", stderr=b"permission denied"
                )
            return original_git(root, *args)

        with patch.object(docs_stamp, "git", side_effect=denied):
            with self.assertRaises(subprocess.CalledProcessError):
                refresh_main(self.root)
        self.assertEqual(pushes, [("push", "--porcelain", "origin", "HEAD:refs/heads/main")])

    def test_repeated_concurrent_updates_fail_after_three_attempts(self):
        self.make_remote()
        original_git = docs_stamp.git
        pushes = []

        def reject(root, *args):
            if args[0] == "push":
                pushes.append(args)
                raise subprocess.CalledProcessError(
                    1, ["git", *args],
                    output=b"!\tHEAD:refs/heads/main\t[rejected] (fetch first)\n",
                    stderr=b"remote advanced",
                )
            return original_git(root, *args)

        with patch.object(docs_stamp, "git", side_effect=reject):
            with self.assertRaisesRegex(ValueError, "all three refresh attempts"):
                refresh_main(self.root)
        self.assertEqual(len(pushes), 3)


class NoticesTests(unittest.TestCase):
    def test_existing_check_ignores_only_header_and_still_rejects_body_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "THIRD_PARTY_NOTICES.md"
            expected = third_party_notices.render([])
            output.write_text(with_stamp(expected, FIRST_UTC), encoding="utf-8")
            with (
                patch.object(third_party_notices, "OUTPUT", output),
                patch.object(third_party_notices, "cargo_metadata", return_value={}),
                patch.object(third_party_notices, "runtime_packages", return_value=[]),
                patch("sys.argv", ["third_party_notices.py", "--check"]),
                contextlib.redirect_stderr(io.StringIO()),
            ):
                self.assertEqual(third_party_notices.main(), 0)
                output.write_text(with_stamp(expected + "unexpected\n", FIRST_UTC), encoding="utf-8")
                self.assertEqual(third_party_notices.main(), 1)
                with patch("sys.argv", ["third_party_notices.py"]), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(third_party_notices.main(), 0)
                self.assertEqual(output.read_text(encoding="utf-8"), with_stamp(expected, FIRST_UTC))


if __name__ == "__main__":
    unittest.main()
