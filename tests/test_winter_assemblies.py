"""Check real model surfaces at every maintained Winter assembly mount."""
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/props'))
sys.path.insert(0, str(ROOT / 'tools/levels'))
import build_winter as author
import geometry
import glb
from mesh import PropBuilder
from parts import winter, winter_village
from string_lights import SPANS, attachment_height


def built_model(name, kit=winter):
    builder = PropBuilder('winter:'+name, name, kit.SIZES[name])
    kit.PROPS[builder.id](builder)
    builder.mesh.validate(builder.size)
    return glb.read_glb(glb.write_glb(builder.mesh, builder.tex.png_bytes(), name=name,
                                   nodes=builder.nodes, animations=builder.clips))


def stock_model(name):
    return glb.read_glb((winter.OUTDOOR / (name+'.glb')).read_bytes())


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
        cls.level = author.build_level()
        cls.props = {p['id']: p for p in cls.level['props']}
        cls.blanket = built_model('snow_roof_blanket')

    def test_control_aim_boxes_are_explicit_and_do_not_overlap(self):
        authored = json.loads((ROOT / 'assets/levels/winter.json').read_text())
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

    def test_blanket_is_one_closed_shell_with_no_internal_bay_caps(self):
        report = geometry.inspect(self.blanket.positions, self.blanket.indices)
        self.assertEqual(report['components'], 1)
        for field in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                      'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
            self.assertEqual(report[field], 0, report)
        self.assertEqual(self.blanket.triangle_count, 140)
        self.assertTrue(all(0 <= v <= 1 for uv in self.blanket.uvs for v in uv))
        # The old bays had exposed stock/snow caps on the same X planes,
        # including the middle of each house. A blanket has only outer caps.
        cap_planes = {round(triangle[0][0], 5) for triangle in winter._triangles(self.blanket)
                      if abs(winter._normal(*triangle)[0]) > 1e-8
                      and abs(winter._normal(*triangle)[1]) < 1e-8
                      and abs(winter._normal(*triangle)[2]) < 1e-8}
        self.assertEqual(len(cap_planes), 2)
        self.assertNotIn(0, cap_planes)

    def test_blanket_keeps_cap_artwork_density_and_continuous_fitted_uvs(self):
        cap = built_model('snow_roof_slope')
        cap_width = max(q[0] for q in cap.positions)-min(q[0] for q in cap.positions)
        cap_depth = max(q[2] for q in cap.positions)-min(q[2] for q in cap.positions)
        by_point = {}
        for point, uv in zip(self.blanket.positions, self.blanket.uvs):
            key = tuple(round(v, 6) for v in point)
            previous = by_point.setdefault(key, uv)
            for actual, expected in zip(uv, previous):
                self.assertAlmostEqual(actual, expected, places=6)
        for start in range(0, len(self.blanket.indices), 3):
            indices = self.blanket.indices[start:start+3]
            points = [self.blanket.positions[i] for i in indices]
            if winter._normal(*points)[1] <= 1e-8:
                continue
            # Every top facet lies entirely within one fitted UV section.
            # Its texture density must match the existing narrow snow cap.
            for i in range(3):
                for j in range(i+1, 3):
                    a, b = points[i], points[j]
                    ua, ub = self.blanket.uvs[indices[i]], self.blanket.uvs[indices[j]]
                    if abs(a[0]-b[0]) > 1e-7:
                        self.assertAlmostEqual(abs((ua[0]-ub[0])/(a[0]-b[0])),
                                               1/cap_width, delta=.0006)
                    if abs(a[2]-b[2]) > 1e-7:
                        self.assertAlmostEqual(abs((ua[1]-ub[1])/(a[2]-b[2])),
                                               1/cap_depth, places=5)

    def test_every_roof_blanket_follows_actual_stock_and_separates_end_planes(self):
        blankets = [p for p in self.level['props'] if p['model'] == 'winter:snow_roof_blanket']
        self.assertEqual(len(blankets), 6)
        self.assertFalse(any(p['model'] in ('winter:snow_roof_slope', 'winter:snow_roof_edge')
                             for p in self.level['props']))
        for home, (_, _, _, family) in enumerate(author.HOMES):
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
                    self.assertAlmostEqual(abs(snow_x-stock_x), .008, places=5)
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
                    support = winter.surface_height(roofs, q[0], q[2])
                    if support is not None:
                        self.assertAlmostEqual(q[1]-support, -.008, places=5)
                # Visible snow stays clear of shingles and small trim blocks.
                for triangle in world_triangles(self.level, prop, self.blanket):
                    if winter._normal(*triangle)[1] <= 1e-8:
                        continue
                    for q in (*triangle, tuple(sum(v[i] for v in triangle)/3 for i in range(3))):
                        support = winter.surface_height(roofs, q[0], q[2])
                        if support is not None:
                            self.assertGreater(q[1]-support, .02)

    def test_every_ridge_rim_sinks_into_shingles_and_gables_use_exterior_floor(self):
        for home, (_, _, floor, family) in enumerate(author.HOMES):
            roofs = [triangle for p in self.level['props'] if p['id'].startswith(f'home_{home}_roof_')
                     for triangle in world_triangles(self.level, p, stock_model(f'house_{family}_roof_slope'))]
            ridge = stock_model(f'house_{family}_roof_ridge')
            for bay in (0, 1):
                prop = self.props[f'home_{home}_ridge_{bay}']
                for point in set(ridge.positions):
                    if abs(point[1]) > 1e-6:
                        continue
                    q = world_point(self.level, prop, point)
                    support = winter.surface_height(roofs, q[0], q[2])
                    self.assertIsNotNone(support)
                    self.assertAlmostEqual(q[1]-support, -.008, places=4)
            for side in ('west', 'east'):
                prop = self.props[f'home_{home}_gable_{side}']
                self.assertEqual(floor_at(self.level, prop['x'], prop['z']), 0)
                self.assertGreater(prop['y'], floor+2.4)

    def test_all_strings_intersect_their_real_supports_and_clear_hoods(self):
        boxes = {b['id']: b for b in self.level['void_walls']}
        spans = [p for p in self.level['props'] if p['model'].startswith('winter:string_lights_')]
        self.assertEqual(len(spans), 8)
        self.assertEqual(sum(len(p['lights']) for p in spans), 38)
        for prop in spans:
            variant = prop['model'].removeprefix('winter:string_lights_')
            ends = [world_point(self.level, prop, (side*SPANS[variant][0]/2,
                                                  attachment_height(variant), 0))
                    for side in (-1, 1)]
            if prop['id'].startswith('home_'):
                home = int(prop['id'].split('_')[1])
                _, z, floor, family = author.HOMES[home]
                _, _, fascia_z, fascia_top = winter.roof_support_profile(family)
                roof_front = z+3+1.82+fascia_z*winter.ROOF_SCALE
                self.assertGreaterEqual(prop['z']-.045-roof_front, .0149)
                self.assertAlmostEqual(world_point(self.level, prop, (0, 0, 0))[1], floor+2.77)
                for side, endpoint in zip((-1, 1), ends):
                    upstand = boxes[f'home_{home}_string_upstand_{side}']
                    for axis, half in enumerate((.025, .025, .02)):
                        self.assertGreaterEqual(endpoint[axis]-half, upstand['min'][axis])
                        self.assertLessEqual(endpoint[axis]+half, upstand['max'][axis])
                    arm = boxes[f'home_{home}_string_bracket_{side}']
                    self.assertLess(arm['min'][2], roof_front)
                    self.assertGreater(arm['max'][2], roof_front)
                    self.assertLess(arm['min'][1], floor+2.7+fascia_top*winter.ROOF_SCALE)
                    self.assertLess(arm['max'][2]-arm['min'][2], .2)
            elif prop['id'].startswith('lodge_rail_string_'):
                rail = self.level['guardrails'][int(prop['id'].split('_')[-1])]
                for endpoint, x in zip(ends, (rail['x'], rail['x']+rail['length'])):
                    self.assertLess(abs(endpoint[0]-x), .03+.025)
                    self.assertLess(abs(endpoint[2]-rail['z']), .03+.02)
                    self.assertGreater(endpoint[1], rail['y'])
                    self.assertLess(endpoint[1], rail['y']+rail['height'])
            else:
                prefix = ('porch_post_' if prop['id'] == 'lodge_porch_string'
                          else 'string_post_'+str(2 if prop['id'] == 'path_string'
                                                  else int(prop['id'].split('_')[-1]))+'_')
                posts = [p for p in self.level['props'] if p['id'].startswith(prefix)]
                model = stock_model('house_02_porch_post')
                for endpoint in ends:
                    supported = False
                    for post in posts:
                        points = [world_point(self.level, post, q) for q in model.positions]
                        low = [min(q[i] for q in points) for i in range(3)]
                        high = [max(q[i] for q in points) for i in range(3)]
                        supported |= all(endpoint[i]+half > low[i] and endpoint[i]-half < high[i]
                                         for i, half in enumerate((.025, .025, .02)))
                    self.assertTrue(supported, prop['id'])

    def test_hood_snow_undersides_are_embedded_in_timber(self):
        hood = built_model('door_hood_snow', winter_village)
        stock = list(winter._triangles(hood, hood.material_names.index('village_stock')))
        snow = list(winter._triangles(hood, hood.material_names.index('snow')))
        bases = [triangle for triangle in snow if winter._normal(*triangle)[1] < -1e-8]
        self.assertTrue(bases)
        for triangle in bases:
            q = tuple(sum(v[i] for v in triangle)/3 for i in range(3))
            support = winter.surface_height(stock, q[0], q[2])
            self.assertIsNotNone(support)
            self.assertGreater(support-q[1], .008)
            # The projecting nose beam slightly overlaps the sloped hood;
            # the snow stays embedded in that higher timber face as well.
            self.assertLess(support-q[1], .02)


if __name__ == '__main__':
    unittest.main()
