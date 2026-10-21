"""Protect modular stock, closed snow shells, material roles and icicle mounts."""
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
from mesh import PropBuilder
from parts import winter, winter_village


def load(name, theme='winter'):
    return glb.read_glb((ROOT / f'assets/environments/{theme}/props/{name}.glb').read_bytes())


def snow_indices(model):
    return [index for face in range(model.triangle_count)
            if model.material_names[model.triangle_materials[face]].split('.')[0] in ('snow', 'ice')
            for index in model.indices[face*3:face*3+3]]


class WinterAssetTests(unittest.TestCase):
    def test_snow_and_icicles_are_closed_outward_and_grounded(self):
        sizes = dict(winter.SIZES)
        sizes.update((name + '_assembly', winter.SIZES[name])
                     for name in ('railing_snow_straight', 'railing_snow_corner'))
        for name, size in sizes.items():
            with self.subTest(asset=name):
                model = load(name)
                report = geometry.inspect(model.positions, snow_indices(model))
                for field in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                              'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
                    self.assertEqual(report[field], 0, (name, report))
                # Generic shipped-asset validation owns reviewed geometry budgets.
                self.assertTrue(all(math.isfinite(v) and 0 <= v <= 1 for uv in model.uvs for v in uv))
                low, high = model.bounds()
                clearance = (.14 if name in ('railing_snow_straight', 'railing_snow_corner')
                             else .345 if name == 'fence_two_rail_section_snow' else 0)
                self.assertAlmostEqual(low[1], clearance, places=5)
                for axis in range(3):
                    extent = high[axis] if axis == 1 and clearance else high[axis]-low[axis]
                    self.assertLessEqual(abs(extent-size[axis]), max(.02, size[axis]*.06))

    def test_tree_snow_keeps_opaque_foliage_and_bark_and_varies_in_load(self):
        # Independent completed trees no longer contain an Outdoors vertex
        # prefix. Keep their visible material roles and distinct snow loads,
        # without the old quadratic probe against an unrelated tree mesh.
        counts = []
        for name in ('tree_snow_01', 'tree_snow_02'):
            model = load(name)
            self.assertEqual(set(model.material_names), {'snow', 'foliage', 'bark'})
            for slot, material in enumerate(model.json['materials']):
                self.assertIn(slot, model.triangle_materials)
                self.assertEqual(material.get('alphaMode', 'OPAQUE'), 'OPAQUE')
                self.assertFalse(any(material.get('emissiveFactor', [])))
                self.assertEqual(material['pbrMetallicRoughness'].get('metallicFactor', 0), 0)
            counts.append(len(snow_indices(model)))
        self.assertGreater(counts[1], counts[0])

    def test_icicle_cluster_roots_share_one_mount_plane_and_do_not_touch(self):
        for name in winter.SIZES:
            if not name.startswith('icicle_'):
                continue
            model = load(name)
            height = winter.SIZES[name][1]
            roots = [v for v in set(model.positions) if abs(v[1]-height) < 1e-5]
            self.assertTrue(roots)
            # Each pentagonal root is separated from its neighbouring spike.
            xs = sorted(v[0] for v in roots)
            if 'cluster' in name:
                self.assertGreater(max(b-a for a, b in zip(xs, xs[1:])), winter.SIZES[name][2]*.3)

    def test_exposed_snow_classes_preserve_collision_and_sheltered_deck_stays_dry(self):
        level = json.loads((environment_source('winter')).read_text())
        props = level['props']
        models = {p['model'] for p in props}
        for name in winter.SIZES:
            if not name.startswith('railing_snow') and name not in (
                    'fence_post_snow', 'snow_porch_edge', 'snow_door_overhang',
                    'snow_roof_slope', 'snow_roof_edge', 'snow_rail_top', 'snow_post_cap'):
                self.assertIn('winter:'+name, models)
        # Small roof/eave modules remain reusable library/Zoo assets; the
        # maintained houses use continuous blankets without duplicated ends.
        self.assertNotIn('winter:snow_roof_slope', models)
        self.assertNotIn('winter:snow_roof_edge', models)
        # The original simple hood overlay stays in the library/Zoo. Both
        # exposed cottage entrances now require the braced construction with
        # its own supported snow nose, rather than stacking the old cap on it.
        self.assertEqual(sum(p['model']=='winter:door_hood_snow' for p in props), 2)
        self.assertNotIn('winter:snow_door_overhang', models)
        for name in winter_village.SIZES:
            self.assertIn('winter:'+name, models)
        # The covered porch must not be populated solely to exhibit this cap.
        # Library/Model Zoo coverage still checks all reusable asset classes.
        self.assertNotIn('winter:snow_porch_edge', models)
        awning = next(b for b in level['void_walls'] if b['id'] == 'lodge_awning')
        for p in props:
            if not p['model'].startswith(('winter:snow_', 'winter:drift_', 'winter:mound_')):
                continue
            if awning['min'][0] < p['x'] < awning['max'][0] and awning['min'][2] < p['z'] < awning['max'][2]:
                self.assertGreater(p.get('y', 0) + .6, awning['max'][1]-.01,
                                   'snow under the sealed lodge awning')
        for p in props:
            if p['model'].startswith('winter:') and p.get('solid'):
                expected = {'winter:stone_wall_snow':[2.4,.98,.5],
                            'winter:masonry_pier_snow':[.65,1.16,.65],
                            'winter:timber_lantern':[.155,2.6,.155],
                            'winter:fence_two_rail_section_snow':[1.78,.845,.065],
                            'winter:fence_two_rail_post_snow':[.12,1.05,.12]}.get(p['model'])
                if expected is None:
                    expected = [1.4, .85, 1.3] if 'boulder' in p['model'] else [4, 5, 2.2] if 'rock_face' in p['model'] else [.6, 6.8, .6]
                self.assertEqual(p['size'], expected, p)
        self.assertFalse(level['guardrails'])
        self.assertEqual(level['stairs'][0]['rise'], .6)
        self.assertEqual(level['stairs'][0]['steps'], 3)
        self.assertFalse(level.get('water'))

    def test_raised_lodge_frames_align_with_the_real_openings(self):
        level = json.loads((environment_source('winter')).read_text())
        props = {p['id']:p for p in level['props']}
        # These anchors lie on the 0.6 m raised deck. Floor-relative prop Y
        # must not add that rise again: the window's real sill is world 1.6 m.
        for j in range(2):
            base = .6 + props[f'home_0_window_{j}'].get('y',0)
            self.assertLessEqual(base,1.6)
            self.assertGreaterEqual(base+1.16,2.6)
        self.assertEqual(props['home_0_doorway'].get('y',0),0)

    def test_village_construction_is_closed_grounded_and_emission_stays_on_glass(self):
        for name,size in winter_village.SIZES.items():
            with self.subTest(asset=name):
                model=load(name)
                report=geometry.inspect(model.positions, model.indices)
                for field in ('degenerate','boundary_edges','nonmanifold_edges','inconsistent_edges',
                              'flipped_triangles','contradictory_components'):
                    self.assertEqual(report[field],0,(name,report))
                # Reviewed triangle limits are enforced by the shared prop gate.
                self.assertTrue(all(math.isfinite(v) and 0 <= v <= 1 for uv in model.uvs for v in uv))
                low,high=model.bounds()
                self.assertAlmostEqual(low[1],0,places=5)
                for axis in range(3):
                    self.assertAlmostEqual(high[axis]-low[axis],size[axis],places=4)
                emitters = []
                for material in model.json['materials']:
                    if any(material.get('emissiveFactor', [])):
                        self.assertEqual(name, 'timber_lantern')
                        self.assertEqual(material['name'], 'winter_bulb_emitter')
                        emitters.append(material)
                self.assertEqual(len(emitters), 1 if name == 'timber_lantern' else 0)


if __name__ == '__main__':
    unittest.main()
