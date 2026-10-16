"""Tiny offline overlap fixtures: no map bake, renderer, PNG writer or GPU.

Each fixture protects a repair boundary (attributes/layers/morphs), an actual
plane-bin miss, the work bound, or affine UV clipping. The suite uses at most
three triangles per audit and one in-memory GLB; its cost is milliseconds.
"""
from dataclasses import replace
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools/props"))
from geometry import SurfaceTriangle, clip_attribute_triangle, glb_triangles, overlap_report, transformed_triangles
from repair_geometry import remove_duplicate_triangles, instance_overlap_audit


def triangle(reference="a", *, z=0.0):
    points = ((0.0, 0.0, z), (1.0, 0.0, z), (0.0, 1.0, z))
    records = tuple((("POSITION", point), ("TEXCOORD_0", point[:2])) for point in points)
    return SurfaceTriangle(reference, points, "wood", "OPAQUE", records, "primitive0")


def tiny_glb(*, morph=False, weights=None):
    positions = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    blob = b"".join(struct.pack("<3f", *p) for p in positions)
    blob += b"".join(struct.pack("<2f", *p[:2]) for p in positions)
    blob += struct.pack("<6H", 0, 1, 2, 0, 1, 2)
    primitive = {"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "indices": 2}
    if morph:
        primitive["targets"] = [{"POSITION": 0}]
    mesh = {"primitives": [primitive]}
    if weights is not None:
        mesh["weights"] = weights
    document = {"asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}],
                "nodes": [{"mesh": 0}], "meshes": [mesh],
                "buffers": [{"byteLength": len(blob)}],
                "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36},
                                {"buffer": 0, "byteOffset": 36, "byteLength": 24},
                                {"buffer": 0, "byteOffset": 60, "byteLength": 12}],
                "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                              {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
                              {"bufferView": 2, "componentType": 5123, "count": 6, "type": "SCALAR"}]}
    payload = json.dumps(document).encode()
    payload += b" " * (-len(payload) % 4)
    return (struct.pack("<III", 0x46546C67, 2, 28 + len(payload) + len(blob))
            + struct.pack("<II", len(payload), 0x4E4F534A) + payload
            + struct.pack("<II", len(blob), 0x004E4942) + blob)


class OfflineOverlapTests(unittest.TestCase):
    def test_only_exact_full_record_opaque_duplicates_are_deletable(self):
        first = triangle()
        variants = [replace(first, reference="b", material="paint"),
                    replace(first, reference="b", alpha_mode="BLEND"),
                    replace(first, reference="b", owner="another primitive"),
                    replace(first, reference="b", records=tuple(reversed(first.records))),
                    triangle("b", z=0.0001)]
        for second in variants:
            with self.subTest(second=second):
                report = overlap_report([first, second])
                self.assertTrue(report["findings"])
                self.assertEqual(report["repair_plan"], [])
        report = overlap_report([first, replace(first, reference="b"), replace(first, reference="c")])
        self.assertEqual([row["reference"] for row in report["repair_plan"]], ["b", "c"])

    def test_neighbor_planes_translations_and_world_instances(self):
        # A third face fixes the shared origin across the plane-bin boundary.
        anchor = replace(triangle("anchor"), points=((-10, 0, 0), (-9, 0, 0), (-10, 1, 0)))
        faces = [anchor, triangle("a", z=0.00299), triangle("b", z=0.00301)]
        report = overlap_report(faces)
        self.assertEqual(report["totals"], {"near-coplanar": 1})
        matrix = (1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 10000, 5000, -10000, 1)
        moved = overlap_report(transformed_triangles(faces, matrix, owner="props/7"))
        self.assertEqual(report["totals"], moved["totals"])
        self.assertTrue(moved["findings"][0]["first"].startswith("props/7/"))

    def test_budget_exhaustion_never_publishes_partial_repairs(self):
        report = overlap_report([triangle(str(i)) for i in range(3)], max_candidates=1)
        self.assertFalse(report["complete"])
        self.assertEqual(report["repair_plan"], [])

    def test_distant_anchor_and_dominant_axis_ties_do_not_hide_near_planes(self):
        # These two distinct failures cannot be represented by the old rounded
        # plane bins: remote normal-distance drift and a dominant-axis tie.
        a = replace(triangle("a"), points=((1000, 0, 0), (1001, 0, 0), (1000, 1, 0)))
        b = replace(triangle("b"), points=((1000, 0, 0), (1001, 0, 0.00001), (1000, 1, 0)))
        self.assertEqual(overlap_report([a, b])["totals"], {"near-coplanar": 1})
        self.assertEqual(overlap_report([triangle("remote"), a, b])["totals"], {"near-coplanar": 1})
        a = replace(a, points=((0, 0, 0), (1, -0.99999, 0), (0, 0, 1)))
        b = replace(b, points=((0, 0, 0), (1, -1.00001, 0), (0, 0, 1)))
        self.assertEqual(overlap_report([a, b])["totals"], {"near-coplanar": 1})
        # A unit normal's dropped coordinate can be sqrt(3) times farther
        # apart than its measured plane distance; a 3 mm AABB pad misses this.
        a = replace(a, points=((0, 0, 0), (-0.0001, 0.0001, 0), (0, 0, 10)))
        b = replace(b, points=((0.004, 0, 0), (0.0039, 0.0001, 0), (0.004, 0, 10)))
        self.assertEqual(overlap_report([a, b])["totals"], {"near-coplanar": 1})

    def test_morph_base_duplicates_and_default_weights_are_refused(self):
        # Runtime morphs can separate base-identical faces. Both declaration
        # paths must fail before the generic duplicate planner sees the base.
        for raw in (tiny_glb(morph=True), tiny_glb(weights=[0.0])):
            with self.assertRaisesRegex(ValueError, "morph.*pose review"):
                glb_triangles(raw)

    def test_duplicate_apply_preserves_records_and_second_plan_is_empty(self):
        raw = tiny_glb()
        source = "tiny.glb"
        before = glb_triangles(raw, source=source)
        report = overlap_report(before)
        output = remove_duplicate_triangles(raw, report["repair_plan"], source=source)
        after = glb_triangles(output, source=source)
        self.assertEqual(len(after), 1)
        self.assertEqual(after[0].records, before[0].records)
        self.assertEqual(overlap_report(after)["repair_plan"], [])

    def test_unknown_glb_chunk_is_readable_but_refused_before_duplicate_repair(self):
        # A valid unknown chunk before BIN must never become the writer's
        # vertex buffer. Inspection may ignore it; a JSON/BIN-only repair
        # cannot preserve it and must refuse before producing any payload.
        raw = tiny_glb()
        boundary = 20 + struct.unpack_from("<I", raw, 12)[0]
        unknown = struct.pack("<II", 4, 0x12345678) + b"ABCD"
        extended = bytearray(raw[:boundary] + unknown + raw[boundary:])
        struct.pack_into("<I", extended, 8, len(extended))
        source = "unknown-chunk.glb"
        report = overlap_report(glb_triangles(bytes(extended), source=source))
        self.assertTrue(report["complete"])
        self.assertEqual(len(report["repair_plan"]), 1)
        with self.assertRaisesRegex(ValueError, "noncanonical GLB repair container"):
            remove_duplicate_triangles(bytes(extended), report["repair_plan"], source=source)
        # The toolkit reader ignores bytes after the declared container. The
        # auditor must not consume a second BIN hidden in those trailing bytes.
        trailing = raw + struct.pack("<II", 4, 0x004E4942) + b"\0" * 4
        with self.assertRaisesRegex(ValueError, "outside the declared GLB container"):
            glb_triangles(trailing, source=source)

    def test_instance_manifest_uses_world_matrices_and_keeps_overrides_review_only(self):
        with tempfile.TemporaryDirectory() as directory:
            model = Path(directory, "tiny.glb")
            model.write_bytes(tiny_glb())
            identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
            instance = {"owner": "props/0", "model": str(model), "matrix": identity,
                        "material_overrides": {"None": "glass"}}
            manifest = Path(directory, "instances.json")
            manifest.write_text(json.dumps({"format": "places-prop-instances", "version": 1,
                                           "level": "tiny", "source_sha256": "source",
                                           "instances": [instance, {**instance, "owner": "props/1"}]}))
            report = instance_overlap_audit(manifest)
            self.assertTrue(report["complete"])
            self.assertEqual(report["totals"], {"coplanar-overlap": 4})
            self.assertEqual(report["repair_plan"], [])
            self.assertTrue(all(not row["auto_repairable"] for row in report["findings"]))
            self.assertEqual(report["findings"][0]["alpha_modes"], ["UNKNOWN_OVERRIDE"] * 2)

    def test_clipping_keeps_winding_and_original_affine_uvs(self):
        first = triangle()
        clipped = clip_attribute_triangle(first.records, axis=1, minimum=0.25)
        self.assertEqual(len(clipped), 1)
        points = [dict(record)["POSITION"] for record in clipped[0]]
        for record in clipped[0]:
            attributes = dict(record)
            self.assertEqual(attributes["TEXCOORD_0"], attributes["POSITION"][:2])
            self.assertGreaterEqual(attributes["POSITION"][1], 0.25)
        self.assertGreater((points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
                           - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]), 0)


if __name__ == "__main__":
    unittest.main()
