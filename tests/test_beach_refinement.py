"""Independent refinement checks over Beach's source and shipped geometry.

No exports, map generation or native rendering are performed by this suite.
The lookout retains its existing raised floor and stair route; its visible
under-deck arch is checked as geometry, not advertised as a playable route.
"""
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'tools/props'), str(ROOT / 'tools/entities')]

from geometry import inspect
from glb import read_glb
from mesh import PropBuilder
from parts import beach_nature, beach_structures
from validate_entities import Model
from tests.test_beach_demo import authored_floor, base_y, collider_box, contains, disc_touches


def triangles(mesh):
    for offset in range(0, len(mesh.indices), 3):
        yield tuple(mesh.positions[index] for index in mesh.indices[offset:offset+3])


def ray_axis(positions, indices, point, axis):
    """Intersect actual triangles along an axis, retaining distinct skins."""
    projected = [i for i in range(3) if i != axis]
    hits = []
    for offset in range(0, len(indices), 3):
        a, b, c = (positions[index] for index in indices[offset:offset+3])
        ux, uy = b[projected[0]]-a[projected[0]], b[projected[1]]-a[projected[1]]
        vx, vy = c[projected[0]]-a[projected[0]], c[projected[1]]-a[projected[1]]
        determinant = ux*vy-uy*vx
        if abs(determinant) < 1e-10:
            continue
        px, py = point[0]-a[projected[0]], point[1]-a[projected[1]]
        u, v = (px*vy-py*vx)/determinant, (ux*py-uy*px)/determinant
        if u >= -1e-8 and v >= -1e-8 and u+v <= 1+1e-8:
            hits.append(a[axis]+u*(b[axis]-a[axis])+v*(c[axis]-a[axis]))
    # Adjacent triangles on the same skin meet on shared edges.
    return sorted({round(hit, 6) for hit in hits})


def world_point(level, prop, point):
    x, y, z = point
    scale = prop.get('scale', 1)
    yaw = math.radians(prop.get('rotation_degrees', 0))
    return (prop['x']+scale*(x*math.cos(yaw)+z*math.sin(yaw)),
            base_y(level, prop)+y*scale,
            prop['z']+scale*(-x*math.sin(yaw)+z*math.cos(yaw)))


def authored_mesh(name, module):
    builder = PropBuilder('beach:'+name, name, module.SIZES[name])
    module.PROPS[builder.id](builder)
    return builder.mesh


class BeachRefinementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = json.loads((ROOT / 'assets/levels/beach_demo.json').read_text())
        cls.props = {prop['id']: prop for prop in cls.level['props']}
        cls.catalog = {entry['id']: entry for entry in
                       json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
        cls.meshes = {}
        for identity in ('cove_foam', 'lookout_base', 'lookout_terrace',
                         'town_bunting', 'town_cream', 'town_blue', 'lookout_stairs'):
            entry = cls.catalog[cls.props[identity]['model']]
            cls.meshes[identity] = read_glb((ROOT / 'assets' / entry['model']).read_bytes())

    def assert_closed(self, mesh, components=1):
        report = inspect(mesh.positions, mesh.indices)
        for key in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                    'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
            self.assertEqual(report[key], 0, report)
        self.assertEqual(report['components'], components, report)

    def test_cove_foam_is_one_closed_strip_with_caps_only_at_its_ends(self):
        for kind, mesh in (('source', authored_mesh('cove_foam', beach_nature)),
                           ('exported', self.meshes['cove_foam'])):
            with self.subTest(kind=kind):
                self.assert_closed(mesh)
                xs = sorted({round(point[0], 6) for point in mesh.positions})
                self.assertEqual(len(xs), 65)
                self.assertAlmostEqual(xs[0], -24)
                self.assertAlmostEqual(xs[-1], 24)
                self.assertTrue(all(abs(b-a-.75) < 1e-6 for a, b in zip(xs, xs[1:])))
                caps = [face for face in triangles(mesh)
                        if max(point[0] for point in face)-min(point[0] for point in face) < 1e-7]
                self.assertEqual(len(caps), 4)
                self.assertEqual({round(face[0][0], 6) for face in caps}, {-24, 24})
                self.assertLessEqual(len(mesh.indices)//3, 600)
        shipped = self.meshes['cove_foam']
        self.assertIn((ROOT / 'assets/environment/beach/props/models/beach_palette.png').read_bytes(),
                      shipped.texture_pngs)
        self.assertTrue(all(material.get('alphaMode', 'OPAQUE') == 'OPAQUE'
                            for material in shipped.json['materials']))

    def test_cove_foam_tracks_the_actual_sloped_coast_without_sinking(self):
        prop = self.props['cove_foam']
        mesh = self.meshes['cove_foam']
        rows = {}
        for point in mesh.positions:
            x, _y, z = world_point(self.level, prop, point)
            rows.setdefault(round(x, 6), []).append(z)
        covered = 0
        for (left, a), (right, b) in zip(sorted(rows.items()), sorted(rows.items())[1:]):
            x = (left+right)/2
            low = (min(a)+min(b))/2
            high = (max(a)+max(b))/2
            self.assertGreater(high-low, .19)
            self.assertLess(high-low, .40)
            bottom = base_y(self.level, prop)
            top = bottom+max(point[1] for point in mesh.positions)
            # The pier covers a cut-out in the coast. Its deck hides the foam
            # below it; that ground is intentionally not a shoreline ramp.
            if 6.96875 < x < 9.03125:
                self.assertLess(top, authored_floor(self.level, x, (low+high)/2))
                continue
            water = [water for water in self.level['water']
                     if water['swimming'] and contains(water, x, (low+high)/2)]
            self.assertTrue(water, (x, low, high))
            surface = max(water['surface_y'] for water in water)
            self.assertAlmostEqual(bottom, surface, places=5)
            self.assertAlmostEqual(top-surface, .015, places=5)
            center_floor = authored_floor(self.level, x, (low+high)/2)
            self.assertGreaterEqual(surface-center_floor, .04)
            self.assertLessEqual(surface-center_floor, .11)
            for z in (low, (low+high)/2, high):
                self.assertGreaterEqual(bottom-authored_floor(self.level, x, z), .02,
                                        (x, z, bottom, authored_floor(self.level, x, z)))
            covered += 1
        self.assertGreaterEqual(covered, 60)

    def test_terrace_base_is_closed_and_supports_the_deck_from_ground(self):
        prop = self.props['lookout_base']
        terrace = self.props['lookout_terrace']
        self.assertAlmostEqual(base_y(self.level, prop), 0)
        self.assertAlmostEqual(base_y(self.level, terrace), 2.22)
        for kind, mesh in (('source', authored_mesh('town_terrace_base', beach_structures)),
                           ('exported', self.meshes['lookout_base'])):
            with self.subTest(kind=kind):
                self.assert_closed(mesh)
                for x in (-1.85, 1.85):
                    for z in (-1.4, -.9, 0, .9, 1.4):
                        hits = ray_axis(mesh.positions, mesh.indices, (x, z), 1)
                        self.assertEqual(hits, [0, 2.22], (kind, x, z, hits))
                # Matching strip/top/return boundaries keep this a single
                # closed union instead of adding independent touching solids.
                self.assertLessEqual(len(mesh.indices)//3, 260)
        # The slab seats directly on the new stock; it has not moved upward.
        slab = self.meshes['lookout_terrace']
        hits = ray_axis(slab.positions, slab.indices, (0, .05), 1)
        self.assertEqual(hits, [0, .18])

    def test_terrace_arch_caps_use_local_facet_charts_without_skinny_fans(self):
        for kind, mesh in (('source', authored_mesh('town_terrace_base', beach_structures)),
                           ('exported', self.meshes['lookout_base'])):
            with self.subTest(kind=kind):
                caps = [face for face in triangles(mesh)
                        if max(point[2] for point in face)-min(point[2] for point in face) < 1e-7
                        and any(abs(face[0][2]-z) < 1e-6 for z in (-1.5, -1.22, 1.22, 1.5))
                        and min(point[1] for point in face) >= 1.45-1e-6
                        and max(abs(point[0]) for point in face) <= 1.7+1e-6]
                self.assertEqual(len(caps), 80)
                for face in caps:
                    xs = sorted({round(point[0], 6) for point in face})
                    self.assertEqual(len(xs), 2, face)
                    self.assertLessEqual(xs[-1]-xs[0], .536, face)
                    a, b, c = face
                    area_twice = abs((b[0]-a[0])*(c[1]-a[1])-
                                     (b[1]-a[1])*(c[0]-a[0]))
                    altitude = area_twice/max(math.dist(face[i], face[(i+1)%3]) for i in range(3))
                    self.assertGreater(altitude, .07, face)

    def test_terrace_arch_stock_preserves_standing_clearance(self):
        mesh = self.meshes['lookout_base']
        for x in (-.30, 0, .30):
            for z in (-1.36, 1.36):
                hits = ray_axis(mesh.positions, mesh.indices, (x, z), 1)
                self.assertTrue(hits)
                self.assertGreaterEqual(min(hits), 1.8+.02, (x, z, hits))
            # A complete front-to-back ray at standing body height stays open.
            self.assertEqual(ray_axis(mesh.positions, mesh.indices, (x, 1.8), 2), [])
        blockers = [(identity, collider_box(self.level, prop))
                    for identity, prop in self.props.items()
                    if identity.startswith('lookout_base_solid_')]
        self.assertTrue(blockers)
        for z in (14.9, 15.05, 16.4, 17.75, 17.9):
            hits = [identity for identity, box in blockers
                    if disc_touches(box, 12, z, .30) and box[4] < 1.8 and box[5] > .001]
            self.assertEqual(hits, [], (z, hits))

    def test_stair_to_terrace_floor_and_standing_headroom_remain_supported(self):
        previous = authored_floor(self.level, 12, 22.12)
        self.assertAlmostEqual(previous, 0)
        base_boxes = [collider_box(self.level, prop) for identity, prop in self.props.items()
                      if identity.startswith('lookout_base_solid_')]
        stair = self.meshes['lookout_stairs']
        stair_positions = [world_point(self.level, self.props['lookout_stairs'], point)
                           for point in stair.positions]
        for index in range(12):
            z = 21.925-index*.35
            floor = authored_floor(self.level, 12, z)
            self.assertGreater(floor, previous)
            self.assertLessEqual(floor-previous, .21)
            heights = ray_axis(stair_positions, stair.indices, (12, z), 1)
            self.assertTrue(any(abs(height-floor-.005) < 1e-5 for height in heights))
            previous = floor
        for z in (17.82, 17.6, 17.2, 16.4, 15.2):
            floor = authored_floor(self.level, 12, z)
            self.assertAlmostEqual(floor, 2.395)
            self.assertLessEqual(abs(floor-previous), .01)
            self.assertTrue(all(box[5] <= floor for box in base_boxes
                                if disc_touches(box, 12, z, .30)))
            previous = floor

    def test_bunting_cord_end_faces_are_inside_real_cream_and_blue_facades(self):
        prop = self.props['town_bunting']
        mesh = self.meshes['town_bunting']
        cord_slot = mesh.material_names.index('beach_matte')
        cord_indices = {index for face, offset in enumerate(range(0, len(mesh.indices), 3))
                        if mesh.triangle_materials[face] == cord_slot
                        for index in mesh.indices[offset:offset+3]}
        low = min(mesh.positions[index][0] for index in cord_indices)
        high = max(mesh.positions[index][0] for index in cord_indices)
        for edge, identity in ((low, 'town_blue'), (high, 'town_cream')):
            facade = self.meshes[identity]
            world_facade = [world_point(self.level, self.props[identity], point)
                            for point in facade.positions]
            # The four end-face corners and the cap's triangulation centre,
            # including the cord's actual thickness.
            end = {tuple(round(value, 7) for value in world_point(self.level, prop, mesh.positions[index]))
                   for index in cord_indices if abs(mesh.positions[index][0]-edge) < .002}
            self.assertEqual(len(end), 5)
            for x, y, z in end:
                hits = ray_axis(world_facade, facade.indices, (x, y), 2)
                self.assertEqual(len(hits) % 2, 0, (identity, x, y, hits))
                self.assertTrue(any(a+1e-5 < z < b-1e-5 for a, b in zip(hits[::2], hits[1::2])),
                                (identity, x, y, z, hits))

    def test_full_swim_clip_clears_water_surface_and_seabed_at_120_hz(self):
        model = Model(ROOT / 'assets/entities/beach_fish/model/beach_fish.glb')
        clip = next(clip for clip in model.clips if clip['name'] == 'swim')
        fish = [prop for prop in self.level['props'] if prop['model'] == 'beach:fish']
        self.assertEqual({prop['id'] for prop in fish}, {'shallows_fish', 'shallows_fish_pair'})
        frames = math.ceil(clip['duration']*120)
        minimum = {prop['id']: [math.inf, math.inf] for prop in fish}
        for frame in range(frames+1):
            posed = model._skin(model._pose_globals(clip, clip['duration']*frame/frames))
            for prop in fish:
                for point in posed:
                    x, y, z = world_point(self.level, prop, point)
                    floor = authored_floor(self.level, x, z)
                    self.assertIsNotNone(floor, (prop['id'], frame, x, z))
                    water = next((water for water in reversed(self.level['water'])
                                  if water['swimming'] and contains(water, x, z)), None)
                    self.assertIsNotNone(water, (prop['id'], frame, x, z))
                    minimum[prop['id']][0] = min(minimum[prop['id']][0], y-floor)
                    minimum[prop['id']][1] = min(minimum[prop['id']][1], water['surface_y']-y)
        for identity, (seabed, surface) in minimum.items():
            self.assertGreaterEqual(seabed, .02, (identity, seabed))
            self.assertGreaterEqual(surface, .02, (identity, surface))
        self.__class__.fish_clearance = minimum


if __name__ == '__main__':
    unittest.main()
