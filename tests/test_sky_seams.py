"""Sky longitude contracts: periodic U artwork, distinct clamped V poles."""
import json
from pathlib import Path
import unittest

from assets.inspection.png import decode_png

ROOT = Path(__file__).resolve().parents[1]


class SkySeamTests(unittest.TestCase):
    def test_all_shipped_panorama_edges_match(self):
        catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
        skies = [item for item in catalog['assets']
                 if item.get('asset_type') == 'texture' and item.get('surface') == 'sky']
        self.assertGreaterEqual(len(skies), 4)
        for item in skies:
            with self.subTest(sky=item['id']):
                width, height, rgba = decode_png((ROOT / 'assets' / item['model']).read_bytes())
                self.assertEqual(width, height * 2)
                self.assertLessEqual(width, 2048)
                self.assertEqual(width & (width - 1), 0)
                for row in range(height):
                    left = row * width * 4
                    right = left + (width - 1) * 4
                    self.assertEqual(rgba[left:left + 3], rgba[right:right + 3],
                                     f'longitude columns must join at row {row}')


if __name__ == '__main__':
    unittest.main()
