"""Focused data-selection regression for the existing optional dump inspector."""
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


class LightingDumpContract(unittest.TestCase):
    def test_indirect_uses_saved_component_not_difference_of_reconstructions(self):
        with tempfile.TemporaryDirectory() as directory:
            dump = Path(directory)
            chart = dict(index=0, room=0, kind="floor", page=0,
                         rectangle=[2, 2, 2, 2], origin=[0, 0, 0],
                         u_axis=[1, 0, 0], v_axis=[0, 0, 1],
                         diagonal_correction=[0, 0, 0], geometric_normal=[0, 1, 0],
                         texels_per_metre=[1, 1], triangle=False)
            (dump / "charts.json").write_text(json.dumps([chart]))
            for stage, value in (("direct", 1.0), ("bounced", 1.0), ("indirect", 0.25)):
                (dump / (stage + ".rgb-f32le")).write_bytes(struct.pack("<12f", *([value] * 12)))
            output = dump / "indirect.png"
            result = subprocess.run([sys.executable, str(ROOT / "tools/bench/inspect_lighting_dump.py"),
                                     str(dump), "--stage", "indirect", "--maximum", "1",
                                     "--edge", "32", "--out", str(output)],
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            # Exact stored 0.25 maps to 136; bounced minus direct would be black.
            with Image.open(output) as image:
                self.assertEqual(image.getpixel((16, 64)), (136, 136, 136))


if __name__ == "__main__":
    unittest.main()
