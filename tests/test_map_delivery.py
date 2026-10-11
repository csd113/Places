"""Regression proof for the isolated storage audit's on-disk reconstruction."""
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zipfile

TOOLS = Path(__file__).resolve().parents[1] / 'tools/bench'


def load_module(name):
    spec = importlib.util.spec_from_file_location(name, TOOLS / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


load_module('map_storage')
delivery = load_module('map_delivery')


class FactorProofTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.original = self.root / 'original.zip'
        self.prototype = self.root / 'prototype.zip'
        # Arbitrary original bit patterns include negative zero/half-float bits.
        self.vertices = (struct.pack('<I', 0x80000000) + bytes(range(65))) * 2
        prefix = self.vertices[:36] + self.vertices[69:105]
        tail = self.vertices[36:69] + self.vertices[105:138]
        segment = b'header' + prefix + tail + b'ordered-indices'
        self.recipe = dict(investigation_format='exact-prop-factoring-1',
                           chunk_locations=[[0, 0, 6], [0, 6, 72], [0, 78, 66], [0, 144, 15]],
                           records={'props': dict(bytes=159, parts=[['raw', [0]],
                                                  ['vertices', 69, 2, [1], [2]], ['raw', [3]]])})
        with zipfile.ZipFile(self.original, 'x') as archive:
            archive.writestr('props', b'header' + self.vertices + b'ordered-indices')
            archive.writestr('semantics', b'opaque unchanged record\x00\xff')
        self.segment = segment
        self.write_prototype()

    def write_prototype(self):
        with zipfile.ZipFile(self.prototype, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('prototype-recipes.json', json.dumps(self.recipe))
            archive.writestr('prototype-0.props', self.segment)
            archive.writestr('semantics', b'opaque unchanged record\x00\xff')

    def test_all_original_bits_and_unmodified_records_reconstruct(self):
        result = delivery.verify_factor(self.original, self.prototype)
        self.assertTrue(result['all_original_entries_byte_equal'])
        self.assertFalse(result['runtime_loadable'])
        self.assertEqual(set(result['original_entry_sha256']), {'props', 'semantics'})

    def test_corrupt_payload_and_invalid_chunk_rejected(self):
        original = self.segment
        self.segment = b'X' + original[1:]
        self.write_prototype()
        with self.assertRaisesRegex(ValueError, 'changed original bytes'):
            delivery.verify_factor(self.original, self.prototype)
        self.segment = original
        self.recipe['chunk_locations'][1][1] = len(original)
        self.write_prototype()
        with self.assertRaisesRegex(ValueError, 'chunk outside segment'):
            delivery.verify_factor(self.original, self.prototype)


if __name__ == '__main__':
    unittest.main()
