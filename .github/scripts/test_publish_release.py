import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from publish_release import (
    TARGET_FORMATS, find_release, matches, pages, publish, remote_assets, verify_assets,
)


def make_assets(directory: Path):
    for target, suffixes in TARGET_FORMATS.items():
        lines = []
        for suffix in suffixes:
            name = f"Thinoda-{target}{suffix}"
            data = name.encode()
            (directory / name).write_bytes(data)
            lines.append(f"{hashlib.sha256(data).hexdigest()}  {name}\n")
        (directory / f"SHA256SUMS-{target}.txt").write_text("".join(lines), encoding="ascii")


def as_remote(assets):
    return {
        asset.path.name: {
            "name": asset.path.name,
            "state": "uploaded",
            "digest": asset.digest,
            "size": asset.size,
        }
        for asset in assets
    }


class PublishReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        make_assets(self.directory)
        self.assets = verify_assets(self.directory)
        self.release = {"id": 123, "tag_name": "v1.0.0", "draft": False, "prerelease": False}

    def test_collection_contains_all_six_targets_and_sixteen_verified_assets(self):
        self.assertEqual(len(self.assets), 16)
        self.assertEqual(sum(a.path.name.startswith("SHA256SUMS-") for a in self.assets), 6)

    def test_missing_corrupt_extra_or_unsafe_files_are_rejected(self):
        target = "x86_64-pc-windows-msvc"
        manifest = self.directory / f"SHA256SUMS-{target}.txt"
        original = manifest.read_text(encoding="ascii")
        package = next(a.path for a in self.assets if a.path.name.endswith(f"-{target}.exe"))
        with self.subTest("checksum mismatch"):
            package.write_bytes(b"modified")
            with self.assertRaisesRegex(ValueError, "Checksum mismatch"):
                verify_assets(self.directory)
            package.write_bytes(package.name.encode())
        with self.subTest("missing manifest"):
            manifest.unlink()
            with self.assertRaisesRegex(ValueError, "Missing, empty or unsafe"):
                verify_assets(self.directory)
            manifest.write_text(original, encoding="ascii")
        with self.subTest("path traversal"):
            manifest.write_text(f"{'0' * 64}  ../package.exe\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "Invalid checksum entry"):
                verify_assets(self.directory)
            manifest.write_text(original, encoding="ascii")
        with self.subTest("unexpected asset"):
            (self.directory / "extra.txt").write_text("unexpected")
            with self.assertRaisesRegex(ValueError, "Unexpected or missing"):
                verify_assets(self.directory)

    @patch("publish_release.gh")
    def test_api_discovery_handles_pagination_and_rejects_ambiguous_responses(self, gh):
        release = self.release
        gh.return_value = subprocess.CompletedProcess(
            [], 0, stdout=json.dumps([[{"tag_name": "v0.9.0"}], [release]])
        )
        self.assertEqual(find_release("owner/repo", "v1.0.0"), release)
        self.assertIsNone(find_release("owner/repo", "v2.0.0"))
        gh.return_value.stdout = json.dumps([[release], [release]])
        with self.assertRaisesRegex(ValueError, "Multiple releases"):
            find_release("owner/repo", "v1.0.0")
        gh.return_value.stdout = json.dumps({"message": "unexpected"})
        with self.assertRaisesRegex(ValueError, "Unexpected GitHub API response"):
            pages("repos/owner/repo/releases")
        gh.side_effect = subprocess.CalledProcessError(1, ["gh", "api"])
        with self.assertRaises(subprocess.CalledProcessError):
            find_release("owner/repo", "v1.0.0")

    @patch("publish_release.pages")
    def test_remote_assets_reject_duplicate_names(self, get_pages):
        get_pages.return_value = [{"name": "same"}, {"name": "same"}]
        with self.assertRaisesRegex(ValueError, "Duplicate release asset"):
            remote_assets("owner/repo", 123)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_existing_release_uploads_only_missing_files_without_editing_notes(
        self, find, remote, gh,
    ):
        existing = as_remote(self.assets[:4])
        find.return_value = self.release
        remote.side_effect = [existing, as_remote(self.assets)]
        gh.return_value = subprocess.CompletedProcess([], 0)
        publish("v1.0.0", "owner/repo", self.directory)
        self.assertEqual(gh.call_count, 12)
        for call in gh.call_args_list:
            self.assertEqual(call.args[:2], ("release", "upload"))
            self.assertNotIn("--clobber", call.args)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_conflicts_abort_before_upload_unless_replacement_is_explicit(self, find, remote, gh):
        conflicting = as_remote(self.assets)
        conflicting[self.assets[0].path.name]["digest"] = "sha256:different"
        find.return_value = self.release
        remote.return_value = conflicting
        with self.assertRaisesRegex(ValueError, "replace_assets enabled"):
            publish("v1.0.0", "owner/repo", self.directory)
        gh.assert_not_called()

        remote.side_effect = [conflicting, as_remote(self.assets)]
        gh.return_value = subprocess.CompletedProcess([], 0)
        publish("v1.0.0", "owner/repo", self.directory, replace_existing=True)
        self.assertEqual(gh.call_count, 1)
        self.assertIn("--clobber", gh.call_args.args)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_failed_upload_is_accepted_only_after_exact_remote_verification(self, find, remote, gh):
        find.return_value = self.release
        first = self.assets[0]
        existing = as_remote(self.assets[1:])
        remote.side_effect = [existing, as_remote(self.assets), as_remote(self.assets)]
        gh.return_value = subprocess.CompletedProcess([], 1)
        publish("v1.0.0", "owner/repo", self.directory)
        gh.assert_called_once()
        self.assertFalse(matches(first, {"digest": first.digest, "size": first.size}))

        remote.side_effect = [existing, existing]
        with self.assertRaisesRegex(RuntimeError, "Upload failed"):
            publish("v1.0.0", "owner/repo", self.directory)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_new_release_uses_existing_tag_then_publishes_verified_draft(self, find, remote, gh):
        draft = dict(self.release, draft=True)
        find.side_effect = [None, draft, self.release]
        remote.side_effect = [{}, as_remote(self.assets)]
        gh.return_value = subprocess.CompletedProcess([], 0)
        publish("v1.0.0", "owner/repo", self.directory)
        create = gh.call_args_list[0].args
        self.assertEqual(create[:2], ("release", "create"))
        self.assertIn("--verify-tag", create)
        self.assertIn("--generate-notes", create)
        self.assertIn("--draft", create)
        edit = gh.call_args_list[-1].args
        self.assertEqual(edit[:2], ("release", "edit"))
        self.assertIn("--draft=false", edit)
        self.assertIn("--latest", edit)
        self.assertNotIn("--target", edit)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_existing_prerelease_draft_is_published_without_promoting_latest(self, find, remote, gh):
        draft = dict(self.release, draft=True, prerelease=True)
        find.side_effect = [draft, dict(draft, draft=False)]
        remote.return_value = as_remote(self.assets)
        publish("v1.0.0", "owner/repo", self.directory)
        gh.assert_called_once()
        self.assertIn("--draft=false", gh.call_args.args)
        self.assertNotIn("--latest", gh.call_args.args)

    @patch("publish_release.gh")
    @patch("publish_release.remote_assets")
    @patch("publish_release.find_release")
    def test_unverified_assets_never_publish_draft(self, find, remote, gh):
        find.return_value = dict(self.release, draft=True)
        remote.side_effect = [{}, {}]
        gh.return_value = subprocess.CompletedProcess([], 0)
        with self.assertRaisesRegex(RuntimeError, "verification failed"):
            publish("v1.0.0", "owner/repo", self.directory)
        self.assertTrue(all(call.args[:2] == ("release", "upload") for call in gh.call_args_list))


if __name__ == "__main__":
    unittest.main()
