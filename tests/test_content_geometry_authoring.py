"""Check the outdoor perimeter adoption against its real historical solids.

These source controls prove occupied-box equality, retained content and
deterministic authoring. Compiled navigation and native geometry/captures are
separate gates; no player, compiler, bake or target directory is used here.
"""

import copy
from fractions import Fraction
import hashlib
import itertools
import json
from pathlib import Path
import struct
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/levels"))
import build_outdoor_fixture as outdoor  # noqa: E402

# Actual pre-adoption source pieces, in stable authored order. House side
# walls own the shortened lower terminal strips; they have no openings.
HISTORICAL_PERIMETER = (
    (0.0, 19.8, 26.0, 0.2),
    (0.0, 0.0, 0.2, 20.0),
    (25.8, 0.0, 0.2, 6.0),
    (25.8, 11.0, 0.2, 9.0),
    (0.0, 0.0, 2.5, 0.2),
    (17.5, 0.0, 8.5, 0.2),
)
HISTORICAL_HOUSE_SIDES = (
    dict(x=2.2, z=-4.1, width=0.3, depth=5.25, height=2.7),
    dict(x=17.5, z=-4.1, width=0.3, depth=5.25, height=2.7),
)
UNRELATED_CONTENT_SHA256 = "61c2fa17f8b94436277f92a1b5712bac9130ce8bcc874701062cfde8dec0b6d7"


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def solid_box(piece):
    """Exact f32 parsing/addition for the flat-floor witness, like LevelDef."""
    x, z = f32(piece["x"]), f32(piece["z"])
    y = f32(piece.get("y", 0.0))
    end_x = f32(x + f32(piece["width"]))
    end_z = f32(z + f32(piece["depth"]))
    end_y = f32(y + f32(piece["height"]))
    return tuple(Fraction(v) for v in (x, end_x, y, end_y, z, end_z))


def historical_boxes():
    pieces = [dict(x=x, z=z, width=w, depth=d, height=1.05)
              for x, z, w, d in HISTORICAL_PERIMETER]
    return [solid_box(p) for p in pieces + list(HISTORICAL_HOUSE_SIDES)]


def adopted_boxes(level):
    return [solid_box(p) for p in level["half_walls"] +
            [level["walls"][2], level["walls"][3]]]


def union_differences(before, after):
    """Compare every exact partition cell; finite closed boxes follow by closure."""
    axes = [sorted({p for box in before + after for p in box[axis:axis + 2]})
            for axis in (0, 2, 4)]
    witnesses = [[(a + b) / 2 for a, b in zip(axis, axis[1:])]
                 for axis in axes]

    def occupied(boxes, point):
        return any(all(box[2 * axis] < value < box[2 * axis + 1]
                       for axis, value in enumerate(point)) for box in boxes)

    differences = []
    checked = 0
    for point in itertools.product(*witnesses):
        checked += 1
        if occupied(before, point) != occupied(after, point):
            differences.append(point)
    return checked, differences


def same_facing_rectangular_overlaps(boxes):
    """Upper bound every triangle overlap with its exact full box face area."""
    areas = []
    for first, second in itertools.combinations(boxes, 2):
        for axis in range(3):
            for side in (0, 1):
                if first[2 * axis + side] != second[2 * axis + side]:
                    continue
                area = Fraction(1)
                for other in range(3):
                    if other == axis:
                        continue
                    low = max(first[2 * other], second[2 * other])
                    high = min(first[2 * other + 1], second[2 * other + 1])
                    area *= max(Fraction(0), high - low)
                if area:
                    areas.append(area)
    return areas


class OutdoorPerimeterAuthoringTests(unittest.TestCase):
    def setUp(self):
        self.level = json.loads(outdoor.OUTPUT_PATH.read_text())

    def test_generator_matches_maintained_source_and_repeats_exactly(self):
        first = outdoor.render(outdoor.level())
        self.assertEqual(first, outdoor.OUTPUT_PATH.read_text())
        self.assertEqual(first, outdoor.render(outdoor.level()))

    def test_all_six_real_perimeter_pieces_keep_material_height_and_thickness(self):
        pieces = self.level["half_walls"]
        self.assertEqual(len(pieces), 6)
        self.assertEqual(len(self.level["props"]), 328)
        self.assertEqual(len(self.level["walls"]), 8)
        self.assertEqual([self.level["walls"][2], self.level["walls"][3]],
                         list(HISTORICAL_HOUSE_SIDES))
        for piece in pieces:
            self.assertEqual(piece["height"], 1.05)
            self.assertEqual(min(piece["width"], piece["depth"]), 0.2)
            self.assertEqual(piece["material"], "outdoor:house_siding_01")
            self.assertNotIn("y", piece)
        self.assertFalse(self.level.get("floor_regions"))
        self.assertTrue(all(room.get("floor_y", 0.0) == 0.0
                            for room in self.level["rooms"]))

    def test_occupied_closed_solid_union_is_exact_after_f32_parsing(self):
        checked, differences = union_differences(historical_boxes(),
                                                 adopted_boxes(self.level))
        self.assertGreater(checked, 100)
        self.assertEqual(differences, [])

    def test_union_oracle_rejects_real_corner_and_terminal_gaps(self):
        for index, field in ((0, "width"), (4, "width"), (5, "width")):
            broken = copy.deepcopy(self.level)
            broken["half_walls"][index][field] -= 0.01
            _, differences = union_differences(historical_boxes(),
                                               adopted_boxes(broken))
            self.assertTrue(differences, (index, field))

    def test_adoption_removes_confirmed_same_facing_overlap_without_tolerance_change(self):
        old = same_facing_rectangular_overlaps(historical_boxes())
        new = same_facing_rectangular_overlaps(adopted_boxes(self.level))
        self.assertGreater(max(old), Fraction("0.001"))
        # A rectangle bounds each emitted triangle. Remaining exact f32 endpoint
        # slivers are below even the existing report threshold, not just Error.
        self.assertLess(max(new, default=Fraction(0)), Fraction("0.0001"))

    def test_unrelated_authored_content_and_asset_references_stay_exact(self):
        unrelated = {k: v for k, v in self.level.items() if k != "half_walls"}
        canonical = json.dumps(unrelated, sort_keys=True, separators=(",", ":"))
        self.assertEqual(hashlib.sha256(canonical.encode()).hexdigest(),
                         UNRELATED_CONTENT_SHA256)


if __name__ == "__main__":
    unittest.main()
