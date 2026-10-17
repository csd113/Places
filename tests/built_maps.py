"""Explicit compiled-map acceptance; first run sh tools/build-maps.sh."""
import os
from pathlib import Path
import unittest
import hashlib
import json
import zipfile

ROOT = Path(__file__).resolve().parents[1]

def selected_sources():
    """Explicit CI shards retain the same assertions for every selected map."""
    identities = (ROOT / 'levels/lineup.txt').read_text().splitlines()
    selected = os.environ.get('PLACES_PACKAGE_TEST_IDS')
    requested = identities if selected is None else selected.split(',')
    if not requested or len(requested) != len(set(requested)) or any(
            identity not in identities for identity in requested):
        raise ValueError('PLACES_PACKAGE_TEST_IDS must name unique curated map IDs')
    return [ROOT / 'levels' / (identity + '.json') for identity in requested]

def package_files():
    return sorted(Path(os.environ.get('PLACES_PACKAGE_TEST_ROOT', ROOT / 'assets/levels')).glob('*.placesmap'))

class BuiltMapTests(unittest.TestCase):
    def test_the_bundled_packages_are_present_valid_and_content_addressed(self):
        packages = {path.stem: path for path in package_files()}
        self.assertEqual(
            set(packages),
            {source.stem for source in selected_sources()},
            "each bundled level has a compiled package",
        )
        for stem, path in packages.items():
            with zipfile.ZipFile(path) as archive:
                names = archive.namelist()
                self.assertIn("manifest.json", names, f"{stem} has a manifest")
                manifest = json.loads(archive.read("manifest.json"))
                self.assertEqual(manifest["package_format"], 1, stem)
                self.assertEqual(manifest["id"], stem, stem)
                self.assertTrue(manifest["dependencies"], f"{stem} declares dependencies")
                self.assertTrue(manifest["variants"], f"{stem} has variants")
                variant_qualities = {v["lightmap_quality"] for v in manifest["variants"]}
                self.assertEqual(
                    variant_qualities, {"off", "full"}, stem
                )
                semantics = json.loads(archive.read("semantics.json"))
                self.assertEqual(semantics["id"], stem, stem)
                self.assertEqual(semantics["format_version"], 1, stem)
                # Content addressing: every declared entry's SHA-256 matches its
                # bytes, and every blob name embeds that same hash.
                for entry in manifest["entries"]:
                    data = archive.read(entry["name"])
                    digest = hashlib.sha256(data).hexdigest()
                    self.assertEqual(digest, entry["sha256"], entry["name"])
                    self.assertEqual(len(data), entry["bytes"], entry["name"])
                    if entry["name"].startswith("blobs/"):
                        self.assertEqual(entry["name"].split("/", 1)[1].split(".", 1)[0], digest)
                for entry_name in names:
                    if entry_name == "manifest.json":
                        continue
                    self.assertIn(
                        entry_name,
                        {entry["name"] for entry in manifest["entries"]},
                        f"{stem} declares every archive entry",
                    )
                # The player never compiles: each package must carry the
                # prepared payloads for every declared variant.
                for variant in manifest["variants"]:
                    for role in ("mesh", "props", "lighting", "collision", "navigation"):
                        self.assertIn(variant["entries"][role], names, f"{stem} {role}")
                    if variant["lightmap_quality"] != "off":
                        self.assertIn(variant["entries"]["lightmaps"], names, stem)
                    if "probes-rgba8" in manifest["required_capabilities"]:
                        self.assertTrue(
                            variant["entries"]["probes"], f"{stem} probe payload"
                        )
                    for payload in variant["entries"]["probes"]:
                        self.assertEqual(payload["count"], len(payload["cubemaps"]), stem)
                        for cubemap in payload["cubemaps"]:
                            self.assertIn(cubemap, names, stem)
                            self.assertEqual(
                                int(cubemap.split("/", 1)[1].split(".", 1)[0], 16),
                                int(hashlib.sha256(archive.read(cubemap)).hexdigest(), 16),
                                cubemap,
                            )
