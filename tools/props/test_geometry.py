"""Regression checks for winding repair's cavity and seam edge cases."""
import unittest

from geometry import inspect
from repair_geometry import compact, orient


class GeometryTests(unittest.TestCase):
    def test_float32_planar_screen_keeps_front_orientation(self):
        positions = [(-.51, .155, .03999999910593033),
                     (.51, .155, .03999999910593033),
                     (.51, .67, .03999999910593033),
                     (-.51, .67, .03999999910593033)]
        indices = [0, 1, 2, 0, 2, 3]
        report = inspect(positions, indices, repair=True)
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
        report = inspect(positions, indices, repair=True)
        self.assertEqual(report['flipped_triangles'], 4)
        self.assertEqual(inspect(positions, indices)['flipped_triangles'], 0)

    def test_compaction_preserves_uv_and_color_seams(self):
        positions = [(0, 0, 0)] * 4
        uvs = [(0, 0), (1, 0), (0, 0), (0, 0)]
        colors = [(1, 1, 1, 1)] * 3 + [(.5, .5, .5, 1)]
        packed = compact(positions, uvs, colors, [0, 1, 2, 3])
        self.assertEqual(len(packed[0]), 3)
        self.assertEqual(packed[3], [0, 1, 0, 2])


if __name__ == '__main__':
    unittest.main()
