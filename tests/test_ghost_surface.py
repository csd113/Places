"""Facial attachment contracts for the two shipped sheet ghosts."""
import copy
from collections import Counter
import math
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools" / "entities"))
import author_halloween_assets as assets
import ghost_surface
import rig
from validate_entities import Model


class GhostAttachmentTests(unittest.TestCase):
    def test_overlap_detects_a_cloth_ridge_between_feature_vertices(self):
        face = [(0, 0, .1), (1, 0, .1), (0, 1, .1)]
        cloth = [(.2, .2, .2), (.3, .2, .2), (.2, .3, .2)]
        overlap = ghost_surface.projected_overlap(face, cloth)
        self.assertEqual(len(overlap), 3)
        self.assertTrue(all(ghost_surface.triangle_height(face, x, y) <
                            ghost_surface.triangle_height(cloth, x, y) for x, y in overlap))

    def test_both_faces_have_surface_clearance_and_unchanged_rigid_attachment(self):
        for name in sorted(assets.HOVERING_ASSETS):
            with self.subTest(name=name):
                document, binary = assets.read_glb(ROOT / assets.ASSETS[name])
                count = ghost_surface.validate(document, binary, name, assets.accessor,
                                               assets.indices_of, assets.vertex_colours)
                self.assertEqual(count, 648)
                attributes = document["meshes"][0]["primitives"][0]["attributes"]
                normals, _, _ = assets.accessor(document, binary, attributes["NORMAL"])
                for normal in normals:
                    self.assertTrue(all(math.isfinite(v) for v in normal))
                    self.assertAlmostEqual(sum(v*v for v in normal), 1., delta=1e-5)
                self.assertEqual(assets.planned_bytes(name, ROOT / assets.ASSETS[name]),
                                 (ROOT / assets.ASSETS[name]).read_bytes())

    def test_a_recessed_face_is_rejected_even_when_its_attachment_marker_remains(self):
        for name in sorted(assets.HOVERING_ASSETS):
            with self.subTest(name=name):
                document, binary = assets.read_glb(ROOT / assets.ASSETS[name])
                document = copy.deepcopy(document)
                primitive = document["meshes"][0]["primitives"][0]
                index = primitive["attributes"]["POSITION"]
                positions, _, _ = assets.accessor(document, binary, index)
                colours = assets.vertex_colours(document, binary, primitive)
                changed = [(x, y, z - .5) if sum(c[:3])/3 < .35 else (x, y, z)
                           for (x, y, z), c in zip(positions, colours)]
                buffer = bytearray(binary)
                ghost_surface.write_values(document, buffer, index, changed)
                with self.assertRaisesRegex(ValueError, "recessed|touches"):
                    ghost_surface.validate(document, bytes(buffer), name, assets.accessor,
                                           assets.indices_of, assets.vertex_colours)

    def test_cat_face_partition_preserves_geometry_and_darkness_without_depth_writes(self):
        document, binary = assets.read_glb(ROOT / assets.ASSETS["sheet-ghost-cat"])
        body, face_primitive = document["meshes"][0]["primitives"]
        self.assertEqual(body["attributes"], face_primitive["attributes"])
        cloth_material, face_material = document["materials"]
        self.assertEqual(cloth_material["emissiveFactor"], assets.GHOST_EMISSIVE)
        human, _ = assets.read_glb(ROOT / assets.ASSETS["sheet-ghost"])
        self.assertEqual(face_material, human["materials"][1])
        self.assertTrue(all(f < c for f, c in zip(face_material["emissiveFactor"], cloth_material["emissiveFactor"])))
        self.assertEqual(cloth_material["alphaMode"], "BLEND")
        self.assertEqual(face_material["alphaMode"], "BLEND")
        colours = assets.vertex_colours(document, binary, body)
        face = {i for i, c in enumerate(colours) if sum(c[:3])/3 < .35}
        points, _, _ = assets.accessor(document, binary, body["attributes"]["POSITION"])
        indices = [i for primitive in (body, face_primitive) for i in assets.indices_of(document, binary, primitive)]
        triangles = [indices[i:i+3] for i in range(0, len(indices), 3)]
        features = [t for t in triangles if all(i in face for i in t)]
        cloth = [t for t in triangles if not any(i in face for i in t)]
        self.assertEqual(len(features), 216)
        # With no self-depth writes, the final covering triangle wins even
        # when it is behind the feature. Replay the actual projected geometry.
        def visible_centres(ordered):
            visible = 0
            for feature in features:
                centre = tuple(sum(points[i][k] for i in feature)/3 for k in (0, 1))
                final = None
                for triangle in ordered:
                    a, b, c = (points[i] for i in triangle)
                    cross = lambda p, q: (q[0]-p[0])*(centre[1]-p[1])-(q[1]-p[1])*(centre[0]-p[0])
                    area = (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
                    if abs(area) < 1e-12:
                        continue
                    signs = [cross(a, b), cross(b, c), cross(c, a)]
                    if min(signs) >= -1e-10 or max(signs) <= 1e-10:
                        final = triangle
                visible += final is not None and all(i in face for i in final)
            return visible
        self.assertLess(visible_centres(features+cloth), len(features))
        self.assertEqual(visible_centres(triangles), len(features))
        broken = copy.deepcopy(document)
        broken["meshes"][0]["primitives"].reverse()
        with self.assertRaisesRegex(ValueError, "cloth is drawn after"):
            ghost_surface.validate_face_order(broken, binary, assets.indices_of, assets.vertex_colours)
        # Recreate the single primitive source and split it again. The writer
        # appends indices, preserving every existing attribute/clip byte.
        source = copy.deepcopy(document)
        source["meshes"][0]["primitives"] = [copy.deepcopy(body)]
        source["materials"] = [copy.deepcopy(cloth_material)]
        source["accessors"][body["indices"]]["count"] = len(indices)
        before = copy.deepcopy(source)
        repaired_document, repaired = assets.split_ghost_face(source, binary, "sheet-ghost-cat")
        repaired_indices = [i for primitive in repaired_document["meshes"][0]["primitives"] for i in assets.indices_of(repaired_document, repaired, primitive)]
        self.assertEqual(Counter(map(tuple, triangles)), Counter(tuple(repaired_indices[i:i+3]) for i in range(0, len(repaired_indices), 3)))
        self.assertEqual(repaired[:len(binary)], binary)
        self.assertEqual(repaired_document["materials"][0], before["materials"][0])
        for key in ("animations", "skins", "nodes", "images", "textures"):
            self.assertEqual(repaired_document[key], before[key])
        for index, entry in enumerate(before["accessors"]):
            if index != body["indices"]:
                self.assertEqual(repaired_document["accessors"][index], entry)

    def test_cat_face_clears_the_original_animated_hem(self):
        path = ROOT / assets.ASSETS["sheet-ghost-cat"]
        document, binary = assets.read_glb(path)
        primitive = document["meshes"][0]["primitives"][0]
        colours = assets.vertex_colours(document, binary, primitive)
        face = {i for i, c in enumerate(colours) if sum(c[:3])/3 < .35}
        indices = [i for p in document["meshes"][0]["primitives"] for i in assets.indices_of(document, binary, p)]
        triangles = [indices[i:i+3] for i in range(0, len(indices), 3)]
        features = [t for t in triangles if all(i in face for i in t)]
        cloth = [t for t in triangles if not any(i in face for i in t)]
        model = Model(path)
        for clip in model.clips:
            for phase in (0, .25, .5, .75):
                globals_ = model._pose_globals(clip, clip["duration"]*phase)
                inverse = rig._mat_inverse(rig._mat_mul(globals_[model.joints[1]],
                                                       model.inverse_bind[1]))
                points = [rig.transform_point(inverse, p) for p in model._skin(globals_)]
                pairs = list(ghost_surface.support_pairs(points, features, cloth))
                self.assertTrue(pairs)
                for tri, support, overlap in pairs:
                    a, b = [points[i] for i in tri], [points[i] for i in support]
                    self.assertTrue(all(ghost_surface.triangle_height(a, x, y)-
                                        ghost_surface.triangle_height(b, x, y) >= .003-1e-6
                                        for x, y in overlap))


if __name__ == "__main__":
    unittest.main()
