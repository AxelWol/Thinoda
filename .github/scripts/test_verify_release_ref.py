"""Exercise tag resolution and main-ancestry checks against a real git fixture."""

from pathlib import Path
import subprocess
import tempfile
import unittest

from verify_release_ref import verified_commit


class ReleaseRefTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Release test")
        self.git("config", "user.email", "release@example.invalid")
        self.git("commit", "--quiet", "--allow-empty", "-m", "Approved")
        self.approved = self.git("rev-parse", "HEAD")
        self.git("update-ref", "refs/remotes/origin/main", self.approved)
        self.git("update-ref", "refs/heads/main", self.approved)

    def git(self, *args):
        return subprocess.check_output(
            ["git", *args], cwd=self.root, text=True, stderr=subprocess.PIPE
        ).strip()

    def test_lightweight_and_annotated_tags_resolve_to_approved_commit(self):
        self.git("tag", "v1.0.0")
        self.git("tag", "-a", "v1.0.1", "-m", "Annotated")
        for ref in ["refs/tags/v1.0.0", "refs/tags/v1.0.1",
                    "refs/heads/main", self.approved]:
            self.assertEqual(verified_commit(ref, self.root), self.approved)

    def test_unmerged_or_moved_tag_is_rejected(self):
        self.git("tag", "v1.0.0")
        self.git("commit", "--quiet", "--allow-empty", "-m", "Not on main")
        unapproved = self.git("rev-parse", "HEAD")
        self.git("tag", "--force", "v1.0.0")
        for ref in ["refs/tags/v1.0.0", unapproved]:
            with self.assertRaisesRegex(ValueError, "not reachable"):
                verified_commit(ref, self.root)

    def test_main_ancestor_remains_valid_but_noncommit_tag_does_not(self):
        self.git("tag", "v1.0.0")
        self.git("commit", "--quiet", "--allow-empty", "-m", "New main commit")
        self.git("update-ref", "refs/remotes/origin/main", self.git("rev-parse", "HEAD"))
        self.assertEqual(verified_commit("refs/tags/v1.0.0", self.root), self.approved)
        self.git("tag", "v-tree", self.git("rev-parse", "HEAD^{tree}"))
        with self.assertRaises(subprocess.CalledProcessError):
            verified_commit("refs/tags/v-tree", self.root)

    def test_missing_invalid_and_ambiguous_refs_fail_closed(self):
        for ref in ["--help", "v1.0.0", "HEAD", "refs/heads/topic", "a\ncommit"]:
            with self.assertRaises(ValueError):
                verified_commit(ref, self.root)
        with self.assertRaises(subprocess.CalledProcessError):
            verified_commit("refs/tags/vmissing", self.root)
        self.git("tag", "v1.0.0")
        with self.assertRaises(subprocess.CalledProcessError):
            verified_commit("refs/tags/v1.0.0^{}", self.root)
        self.git("update-ref", "-d", "refs/remotes/origin/main")
        with self.assertRaises(RuntimeError):
            verified_commit(self.approved, self.root)


if __name__ == "__main__":
    unittest.main()
