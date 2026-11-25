"""Check delivered Winter materials, icicle dimensions and authored support."""
import json
from pathlib import Path
import unittest

from release.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]
from assets.inspection import glb


def load(name, theme='winter'):
    return glb.read_glb((ROOT / f'assets/environments/{theme}/props/{name}.glb').read_bytes())


def snow_indices(model):
    return [index for face in range(model.triangle_count)
            if model.material_names[model.triangle_materials[face]].split('.')[0] in ('snow', 'ice')
            for index in model.indices[face*3:face*3+3]]


class WinterAssetTests(unittest.TestCase):
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
        catalog = json.loads((ROOT / 'assets/catalog.json').read_text(encoding='utf8'))['assets']
        entries = [entry for entry in catalog if entry['id'].startswith('winter:icicle_')]
        self.assertEqual({entry['id'] for entry in entries}, {
            'winter:icicle_short', 'winter:icicle_medium', 'winter:icicle_long',
            'winter:icicle_cluster_mixed', 'winter:icicle_cluster_sparse',
        })
        for entry in entries:
            with self.subTest(asset=entry['id']):
                model = glb.read_glb((ROOT / 'assets' / entry['model']).read_bytes())
                low, high = model.bounds()
                self.assertAlmostEqual(low[1], 0)
                for a, b, extent in zip(low, high, entry['size']):
                    self.assertAlmostEqual(b-a, extent, delta=1e-5)
                self.assertEqual(model.material_names, ['ice'])
                material, = model.json['materials']
                self.assertEqual(material.get('alphaMode', 'OPAQUE'), 'OPAQUE')
                self.assertFalse(any(material.get('emissiveFactor', [])))
                roots = [point for point in set(model.positions) if abs(point[1]-high[1]) < 1e-5]
                self.assertTrue(roots)
                if 'cluster' in entry['id']:
                    xs = sorted(point[0] for point in roots)
                    self.assertGreater(max(b-a for a, b in zip(xs, xs[1:])), entry['size'][2]*.3)

    def test_exposed_snow_classes_preserve_collision_and_sheltered_deck_stays_dry(self):
        level = json.loads((environment_source('winter')).read_text())
        props = level['props']
        models = {p['model'] for p in props}
        required = {
            'tree_snow_01', 'tree_snow_02', 'drift_small', 'drift_medium', 'drift_large',
            'drift_wall', 'drift_fence', 'mound_irregular', 'boulder_snow', 'rock_face_snow',
            'snow_awning', 'snow_stair_edge', 'snow_ledge', 'snow_roof_blanket',
            'fence_two_rail_section_snow', 'fence_two_rail_post_snow',
            'icicle_short', 'icicle_medium', 'icicle_long',
            'icicle_cluster_mixed', 'icicle_cluster_sparse',
        }
        self.assertTrue({'winter:'+name for name in required} <= models)
        # Small roof/eave modules remain reusable library/Zoo assets; the
        # maintained houses use continuous blankets without duplicated ends.
        self.assertNotIn('winter:snow_roof_slope', models)
        self.assertNotIn('winter:snow_roof_edge', models)
        # The original simple hood overlay stays in the library/Zoo. Both
        # exposed cottage entrances now require the braced construction with
        # its own supported snow nose, rather than stacking the old cap on it.
        self.assertEqual(sum(p['model']=='winter:door_hood_snow' for p in props), 2)
        self.assertNotIn('winter:snow_door_overhang', models)
        self.assertTrue({'winter:stone_wall_snow', 'winter:masonry_pier_snow',
                         'winter:timber_lantern', 'winter:entrance_frame',
                         'winter:door_hood_snow', 'winter:window_frame',
                         'winter:ice_fragment'} <= models)
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

if __name__ == '__main__':
    unittest.main()
