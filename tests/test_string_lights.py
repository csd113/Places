"""Protect the reusable asset, source alignment, mounting and bounded light cost."""
import json
import math
from pathlib import Path
import sys
import unittest

from tools.levels.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/props'))
import glb
import geometry
from string_lights import SPANS, TIMBER_LANTERN_SOURCE, attachment_height, lights


def material_points(model, slot):
    return {model.positions[index]
            for triangle, start in enumerate(range(0, len(model.indices), 3))
            if model.triangle_materials[triangle] == slot
            for index in model.indices[start:start+3]}


def axis_intersections(model, point, axis):
    """Intersect the delivered skins to catch housing below an authored source."""
    projected = [i for i in range(3) if i != axis]
    hits = []
    for triangle, start in enumerate(range(0, len(model.indices), 3)):
        a, b, c = (model.positions[i] for i in model.indices[start:start+3])
        ux, uy = b[projected[0]]-a[projected[0]], b[projected[1]]-a[projected[1]]
        vx, vy = c[projected[0]]-a[projected[0]], c[projected[1]]-a[projected[1]]
        determinant = ux*vy-uy*vx
        if abs(determinant) < 1e-10:
            continue
        px, py = point[0]-a[projected[0]], point[1]-a[projected[1]]
        u, v = (px*vy-py*vx)/determinant, (ux*py-uy*px)/determinant
        if u >= -1e-8 and v >= -1e-8 and u+v <= 1+1e-8:
            hits.append((a[axis]+u*(b[axis]-a[axis])+v*(c[axis]-a[axis]),
                         model.triangle_materials[triangle]))
    return sorted(hits)


class StringLightTests(unittest.TestCase):
    def test_delivered_bulbs_have_clear_sources_and_isolated_emission(self):
        for variant, count in (('short', 5), ('medium', 7), ('long', 11)):
            with self.subTest(variant=variant):
                path = ROOT / f'assets/environments/winter/props/string_lights_{variant}.glb'
                model = glb.read_glb(path.read_bytes())
                report = geometry.inspect(model.positions, model.indices)
                for field in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                              'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
                    self.assertEqual(report[field], 0)
                self.assertAlmostEqual(model.bounds()[0][1], 0)
                self.assertTrue(all(0 <= v <= 1 for uv in model.uvs for v in uv))
                emitting = [i for i, material in enumerate(model.json['materials'])
                            if any(material.get('emissiveFactor', [0, 0, 0]))]
                self.assertEqual(len(emitting), 1, 'cable and sockets remain non-emissive')
                glass = model.json['materials'][emitting[0]]
                for actual, expected in zip(glass['emissiveFactor'], [1, .56, .16]):
                    self.assertAlmostEqual(actual, expected)
                self.assertEqual(glass.get('extensions', {}).get('KHR_materials_emissive_strength', {})
                                 .get('emissiveStrength', 1), 2)
                # Discover each physical bulb from the delivered glass, independently
                # of the authoring table. Glass and neck share the same X interval.
                bulbs = []
                for point in sorted(material_points(model, emitting[0])):
                    if not bulbs or point[0]-bulbs[-1][-1][0] > .1:
                        bulbs.append([])
                    bulbs[-1].append(point)
                self.assertEqual(len(bulbs), count)
                sources = lights(variant)
                self.assertEqual(len(sources), count)
                self.assertAlmostEqual(sum(source['intensity'] for source in sources),
                                       .32*SPANS[variant][2])
                for source, points in zip(sources, bulbs):
                    x, y, z = source['offset']
                    self.assertAlmostEqual(x, (min(p[0] for p in points)+
                                               max(p[0] for p in points))/2, places=5)
                    self.assertAlmostEqual(y, min(p[1] for p in points)-.02, places=5)
                    self.assertEqual(z, 0)
                    skins = axis_intersections(model, (x, z), 1)
                    self.assertTrue(skins)
                    self.assertAlmostEqual(min(hit[0] for hit in skins)-y, .02, places=5)

    def test_lantern_front_spill_clears_glass_and_opaque_base(self):
        model = glb.read_glb((ROOT / 'assets/environments/winter/props/timber_lantern.glb').read_bytes())
        slot = model.material_names.index('winter_bulb_emitter')
        glass = material_points(model, slot)
        x, y, z = TIMBER_LANTERN_SOURCE
        self.assertEqual(x, 0)
        self.assertAlmostEqual(y, (min(p[1] for p in glass)+max(p[1] for p in glass))/2, places=5)
        front_hits = axis_intersections(model, (x, y), 2)
        self.assertEqual(front_hits[-1][1], slot)
        self.assertGreater(z-front_hits[-1][0], .06)
        base_front = max(hit[0] for hit in axis_intersections(model, (x, 2.32), 2)
                         if hit[1] != slot)
        self.assertAlmostEqual(z-base_front, .02, places=5)
        self.assertTrue(all(hit[0] > y for hit in axis_intersections(model, (x, z), 1)),
                        'the downward spill clears the opaque base')
        self.assertFalse(axis_intersections(model, (y, z), 0),
                         'lateral spill clears the corner cage')
        level = json.loads((environment_source('winter')).read_text())
        lanterns = [p for p in level['props'] if p['model'] == 'winter:timber_lantern']
        self.assertEqual(len(lanterns), 2)
        for prop in lanterns:
            self.assertEqual(prop['lights'], [{'shape': 'point', 'offset': list(TIMBER_LANTERN_SOURCE),
                                              'intensity': 1.5, 'range': 14,
                                              'color': [1, .81, .55], 'falloff': 'smooth'}])

    def test_mounts_are_supported_clear_of_walks_and_light_budget_stays_local(self):
        level = json.loads((environment_source('winter')).read_text())
        spans = [p for p in level['props'] if p['model'].startswith('winter:string_lights_')]
        self.assertEqual(len(spans), 8)
        self.assertEqual(sum(len(p['lights']) for p in spans), 60)
        for p in spans:
            self.assertFalse(p.get('solid', False))
            self.assertFalse(p['occludes'])
            self.assertNotIn('scale', p, 'repeat spans instead of stretching bulbs')
            variant = p['model'].removeprefix('winter:string_lights_')
            self.assertEqual(len(p['lights']), {'short': 5, 'medium': 7, 'long': 11}[variant])
            legacy_intensity, radius = (.10, 3) if p['id'].startswith('lodge_rail_') else (.32, 4.5)
            self.assertAlmostEqual(sum(light['intensity'] for light in p['lights']),
                                   legacy_intensity*SPANS[variant][2])
            self.assertTrue(all(light['range'] == radius for light in p['lights']))
            self.assertTrue(all(l['shape'] == 'point' and l['range'] <= 4.5 and
                                l['intensity'] <= .32 and l['color'][0] > l['color'][2]
                                for l in p['lights']))
            self.assertGreater(p['z'], -25, 'forest north remains unstrung')
        # Square/path attachments sit in the existing timber post model envelope.
        for p in (p for p in spans if p['id'].startswith(('square_string_', 'path_string'))):
            variant = p['model'].removeprefix('winter:string_lights_')
            angle = math.radians(p.get('rotation_degrees', 0))
            for side in (-1, 1):
                x = p['x'] + side*SPANS[variant][0]/2*math.cos(angle)
                z = p['z'] - side*SPANS[variant][0]/2*math.sin(angle)
                post = next(q for q in level['props'] if q['id'].startswith('string_post_')
                            and abs(q['x']-x) < .0001 and abs(q['z']-z) < .0001)
                self.assertLess(abs(post['x']), 4)
                self.assertGreater(abs(post['x']), 1.9, 'central packed path remains clear')
                self.assertLess(p['y']+attachment_height(variant), post['size'][1]*post['scale'])


if __name__ == '__main__':
    unittest.main()
