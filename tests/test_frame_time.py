"""Qualify package identity and recovery before recording native timings."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "frame_time", ROOT / "projects/performance-lab/acceptance/frame_time.py"
)
FRAME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FRAME)


class FrameTimeIdentityTests(unittest.TestCase):
    # Prevent acceptance of another discovered package/member: two tiny ZIPs and
    # short log strings add unique project/runtime identity coverage, no bake/GPU.
    # A sub-KiB mocked codec boundary protects older Python installations and
    # invalid reader receipts without baking maps, loading a GPU or a codec.
    def test_zstd_metadata_uses_bounded_rust_reader_and_checks_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "new-format.placesmap"
            with zipfile.ZipFile(package, "w") as archive:
                archive.writestr("manifest.json", b'{"id":"member"}')
            identity = "a" * 64
            with mock.patch.object(zipfile.ZipFile, "read", side_effect=NotImplementedError), \
                 mock.patch.object(FRAME.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, identity + "\n", "")) as invoke:
                self.assertEqual(FRAME.package_manifest_identity(package, "member", Path("bounded-reader")), identity)
                self.assertEqual(invoke.call_args.args[0], ["bounded-reader", "--debug", "manifest-identity", str(package), "member"])
                invoke.return_value.stdout = "wrong member receipt"
                with self.assertRaises(ValueError):
                    FRAME.package_manifest_identity(package, "member", Path("bounded-reader"))
                info = zipfile.ZipInfo("project.json")
                info.compress_type = 93
                invoke.return_value.stdout = identity + "\n"
                with mock.patch.object(zipfile.ZipFile, "infolist", return_value=[info]), \
                     mock.patch.object(zipfile.ZipFile, "read", side_effect=AssertionError("must use bounded Rust")):
                    self.assertEqual(FRAME.package_manifest_identity(package, "member", Path("bounded-reader")), identity)

    def test_project_member_uses_same_identity_as_single_map_manifest(self):
        raw = b'{"id":"member","variants":[]}\n'
        with tempfile.TemporaryDirectory() as directory:
            single = Path(directory) / "single.placesmap"
            project = Path(directory) / "project.placesmap"
            with zipfile.ZipFile(single, "w") as archive:
                archive.writestr("manifest.json", raw)
            with zipfile.ZipFile(project, "w") as archive:
                archive.writestr("project.json", json.dumps(dict(levels=[
                    dict(id="member", records={"manifest.json": "blobs/member.record"})])))
                archive.writestr("blobs/member.record", raw)
            expected = hashlib.sha256(raw).hexdigest()
            self.assertEqual(FRAME.package_manifest_identity(single, "member"), expected)
            self.assertEqual(FRAME.package_manifest_identity(project, "member"), expected)
            for package in (single, project):
                with self.assertRaises(ValueError):
                    FRAME.package_manifest_identity(package, "another")

    def test_runtime_must_load_planned_member_content_and_unscripted_variant(self):
        digest = "a" * 64
        def line(level="member", variant="full", sha=digest):
            return f"[loading] package identity level={level} variant={variant} sha256={sha} source=/map\n"
        FRAME.verify_runtime_identity(line(), "member", "full", digest)
        FRAME.verify_runtime_identity(line() + line(variant="off"), "member", None, digest)
        for log in ("", line(level="another"), line(sha="b" * 64), line(variant="off")):
            with self.assertRaises(RuntimeError):
                FRAME.verify_runtime_identity(log, "member", "full", digest)
        with self.assertRaises(RuntimeError):
            FRAME.verify_runtime_identity(line(sha="b" * 64), "member", None, digest)

    def test_unrecoverable_spawn_cannot_qualify_native_timings(self):
        # The Home wall-overlap recipe rendered nonzero geometry while falling
        # below the map. One warning string protects timing qualification;
        # no executable, assets, bake or wall-clock assertion is needed.
        FRAME.verify_player_recovery("[loading] committed home\n[bench] 15 draws")
        FRAME.verify_player_recovery("[script] Help: escaped map has no clear spawn destination")
        with self.assertRaisesRegex(RuntimeError, "correct the benchmark spawn"):
            FRAME.verify_player_recovery("[bench] 15 draws\n[movement] escaped map has no clear spawn destination; correct the level spawn")
