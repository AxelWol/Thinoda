from pathlib import Path
import plistlib
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
    def test_all_custom_messages_are_wired_and_complete(self):
        config = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
        nsis = config["package"]["metadata"]["packager"]["nsis"]
        languages = [
            "English", "German", "Spanish", "French", "Danish", "Dutch",
            "Portuguese", "PortugueseBR", "Italian", "Polish",
        ]
        self.assertEqual(nsis["languages"], languages)
        self.assertTrue(nsis["display-language-selector"])
        custom = nsis["custom-language-file"]
        self.assertEqual(set(custom), set(languages) - {"English"})
        placeholders = {
            "alreadyInstalledLong": ["${PRODUCTNAME}", "${VERSION}"],
            "appRunning": ["${PRODUCTNAME}"],
            "appRunningOkKill": ["${PRODUCTNAME}", r"$\n"],
            "choowHowToInstall": ["${PRODUCTNAME}"],
            "failedToKillApp": ["${PRODUCTNAME}"],
            "newerVersionInstalled": ["${PRODUCTNAME}"],
            "olderOrUnknownVersionInstalled": ["$R4", "${PRODUCTNAME}"],
            "silentDowngrades": [r"$\n"],
            "uninstallApp": ["${PRODUCTNAME}"],
        }
        for name, path in custom.items():
            with self.subTest(language=name):
                source = (MANIFEST.parent / path).resolve().read_text(encoding="utf-8")
                messages = re.findall(
                    rf'^LangString (\w+) \$\{{LANG_{name.upper()}\}} "([^"\r\n]+)"$',
                    source, re.MULTILINE,
                )
                self.assertEqual(len(messages), len(MESSAGES), "Missing or duplicate translations")
                self.assertEqual({key for key, _ in messages}, MESSAGES)
                for key, text in messages:
                    self.assertTrue(text.strip())
                    self.assertNotIn("\ufffd", text)
                    self.assertCountEqual(
                        re.findall(r'\$\{\w+\}|\$R4|\$\\n', text),
                        placeholders.get(key, []),
                        f"{name}/{key}: changed interpolation contract",
                    )
                if name == "German":
                    self.assertEqual(dict(messages)["createDesktop"], "Desktop-Verknüpfung erstellen")

    def test_macos_declares_all_supported_languages(self):
        config = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
        macos = config["package"]["metadata"]["packager"]["macos"]
        with (MANIFEST.parent / macos["info-plist-path"]).open("rb") as stream:
            info = plistlib.load(stream)
        self.assertEqual(info["CFBundleDevelopmentRegion"], "en")
        self.assertEqual(
            info["CFBundleLocalizations"],
            ["en", "de", "es", "fr", "da", "nl", "pt-PT", "pt-BR", "it", "pl"],
        )

    def test_linux_launcher_comments_are_translated_without_changing_execution(self):
        config = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
        deb = config["package"]["metadata"]["packager"]["deb"]
        source = (MANIFEST.parent / deb["desktop-template"]).read_text(encoding="utf-8")
        comments = dict(re.findall(r'^Comment\[([^]]+)\]=(.+)$', source, re.MULTILINE))
        self.assertEqual(set(comments), {"de", "es", "fr", "da", "nl", "pt", "pt_PT", "pt_BR", "it", "pl"})
        self.assertTrue(all(value.strip() for value in comments.values()))
        self.assertIn("Exec={{exec}} {{exec_arg}}\n", source)
        self.assertIn("MimeType={{mime_type}}\n", source)
        self.assertIn("Name={{name}}\n", source)


if __name__ == "__main__":
    unittest.main()
