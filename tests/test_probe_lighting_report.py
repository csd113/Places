"""Independent binary/header and neutral-light oracles for the report tool."""
import struct
import unittest

from tools.bench.probe_lighting_report import atlas_samples, read_field, reconstruct


class ProbeReportTests(unittest.TestCase):
    def record(self):
        return b"PLPF" + struct.pack("<H4f4I8fi",2,-10,5,20,1.5,1,1,1,1,8,0.125,0.001,-2,1,0,0.5,0.5,3)

    def test_world_positions_linear_hdr_and_signed_moments(self):
        field = read_field(self.record())
        probe = field["probes"][0]
        self.assertEqual(probe["position"],(-9.25,5.75,20.75))
        self.assertEqual(probe["irradiance"][:2],(8,0.125))
        self.assertEqual(probe["moment"],(-2,1,0))
        self.assertEqual(probe["room"],3)

    def test_truncation_nonfinite_and_negative_energy_are_rejected(self):
        with self.assertRaises(ValueError): read_field(self.record()[:-1])
        for invalid in (float("nan"),float("inf"),-0.01):
            data = bytearray(self.record())
            struct.pack_into("<f",data,38,invalid)
            with self.assertRaises(ValueError): read_field(data)

    def split_record(self):
        data = bytearray(self.record())
        struct.pack_into("<H", data, 4, 3)
        return bytes(data) + struct.pack("<3I6f", 2, 3, 17, 4, 0.0625, 0.0005, -1, 0.5, 0)

    def test_version_three_source_identity_and_direct_residual_are_preserved(self):
        field = read_field(self.split_record())
        self.assertEqual(field["record_version"], 3)
        self.assertEqual(field["selected_light_indices"], (3, 17))
        probe = field["probes"][0]
        self.assertEqual(probe["selected_direct"][:2], (4, 0.0625))
        self.assertEqual(probe["selected_direct_moment"], (-1, 0.5, 0))
        self.assertEqual(probe["nonlocal_irradiance"][:2], (4, 0.0625))
        self.assertEqual(probe["nonlocal_moment"], (-1, 0.5, 0))
        legacy = read_field(self.record())
        self.assertEqual(legacy["selected_light_indices"], ())
        self.assertNotIn("selected_direct", legacy["probes"][0])

    def test_version_three_invalid_source_ids_and_direct_data_are_rejected(self):
        with self.assertRaises(ValueError): read_field(self.split_record()[:-1])
        for offset, value, fmt in [(82, 3, "I"), (86, float("nan"), "f"), (86, 9, "f")]:
            data = bytearray(self.split_record())
            struct.pack_into("<" + fmt, data, offset, value)
            with self.assertRaises(ValueError): read_field(data)

    def zero_source_record(self):
        data = bytearray(self.record())
        struct.pack_into("<H", data, 4, 3)
        return bytes(data) + struct.pack("<I6f", 0, 0, 0, 0, 0, 0, 0)

    def test_zero_source_version_three_preserves_spatial_sidecar(self):
        field = read_field(self.zero_source_record())
        probe = field["probes"][0]
        self.assertEqual(field["record_version"], 3)
        self.assertEqual(field["selected_light_indices"], ())
        self.assertEqual(probe["selected_direct"], (0, 0, 0))
        self.assertEqual(probe["selected_direct_moment"], (0, 0, 0))
        self.assertEqual(probe["nonlocal_irradiance"], probe["irradiance"])
        self.assertEqual(probe["nonlocal_moment"], probe["moment"])
        self.assertNotIn("selected_direct", read_field(self.record())["probes"][0])

    def test_zero_source_version_three_requires_aligned_zero_coefficients(self):
        for data in (self.zero_source_record()[:-1], self.zero_source_record()[:-24]):
            with self.assertRaises(ValueError): read_field(data)
        for channel in range(6):
            data = bytearray(self.zero_source_record())
            struct.pack_into("<f", data, 38 + 36 + 4 + channel * 4, 0.01)
            with self.assertRaises(ValueError): read_field(data)

    def test_single_direction_neutral_diffuse_is_exact(self):
        self.assertEqual(reconstruct((0.25,0.125,0),(0,0.375,0),(0,1,0)),(0.5,0.25,0))
        self.assertEqual(reconstruct((0.25,0.125,0),(0,0.375,0),(0,-1,0)),(0,0,0))

    def test_packaged_half_float_atlas_uses_the_single_texel_center(self):
        image = b"\xabKTX 20\xbb\r\n\x1a\n" + struct.pack("<9I",97,2,1,1,0,2,1,1,0)
        image += bytes(80-len(image)) + struct.pack("<3Q",104,16,16)
        image += struct.pack("<8e",0.25,0.125,0,0,0,0.375,0,0)
        meta = {"record_version":3,"page_edge":1,"page_count":1,"charts":[{
            "patch":{"origin":[-4,5,10],"u_axis":[0,0,2],"v_axis":[2,0,0],"room":3,"kind":"Floor"},
            "chart":{"page":0,"x":0,"y":0,"width":1,"height":1}}]}
        sample = atlas_samples(meta,image)[0]
        self.assertEqual(sample["position"],(-3,5,11))
        self.assertEqual(sample["normal"],(0,1,0))
        self.assertEqual(sample["light"],(0.5,0.25,0))

    def test_requested_location_reads_its_real_texel_between_coarse_taps(self):
        image = b"\xabKTX 20\xbb\r\n\x1a\n" + struct.pack("<9I",97,2,5,5,0,2,1,1,0)
        image += bytes(80-len(image)) + struct.pack("<3Q",104,400,400)
        image = bytearray(image + bytes(400))
        struct.pack_into("<4e",image,104+(3*5+3)*8,8,0.125,0.001,0)
        meta = {"record_version":3,"page_edge":5,"page_count":1,"charts":[{
            "patch":{"origin":[0,0,0],"u_axis":[0,0,4],"v_axis":[4,0,0],"room":3,"kind":"floor"},
            "chart":{"page":0,"x":0,"y":0,"width":5,"height":5}}]}
        samples = atlas_samples(meta,image,[{"position":[3,1,3],"room":3}])
        self.assertTrue(all(sample["light"] == (0,0,0) for sample in samples[:-1]))
        self.assertEqual(samples[-1]["position"],(3,0,3))
        self.assertEqual(samples[-1]["light"][:2],(8,0.125))
