"""Facial attachment contracts for the retained human-ghost constructor.

The delivered cat has independent fitted geometry/materials and is covered by
the accepted-export, structural and clip-boundary checks, not COLOR_0 heuristics."""
import copy
import math
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools" / "entities"))
import author_halloween_assets as assets
import ghost_surface


class GhostAttachmentTests(unittest.TestCase):
    def test_overlap_detects_a_cloth_ridge_between_feature_vertices(self):
        face = [(0, 0, .1), (1, 0, .1), (0, 1, .1)]
        cloth = [(.2, .2, .2), (.3, .2, .2), (.2, .3, .2)]
        overlap = ghost_surface.projected_overlap(face, cloth)
        self.assertEqual(len(overlap), 3)
        self.assertTrue(all(ghost_surface.triangle_height(face, x, y) <
                            ghost_surface.triangle_height(cloth, x, y) for x, y in overlap))

    def test_human_face_has_surface_clearance_and_unchanged_rigid_attachment(self):
        for name in ("sheet-ghost",):
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
        for name in ("sheet-ghost",):
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



if __name__ == "__main__":
    unittest.main()
