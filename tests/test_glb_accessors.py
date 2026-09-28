"""AUD-002 regression fixtures: GLB accessor reads must stay inside their view.

A bufferView's ``byteLength`` is relative to its own ``byteOffset``: the view is
``blob[view_offset : view_offset + view_length]``, and every element an accessor
addresses must fit inside that view. A binary chunk that continues past the
declared view does not make a crossing accessor valid.

The Rust reader (``src/gltf.rs::accessor_data``) and the toolkit reader
(``tools/props/glb.py::_read_accessor``) apply the same checks. This module
covers the fixtures both readers share. ``glb.py`` supports SCALAR/VEC2/VEC3/
VEC4 float and integer accessors only, so the MAT4 (inverse bind matrices)
fixture lives in ``src/gltf/tests.rs``.
"""
import struct
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.props import glb  # noqa: E402


def vec3_document(view_offset, view_length, *, accessor_offset=0, count=3, stride=None):
    """A VEC3 float accessor over bufferView 0, with an optional byteStride."""
    view = {"buffer": 0, "byteOffset": view_offset, "byteLength": view_length}
    if stride is not None:
        view["byteStride"] = stride
    return {
        "bufferViews": [view],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": glb.COMPONENT_FLOAT,
                "count": count,
                "type": "VEC3",
                "byteOffset": accessor_offset,
            }
        ],
    }


def float_bytes(values):
    """Little-endian float32 bytes for ``values``."""
    return struct.pack("<%df" % len(values), *values)


class GlbAccessorBoundsTests(unittest.TestCase):
    def test_nonzero_accessor_offset_that_ends_at_the_view_end_decodes(self):
        # Fixture 1 (AUD-002 case A): the 48-byte view holds three VEC3 float
        # elements starting at byte 12; the last element ends exactly at the
        # view end, so this is a valid file and must decode.
        document = vec3_document(0, 48, accessor_offset=12)
        blob = float_bytes(range(12))
        self.assertEqual(
            glb._read_accessor(document, blob, 0),
            [(3.0, 4.0, 5.0), (6.0, 7.0, 8.0), (9.0, 10.0, 11.0)],
        )

    def test_accessor_that_crosses_its_view_is_rejected(self):
        # Fixture 2 (AUD-002 case B): the view declares only 36 bytes, so the
        # accessor's last element would read 12 bytes of adjacent binary data.
        # The file must be rejected even though 48 binary bytes exist.
        document = vec3_document(0, 36, accessor_offset=12)
        blob = float_bytes(range(12))
        with self.assertRaisesRegex(glb.GltfError, "bufferView|too small"):
            glb._read_accessor(document, blob, 0)

    def test_nonzero_view_offset_with_an_exact_end_decodes(self):
        # Fixture 3: the view starts at byte 12 and ends exactly at the end of
        # the 48-byte binary chunk.
        document = vec3_document(12, 36)
        blob = float_bytes(range(12))
        self.assertEqual(
            glb._read_accessor(document, blob, 0),
            [(3.0, 4.0, 5.0), (6.0, 7.0, 8.0), (9.0, 10.0, 11.0)],
        )

    def test_valid_interleaving_decodes(self):
        # Fixture 4: VEC3 floats interleaved with one padding float per
        # element, i.e. a 16-byte stride over 12-byte elements.
        document = vec3_document(0, 48, stride=16)
        blob = float_bytes([1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0, 7.0, 8.0, 9.0, 0.0])
        self.assertEqual(
            glb._read_accessor(document, blob, 0),
            [(1.0, 2.0, 3.0), (4.0, 5.0, 6.0), (7.0, 8.0, 9.0)],
        )

    def test_stride_smaller_than_the_element_is_rejected(self):
        # Fixture 5a: an 8-byte stride cannot hold a 12-byte VEC3 float.
        document = vec3_document(0, 36, stride=8)
        with self.assertRaisesRegex(glb.GltfError, "byteStride"):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_zero_stride_for_multiple_elements_is_rejected(self):
        # Fixture 5b: a declared zero stride would read the same bytes for
        # every element; only a single-element accessor may use it.
        document = vec3_document(0, 36, stride=0)
        with self.assertRaisesRegex(glb.GltfError, "byteStride"):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_stride_that_is_not_a_multiple_of_four_is_rejected(self):
        # Fixture 5c: 18 bytes is large enough for a VEC3 element but breaks
        # glTF's 4-byte stride rule.
        document = vec3_document(0, 48, stride=18)
        with self.assertRaisesRegex(glb.GltfError, "byteStride"):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_view_that_extends_past_the_binary_is_rejected(self):
        # Fixture 6: the view claims bytes 4..52 of a 48-byte binary chunk.
        document = vec3_document(4, 48)
        with self.assertRaisesRegex(glb.GltfError, "truncated|extends past"):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_overflowing_metadata_is_rejected(self):
        # Fixture 7: a huge count and a huge accessor offset. Python integers
        # do not wrap, but both results exceed the declared view and must be
        # rejected before unpacking.
        document = vec3_document(0, 48, count=10**18)
        with self.assertRaises(glb.GltfError):
            glb._read_accessor(document, float_bytes(range(12)), 0)
        document = vec3_document(0, 48, accessor_offset=2**63)
        with self.assertRaises(glb.GltfError):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_zero_count_is_rejected(self):
        # Fixture 9: glTF accessors hold at least one element; an empty
        # accessor must not be silently accepted as an empty tuple list.
        document = vec3_document(0, 48, count=0)
        with self.assertRaisesRegex(glb.GltfError, "zero|at least one"):
            glb._read_accessor(document, float_bytes(range(12)), 0)

    def test_negative_byte_fields_are_rejected(self):
        # Fixture 10: glTF byte offsets, lengths and strides are unsigned. A
        # present negative value must fail instead of being treated as absent
        # or used as a negative index into the binary chunk.
        blob = float_bytes(range(12))
        document = vec3_document(0, 48)
        document["bufferViews"][0]["byteOffset"] = -4
        with self.assertRaisesRegex(glb.GltfError, "non-negative"):
            glb._read_accessor(document, blob, 0)

        document = vec3_document(0, 48, accessor_offset=-12)
        with self.assertRaisesRegex(glb.GltfError, "non-negative"):
            glb._read_accessor(document, blob, 0)

        document = vec3_document(0, 48, stride=-12)
        with self.assertRaisesRegex(glb.GltfError, "non-negative"):
            glb._read_accessor(document, blob, 0)

        document = vec3_document(0, 48)
        document["bufferViews"][0]["byteLength"] = -48
        with self.assertRaisesRegex(glb.GltfError, "non-negative"):
            glb._read_accessor(document, blob, 0)

        document = vec3_document(0, 48, count=-3)
        with self.assertRaisesRegex(glb.GltfError, "non-negative"):
            glb._read_accessor(document, blob, 0)


if __name__ == "__main__":
    unittest.main()
