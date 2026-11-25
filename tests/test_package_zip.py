"""Tiny routing/lifetime checks; real hostile codec coverage lives in Rust.

The sub-KiB method-93 fixtures protect original accounting metadata, one-copy
reuse and temporary ownership on error. The comparison witness is just over
one streaming block to catch errors at the boundary.
Two sub-KiB legacy ZIPs and a synthetic stat change check that the comparison
verifies both source handles after its final record read. These tests need no
map, bake or native process.
"""
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

from release.package_zip import PackageZip
from release.compare_packages import BLOCK_BYTES, compare, compare_record


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
                output = Path(command[4]);outputs.append(output)
                self.assertEqual(command, [str(reader), "--debug", "repack",
                                          str(path.resolve()), str(output), "1"])
                with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                    archive.writestr("blobs/data", b"small exact fixture")
                return subprocess.CompletedProcess(command, 0, "{}", "")

            # Reproduce the same unchanged file reporting different change
            # times through path stat and handle stat on Windows.
            real_stat = Path.stat

            def path_stat(item, *args, **kwargs):
                result = real_stat(item, *args, **kwargs)
                if item == path:
                    fields = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns")
                    return mock.Mock(st_ctime_ns=result.st_ctime_ns - 1_000_000,
                                     **{name: getattr(result, name) for name in fields})
                return result

            with (mock.patch.dict(os.environ, PLACES_COMPILER=str(reader)),
                  mock.patch.object(Path, "stat", path_stat),
                  mock.patch("subprocess.run", side_effect=decode) as decoder):
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
                    output = Path(command[4]);outputs.append(output)
                    if changed:
                        with zipfile.ZipFile(output, "w") as archive:
                            archive.writestr("blobs/data", b"substituted")
                    else:
                        output.write_bytes(b"partial")
                    return subprocess.CompletedProcess(command, 0 if changed else 1, "", "window bound refused")

                with mock.patch.dict(os.environ, PLACES_COMPILER=str(reader)), mock.patch("subprocess.run", side_effect=fail):
                    with PackageZip(path) as archive:
                        with self.assertRaises(zipfile.BadZipFile):
                            archive.read("blobs/data")
                    self.assertFalse(outputs[0].parent.exists(), "failure cannot retain a task-owned temp")

    def test_alternate_zip_encoding_and_late_record_mismatch(self):
        payload = b"x" * (BLOCK_BYTES + 7)
        manifest = json.dumps(dict(entries=[dict(name="record", role="fixture")]))
        with tempfile.TemporaryDirectory() as temporary:
            left = Path(temporary) / "stored.zip"
            right = Path(temporary) / "deflated.zip"

            def write(path, mode, record):
                with zipfile.ZipFile(path, "w", compression=mode) as archive:
                    archive.writestr("manifest.json", manifest)
                    archive.writestr("record", record)

            write(left, zipfile.ZIP_STORED, payload)
            write(right, zipfile.ZIP_DEFLATED, payload)
            report = compare(left, right)
            self.assertFalse(report["archive_byte_equal"])
            self.assertTrue(report["quality_equal"])
            self.assertEqual(report["roles"]["fixture"], dict(bytes=len(payload), records=1, equal=1))

            changed = payload[:-1] + b"y"
            write(right, zipfile.ZIP_DEFLATED, changed)
            report = compare(left, right)
            self.assertFalse(report["quality_equal"])
            self.assertEqual(report["differing_records"], [dict(
                name="record", role="fixture", before_sha256=hashlib.sha256(payload).hexdigest(),
                after_sha256=hashlib.sha256(changed).hexdigest())])

    def test_comparison_rejects_identity_change_after_legacy_reads(self):
        with tempfile.TemporaryDirectory() as temporary:
            paths = [Path(temporary) / name for name in ("before.zip", "after.zip")]
            for path in paths:
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr(zipfile.ZipInfo("manifest.json"), '{"entries": []}')

            report = compare(*paths)
            self.assertTrue(report["archive_byte_equal"])
            self.assertTrue(report["quality_equal"])

            real_fstat = os.fstat
            for side in (0, 1):
                with self.subTest(archive=paths[side].name):
                    changed_fd = None

                    def read_record(left, right, name):
                        nonlocal changed_fd
                        record = compare_record(left, right, name)
                        changed_fd = (left, right)[side].fp.fileno()
                        return record

                    def fstat(fd):
                        stat = real_fstat(fd)
                        if fd == changed_fd:
                            return mock.Mock(wraps=stat, st_size=stat.st_size + 1)
                        return stat

                    with (mock.patch("release.compare_packages.compare_record", side_effect=read_record),
                          mock.patch.object(os, "fstat", side_effect=fstat)):
                        with self.assertRaisesRegex(zipfile.BadZipFile, "package changed since opening"):
                            compare(*paths)
