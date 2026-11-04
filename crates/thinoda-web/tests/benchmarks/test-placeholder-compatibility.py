import contextlib
import hashlib
import importlib.util
import io
import unittest
from pathlib import Path
from types import SimpleNamespace


HERE = Path(__file__).parent
SOURCE = HERE.parent.joinpath("fixtures", "affinity-diagram.thinoda")
LEGACY = HERE.joinpath("make-board-fixtures-legacy.py")
CURRENT = HERE.joinpath("make-board-fixtures.py")
# Snapshot of the generator in commit 2b94a6f89c6c9cf6702f4d4e4bf1555b950b6e86.
LEGACY_SHA256 = "4cb51db0fd2b6c3bb209f9c40849226e15c451d6b574a1db205046c2f97f13e7"


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class MemoryDestination(io.BytesIO):
    def stat(self):
        return SimpleNamespace(st_size=len(self.getbuffer()))


class PlaceholderCompatibility(unittest.TestCase):
    def test_snapshot_matches_the_committed_generator(self):
        source = LEGACY.read_bytes().replace(b"\r\n", b"\n")
        self.assertEqual(hashlib.sha256(source).hexdigest(), LEGACY_SHA256)

    def test_default_archives_match_the_committed_generator_byte_for_byte(self):
        legacy = load_module("legacy_board_fixture", LEGACY)
        current = load_module("current_board_fixture", CURRENT)
        for strokes in (1000, 10000):
            with self.subTest(strokes=strokes):
                destination = MemoryDestination()
                with contextlib.redirect_stdout(io.StringIO()):
                    legacy.fixture(SOURCE, destination, strokes)
                expected = destination.getvalue()
                actual, _ = current.build_archive(SOURCE, strokes)
                expected_hash = hashlib.sha256(expected).hexdigest()
                actual_hash = hashlib.sha256(actual).hexdigest()
                self.assertTrue(
                    actual == expected,
                    f"{strokes}: committed={expected_hash}, current={actual_hash}",
                )
                print(f"{strokes}: {len(actual)} identical bytes, SHA-256 {actual_hash}")


if __name__ == "__main__":
    unittest.main()
