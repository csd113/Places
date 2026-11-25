"""Shipped Lantern Hollow texture and campfire animation contracts."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
from assets.inspection import glb
from assets.inspection.png import decode_png

MODEL_DIR = ROOT / "assets/environments/outdoor/props"


class ShowcaseAssetTests(unittest.TestCase):
    def test_retained_showcase_pngs_are_opaque_native_sheets(self):
        # Accepted Outdoors art supersedes the old source-copy relationship.
        for name in ('showcase_surface.png', 'showcase_wood.png'):
            w, h, rgba = decode_png((MODEL_DIR/name).read_bytes())
            self.assertEqual((w,h), (128,128))
            self.assertTrue(all(a==255 for a in rgba[3::4]))

    def test_campfire_has_five_bounded_looping_flames_and_separate_emission(self):
        package = (MODEL_DIR / 'showcase_campfire.glb').read_bytes()
        document, binary = glb.read_glb_chunks(package)
        clip, = document['animations']
        self.assertEqual(clip['name'], 'flicker')
        self.assertEqual(len(clip['channels']), 5)
        targets = set()
        for channel in clip['channels']:
            target = channel['target']
            targets.add(document['nodes'][target['node']]['name'])
            self.assertEqual(target['path'], 'scale')
            sampler = clip['samplers'][channel['sampler']]
            times = glb._read_accessor(document, binary, sampler['input'])
            values = glb._read_accessor(document, binary, sampler['output'])
            self.assertEqual(len(times), len(values))
            self.assertEqual(times[0], (0,))
            self.assertAlmostEqual(times[-1][0], 1.25, delta=1e-6)
            self.assertTrue(all(a < b for a, b in zip(times, times[1:])))
            for a, b in zip(values[0], values[-1]):
                self.assertAlmostEqual(a, b, delta=1e-6)
            self.assertTrue(all(0 < value <= 1 for key in values for value in key))
        self.assertEqual(targets, {f'flame_node_{i}' for i in range(5)})
        self.assertEqual({material['name'] for material in document['materials']
                          if any(material.get('emissiveFactor', []))},
                         {'campfire_flame', 'campfire_embers'})


if __name__ == '__main__':
    unittest.main()
