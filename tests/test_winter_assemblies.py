"""Check real model surfaces at every maintained Winter assembly mount."""
import json
import math
from pathlib import Path
import unittest

from assets.inspection import glb
from release.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]


# Independent source witness for the three maintained cottage mounts.
# x, z, floor_y and stock roof family; no historical map generator is imported.
HOMES = ((-16, -16, .6, '02'), (8, -30, 0, '01'), (-17, 4, 0, '04'))


def shipped_model(name, theme='winter'):
    path = ROOT / f'assets/environments/{theme}/props/{name}.glb'
    return glb.read_glb(path.read_bytes())


def normal_y(a, b, c):
    return (b[2]-a[2])*(c[0]-a[0]) - (b[0]-a[0])*(c[2]-a[2])


def triangles(model, material):
    for face, start in enumerate(range(0, len(model.indices), 3)):
        if model.triangle_materials[face] == material:
            yield tuple(model.positions[i] for i in model.indices[start:start+3])


def surface_height(surfaces, x, z):
    # Highest actual face intersecting this X/Z position.
    heights = []
    for a, b, c in surfaces:
        den = (b[2]-c[2])*(a[0]-c[0]) + (c[0]-b[0])*(a[2]-c[2])
        if abs(den) < 1e-9:
            continue
        u = ((b[2]-c[2])*(x-c[0]) + (c[0]-b[0])*(z-c[2])) / den
        v = ((c[2]-a[2])*(x-c[0]) + (a[0]-c[0])*(z-c[2])) / den
        if min(u, v, 1-u-v) >= -1e-6:
            heights.append(u*a[1] + v*b[1] + (1-u-v)*c[1])
    return max(heights, default=None)


def stock_model(name):
    return shipped_model(name, 'outdoor')


def floor_at(level, x, z):
    room = next(r for r in level['rooms'] if r['x']-.01 <= x <= r['x']+r['width']+.01
                and r['z']-.01 <= z <= r['z']+r['depth']+.01)
    floor = room.get('floor_y', 0)
    for region in level['floor_regions']:
        if (region['x'] <= x <= region['x']+region['width'] and
                region['z'] <= z <= region['z']+region['depth']):
            floor = room.get('floor_y', 0)+region['offset_y']
    return floor


def world_point(level, prop, point):
    x, y, z = point
    angle = math.radians(prop.get('rotation_degrees', 0))
    scale = prop.get('scale', 1)
    return (prop['x']+scale*(x*math.cos(angle)+z*math.sin(angle)),
            floor_at(level, prop['x'], prop['z'])+prop.get('y', 0)+scale*y,
            prop['z']+scale*(-x*math.sin(angle)+z*math.cos(angle)))


def world_triangles(level, prop, model):
    points = [world_point(level, prop, point) for point in model.positions]
    return [tuple(points[i] for i in model.indices[start:start+3])
            for start in range(0, len(model.indices), 3)]


class WinterAssemblyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = json.loads(environment_source('winter').read_text())
        cls.props = {p['id']: p for p in cls.level['props']}
        cls.blanket = shipped_model('snow_roof_blanket')

    def test_bank_tree_roots_share_their_center_support_height(self):
        # Nine contact points from one saved mesh catch the measured bank-edge
        # float without building or baking any map.
        prop = self.props['forest_3_1']
        model = glb.read_glb((ROOT / 'assets/environments/winter/props/tree_snow_02.glb').read_bytes())
        roots = {point for point in model.positions if abs(point[1]) < 1e-6}
        self.assertEqual(len(roots), 9)
        for point in roots:
            x, y, z = world_point(self.level, prop, point)
            self.assertAlmostEqual(y, floor_at(self.level, x, z), places=5)

    def test_control_aim_boxes_are_explicit_and_do_not_overlap(self):
        authored = json.loads((environment_source('winter')).read_text())
        controls = [p for p in authored['props'] if p['id'] in {
            'winter_blizzard_button', 'winter_weather_mild',
            'winter_weather_moderate', 'winter_weather_severe', 'winter_weather_timer'}]
        self.assertEqual(len(controls), 5)
        intervals = []
        for control in controls:
            self.assertEqual(control['size'], [.18, .18, .10])
            self.assertFalse(control.get('solid', False))
            self.assertEqual(control, self.props[control['id']])
            bottom = floor_at(authored, control['x'], control['z']) + control['y']
            top = bottom + control['size'][1] * control['scale']
            intervals.append((bottom, top, control['id']))
        intervals.sort()
        for lower, upper in zip(intervals, intervals[1:]):
            self.assertLess(lower[1], upper[0], (lower, upper))

    def test_every_roof_blanket_follows_actual_stock_and_separates_end_planes(self):
        blankets = [p for p in self.level['props'] if p['model'] == 'winter:snow_roof_blanket']
        self.assertEqual(len(blankets), 6)
        self.assertFalse(any(p['model'] in ('winter:snow_roof_slope', 'winter:snow_roof_edge')
                             for p in self.level['props']))
        for home, (_, _, _, family) in enumerate(HOMES):
            roof_props = [p for p in self.level['props'] if p['id'].startswith(f'home_{home}_roof_')]
            roof_model = stock_model(f'house_{family}_roof_slope')
            roofs = [triangle for p in roof_props for triangle in
                     world_triangles(self.level, p, roof_model)]
            for side in ('front', 'back'):
                prop = self.props[f'home_{home}_snow_roof_{side}']
                points = [world_point(self.level, prop, q) for q in self.blanket.positions]
                for edge in (min, max):
                    snow_x = edge(q[0] for q in points)
                    stock_x = edge(q[0] for triangle in roofs for q in triangle)
                    # The GLB carries f32 end planes, which round by a few microns.
                    self.assertAlmostEqual(abs(snow_x-stock_x), .008, delta=1e-5)
                # Test actual exported base vertices on both shingles and
                # fascia, including the raised lodge's floor-relative anchor.
                # UV cuts also insert top-diagonal vertices between contact
                # rows; those have no matching underside vertex at their Z.
                end_x = max(q[0] for q in self.blanket.positions)
                contact_z = {q[2] for q in self.blanket.positions if q[0] == end_x}
                bases = {}
                for q in self.blanket.positions:
                    if q[2] not in contact_z:
                        continue
                    key = (q[0], q[2])
                    bases[key] = min(q[1], bases.get(key, q[1]))
                for (x, z), y in bases.items():
                    q = world_point(self.level, prop, (x, y, z))
                    support = surface_height(roofs, q[0], q[2])
                    if support is not None:
                        # Accepted roof trim changes the exact underlap. Require buried
                        # contact within the 2 cm structural margin, never a floating base.
                        self.assertGreaterEqual(q[1]-support, -.02)
                        self.assertLess(q[1]-support, 0)
                # Visible snow stays clear of shingles and small trim blocks.
                for triangle in world_triangles(self.level, prop, self.blanket):
                    if normal_y(*triangle) <= 1e-8:
                        continue
                    for q in (*triangle, tuple(sum(v[i] for v in triangle)/3 for i in range(3))):
                        support = surface_height(roofs, q[0], q[2])
                        if support is not None:
                            self.assertGreater(q[1]-support, .02)

    def test_every_ridge_rim_sinks_into_shingles_and_gables_use_exterior_floor(self):
        for home, (_, _, floor, family) in enumerate(HOMES):
            roofs = [triangle for p in self.level['props'] if p['id'].startswith(f'home_{home}_roof_')
                     for triangle in world_triangles(self.level, p, stock_model(f'house_{family}_roof_slope'))]
            ridge = stock_model(f'house_{family}_roof_ridge')
            for bay in (0, 1):
                prop = self.props[f'home_{home}_ridge_{bay}']
                for point in set(ridge.positions):
                    if abs(point[1]) > 1e-6:
                        continue
                    q = world_point(self.level, prop, point)
                    support = surface_height(roofs, q[0], q[2])
                    self.assertIsNotNone(support)
                    self.assertAlmostEqual(q[1]-support, -.008, places=4)
            for side in ('west', 'east'):
                prop = self.props[f'home_{home}_gable_{side}']
                self.assertEqual(floor_at(self.level, prop['x'], prop['z']), 0)
                self.assertGreater(prop['y'], floor+2.4)

    def test_hood_snow_undersides_are_embedded_in_timber(self):
        hood = shipped_model('door_hood_snow')
        stock = list(triangles(hood, hood.material_names.index('winter_timber')))
        snow = list(triangles(hood, hood.material_names.index('snow.001')))
        undersides = [face for face in snow if normal_y(*face) < -1e-8]
        exposed = [face for face in snow if normal_y(*face) > 1e-8]
        self.assertTrue(undersides and exposed)
        # The saved Blender hood's snow underlaps the timber more than the old
        # procedural fixture. Check both contact and a genuinely exposed snow face.
        for face, embedded in [(face, True) for face in undersides] + [(face, False) for face in exposed]:
            x, y, z = (sum(point[i] for point in face)/3 for i in range(3))
            support = surface_height(stock, x, z)
            self.assertIsNotNone(support)
            if embedded:
                self.assertGreater(support-y, .008, 'the snow must not float above timber')
            else:
                self.assertGreater(y-support, .04, 'the snow surface must not be buried')


if __name__ == '__main__':
    unittest.main()
