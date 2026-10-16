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
import preview
from repair_geometry import compact, orient, remove_pool_table_cap, should_close


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


    def test_reader_retains_all_texture_slots_and_triangle_materials(self):
        # The immutable three-image export is a deliberate reader fixture;
        # the adopted rig shares one image across six material primitives.
        path = ROOT / "tools/props/sources/spooner_man_original.glb"
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


# Four tiny deterministic controls preserve winding, cavity and attribute-seam
# failures; they were previously outside normal test discovery. No asset export.
class PropGeometryRepairTests(unittest.TestCase):
    def test_float32_planar_screen_keeps_front_orientation(self):
        positions = [(-.51, .155, .03999999910593033),
                     (.51, .155, .03999999910593033),
                     (.51, .67, .03999999910593033),
                     (-.51, .67, .03999999910593033)]
        indices = [0, 1, 2, 0, 2, 3]
        report = geometry.inspect(positions, indices, repair=True)
        self.assertEqual(report['flipped_triangles'], 0)
        self.assertEqual(indices, [0, 1, 2, 0, 2, 3])

    def test_recessed_washer_back_faces_the_opening(self):
        positions = [(0, .44, -.02), (.155, .44, -.02), (0, .595, -.02)]
        indices = [0, 2, 1]
        orient('washing_machine', positions, indices)
        self.assertEqual(indices, [0, 1, 2])
        self.assertEqual(orient('washing_machine', positions, indices)['flipped_triangles'], 0)

    def test_inside_out_closed_tetrahedron_is_repaired(self):
        positions = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1)]
        indices = [0, 1, 2, 0, 3, 1, 0, 2, 3, 1, 3, 2]
        report = geometry.inspect(positions, indices, repair=True)
        self.assertEqual(report['flipped_triangles'], 4)
        self.assertEqual(geometry.inspect(positions, indices)['flipped_triangles'], 0)

    def test_compaction_preserves_uv_and_color_seams(self):
        positions = [(0, 0, 0)] * 4
        uvs = [(0, 0), (1, 0), (0, 0), (0, 0)]
        colors = [(1, 1, 1, 1)] * 3 + [(.5, .5, .5, 1)]
        packed = compact(positions, uvs, colors, [0, 1, 2, 3])
        self.assertEqual(len(packed[0]), 3)
        self.assertEqual(packed[3], [0, 1, 0, 2])


if __name__ == "__main__":
    unittest.main()
