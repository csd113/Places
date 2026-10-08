"""Protect canonical geometry, supported snow, sealed normals and icicle mounts."""
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
from parts import winter, winter_village


def load(name, theme='winter'):
    return glb.read_glb((ROOT / f'assets/environment/{theme}/props/models/{name}.glb').read_bytes())


def snow_indices(model):
    return [index for face in range(model.triangle_count)
            if model.material_names[model.triangle_materials[face]] in ('snow', 'ice')
            for index in model.indices[face*3:face*3+3]]


class WinterAssetTests(unittest.TestCase):
    def test_variants_preserve_the_canonical_geometry_uvs_artwork_and_alpha(self):
        pairs = [('tree_snow_01', 'tree_03'), ('tree_snow_02', 'tree_03'),
                 ('boulder_snow', 'showcase_boulder'), ('rock_face_snow', 'showcase_rock_face'),
                 ('railing_snow_straight', 'porch_railing_straight'),
                 ('railing_snow_corner', 'porch_railing_corner'),
                 ('railing_snow_end', 'porch_railing_end'), ('fence_post_snow', 'fence_post')]
        for variant, canonical in pairs:
            with self.subTest(variant=variant):
                base, snow = load(canonical, 'outdoor'), load(variant)
                count = len(base.positions)
                self.assertEqual(snow.positions[:count], base.positions)
                self.assertEqual(snow.uvs[:count], base.uvs)
                self.assertEqual(snow.colors[:count], base.colors)
                self.assertEqual(snow.indices[:len(base.indices)], base.indices)
                self.assertEqual(snow.json['materials'][:base.material_count], base.json['materials'])
                self.assertEqual(snow.texture_png, base.texture_png)
                self.assertIn('snow_accumulation', snow.mesh_names)
                snow_material = next(m for m in snow.json['materials'] if m['name'] == 'snow')
                self.assertNotIn('baseColorTexture', snow_material['pbrMetallicRoughness'])

    def test_snow_and_icicles_are_closed_outward_and_within_budget(self):
        for name, size in winter.SIZES.items():
            with self.subTest(asset=name):
                model = load(name)
                report = geometry.inspect(model.positions, snow_indices(model))
                for field in ('degenerate', 'boundary_edges', 'nonmanifold_edges',
                              'inconsistent_edges', 'flipped_triangles', 'contradictory_components'):
                    self.assertEqual(report[field], 0, (name, report))
                self.assertLessEqual(model.triangle_count, 1500)
                self.assertTrue(all(math.isfinite(v) and 0 <= v <= 1 for uv in model.uvs for v in uv))
                low, high = model.bounds()
                self.assertAlmostEqual(low[1], 0, places=5)
                for axis in range(3):
                    self.assertLessEqual(abs(high[axis]-low[axis]-size[axis]), max(.02, size[axis]*.06))

    def test_tree_snow_is_supported_by_opaque_branches_and_varies_in_load(self):
        base = load('tree_03', 'outdoor')
        branches = list(winter._triangles(base, 1))
        counts = []
        for name in ('tree_snow_01', 'tree_snow_02'):
            model = load(name)
            indices = snow_indices(model)
            counts.append(len(indices))
            for index in set(indices):
                x, y, z = model.positions[index]
                support = winter.surface_height(branches, x, z)
                self.assertIsNotNone(support, (name, x, y, z))
                self.assertLessEqual(y-support, .17, 'snow cannot float above a branch')
            # Neither load replaces or recolours any dark-green base vertex.
            self.assertGreater(len(base.indices), len(indices))
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

    def test_registered_builders_reproduce_the_shipped_kit(self):
        for name, size in winter.SIZES.items():
            with self.subTest(asset=name):
                p = PropBuilder('winter:'+name, name, size)
                winter.PROPS[p.id](p)
                p.mesh.validate(size)
                self.assertEqual(glb.write_glb(p.mesh, p.tex.png_bytes(), name=p.id.replace(':', '_'),
                                              nodes=p.nodes, animations=p.clips),
                                 (winter.WINTER / (name+'.glb')).read_bytes())
        for name, size in winter_village.SIZES.items():
            with self.subTest(village_asset=name):
                p = PropBuilder('winter:'+name, name, size)
                winter_village.PROPS[p.id](p)
                p.mesh.validate(size)
                self.assertEqual(glb.write_glb(p.mesh, p.tex.png_bytes(), name=p.id.replace(':', '_'),
                                              nodes=p.nodes, animations=p.clips),
                                 (winter_village.ROOT / (name+'.glb')).read_bytes())

    def test_exposed_snow_classes_preserve_collision_and_sheltered_deck_stays_dry(self):
        level = json.loads((ROOT / 'assets/levels/winter.json').read_text())
        props = level['props']
        models = {p['model'] for p in props}
        for name in winter.SIZES:
            if not name.startswith('railing_snow') and name not in ('fence_post_snow', 'snow_porch_edge', 'snow_door_overhang'):
                self.assertIn('winter:'+name, models)
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
                expected = {'winter:stone_wall_snow':[.5,.98,2.4],
                            'winter:masonry_pier_snow':[.65,1.16,.65],
                            'winter:timber_lantern':[.155,2.6,.155]}.get(p['model'])
                if expected is None:
                    expected = [1.4, .85, 1.3] if 'boulder' in p['model'] else [4, 5, 2.2] if 'rock_face' in p['model'] else [.6, 6.8, .6]
                self.assertEqual(p['size'], expected, p)
        self.assertEqual(len(level['guardrails']), 3)
        self.assertEqual(level['stairs'][0]['rise'], .6)
        self.assertEqual(level['stairs'][0]['steps'], 3)
        self.assertFalse(level.get('water'))

    def test_raised_lodge_frames_align_with_the_real_openings(self):
        level = json.loads((ROOT / 'assets/levels/winter.json').read_text())
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
                self.assertLess(model.triangle_count,800)
                self.assertTrue(all(math.isfinite(v) and 0 <= v <= 1 for uv in model.uvs for v in uv))
                low,high=model.bounds()
                self.assertAlmostEqual(low[1],0,places=5)
                for axis in range(3):
                    self.assertAlmostEqual(high[axis]-low[axis],size[axis],places=4)
                for material in model.json['materials']:
                    if material['name']!='lantern_amber':
                        self.assertNotIn('emissiveFactor',material)


if __name__ == '__main__':
    unittest.main()
