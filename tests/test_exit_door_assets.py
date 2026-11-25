"""Current exit-door art and runtime presets must match the reviewed exports.

Read the two shipped GLBs and the committed source identities. This catches
a rig or preset silently drifting behind a successful generic import, with
no Blender, native launch, or level bake.
"""
import hashlib
import json
from pathlib import Path
import struct
import unittest

from assets.inspection.glb import read_glb_chunks

ASSETS = Path(__file__).resolve().parents[1] / "assets"
RUNTIME = ASSETS / "shared/doors"
REFERENCE = ASSETS / "references/doors/hollow-metal-exit"
FAMILY = "hollow_metal_exit_door_"


def document(path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(data):
    return hashlib.sha256(data).hexdigest()


class ExitDoorAssetTests(unittest.TestCase):
    def test_editable_masters_and_runtime_exports_match_reviewed_identities(self):
        expected = {
            "source/exit_door_materials.xcf", "source/exit_door_materials_master_1024.png",
            "textures/exit_door_materials.png",
            *(f"source/{FAMILY}{side}.blend" for side in ("single", "double")),
            *(f"runtime/{FAMILY}{side}.glb" for side in ("single", "double")),
        }
        identities = document(REFERENCE / "metadata/artifact_identities.json")["files"]
        # The unchanged ledger also records preparation-script provenance.
        # Only editable artwork and shipped exports are live asset inputs.
        rows = {row["path"]: row for row in identities if not row["path"].endswith(".py")}
        self.assertEqual(set(rows), expected)
        for relative, row in rows.items():
            name = Path(relative)
            path = REFERENCE / name if name.parts[0] == "source" else RUNTIME / name.name
            with self.subTest(artifact=relative):
                data = path.read_bytes()
                self.assertEqual(len(data), row["bytes"])
                self.assertEqual(digest(data), row["sha256"])

    def test_shipped_rigid_doors_keep_the_reviewed_clip_and_collision_contacts(self):
        reports = {row["variant"]: row
                   for row in document(REFERENCE / "metadata/asset_gate_report.json")["models"]}
        catalog = {asset["id"]: asset for asset in document(ASSETS / "catalog.json")["assets"]}
        completed = document(ASSETS / "completed-models.json")
        for variant, triangle_count, box_count, contacts in (("single", 728, 9, 3), ("double", 1268, 15, 4)):
            with self.subTest(variant=variant):
                name = FAMILY + variant
                data = (RUNTIME / (name + ".glb")).read_bytes()
                self.assertEqual(struct.unpack_from("<4sII", data), (b"glTF", 2, len(data)))
                self.assertEqual(struct.unpack_from("<4s", data, 16), (b"JSON",))
                glb, _ = read_glb_chunks(data)
                self.assertEqual(digest(data), reports[variant]["sha256"])
                total = sum(glb["accessors"][primitive["indices"]]["count"] // 3
                            for mesh in glb["meshes"] for primitive in mesh["primitives"])
                self.assertEqual(total, triangle_count)
                self.assertFalse(glb.get("skins"))
                self.assertEqual(len(glb["images"]), 1)
                self.assertEqual(len(glb["materials"]), 2)
                self.assertTrue(all(m.get("alphaMode", "OPAQUE") == "OPAQUE" and not m.get("doubleSided", False)
                                    for m in glb["materials"]))
                self.assertEqual([clip["name"] for clip in glb["animations"]], ["exit_open"])
                nodes = {node["name"] for node in glb["nodes"]}
                preset = document(RUNTIME / (name + ".json"))
                self.assertIs(preset["solid"], False)
                self.assertIs(preset["occludes"], False)
                boxes = preset["animated_door"]["boxes"]
                self.assertEqual((len(boxes), sum(box["grounding"] for box in boxes)), (box_count, contacts))
                self.assertTrue(all(box["node"] in nodes and not box["node"].endswith("_hinges") for box in boxes))
                self.assertEqual(preset["bindings"], [{"on": "interact", "actions": [{"action": "toggle"}]}])
                self.assertEqual(preset["animated_door"]["clip"], "exit_open")
                source = f"shared/doors/{name}.glb"
                self.assertEqual(catalog["core:" + name]["model"], source)
                self.assertIs(catalog["core:" + name]["solid"], False)
                self.assertEqual(completed[source]["sha256"], digest(data))
                self.assertEqual(completed[source]["triangles"], triangle_count)
