"""Geometric continuity and clearance checks over in-memory Aero authoring."""
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/props'))

from geometry import inspect
from glb import read_glb
from mesh import PropBuilder
from parts import frutiger_aero as aero


def authored_mesh(name):
    builder = PropBuilder('frutiger_aero:'+name, name, aero.SIZES[name])
    aero.PROPS[builder.id](builder)
    return builder.mesh


def triangles(mesh, material=None):
    for face, offset in enumerate(range(0, len(mesh.indices), 3)):
        if material is not None:
            slot = mesh._triangle_materials[face]
            if mesh.materials[slot]['name'] != material:
                continue
        yield tuple(mesh.positions[index] for index in mesh.indices[offset:offset+3])


def ray_heights(mesh, point, axis):
    """Actual triangle intersections along one axis, including duplicate faces."""
    projected = [index for index in range(3) if index != axis]
    result = []
    for a, b, c in triangles(mesh):
        ux, uy = b[projected[0]]-a[projected[0]], b[projected[1]]-a[projected[1]]
        vx, vy = c[projected[0]]-a[projected[0]], c[projected[1]]-a[projected[1]]
        determinant = ux*vy-uy*vx
        if abs(determinant) < 1e-10:
            continue
        px, py = point[0]-a[projected[0]], point[1]-a[projected[1]]
        u, v = (px*vy-py*vx)/determinant, (ux*py-uy*px)/determinant
        if u >= -1e-8 and v >= -1e-8 and u+v <= 1+1e-8:
            result.append(a[axis]+u*(b[axis]-a[axis])+v*(c[axis]-a[axis]))
    return sorted(result)


class AeroAssetContinuityTests(unittest.TestCase):
    def test_folded_leaves_are_seated_in_their_own_pane_planes(self):
        mesh = authored_mesh('glass_partition')
        leaves = list(triangles(mesh, 'aero_leaf'))
        self.assertEqual(len(leaves), 30)
        endpoints = [(-1.72, .32), (-.58, -.32), (.58, .32), (1.72, -.32)]
        for index, (a, b) in enumerate(zip(endpoints, endpoints[1:])):
            vx, vz = b[0]-a[0], b[1]-a[1]
            length = math.hypot(vx, vz)
            nx, nz = -vz/length, vx/length
            for triangle in leaves[index*10:(index+1)*10]:
                for x, _y, z in triangle:
                    distance = (x-a[0])*nx+(z-a[1])*nz
                    self.assertGreaterEqual(distance, .008-1e-8)
                    self.assertLessEqual(distance, .014+1e-8)

    def test_joined_bays_close_their_exterior_corners(self):
        for name in ('wall_bay', 'wall_bay_tall'):
            mesh = authored_mesh(name)
            height = aero.SIZES[name][1]
            # These four points were real holes between rounded outer corners.
            for x in (-1.79, 1.79):
                for y in (.015, height-.015):
                    with self.subTest(name=name, x=x, y=y):
                        intersections = ray_heights(mesh, (x, y), 2)
                        self.assertTrue(any(abs(z-.272) < 1e-7 for z in intersections))

    def test_returning_mullion_has_no_duplicate_front_plane(self):
        for name in ('wall_bay', 'wall_bay_tall'):
            mesh = authored_mesh(name)
            heights = ray_heights(mesh, (1.68, aero.SIZES[name][1]*.47), 2)
            # Main and return skins remain closed, with 4 mm of depth between
            # their two fronts instead of overlapping coplanar visible faces.
            self.assertEqual(sum(abs(z-.272) < 1e-7 for z in heights), 1)
            self.assertTrue(any(abs(z-.268) < 1e-7 for z in heights))

    def test_roof_soffit_covers_square_corners_and_keeps_the_skylight(self):
        mesh = authored_mesh('atrium_dome')
        for x, z in ((-5.8, -5.7), (5.8, -5.7), (-5.8, 5.7), (5.8, 5.7)):
            heights = ray_heights(mesh, (x, z), 1)
            self.assertTrue(any(abs(y) < 1e-7 for y in heights))
            self.assertTrue(any(abs(y-.2) < 1e-7 for y in heights))
        center = ray_heights(mesh, (.07, -.03), 1)
        self.assertTrue(center)
        self.assertGreater(min(center), 2.2)
        self.assertEqual(mesh.triangle_count, 1152)

    def test_portal_spandrel_closes_to_ring_without_lowering_headroom(self):
        mesh = authored_mesh('portal_spandrel')
        for x in (-1.69, -.84, .13, .92, 1.68):
            heights = ray_heights(mesh, (x, .027), 1)
            self.assertTrue(heights)
            self.assertAlmostEqual(min(heights)+1.834,
                                   aero._facet_height(x, 3.6, 3.4, 1.84)-.006)
            self.assertAlmostEqual(max(heights)+1.834, 4.2)
        for x, base, _z, width, height, _depth, _yaw in aero.structural_components('portal_spandrel')['collision_boxes']:
            lower = max(aero._facet_height(x-width/2, 3.6, 3.4, 1.84),
                        aero._facet_height(x+width/2, 3.6, 3.4, 1.84))
            self.assertGreater(base+1.834, lower)
            self.assertLess(base+height+1.834, 4.2)

    def test_portal_caps_follow_individual_facets_without_skinny_fans(self):
        path = ROOT / 'assets/environment/frutiger_aero/props/models/portal_spandrel.glb'
        for kind, mesh in (('source', authored_mesh('portal_spandrel')),
                           ('exported', read_glb(path.read_bytes()))):
            with self.subTest(kind=kind):
                caps = [face for face in triangles(mesh)
                        if max(point[2] for point in face)-min(point[2] for point in face) < 1e-7
                        and abs(abs(face[0][2])-.12) < 1e-6]
                self.assertEqual(len(caps), 32)
                for face in caps:
                    xs = sorted({round(point[0], 6) for point in face})
                    self.assertEqual(len(xs), 2, face)
                    self.assertLessEqual(xs[-1]-xs[0], .689, face)
                    a, b, c = face
                    area_twice = abs((b[0]-a[0])*(c[1]-a[1])-
                                     (b[1]-a[1])*(c[0]-a[0]))
                    altitude = area_twice/max(math.dist(face[i], face[(i+1)%3]) for i in range(3))
                    self.assertGreater(altitude, .09, face)

    def test_refined_stock_is_closed_and_stays_within_existing_asset_budgets(self):
        for name in ('wall_bay', 'wall_bay_tall', 'glass_partition', 'atrium_dome', 'portal_spandrel'):
            with self.subTest(name=name):
                mesh = authored_mesh(name)
                report = inspect(mesh.positions, mesh.indices)
                for key in ('degenerate', 'boundary_edges', 'nonmanifold_edges', 'inconsistent_edges',
                            'flipped_triangles', 'contradictory_components'):
                    self.assertEqual(report[key], 0)
                self.assertLessEqual(mesh.triangle_count, 1500)


if __name__ == '__main__':
    unittest.main()
