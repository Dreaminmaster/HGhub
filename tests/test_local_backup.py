import importlib.util
import pathlib
import tempfile
import unittest
import json
import sys
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("local_backup", pathlib.Path(__file__).resolve().parents[1] / "tools" / "local_backup.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class BackupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = pathlib.Path(self.temp.name).resolve()
        self.app = root / "Example.app"
        self.data = root / "data"
        self.dest = root / "private"
        for p in (self.app, self.data, self.dest):
            p.mkdir(mode=0o700)
        (self.app / "fixture.txt").write_text("example application")
        (self.data / "settings.json").write_text('{"version":1}')

    def test_verified_copy_and_originals_preserved(self):
        out = module.backup(self.app, self.data, self.dest)
        receipt = json.loads((out / "verified-receipt.json").read_text())
        self.assertEqual(receipt["app_name"], "Example.app")
        self.assertEqual(module.manifest(out / "application"), module.manifest(self.app))
        self.assertEqual(module.manifest(out / "data"), module.manifest(self.data))
        (self.data / "settings.json").write_text('{"version":2}')
        self.assertEqual((out / "data" / "settings.json").read_text(), '{"version":1}')
        self.assertEqual(out.stat().st_mode & 0o077, 0)

    def test_refuses_overlap_and_public_destination(self):
        with self.assertRaises(ValueError):
            module.backup(self.app, self.data, self.data)
        self.dest.chmod(0o755)
        with self.assertRaises(ValueError):
            module.backup(self.app, self.data, self.dest)
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_refuses_external_symlink(self):
        (self.data / "external").symlink_to("../Example.app")
        with self.assertRaises(ValueError):
            module.backup(self.app, self.data, self.dest)
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_copy_failure_does_not_produce_verified_receipt(self):
        with patch.object(module.subprocess, "run", side_effect=OSError("fixture write failure")):
            with self.assertRaises(OSError):
                module.backup(self.app, self.data, self.dest)
        outputs = list(self.dest.iterdir())
        self.assertEqual(len(outputs), 1)
        self.assertFalse((outputs[0] / "verified-receipt.json").exists())
        self.assertEqual((self.data / "settings.json").read_text(), '{"version":1}')


if __name__ == "__main__":
    unittest.main()
