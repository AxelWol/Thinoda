import unittest

import third_party_notices


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
