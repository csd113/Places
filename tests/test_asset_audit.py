"""Asset audit and inspection previews protect actual exported data contracts."""
from pathlib import Path
import math
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/assets"))
sys.path.insert(0, str(ROOT / "tools/props"))
import audit
import geometry
import glb
from mesh import PropBuilder
from parts.pool import build_hot_tub, HOT_TUB_OUTER_R, HOT_TUB_WALL_R
import preview
from repair_geometry import remove_pool_table_cap, should_close


class AssetAuditTests(unittest.TestCase):
    def test_opposite_quad_diagonals_are_detected_and_table_cap_is_not_recreated(self):
        model = glb.ReadMesh()
        model.positions = [(-.395, .728, -.395), (-.395, .728, .395),
                           (.395, .728, .395), (.395, .728, -.395)]
        model.indices = [0, 1, 2, 0, 2, 3, 1, 3, 2, 1, 0, 3]
        overlaps = audit.coincident_quads(model)
        self.assertEqual(len(overlaps), 1)
        self.assertTrue(overlaps[0]["opposite_facing"])
        self.assertEqual(remove_pool_table_cap(model.positions, model.indices), 2)
        self.assertEqual(model.indices, [0, 1, 2, 0, 2, 3])
        self.assertEqual(audit.coincident_quads(model), [])
        self.assertEqual(remove_pool_table_cap(model.positions, model.indices), 0)
        self.assertFalse(should_close("pool_table", model.positions))
        self.assertTrue(should_close("pool_table", [(x, .7, z) for x, _, z in model.positions]))

    def test_hot_tub_faces_the_basin_and_exterior_and_keeps_square_tiles(self):
        p = PropBuilder("core:hot_tub", "Hot Tub", (2.6, 1.56, 2.6))
        build_hot_tub(p)
        topology = geometry.inspect(p.mesh.positions, p.mesh.indices)
        self.assertEqual(topology["inconsistent_edges"], 0)
        self.assertEqual(topology["degenerate"], 0)
        wall_count = 0
        u0, v0, u1, v1 = p.tex.uv("tile", inset=1)
        for offset in range(0, len(p.mesh.indices), 3):
            ids = p.mesh.indices[offset:offset+3]
            a, b, c = [p.mesh.positions[i] for i in ids]
            normal = geometry._cross(geometry._sub(b, a), geometry._sub(c, a))
            if abs(normal[1]) > 1e-8:
                self.assertGreater(normal[1], 0, "both horizontal rim surfaces face up")
                continue
            wall_count += 1
            radius = math.hypot(a[0], a[2])
            outward = normal[0]*a[0]+normal[2]*a[2]
            if abs(radius-HOT_TUB_WALL_R) < 1e-5:
                self.assertLess(outward, 0, "basin normal faces the water")
            else:
                self.assertAlmostEqual(radius, HOT_TUB_OUTER_R)
                self.assertGreater(outward, 0, "outer wall normal faces the room")
            uvs = [p.mesh.uvs[i] for i in ids]
            du = max(u for u, _ in uvs)-min(u for u, _ in uvs)
            dv = max(v for _, v in uvs)-min(v for _, v in uvs)
            # The fitted atlas region paints eight cells on each axis. Its
            # rectangle is not square in pixels; compare metres per painted
            # cell, not metres per pixel.
            points = [a, b, c]
            horizontal = max(math.hypot(x[0]-y[0], x[2]-y[2]) for x in points for y in points)
            vertical = max(v[1] for v in points)-min(v[1] for v in points)
            ratio = (horizontal / (du/(u1-u0)*8)) / (vertical / (dv/(v1-v0)*8))
            self.assertGreater(ratio, 0.9)
            self.assertLess(ratio, 1.2)
        self.assertGreater(wall_count, 0)

    def test_reader_retains_all_texture_slots_and_triangle_materials(self):
        path = ROOT / "assets/entities/spooner-man/model/spooner-man.glb"
        mesh = glb.read_glb(path.read_bytes())
        self.assertEqual(len(mesh.texture_pngs), 3)
        self.assertEqual(len(mesh.triangle_materials), mesh.triangle_count)
        self.assertEqual(set(mesh.triangle_materials), {0, 1, 2})

    def test_audit_rejects_non_finite_geometry(self):
        p = PropBuilder("test:box", "Box", (1, 1, 1))
        p.set_texture(32)
        p.box((0, 0.5, 0), (1, 1, 1), uv=(0, 0, 1, 1))
        p.mesh.positions[0] = (math.nan, 0, 0)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "box.glb"
            path.write_bytes(glb.write_glb(p.mesh, p.tex.png_bytes()))
            record = audit.inspect_model(path)
        self.assertIn("non-finite positions", record["errors"])

    def test_animation_rejects_non_increasing_times_and_bad_quaternions(self):
        document = {"nodes": [{}], "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"}],
            "bufferViews": [{"byteOffset": 0, "byteLength": 8}, {"byteOffset": 8, "byteLength": 32}]}
        binary = audit.struct.pack("<10f", 0, 0, 0, 0, 0, 2, 0, 0, 0, 2)
        animation = {"name": "bad", "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}],
                     "samplers": [{"input": 0, "output": 1}]}
        errors = audit.animation_record(document, binary, animation)["errors"]
        self.assertIn("animation times must be non-negative and strictly increasing", errors)
        self.assertIn("animation quaternion is not unit length", errors)

    def test_rigid_rotation_sweep_keeps_radius_and_samples_exact_endpoints(self):
        document = {"nodes": [{"mesh": 0}], "meshes": [{"primitives": [
            {"attributes": {"POSITION": 2}}]}], "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"},
            {"bufferView": 2, "componentType": 5126, "count": 1, "type": "VEC3"}],
            "bufferViews": [{"byteOffset": 0, "byteLength": 8},
                            {"byteOffset": 8, "byteLength": 32},
                            {"byteOffset": 40, "byteLength": 12}]}
        binary = audit.struct.pack("<13f", 0, 0.1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0)
        animation = {"channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}],
                     "samplers": [{"input": 0, "output": 1}]}
        points = []
        transform = glb._transform_point
        def record_point(matrix, point):
            result = transform(matrix, point)
            points.append(result)
            return result
        with patch.object(glb, "_transform_point", side_effect=record_point):
            frames = audit.inspect_rigid_frames(document, binary, animation, 0.1)
        self.assertEqual(frames, 7)
        self.assertEqual(points[0], (1, 0, 0))
        self.assertAlmostEqual(points[-1][0], -1)
        for point in points:
            self.assertAlmostEqual(sum(value*value for value in point), 1)

    def test_preview_cutout_preserves_background_and_uses_each_material_texture(self):
        model = preview.Model()
        model.positions = [(-1, 0, 0), (0, 0, 0), (-0.5, 1, 0),
                           (0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
        model.uvs = [(0.5, 0.5)]*6
        model.colors = [(1, 1, 1, 1)]*6
        model.indices = list(range(6))
        model.triangle_materials = [0, 1]
        model.textures = [(1, 1, bytes((255, 0, 0, 255))), (1, 1, bytes((0, 0, 255, 0)))]
        model.materials = [
            {"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}},
            {"alphaMode": "MASK", "pbrMetallicRoughness": {"baseColorTexture": {"index": 1}}}]
        data = preview.render(model, 100, 100, direction=(0, 0, 1), ground=False)
        colors = set(zip(data[0::4], data[1::4], data[2::4]))
        self.assertIn((255, 0, 0), colors)
        self.assertNotIn((0, 0, 255), colors)
        model.textures[1] = (1, 1, bytes((0, 0, 255, 255)))
        data = preview.render(model, 100, 100, direction=(0, 0, 1), ground=False)
        self.assertIn((0, 0, 255), set(zip(data[0::4], data[1::4], data[2::4])))


if __name__ == "__main__":
    unittest.main()
