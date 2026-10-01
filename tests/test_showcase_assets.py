"""Lantern Hollow asset contracts: topology, albedo, animation and saved art."""
from pathlib import Path
import hashlib
import math
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/props'))
import glb
import geometry
from mesh import PropBuilder
from parts.showcase import MODEL_DIR, PROPS, SIZES
from tex import decode_png


def builder(prop_id):
    short = prop_id.removeprefix('outdoor:showcase_')
    p = PropBuilder(prop_id, short, SIZES[short])
    PROPS[prop_id](p)
    return p


class ShowcaseAssetTests(unittest.TestCase):
    def test_all_modules_are_closed_outward_and_within_shipped_budget(self):
        for prop_id in PROPS:
            with self.subTest(prop_id=prop_id):
                p = builder(prop_id)
                p.mesh.validate(p.size)
                audit = geometry.inspect(p.mesh.positions, p.mesh.indices)
                for field in ('boundary_edges', 'nonmanifold_edges', 'inconsistent_edges',
                              'flipped_triangles', 'degenerate', 'contradictory_components'):
                    self.assertEqual(audit[field], 0, (prop_id, field, audit))
                self.assertLessEqual(p.mesh.triangle_count, 800)
                parsed = glb.read_glb((MODEL_DIR / (prop_id.split(':')[1]+'.glb')).read_bytes())
                low, high = parsed.bounds()
                for axis in range(3):
                    self.assertAlmostEqual(high[axis]-low[axis], p.size[axis], places=5)
                self.assertAlmostEqual(low[1], 0, places=5)
                self.assertTrue(all(math.isfinite(v) for pos in parsed.positions for v in pos))

    def test_rebuild_preserves_saved_glb_bytes(self):
        for prop_id in PROPS:
            with self.subTest(prop_id=prop_id):
                p = builder(prop_id)
                payload = glb.write_glb(p.mesh, p.tex.png_bytes(), name=prop_id.replace(':', '_'),
                                        nodes=p.nodes, animations=p.clips)
                saved = (MODEL_DIR / (prop_id.split(':')[1]+'.glb')).read_bytes()
                self.assertEqual(hashlib.sha256(payload).digest(), hashlib.sha256(saved).digest())

    def test_original_pngs_retained_without_repainting(self):
        for source, copy in (('concrete_step.png', 'showcase_surface.png'),
                             ('fence_post.png', 'showcase_wood.png')):
            self.assertEqual((MODEL_DIR/source).read_bytes(), (MODEL_DIR/copy).read_bytes())
            w, h, rgba = decode_png((MODEL_DIR/copy).read_bytes())
            self.assertEqual((w,h), (128,128))
            self.assertTrue(all(a==255 for a in rgba[3::4]))

    def test_ordinary_surfaces_have_no_artificial_face_light_or_emission(self):
        for prop_id in PROPS:
            if prop_id.endswith('campfire'):
                continue
            with self.subTest(prop_id=prop_id):
                p = builder(prop_id)
                self.assertEqual(p.ao_strength, 0)
                # Material colours may differ between road marks and asphalt;
                # every closed primitive has one orientation-independent albedo.
                for offset in range(0, len(p.mesh.indices), 3):
                    colors = [p.mesh.colors[index] for index in p.mesh.indices[offset:offset+3]]
                    self.assertEqual(colors, [colors[0]]*3)
                self.assertFalse(any(slot.get('emissive') for slot in p.mesh.materials))
        stump = builder('outdoor:showcase_stump_seat')
        self.assertEqual(set(stump.mesh.colors), {(255,255,255)})

    def test_campfire_has_five_bounded_looping_flames_and_separate_emission(self):
        p = builder('outdoor:showcase_campfire')
        self.assertEqual(p.mesh.triangle_count, 744)
        self.assertEqual(len(p.nodes), 11)
        self.assertEqual(len(p.mesh.submeshes), 6)
        self.assertEqual(len(p.mesh.materials), 4)
        self.assertEqual(len(p.clips), 1)
        clip = p.clips[0]
        self.assertEqual(clip['name'], 'flicker')
        self.assertEqual(len(clip['channels']), 5)
        for channel in clip['channels']:
            self.assertEqual(channel['path'], 'scale')
            self.assertEqual(channel['times'][0], 0)
            self.assertEqual(channel['times'][-1], 1.25)
            self.assertEqual(channel['values'][0], channel['values'][-1])
            self.assertTrue(all(0 < value <= 1 for key in channel['values'] for value in key))
        self.assertFalse(p.mesh.materials[0]['emissive'])
        self.assertFalse(p.mesh.materials[1]['emissive'])
        self.assertTrue(p.mesh.materials[2]['emissive'])
        self.assertTrue(p.mesh.materials[3]['emissive'])
        self.assertTrue(any('[0,1.48,0]' in note for note in p.notes))


if __name__ == '__main__':
    unittest.main()
