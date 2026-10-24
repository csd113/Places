"""Tiny routing/lifetime checks; real hostile codec coverage lives in Rust.

These two sub-KiB fixtures uniquely protect Python's method93 bypass on all
Python versions, original accounting metadata, one-copy reuse and cleanup on
failure. The decoder subprocess is mocked; no maps, baking or native resources
are required. Runtime is proportional to a few hundred local bytes.
"""
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

from tools.levels.package_zip import PackageZip


class PackageZipTests(unittest.TestCase):
    def fixture(self, root):
        path = root / "input.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("blobs/data", b"small exact fixture")
        data = bytearray(path.read_bytes())
        central = data.index(b"PK\x01\x02")
        for offset in (8, central + 10):
            struct.pack_into("<H", data, offset, 93)
        path.write_bytes(data)
        reader = root / "reader"
        reader.write_bytes(b"mock executable; never invoked")
        return path, reader

    def test_legacy_stays_local_and_method93_uses_one_bounded_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            legacy = root / "legacy.zip"
            with PackageZip(legacy, "w") as archive:
                archive.writestr("manifest.json", b"{}")
            with mock.patch("subprocess.run", side_effect=AssertionError("legacy needs no reader")):
                with PackageZip(legacy) as archive:
                    self.assertEqual(archive.read("manifest.json"), b"{}")
            path, reader = self.fixture(root)
            outputs = []

            def decode(command, **kwargs):
                output = Path(command[2]);outputs.append(output)
                self.assertEqual(command, [str(reader), str(path.resolve()), str(output), "1"])
                with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                    archive.writestr("blobs/data", b"small exact fixture")
                return subprocess.CompletedProcess(command, 0, "{}", "")

            with mock.patch.dict(os.environ, PLACES_PACKAGE_REPACK=str(reader)), mock.patch("subprocess.run", side_effect=decode) as decoder:
                with PackageZip(path) as archive:
                    self.assertEqual(archive.read("blobs/data"), b"small exact fixture")
                    with archive.open(archive.getinfo("blobs/data")) as stream:
                        self.assertEqual(stream.read(), b"small exact fixture")
                    self.assertEqual(archive.getinfo("blobs/data").compress_type, 93)
                    self.assertEqual(decoder.call_count, 1)
                    self.assertTrue(outputs[0].is_file())
                self.assertFalse(outputs[0].parent.exists(), "owned compatibility copy is removed at close")

    def test_decoder_errors_and_changed_receipts_remove_owned_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary);path, reader = self.fixture(root)
            for changed in (False, True):
                outputs = []

                def fail(command, **kwargs):
                    output = Path(command[2]);outputs.append(output)
                    if changed:
                        with zipfile.ZipFile(output, "w") as archive:
                            archive.writestr("blobs/data", b"substituted")
                    else:
                        output.write_bytes(b"partial")
                    return subprocess.CompletedProcess(command, 0 if changed else 1, "", "window bound refused")

                with mock.patch.dict(os.environ, PLACES_PACKAGE_REPACK=str(reader)), mock.patch("subprocess.run", side_effect=fail):
                    with PackageZip(path) as archive:
                        with self.assertRaises(zipfile.BadZipFile):
                            archive.read("blobs/data")
                    self.assertFalse(outputs[0].parent.exists(), "failure cannot retain a task-owned temp")
