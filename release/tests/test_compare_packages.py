"""Compare prepared map records without running a benchmark or baking a map."""
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

from release import compare_packages as COMPARE


class PackageComparisonQualityTests(unittest.TestCase):
    def test_package_comparison_rejects_any_lighting_or_metadata_difference(self):
        with tempfile.TemporaryDirectory() as directory:
            def package(name, stage="v1", title="Room", lighting=b"exact HDR bytes"):
                path = Path(directory) / name
                manifest = dict(name=title, lighting_fingerprint=stage,
                                entries=[dict(name="blobs/atlas.lightmaps", role="lightmaps")])
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("manifest.json", json.dumps(manifest))
                    archive.writestr("blobs/atlas.lightmaps", lighting)
                return path
            original = package("before.zip")
            same = package("same.zip")
            self.assertTrue(COMPARE.compare(original, same)["quality_equal"])
            stage_edit = package("stage.zip", stage="v2")
            self.assertFalse(COMPARE.compare(original, stage_edit)["quality_equal"])
            self.assertTrue(COMPARE.compare(original, stage_edit, True)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("title.zip", title="Other"), True)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("lighting.zip", lighting=b"wrong HDR bytes"), True)["quality_equal"])

    def test_input_key_exception_verifies_only_new_texture_identities(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "normal.png"
            image.write_bytes(b"existing input bytes; never decoded by the comparator")
            import hashlib
            dependency = dict(kind="texture", path=image.name, bytes=image.stat().st_size,
                              sha256=hashlib.sha256(image.read_bytes()).hexdigest())
            def package(filename, dependencies, **changes):
                path = root / filename
                value = dict(compiler_fingerprint="v1", lighting_fingerprint="stage1",
                             dependencies=dependencies, entries=[], name="Test")
                value.update(changes)
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("manifest.json", json.dumps(value))
                return path
            original = package("old.zip", [])
            updated = package("new.zip", [dependency], compiler_fingerprint="v2", lighting_fingerprint="stage2")
            self.assertFalse(COMPARE.compare(original, updated, True)["quality_equal"])
            self.assertTrue(COMPARE.compare(original, updated, asset_root=root)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("name.zip", [dependency], name="Changed"), asset_root=root)["quality_equal"])
            image.write_bytes(b"corrupt")
            self.assertFalse(COMPARE.compare(original, updated, asset_root=root)["quality_equal"])
