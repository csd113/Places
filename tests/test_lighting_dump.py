"""Focused saved-component and bounded chart-audit contracts for the dump inspector."""
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("lighting_dump", ROOT / "tools/bench/inspect_lighting_dump.py")
DUMP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DUMP)


class LightingDumpContract(unittest.TestCase):
    @staticmethod
    def chart(index=0, page=0, kind="floor", rectangle=None):
        return dict(index=index, room=0, kind=kind, page=page,
                    rectangle=rectangle or [2, 2, 2, 2], origin=[0, 0, 0],
                    u_axis=[1, 0, 0], v_axis=[0, 0, 1],
                    diagonal_correction=[0, 0, 0], geometric_normal=[0, 1, 0],
                    texels_per_metre=[1, 1], triangle=False)

    def test_audit_accepts_pages_eight_and_nine_with_independent_reservations(self):
        result = DUMP.audit_records([self.chart(page=8), self.chart(index=1, page=9, kind="prop")])
        self.assertEqual(result["error_count"], 0, result["errors"])
        self.assertEqual(result["pages"], 2)
        self.assertEqual(result["receiver_texels"], 8)

    def test_audit_rejects_page_ten_and_negative_pages_before_reservation(self):
        for page in (10, -1):
            with self.subTest(page=page):
                result = DUMP.audit_records([self.chart(page=page)])
                self.assertEqual(result["errors"], [dict(chart=0, reason="Invalid padded atlas bounds")])
                self.assertEqual(result["pages"], 0)
                self.assertEqual(result["receiver_texels"], 0)

    def test_audit_rejects_gutter_overlap_on_the_last_page(self):
        # Receiver rectangles are disjoint; only their two-texel world gutters overlap.
        records = [self.chart(page=9), self.chart(index=1, page=9, rectangle=[6, 2, 2, 2])]
        result = DUMP.audit_records(records)
        self.assertEqual(result["errors"], [dict(chart=1, reason="Overlapping padded atlas reservations")])
        records[1]["rectangle"] = [8, 2, 2, 2]
        self.assertEqual(DUMP.audit_records(records)["error_count"], 0)

    def test_audit_preserves_world_and_prop_edge_gutters(self):
        for kind, gutter in (("floor", 2), ("prop", 1)):
            for rectangle in ([gutter, gutter, 2, 2], [1024 - gutter - 2, 1024 - gutter - 2, 2, 2]):
                with self.subTest(kind=kind, rectangle=rectangle):
                    result = DUMP.audit_records([self.chart(page=9, kind=kind, rectangle=rectangle)])
                    self.assertEqual(result["error_count"], 0, result["errors"])
            for rectangle in ([gutter - 1, gutter, 2, 2], [gutter, gutter - 1, 2, 2],
                              [1024 - gutter - 1, gutter, 2, 2], [gutter, 1024 - gutter - 1, 2, 2]):
                with self.subTest(kind=kind, rectangle=rectangle):
                    result = DUMP.audit_records([self.chart(page=9, kind=kind, rectangle=rectangle)])
                    self.assertEqual(result["errors"], [dict(chart=0, reason="Invalid padded atlas bounds")])

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
