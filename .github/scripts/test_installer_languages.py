from pathlib import Path
import re
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "crates" / "thinoda-app" / "Cargo.toml"
# Custom message contract of the pinned cargo-packager 0.11.8 NSIS template.
MESSAGES = {
    "addOrReinstall", "alreadyInstalled", "alreadyInstalledLong", "appRunning",
    "appRunningOkKill", "chooseMaintenanceOption", "choowHowToInstall",
    "createDesktop", "dontUninstall", "dontUninstallDowngrade", "failedToKillApp",
    "installingWebview2", "newerVersionInstalled", "older",
    "olderOrUnknownVersionInstalled", "silentDowngrades", "unableToUninstall",
    "uninstallApp", "uninstallBeforeInstalling", "unknown", "deleteAppData",
}


class InstallerLanguageTests(unittest.TestCase):
    def test_german_custom_messages_are_wired_and_complete(self):
        config = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
        nsis = config["package"]["metadata"]["packager"]["nsis"]
        self.assertEqual(nsis["languages"], ["English", "German"])
        custom = nsis["custom-language-file"]
        self.assertEqual(set(custom), {"German"})
        language = (MANIFEST.parent / custom["German"]).resolve()
        source = language.read_text(encoding="utf-8")
        messages = re.findall(
            r'^LangString (\w+) \$\{LANG_GERMAN\} "([^"\r\n]+)"$', source, re.MULTILINE
        )
        self.assertEqual(len(messages), len(MESSAGES), "Missing or duplicate translations")
        self.assertEqual({key for key, _ in messages}, MESSAGES)
        self.assertTrue(all(text.strip() for _, text in messages))
        self.assertEqual(dict(messages)["createDesktop"], "Desktop-Verknüpfung erstellen")


if __name__ == "__main__":
    unittest.main()
