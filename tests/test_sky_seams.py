"""Sky longitude contracts: periodic U artwork, distinct clamped V poles."""
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/textures'))
from normalize_sky_seam import aligned, normalize
from seam_repair import PngImage, read_png


class SkySeamTests(unittest.TestCase):
    def test_all_shipped_panorama_edges_match(self):
        catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
        skies = [item for item in catalog['assets']
                 if item.get('asset_type') == 'texture' and item.get('surface') == 'sky']
        self.assertGreaterEqual(len(skies), 4)
        for item in skies:
            with self.subTest(sky=item['id']):
                image = read_png(str(ROOT / 'assets' / item['model']))
                self.assertEqual(image.width, image.height * 2)
                self.assertLessEqual(image.width, 2048)
                self.assertEqual(image.width & (image.width - 1), 0)
                self.assertTrue(aligned(image), 'longitude columns must join')

    def test_normalization_is_bounded_preserves_alpha_and_never_joins_poles(self):
        width, height, channels = 32, 16, 4
        pixels = bytes(value for y in range(height) for x in range(width)
                       for value in (20 + y + x // 8, 50 + y, 80 + x // 8, x + y))
        original = PngImage(width, height, 6, pixels, [(b'tEXt', b'origin\0test')])
        repaired = normalize(original, 4)
        self.assertTrue(aligned(repaired))
        self.assertEqual((repaired.width, repaired.height, repaired.colour_type), (width, height, 6))
        self.assertEqual(repaired.ancillary, original.ancillary)
        self.assertEqual(repaired.pixels[3::4], original.pixels[3::4])
        for y in range(height):
            start = (y * width + 4) * channels
            end = (y * width + width - 4) * channels
            self.assertEqual(repaired.pixels[start:end], original.pixels[start:end])
        stride = width * channels
        self.assertNotEqual(repaired.pixels[:stride], repaired.pixels[-stride:])
        self.assertEqual(normalize(repaired, 4).pixels, repaired.pixels)

    def test_invalid_contracts_are_rejected(self):
        with self.assertRaises(ValueError):
            normalize(PngImage(8, 8, 2, bytes(8 * 8 * 3), []))
        with self.assertRaises(ValueError):
            normalize(PngImage(32, 16, 2, bytes(32 * 16 * 3), []), 12)


if __name__ == '__main__':
    unittest.main()
