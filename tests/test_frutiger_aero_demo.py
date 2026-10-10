"""Independent support, collision and reference-coverage checks for Aero."""
import json
import subprocess
import sys
import unittest
from pathlib import Path

from tests.test_beach_demo import authored_floor, collider_box, disc_touches

ROOT = Path(__file__).resolve().parents[1]


class AeroDemoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = json.loads((ROOT/'assets/levels/frutiger_aero_demo.json').read_text())
        cls.solids = [(p['id'], collider_box(cls.level, p)) for p in cls.level['props'] if p.get('solid')]

    def test_generator_is_current(self):
        result = subprocess.run([sys.executable, 'tools/levels/build_frutiger_aero.py', '--check'],
                                cwd=ROOT, text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)

    def test_every_aero_model_is_naturally_placed(self):
        catalog = json.loads((ROOT/'assets/catalog.json').read_text())
        expected = {a['id'] for a in catalog['assets'] if a.get('theme') == 'frutiger_aero'
                    and a['asset_type'] == 'prop'}
        placed = {p['model'] for p in self.level['props']}
        self.assertEqual(expected-placed, set())
        identities = [p['id'] for p in self.level['props']]
        self.assertEqual(len(identities), len(set(identities)))

    def test_connected_circulation_has_supported_body_clearance(self):
        # These reciprocal lanes deliberately pass through each true aperture;
        # the native controller campaign separately verifies real movement.
        lanes = [((10, 6), (27, 6)), ((6, 10), (6, 16)),
                 ((6, 15), (26.5, 15)), ((26.5, 15), (26.5, 9)),
                 ((2.0, 9), (4.1, 9)), ((4.1, 9), (8.0, 9)),
                 ((27, 7), (29, 7)), ((25.2, 2), (25.2, 4.0)), ((30.8, 7), (30.8, 2))]
        failures = []
        for start, end in lanes:
            for index in range(161):
                t = index/160
                x, z = (start[axis]+(end[axis]-start[axis])*t for axis in range(2))
                floor = authored_floor(self.level, x, z)
                if floor is None:
                    failures.append(('unsupported', x, z))
                    continue
                for identity, box in self.solids:
                    if box[4] < floor+1.8 and box[5] > floor+.4 and disc_touches(box, x, z, .30):
                        failures.append((identity, x, z))
        self.assertEqual(failures, [])

    def test_fountain_has_shallow_supported_water_and_fitted_rim(self):
        water, = self.level['water']
        self.assertEqual(water['shape'], 'circle')
        self.assertFalse(water['swimming'])
        self.assertLess(water['radius'], 1.32)
        self.assertGreater(water['surface_y'], water['bottom_y'])
        self.assertEqual(authored_floor(self.level, 6, 6), 0)
        supports = [box for identity, box in self.solids if identity.startswith('fountain_floor_')]
        self.assertTrue(any(disc_touches(box, 6.9, 6.3, .01) for box in supports))
        self.assertTrue(all(abs(box[5]-.095) < 1e-7 for box in supports))
        self.assertLess(water['surface_y']-.10, .55)
        rim = [box for identity, box in self.solids if identity.startswith('atrium_fountain_solid_')]
        self.assertEqual(len(rim), 20)
        self.assertTrue(all(not disc_touches(box, 6, 6, .30) for box in rim))

    def test_hollow_architecture_never_uses_its_visual_box(self):
        hollow = {'lime_arch', 'glass_canopy', 'atrium_dome', 'double_doors_open'}
        for prop in self.level['props']:
            if prop['model'].split(':')[-1] in hollow:
                self.assertFalse(prop['solid'], prop['id'])
        self.assertFalse(self.level.get('animated_emissions'))
        self.assertFalse(any(p.get('components') for p in self.level['props']))


if __name__ == '__main__':
    unittest.main()
