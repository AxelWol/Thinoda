"""Guard release ordering, native architecture coverage and Node 24 action choices."""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"


def job(source: str, name: str) -> str:
    jobs = source.split("\njobs:\n", 1)[1]
    match = re.search(rf"(?ms)^  {re.escape(name)}:\n(.*?)(?=^  [\w-]+:|\Z)", jobs)
    if match is None:
        raise AssertionError(f"Missing job: {name}")
    return match[1]


def dependencies(block: str) -> list[str]:
    match = re.search(r"(?m)^    needs: (.+)$", block)
    return [] if match is None else [
        value.strip() for value in match[1].strip("[]").split(",")
    ]


class WorkflowContractTests(unittest.TestCase):
    def test_web_tool_download_is_verified_without_elevated_extraction(self):
        source = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        web = job(source, "web")
        self.assertIn('install_trunk.py "$RUNNER_TEMP/thinoda-tools"', web)
        self.assertIn('echo "$RUNNER_TEMP/thinoda-tools" >> "$GITHUB_PATH"', web)
        self.assertNotIn("sudo", web)
        self.assertNotRegex(web, r"curl[^\n]*\|")
        self.assertLess(web.index("install_trunk.py"), web.index("trunk build"))
        self.assertIn("cargo install cargo-fuzz --version 0.13.2 --locked",
                      job(source, "fuzz-smoke"))

    def test_release_order_separates_apple_jobs_and_orders_linux_architectures(self):
        source = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        expected = {
            "macos-intel": (["verify"], "macos-15-intel", "x86_64-apple-darwin", "app,dmg"),
            "linux-x86": (["verify", "macos-intel"], "ubuntu-22.04", "x86_64-unknown-linux-gnu", "deb,appimage"),
            "linux-arm": (["verify", "linux-x86"], "ubuntu-24.04-arm", "aarch64-unknown-linux-gnu", "deb,appimage"),
            "macos-silicon": (["verify", "windows", "linux-arm"], "macos-latest", "aarch64-apple-darwin", "app,dmg"),
        }
        for name, (needs, os, target, formats) in expected.items():
            with self.subTest(job=name):
                block = job(source, name)
                self.assertEqual(dependencies(block), needs)
                for key, value in [("os", os), ("target", target), ("formats", formats)]:
                    self.assertIn(f"      {key}: {value}\n", block)
                self.assertIn("uses: ./.github/workflows/package-target.yml", block)
                self.assertIn("ref: ${{ needs.verify.outputs.commit }}", block)
                self.assertNotIn("if:", block, "A failed prerequisite must block later stages")
        windows = job(source, "windows")
        self.assertEqual(dependencies(windows), ["verify", "macos-intel"])
        self.assertIn("fail-fast: false", windows)
        self.assertIn("target: x86_64-pc-windows-msvc", windows)
        self.assertIn("target: aarch64-pc-windows-msvc", windows)
        self.assertNotIn("secrets: inherit", windows)
        publish = job(source, "publish")
        self.assertCountEqual(dependencies(publish), ["verify", *expected, "windows"])
        self.assertNotIn("always()", publish)
        self.assertIn("pattern: thinoda-*", publish)
        self.assertIn("merge-multiple: true", publish)
        self.assertIn("Release tag changed during packaging", publish)

    def test_release_preflight_uses_main_without_signing_secrets(self):
        source = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        verify = job(source, "verify")
        self.assertIn("ref: main", verify)
        self.assertIn("fetch-depth: 0", verify)
        self.assertIn("persist-credentials: false", verify)
        self.assertIn('verify_release_ref.py --ref "$ref"', verify)
        self.assertNotIn("secrets.", verify)
        self.assertNotIn("secrets: inherit", verify)

    def test_shared_packaging_preserves_verification_and_artifact_identity(self):
        source = (WORKFLOWS / "package-target.yml").read_text(encoding="utf-8")
        self.assertIn('fuse_package=libfuse2t64', source)
        self.assertIn('cargo test --locked --workspace --target ${{ inputs.target }}', source)
        self.assertIn('--require-notarization', source)
        self.assertIn('if: always() && runner.os == \'macOS\'', source)
        self.assertIn('name: thinoda-${{ inputs.target }}', source)
        self.assertIn('if-no-files-found: error', source)

    def test_acceptance_campaigns_keep_sanitizers_and_failure_evidence(self):
        source = (WORKFLOWS / "importer-campaign.yml").read_text(encoding="utf-8")
        self.assertIn("target: [xmind, freemind, opml]", source)
        self.assertIn("permissions:\n  contents: read", source)
        self.assertNotIn("secrets.", source)
        self.assertIn("persist-credentials: false", source)
        self.assertIn("set -euo pipefail", source)
        self.assertIn("-max_total_time=1800", source)
        self.assertIn("-rss_limit_mb=2048", source)
        self.assertIn("-max_len=1048576", source)
        self.assertNotIn("--sanitizer none", source)
        self.assertIn("if: always()", source)
        self.assertIn("fuzz/artifacts/", source)
        self.assertIn("fuzz/logs/", source)
        self.assertIn("retention-days: 14", source)

    def test_node_version_and_actions_do_not_regress_to_older_runtimes(self):
        self.assertEqual((ROOT / ".node-version").read_text().strip(), "24")
        # Vendor manifests checked for these refs: JavaScript actions use node24;
        # the remaining actions are shell-only composites (including nested steps).
        allowed = {
            "actions/checkout@v7", "actions/setup-python@v7",
            "actions/setup-node@v7", "actions/upload-artifact@v7",
            "actions/download-artifact@v8", "actions/deploy-pages@v5",
            "actions/upload-pages-artifact@v5", "Swatinem/rust-cache@v2",
            "softprops/action-gh-release@v3", "googleapis/release-please-action@v5",
            "dtolnay/rust-toolchain@stable", "dtolnay/rust-toolchain@nightly",
            "vedantmgoyal9/winget-releaser@v2",
        }
        for workflow in WORKFLOWS.glob("*.y*ml"):
            source = workflow.read_text(encoding="utf-8")
            for action in re.findall(r"(?m)^\s*(?:- )?uses:\s*(\S+)", source):
                with self.subTest(workflow=workflow.name, action=action):
                    self.assertTrue(action.startswith("./") or action in allowed,
                                    "Audit the action and its nested runtimes before changing this list")
            self.assertNotIn("FORCE_JAVASCRIPT_ACTIONS_TO_NODE", source)
        ci = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("node-version-file: .node-version", ci)
        self.assertNotRegex(ci, r"(?m)^\s+node-version:")
        self.assertIn("ubuntu-24.04-arm", job(ci, "test"))


if __name__ == "__main__":
    unittest.main()
