"""Protect the reusable asset, source alignment, mounting and bounded light cost."""
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/props'))
import glb
import geometry
from mesh import PropBuilder
from parts import string_lights as mesh
from string_lights import SPANS, BULB_HEIGHT, attachment_height, bulbs, lights, size


class StringLightTests(unittest.TestCase):
    def test_closed_modular_meshes_preserve_bulb_size_and_emission_isolation(self):
        for variant, (length, sag, count) in SPANS.items():
            with self.subTest(variant=variant):
                path = ROOT / f'assets/environment/winter/props/models/string_lights_{variant}.glb'
                model = glb.read_glb(path.read_bytes())
                report = geometry.inspect(model.positions, model.indices)
                for field in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                              'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
                    self.assertEqual(report[field], 0)
                self.assertLess(model.triangle_count, 800)
                self.assertAlmostEqual(model.bounds()[0][1], 0)
                self.assertTrue(all(0 <= v <= 1 for uv in model.uvs for v in uv))
                glass = next(m for m in model.json['materials'] if m['name'] == 'warm_bulb')
                cable = next(m for m in model.json['materials'] if m['name'] == 'dark_cable')
                self.assertEqual(glass['emissiveFactor'], [1, .56, .16])
                self.assertEqual(glass.get('extensions', {}).get('KHR_materials_emissive_strength', {})
                                 .get('emissiveStrength', 1), 1)
                self.assertNotIn('emissiveFactor', cable)
                self.assertEqual(len(lights(variant)), count)
                for source, (x, top) in zip(lights(variant), bulbs(variant)):
                    self.assertAlmostEqual(source['offset'][0], x, places=5)
                    self.assertAlmostEqual(source['offset'][1], top-BULB_HEIGHT-.02, places=5)
                    # The downward source ray starts below every matching bulb.
                    self.assertLess(source['offset'][1], top-BULB_HEIGHT)
                self.assertAlmostEqual(attachment_height(variant), sag+.22)
                p = PropBuilder('winter:string_lights_'+variant, variant, size(variant))
                mesh.build(p, variant)
                p.mesh.validate(size(variant))
                self.assertEqual(glb.write_glb(p.mesh, p.tex.png_bytes(), name=p.id.replace(':', '_'),
                                              nodes=p.nodes, animations=p.clips), path.read_bytes())

    def test_mounts_are_supported_clear_of_walks_and_light_budget_stays_local(self):
        level = json.loads((ROOT / 'assets/levels/winter.json').read_text())
        spans = [p for p in level['props'] if p['model'].startswith('winter:string_lights_')]
        self.assertEqual(len(spans), 8)
        self.assertEqual(sum(len(p['lights']) for p in spans), 38)
        for p in spans:
            self.assertFalse(p.get('solid', False))
            self.assertFalse(p['occludes'])
            self.assertNotIn('scale', p, 'repeat spans instead of stretching bulbs')
            self.assertLessEqual(len(p['lights']), 8)
            self.assertTrue(all(l['shape'] == 'point' and l['range'] <= 4.5 and
                                l['intensity'] <= .32 and l['color'][0] > l['color'][2]
                                for l in p['lights']))
            self.assertGreater(p['z'], -25, 'forest north remains unstrung')
        # Square/path attachments sit in the existing timber post model envelope.
        for p in (p for p in spans if p['id'].startswith(('square_string_', 'path_string'))):
            variant = p['model'].removeprefix('winter:string_lights_')
            for side in (-1, 1):
                x = p['x'] + side*SPANS[variant][0]/2
                post = next(q for q in level['props'] if q['id'].startswith('string_post_')
                            and math.isclose(q['x'], x) and math.isclose(q['z'], p['z']))
                self.assertLess(abs(post['x']), 4)
                self.assertGreater(abs(post['x']), 1.9, 'central packed path remains clear')
                self.assertLess(p['y']+attachment_height(variant), post['size'][1]*post['scale'])


if __name__ == '__main__':
    unittest.main()
