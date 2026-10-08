"""Scalar material authoring contracts; no generated repository assets."""
import json
import struct
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/props"))

from glb import write_glb  # noqa: E402
from mesh import Mesh  # noqa: E402


def document(mesh):
    mesh.box((0, 0, 0), (1, 1, 1), uv=(0, 0, 1, 1))
    data = write_glb(mesh, (ROOT / "assets/core/textures/white_01.png").read_bytes())
    length = struct.unpack_from("<I", data, 12)[0]
    return json.loads(data[20:20 + length])


class ScalarMaterialTests(unittest.TestCase):
    def test_legacy_toolkit_defaults_stay_matte(self):
        pbr = document(Mesh())["materials"][0]["pbrMetallicRoughness"]
        self.assertEqual((pbr["roughnessFactor"], pbr["metallicFactor"]), (1.0, 0.0))

    def test_single_material_preserves_authored_scalars(self):
        mesh = Mesh()
        mesh.roughness, mesh.metallic = 0.25, 0.8
        pbr = document(mesh)["materials"][0]["pbrMetallicRoughness"]
        self.assertEqual((pbr["roughnessFactor"], pbr["metallicFactor"]), (0.25, 0.8))

    def test_material_slots_preserve_controls_and_distinct_identity(self):
        mesh = Mesh()
        first = mesh.material("wood", roughness=0.3)
        self.assertEqual(first, mesh.material("wood", roughness=0.3))
        with self.assertRaises(ValueError):
            mesh.material("wood", roughness=0.8)
        self.assertEqual(mesh.materials[first]["roughness"], 0.3)

    def test_invalid_slot_controls_are_rejected(self):
        for roughness, metallic in [(-0.1, 0), (1, 1.1), (float("nan"), 0)]:
            with self.subTest(roughness=roughness, metallic=metallic):
                with self.assertRaises(ValueError):
                    Mesh().material("invalid", roughness=roughness, metallic=metallic)


if __name__ == "__main__":
    unittest.main()
