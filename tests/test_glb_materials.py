"""Scalar material authoring contracts; no generated repository assets."""
import json
import hashlib
import struct
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/props"))

from glb import GltfError, read_glb, write_glb  # noqa: E402
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


class BlendMaterialTests(unittest.TestCase):
    def test_pre_blend_exports_retain_exact_bytes(self):
        expected = {
            None: "a3309e3260c06ca495be1ce05aa6ce46812843886c3135f15141308c079a6578",
            "opaque": "0f2ab87aa6f69e1450105a0aff9e3345fe6f44f85276cd14125f271e929662e6",
            "mask": "f0b94d03862f39b057f354c61ec2ee9b382d0bbd53ab9328ebb9499fe3edd4b7",
        }
        texture = (ROOT / "assets/core/textures/white_01.png").read_bytes()
        for mode, digest in expected.items():
            with self.subTest(mode=mode):
                mesh = Mesh()
                if mode is not None:
                    mesh.material("baseline", alpha_mode=mode, color=(120, 200, 250), roughness=0.25)
                mesh.box((0, 0, 0), (1, 1, 1), uv=(0, 0, 1, 1))
                self.assertEqual(hashlib.sha256(write_glb(mesh, texture, "baseline")).hexdigest(), digest)

    def test_blend_factor_preserves_color_and_scalar_opacity(self):
        for color, use_texture in [(None, True), ((128, 220, 250), True), (None, False)]:
            with self.subTest(color=color, use_texture=use_texture):
                mesh = Mesh()
                mesh.material("glass", alpha_mode="blend", opacity=0.28, color=color,
                              use_texture=use_texture, roughness=0.18)
                material = document(mesh)["materials"][0]
                self.assertEqual(material["alphaMode"], "BLEND")
                pbr = material["pbrMetallicRoughness"]
                rgb = [1.0] * 3 if color is None else [round(c / 255, 6) for c in color]
                self.assertEqual(pbr["baseColorFactor"], rgb + [0.28])
                self.assertEqual("baseColorTexture" in pbr, use_texture)
                self.assertNotIn("alphaCutoff", material)

    def test_mixed_slots_keep_opaque_identity_and_mask_cutoff(self):
        mesh = Mesh()
        for mode in ["opaque", "mask", "blend"]:
            slot = mesh.material(mode, alpha_mode=mode, alpha_cutoff=0.37,
                                 opacity=0.4 if mode == "blend" else 1.0)
            mesh.begin_material(slot)
            mesh.box((slot, 0, 0), (1, 1, 1), uv=(0, 0, 1, 1))
        data = write_glb(mesh, (ROOT / "assets/core/textures/white_01.png").read_bytes())
        length = struct.unpack_from("<I", data, 12)[0]
        doc = json.loads(data[20:20 + length])
        self.assertNotIn("alphaMode", doc["materials"][0])
        self.assertEqual(doc["materials"][1]["alphaCutoff"], 0.37)
        self.assertEqual(doc["materials"][2]["alphaMode"], "BLEND")
        self.assertEqual([p["material"] for p in doc["meshes"][0]["primitives"]], [0, 1, 2])

    def test_invalid_opacity_and_conflicting_slot_are_rejected(self):
        for opacity in [-0.1, 1.1, float("nan"), float("inf")]:
            with self.subTest(opacity=opacity):
                with self.assertRaises(ValueError):
                    Mesh().material("glass", alpha_mode="blend", opacity=opacity)
        for mode in ["opaque", "mask"]:
            with self.assertRaises(ValueError):
                Mesh().material("invalid", alpha_mode=mode, opacity=0.5)
        mesh = Mesh()
        slot = mesh.material("glass", alpha_mode="blend", opacity=0.3)
        self.assertEqual(slot, mesh.material("glass", alpha_mode="blend", opacity=0.3))
        with self.assertRaises(ValueError):
            mesh.material("glass", alpha_mode="blend", opacity=0.4)

    def test_raw_writer_materials_are_validated(self):
        mesh = Mesh()
        mesh.box((0, 0, 0), (1, 1, 1), uv=(0, 0, 1, 1))
        texture = (ROOT / "assets/core/textures/white_01.png").read_bytes()
        for opacity in [float("nan"), -1, 2]:
            with self.assertRaises(GltfError):
                write_glb(mesh, texture, materials=[{"alpha_mode": "blend", "opacity": opacity}])

    def test_embedded_texture_alpha_is_preserved(self):
        texture = (ROOT / "assets/core/textures/glass/glass_clear_01.png").read_bytes()
        mesh = Mesh()
        mesh.material("glass", alpha_mode="blend", opacity=0.35)
        mesh.box((0, 0, 0), (1, 1, 1), uv=(0, 0, 1, 1))
        self.assertEqual(read_glb(write_glb(mesh, texture)).texture_png, texture)


if __name__ == "__main__":
    unittest.main()
