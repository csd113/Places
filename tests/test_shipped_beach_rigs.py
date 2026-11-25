"""Protect the shipped Beach rigs and loops using their committed GLBs.

The old toolkit cases check pose math on generated inputs. This reads the
actual crab, fish and gull files, so an invalid exported skin or broken loop
cannot pass just because the offline generator and its synthetic tests agree.
"""
import math
from pathlib import Path
import unittest

from assets.inspection import glb
from assets.inspection.animation import animation_record

ASSETS = Path(__file__).resolve().parents[1] / 'assets'
CLIPS = {
    'beach_crab': {'idle', 'walk'},
    'beach_fish': {'swim'},
    'beach_seagull': {'idle', 'fly'},
}


class ShippedBeachRigTests(unittest.TestCase):
    def test_current_rigs_keep_normalized_weights_and_continuous_named_loops(self):
        for name, expected in CLIPS.items():
            with self.subTest(model=name):
                path = ASSETS / 'entities' / name / 'model' / (name + '.glb')
                document, binary = glb.read_glb_chunks(path.read_bytes())
                weighted_vertices = 0
                for node in document['nodes']:
                    if 'mesh' not in node:
                        continue
                    skin = document['skins'][node['skin']]
                    self.assertTrue(skin['joints'])
                    for primitive in document['meshes'][node['mesh']]['primitives']:
                        attributes = primitive['attributes']
                        weights = glb._read_accessor(document, binary, attributes['WEIGHTS_0'])
                        joints = glb._read_accessor(document, binary, attributes['JOINTS_0'])
                        self.assertEqual(len(weights), document['accessors'][attributes['POSITION']]['count'])
                        for vertex, (row, slots) in enumerate(zip(weights, joints, strict=True)):
                            if (len(row) != 4 or len(slots) != 4
                                    or any(not math.isfinite(w) or w < 0 for w in row)
                                    or abs(sum(row) - 1) > 1e-4
                                    or any(not j.is_integer() or not 0 <= j < len(skin['joints']) for j in slots)):
                                self.fail(f'{name} vertex {vertex}: invalid skin {row}, {slots}')
                        weighted_vertices += len(weights)
                self.assertGreater(weighted_vertices, 0)
                self.assertEqual({clip['name'] for clip in document['animations']}, expected)
                self.assertEqual(len(document['animations']), len(expected))
                for clip in document['animations']:
                    record = animation_record(document, binary, clip)
                    self.assertEqual(record['errors'], [], (name, clip['name']))
                    self.assertGreater(record['duration'], 0)
                    self.assertTrue(record['channels'])
                    for channel in record['channels']:
                        self.assertLessEqual(channel['endpoint_delta'], 1e-4, (name, clip['name'], channel))
