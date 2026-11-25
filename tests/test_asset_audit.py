"""Read-only asset inspection guards exported geometry and animation contracts."""
from pathlib import Path
import math
import struct
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
from assets.inspection import glb
from assets.inspection.animation import animation_record, inspect_rigid_frames
from assets.inspection.mesh_checks import coincident_quads, non_finite_attributes


class AssetAuditTests(unittest.TestCase):
    def test_opposite_quad_diagonals_are_detected(self):
        model = glb.ReadMesh()
        model.positions = [(-.395, .728, -.395), (-.395, .728, .395),
                           (.395, .728, .395), (.395, .728, -.395)]
        model.indices = [0, 1, 2, 0, 2, 3, 1, 3, 2, 1, 0, 3]
        overlaps = coincident_quads(model)
        self.assertEqual(len(overlaps), 1)
        self.assertTrue(overlaps[0]["opposite_facing"])
        self.assertEqual(coincident_quads(model), overlaps, "inspection must not change the triangles")
        model.indices = model.indices[:6]
        self.assertEqual(coincident_quads(model), [])


    def test_reader_retains_all_texture_slots_and_triangle_materials(self):
        # The immutable three-image export is a deliberate reader fixture;
        # the adopted rig shares one image across six material primitives.
        path = ROOT / "assets/references/entities/spooner-man/spooner_man_original.glb"
        mesh = glb.read_glb(path.read_bytes())
        self.assertEqual(len(mesh.texture_pngs), 3)
        self.assertEqual(len(mesh.triangle_materials), mesh.triangle_count)
        self.assertEqual(set(mesh.triangle_materials), {0, 1, 2})

    def test_audit_rejects_non_finite_geometry(self):
        triangle = glb.ReadMesh()
        triangle.positions = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        triangle.indices = [0, 1, 2]
        self.assertEqual(non_finite_attributes(triangle), [])
        triangle.positions[0] = (math.nan, 0, 0)
        self.assertIn("non-finite positions", non_finite_attributes(triangle))

    def test_animation_rejects_non_increasing_times_and_bad_quaternions(self):
        document = {"nodes": [{}], "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"}],
            "bufferViews": [{"byteOffset": 0, "byteLength": 8}, {"byteOffset": 8, "byteLength": 32}]}
        binary = struct.pack("<10f", 0, 0, 0, 0, 0, 2, 0, 0, 0, 2)
        animation = {"name": "bad", "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}],
                     "samplers": [{"input": 0, "output": 1}]}
        errors = animation_record(document, binary, animation)["errors"]
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
        binary = struct.pack("<13f", 0, 0.1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0)
        animation = {"channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}],
                     "samplers": [{"input": 0, "output": 1}]}
        points = []
        transform = glb._transform_point
        def record_point(matrix, point):
            result = transform(matrix, point)
            points.append(result)
            return result
        with patch.object(glb, "_transform_point", side_effect=record_point):
            frames = inspect_rigid_frames(document, binary, animation, 0.1)
        self.assertEqual(frames, 7)
        self.assertEqual(points[0], (1, 0, 0))
        self.assertAlmostEqual(points[-1][0], -1)
        for point in points:
            self.assertAlmostEqual(sum(value*value for value in point), 1)

    def test_short_step_track_holds_its_last_key_during_a_longer_linear_track(self):
        document = {"nodes": [{}], "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR"},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3"}],
            "bufferViews": [{"byteOffset": 0, "byteLength": 12},
                            {"byteOffset": 12, "byteLength": 36},
                            {"byteOffset": 48, "byteLength": 8},
                            {"byteOffset": 56, "byteLength": 24}]}
        binary = struct.pack("<20f",
                                   0.125, 0.25, 0.5, 2, 0, 0, 4, 0, 0, 8, 0, 0,
                                   0, 0.75, 1, 1, 1, 4, 4, 4)
        animation = {"channels": [{"sampler": 0, "target": {"node": 0, "path": "translation"}},
                                  {"sampler": 1, "target": {"node": 0, "path": "scale"}}],
                     "samplers": [{"input": 0, "output": 1, "interpolation": "STEP"},
                                  {"input": 2, "output": 3}]}
        record = animation_record(document, binary, animation)
        self.assertEqual(record["errors"], [])
        self.assertEqual(record["duration"], 0.75)
        with patch.object(glb, "_node_rest_matrices", wraps=glb._node_rest_matrices) as matrices:
            frames = inspect_rigid_frames(document, binary, animation, record["duration"])

        self.assertEqual(frames, 46)
        nodes = [call.args[0]["nodes"][0] for call in matrices.call_args_list]
        self.assertEqual([node["scale"][0] for node in nodes[::15]], [1, 2, 3, 4])
        self.assertEqual([node["translation"][0] for node in nodes[:31]], [2]*15 + [4]*15 + [8])
        self.assertEqual([node["translation"][0] for node in nodes[31:]], [8]*15)

if __name__ == "__main__":
    unittest.main()
