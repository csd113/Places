"""Protect the reusable asset, source alignment, mounting and bounded light cost."""
import json
from pathlib import Path
import unittest

from release.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]
from assets.inspection import glb
from tests.test_winter_assemblies import world_point

# Measured attachment centres and accepted light budgets of the installed spans.
# Width is between the two mounts, not the GLB's outer hardware envelope.
SPAN_FIXTURES = {'short': (2.8, .37, 3), 'medium': (4.0, .50, 5), 'long': (6.6, .62, 7)}


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
        level = json.loads(environment_source('winter').read_text())
        for variant, count in (('short', 5), ('medium', 7), ('long', 11)):
            with self.subTest(variant=variant):
                path = ROOT / f'assets/environments/winter/props/string_lights_{variant}.glb'
                model = glb.read_glb(path.read_bytes())
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
                # Separate bulbs by the gap between their delivered glass intervals.
                bulbs = []
                for point in sorted(material_points(model, emitting[0])):
                    if not bulbs or point[0]-bulbs[-1][-1][0] > .1:
                        bulbs.append([])
                    bulbs[-1].append(point)
                self.assertEqual(len(bulbs), count)
                spans = [p for p in level['props'] if p['model'] == f'winter:string_lights_{variant}']
                self.assertTrue(spans)
                for prop in spans:
                    sources = sorted(prop['lights'], key=lambda light: light['offset'][0])
                    self.assertEqual(len(sources), count)
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
        self.assertEqual({i for i, material in enumerate(model.json['materials'])
                          if any(material.get('emissiveFactor', []))}, {slot})
        glass = material_points(model, slot)
        level = json.loads(environment_source('winter').read_text())
        lamp_ids = {'square_lamp_0', 'square_lamp_1'}
        lanterns = [p for p in level['props'] if p['id'] in lamp_ids]
        self.assertEqual({p['id'] for p in lanterns}, lamp_ids)
        for prop in lanterns:
            self.assertEqual(prop['model'], 'winter:timber_lantern')
            light, = prop['lights']
            x, y, z = light['offset']
            self.assertEqual(light, {'shape': 'point', 'offset': [x, y, z], 'intensity': 1.5,
                                     'range': 14, 'color': [1, .81, .55], 'falloff': 'smooth'})
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

    def test_mounts_are_supported_clear_of_walks_and_light_budget_stays_local(self):
        level = json.loads(environment_source('winter').read_text())
        spans = [p for p in level['props'] if p['model'].startswith('winter:string_lights_')]
        self.assertEqual(len(spans), 8)
        self.assertEqual(sum(len(p['lights']) for p in spans), 60)
        catalog = {asset['id']: asset['model'] for asset in
                   json.loads((ROOT / 'assets/catalog.json').read_text(encoding='utf8'))['assets']
                   if 'model' in asset}
        boxes = {box['id']: box for box in level['void_walls']}
        for p in spans:
            with self.subTest(span=p['id']):
                self.assertFalse(p.get('solid', False))
                self.assertFalse(p['occludes'])
                self.assertNotIn('scale', p, 'repeat spans instead of stretching bulbs')
                variant = p['model'].removeprefix('winter:string_lights_')
                width, mount_height, budget_units = SPAN_FIXTURES[variant]
                self.assertEqual(len(p['lights']), {'short': 5, 'medium': 7, 'long': 11}[variant])
                rail = p['id'].startswith('lodge_rail_')
                intensity, radius = (.10, 3) if rail else (.32, 4.5)
                self.assertAlmostEqual(sum(light['intensity'] for light in p['lights']),
                                       intensity*budget_units)
                self.assertTrue(all(light['range'] == radius for light in p['lights']))
                self.assertTrue(all(l['shape'] == 'point' and l['range'] <= 4.5 and
                                    l['intensity'] <= .32 and l['color'][0] > l['color'][2]
                                    for l in p['lights']))
                self.assertGreater(p['z'], -25, 'forest north remains unstrung')
                mesh = glb.read_glb((ROOT / 'assets' / catalog[p['model']]).read_bytes())
                low, high = mesh.bounds()
                self.assertAlmostEqual(high[0] - width/2, .025, delta=1e-5)
                self.assertAlmostEqual(-low[0] - width/2, .025, delta=1e-5)
                self.assertAlmostEqual(high[1] - mount_height, .025, delta=1e-5)
                for side in (-1, 1):
                    point = world_point(level, p, (side*width/2, mount_height, 0))
                    if p['id'].startswith('home_') or rail:
                        stem = (f'home_{p["id"].split("_")[1]}_string_upstand'
                                if not rail else f'lodge_rail_string_mount_{p["id"].split("_")[-1]}')
                        mount = boxes[f'{stem}_{side}']
                        self.assertTrue(all(a < v < b for a, v, b in
                                            zip(mount['min'], point, mount['max'])), (point, mount))
                        continue
                    prefix = ('porch_post_' if p['id'] == 'lodge_porch_string'
                              else 'string_post_'+str(2 if p['id'] == 'path_string'
                                                      else int(p['id'].split('_')[-1]))+'_')
                    supported = False
                    for post in level['props']:
                        if not post['id'].startswith(prefix):
                            continue
                        model = glb.read_glb((ROOT / 'assets' / catalog[post['model']]).read_bytes())
                        placed = [world_point(level, post, q) for q in model.positions]
                        fits = all(min(q[i] for q in placed)-half < point[i] <
                                   max(q[i] for q in placed)+half
                                   for i, half in enumerate((.025, .025, .02)))
                        if fits:
                            if prefix.startswith('string_post_'):
                                self.assertLess(abs(post['x']), 4)
                                self.assertGreater(abs(post['x']), 1.9,
                                                   'central packed path remains clear')
                            supported = True
                    self.assertTrue(supported, (p['id'], point))


if __name__ == '__main__':
    unittest.main()
