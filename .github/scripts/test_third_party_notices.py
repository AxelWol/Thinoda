import unittest
from unittest import mock

import third_party_notices


class CargoMetadataTests(unittest.TestCase):
    def test_metadata_is_decoded_as_utf8(self):
        result = mock.Mock(stdout='{"name": "Gr\\u00f6\\u00dfe"}')
        with mock.patch.object(third_party_notices.subprocess, "run", return_value=result) as run:
            self.assertEqual(third_party_notices.cargo_metadata(), {"name": "Gr\u00f6\u00dfe"})
        self.assertEqual(run.call_args.kwargs["encoding"], "utf-8")


class EmbeddedFontNoticeTests(unittest.TestCase):
    def test_italic_font_licenses_are_embedded_in_user_visible_document(self):
        rendered = third_party_notices.render([])
        self.assertIn("Copyright 2011 Canonical Ltd.", rendered)
        for name in ["Hack-LICENSE.txt", "Ubuntu-LICENCE.txt"]:
            license_text = (third_party_notices.FONT_LICENSE_DIR / name).read_text(
                encoding="utf-8"
            ).strip()
            self.assertIn(license_text, rendered)


class EmbeddedSchemaNoticeTests(unittest.TestCase):
    def test_w3c_notice_is_embedded_in_user_visible_generated_document(self):
        rendered = third_party_notices.render([])
        self.assertIn("Copyright © World Wide Web Consortium.", rendered)
        self.assertIn("Software and Document license - 2023 version", rendered)
        self.assertIn("The full text of this NOTICE", rendered)
        self.assertIn('THIS WORK IS PROVIDED "AS IS,"', rendered)
        self.assertIn("Title to copyright in this work will at all times remain", rendered)
        self.assertIn("https://www.w3.org/2009/01/xml.xsd", rendered)


if __name__ == "__main__":
    unittest.main()
