"""Exercise the real SDK validator against exports and schema-invalid packages.

Ordinary script discovery skips these tests when .NET fixtures are unavailable.
CI supplies both paths and therefore runs (rather than skips) the SDK checks.
"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


VALIDATOR = os.environ.get("THINODA_OPENXML_VALIDATOR")
FIXTURES = os.environ.get("THINODA_OFFICE_FIXTURES")
DOTNET = os.environ.get("THINODA_DOTNET", "dotnet")


@unittest.skipUnless(VALIDATOR and FIXTURES, "Office SDK job supplies validator and fixtures")
class OpenXmlValidationTests(unittest.TestCase):
    def run_validator(self, *paths):
        return subprocess.run(
            [DOTNET, VALIDATOR, *(str(path) for path in paths)],
            text=True, capture_output=True, timeout=120, check=False,
        )

    def test_all_generated_exports_are_schema_valid(self):
        fixtures = Path(FIXTURES)
        self.assertEqual(
            {path.name for path in fixtures.iterdir() if not path.name.startswith("~$")},
            {
                "plain.docx", "rich-notes.docx", "plain.xlsx", "map.xlsx",
                "slides-light-16x9.pptx", "slides-dark-16x9.pptx",
                "slides-light-4x3.pptx", "slides-dark-4x3.pptx",
            },
        )
        result = self.run_validator(fixtures)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Validated 8 Office file(s)", result.stdout)
        self.assertEqual(result.stdout.count(": 0 validation error(s)"), 8)

    def test_office_lock_files_are_ignored_and_corruption_does_not_skip_other_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            shutil.copyfile(Path(FIXTURES) / "plain.docx", directory / "plain.docx")
            (directory / "~$plain.docx").write_bytes(b"Office lock metadata, not a document")
            result = self.run_validator(directory)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Validated 1 Office file(s)", result.stdout)
            (directory / "broken.pptx").write_bytes(b"not a zip")
            result = self.run_validator(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Validated 2 Office file(s)", result.stdout, result.stdout + result.stderr)
            self.assertIn("plain.docx: 0 validation error(s)", result.stdout)

    def test_each_format_rejects_schema_invalid_but_well_formed_xml(self):
        cases = [
            (
                "slides-light-16x9.pptx", "ppt/slideMasters/slideMaster1.xml",
                b'<p:sldLayoutId id="2147483649"', b'<p:sldLayoutId id="1"',
            ),
            (
                "rich-notes.docx", "word/document.xml",
                b'w:w="12240"', b'w:w="invalid"',
            ),
            (
                "map.xlsx", "xl/workbook.xml",
                b'sheetId="1"', b'sheetId="invalid"',
            ),
            (
                "slides-dark-4x3.pptx", "ppt/slideMasters/slideMaster1.xml",
                b'<p:sldLayoutId id="2147483649"', b'<p:sldLayoutId id="2147483648"',
            ),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            for name, part, original, invalid in cases:
                with self.subTest(format=Path(name).suffix):
                    target = Path(temporary) / name
                    with zipfile.ZipFile(Path(FIXTURES) / name) as source:
                        with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as output:
                            for item in source.infolist():
                                data = source.read(item.filename)
                                if item.filename == part:
                                    self.assertEqual(data.count(original), 1)
                                    data = data.replace(original, invalid)
                                output.writestr(item.filename, data)
                    result = self.run_validator(target)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("/" + part, result.stderr)
                    self.assertTrue("Sch_" in result.stderr or "duplicate master/layout ID" in result.stderr,
                                    result.stdout + result.stderr)

    def test_missing_empty_unsupported_and_corrupt_inputs_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for path, content in [("broken.pptx", b"not a zip"), ("unsupported.txt", b"text")]:
                (directory / path).write_bytes(content)
            empty = directory / "empty"
            empty.mkdir()
            for path in [directory / "missing.docx", empty, directory / "broken.pptx",
                         directory / "unsupported.txt"]:
                with self.subTest(path=path):
                    result = self.run_validator(path)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertTrue(result.stderr)


if __name__ == "__main__":
    unittest.main()
