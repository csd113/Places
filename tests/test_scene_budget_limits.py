"""Audit the runtime record-size contracts without allocating giant payloads."""
import importlib.util
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("scene_budget", ROOT / "tools/bench/scene_budget.py")
BUDGET = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUDGET)


class DeclaredArchive:
    """Central-directory/manifest witness; preflight must read only metadata."""
    def __init__(self, records, variant_entries=None):
        self.manifest = dict(entries=[dict(name=name, role=role, bytes=size, sha256="0" * 64)
                                     for name, role, size in records],
                             variants=[dict(entries=variant_entries or {})])
        self.raw = json.dumps(self.manifest).encode()
        self.entries = []
        for name, size in [("manifest.json", len(self.raw)), *[(name, size) for name, _, size in records]]:
            entry = zipfile.ZipInfo(name)
            entry.file_size = size
            self.entries.append(entry)
        self.reads = []

    def infolist(self):
        return self.entries

    def getinfo(self, name):
        return next(entry for entry in self.entries if entry.filename == name)

    def namelist(self):
        return [entry.filename for entry in self.entries]

    def read(self, name):
        self.reads.append(name)
        if name != "manifest.json":
            raise AssertionError("Size preflight must reject before decompressing a payload")
        return self.raw


class SceneBudgetRuntimeLimitsTests(unittest.TestCase):
    def test_maintained_dense_record_fits_props_but_not_generic_payload_budget(self):
        # Independent measured capacity_dense witness also pinned by Rust tests.
        archive = DeclaredArchive([("dense.props", "props", 308_295_329)], dict(props="dense.props"))
        BUDGET.validate_archive_bounds(archive)
        self.assertEqual(archive.reads, ["manifest.json"])
        for role, size in [("props", 536_870_913), ("texture", 308_295_329)]:
            with self.subTest(role=role, size=size), self.assertRaisesRegex(ValueError, "record safety bound"):
                BUDGET.validate_archive_bounds(DeclaredArchive([("dense.props", role, size)]))

    def test_full_atlas_container_fits_without_widening_other_records(self):
        # Both real atlas records include the unchanged 196-byte KTX2 header.
        BUDGET.validate_archive_bounds(DeclaredArchive([("atlas.ktx2", "lightmaps", 268_435_652)]))
        BUDGET.validate_archive_bounds(DeclaredArchive([("atlas.ktx2", "lightmaps", 335_544_516)], dict(lightmaps="atlas.ktx2")))
        BUDGET.validate_archive_bounds(DeclaredArchive([("atlas.ktx2", "lightmaps", 335_609_856)], dict(lightmaps="atlas.ktx2")))
        for role, size in [("lightmaps", 335_609_857), ("texture", 268_435_652), ("texture", 335_544_516)]:
            with self.subTest(role=role, size=size), self.assertRaisesRegex(ValueError, "record safety bound"):
                BUDGET.validate_archive_bounds(DeclaredArchive([("atlas.ktx2", role, size)]))

    def test_actual_typed_record_limits_accept_boundary_and_reject_next_byte(self):
        witnesses = [
            ("semantics.json", "semantics", 67_108_864, {}),
            ("build-inputs.json", "compiler-inputs", 2_097_152, {}),
            ("mesh", "mesh", 536_870_912, dict(mesh="mesh")),
            ("props", "props", 536_870_912, dict(props="props")),
            ("lighting", "lighting", 268_435_456, dict(lighting="lighting")),
            ("collision", "collision", 134_217_728, dict(collision="collision")),
            ("navigation", "navigation", 134_217_728, dict(navigation="navigation")),
            ("atlas", "lightmaps", 335_609_856, dict(lightmaps="atlas")),
            ("charts", "lightmaps-meta", 67_108_864, dict(lightmaps_meta="charts")),
            ("irradiance", "irradiance", 268_435_456, dict(irradiance="irradiance")),
            ("positions", "probes-meta", 16_777_216, dict(probes=[dict(positions="positions", cubemaps=[])])),
            ("cube", "probes", 268_435_456, dict(probes=[dict(positions="positions", cubemaps=["cube"])])),
        ]
        for name, role, limit, references in witnesses:
            with self.subTest(name=name):
                BUDGET.validate_archive_bounds(DeclaredArchive([(name, role, limit)], references))
                with self.assertRaisesRegex(ValueError, "record safety bound"):
                    BUDGET.validate_archive_bounds(DeclaredArchive([(name, role, limit + 1)], references))

    def test_archive_aggregate_count_manifest_and_duplicates_stay_bounded(self):
        aggregate = DeclaredArchive([("left", "props", 536_870_912), ("right", "props", 536_870_912)])
        count = DeclaredArchive([(str(index), "texture", 1) for index in range(512)])
        manifest = DeclaredArchive([("small", "texture", 1)])
        manifest.getinfo("manifest.json").file_size = 2_097_153
        duplicate = DeclaredArchive([("same", "texture", 1), ("same", "texture", 1)])
        for name, archive in [("aggregate", aggregate), ("count", count), ("manifest", manifest), ("duplicate", duplicate)]:
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "reader safety bounds"):
                BUDGET.validate_archive_bounds(archive)
            self.assertEqual(archive.reads, [])

    def test_declared_size_mismatch_is_rejected_before_payload_read(self):
        archive = DeclaredArchive([("mesh", "mesh", 10)])
        archive.getinfo("mesh").file_size = 11
        with self.assertRaisesRegex(ValueError, "entry size differs"):
            BUDGET.validate_archive_bounds(archive)
        self.assertEqual(archive.reads, ["manifest.json"])

    def test_undeclared_and_repeated_manifest_records_are_not_accepted(self):
        for repeated in (False, True):
            archive = DeclaredArchive([("mesh", "mesh", 10)])
            if repeated:
                archive.manifest["entries"].append(archive.manifest["entries"][0])
            else:
                archive.manifest["entries"].clear()
            archive.raw = json.dumps(archive.manifest).encode()
            with self.subTest(repeated=repeated), self.assertRaisesRegex(ValueError, "declared manifest"):
                BUDGET.validate_archive_bounds(archive)

    def test_ten_page_switchable_header_shape_matches_measured_record_without_allocation(self):
        # Independent KTX2 schema witness: ten 1024px pages, two planes and
        # one extra switchable group. This is a header, never a texture image.
        identifier = bytes.fromhex("ab4b5458203230bb0d0a1a0a")
        descriptor = bytes.fromhex(
            "5c000000000000000200580001010100000000000800000000000000"
            "00000f0000000000000000000000803f10000f0100000000000000000000803f"
            "20000f0200000000000000000000803f30000f0300000000000000000000803f")
        header = identifier + struct.pack("<13I2Q3Q", 97, 2, 1024, 1024, 0, 40, 1, 1, 0,
                                          104, 92, 0, 0, 0, 0, 196, 335_544_320, 335_544_320) + descriptor
        self.assertEqual(len(header), 196)
        self.assertEqual(struct.unpack_from("<9I", header, 12), (97, 2, 1024, 1024, 0, 40, 1, 1, 0))
        self.assertEqual(struct.unpack_from("<3Q", header, 80), (196, 335_544_320, 335_544_320))
        self.assertEqual(10 * (1 + 1) * 2, 40)
        self.assertEqual(1024 * 1024 * 40 * 8 + len(header), 335_544_516)
        archive = DeclaredArchive([("atlas.ktx2", "lightmaps", 335_544_516)], dict(lightmaps="atlas.ktx2"))
        BUDGET.validate_archive_bounds(archive)
        self.assertEqual(archive.reads, ["manifest.json"])

    def test_bounds_preflight_does_not_replace_legacy_or_current_payload_decoder(self):
        # RGBA8 single-image/array and RGBA16F array headers retain the same
        # size-only preflight. Rust remains responsible for format acceptance,
        # descriptor/shape/range integrity and actual texel decoding.
        for vk_format, type_size, layers, texel_bytes in [(37, 1, 0, 4), (37, 1, 3, 4), (97, 2, 2, 8)]:
            header = bytes.fromhex("ab4b5458203230bb0d0a1a0a") + struct.pack(
                "<13I2Q3Q", vk_format, type_size, 1, 1, 0, layers, 1, 1, 0,
                104, 92, 0, 0, 0, 0, 196, max(layers, 1) * texel_bytes, max(layers, 1) * texel_bytes)
            with self.subTest(vk_format=vk_format, layers=layers):
                self.assertEqual(len(header), 104)
                self.assertEqual(struct.unpack_from("<I", header, 32)[0], layers)
                size = 196 + max(layers, 1) * texel_bytes
                BUDGET.validate_archive_bounds(DeclaredArchive([("atlas.ktx2", "lightmaps", size)], dict(lightmaps="atlas.ktx2")))

    def test_actual_atlas_header_bytes_cannot_bypass_archive_hash_check(self):
        # Small binary provenance witness only; it is never submitted/rendered
        # or claimed as a decoder-valid atlas/native benchmark.
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            package, log = directory / "header.placesmap", directory / "logic.log"
            header = bytes.fromhex("ab4b5458203230bb0d0a1a0a") + struct.pack("<2I", 97, 2)
            manifest = dict(id="witness", variants=[dict(lightmap_quality="full", entries={})],
                            entries=[dict(name="atlas.ktx2", role="lightmaps", bytes=len(header),
                                          sha256=hashlib.sha256(header).hexdigest())])
            raw = json.dumps(manifest).encode()
            log.write_text("[loading] package identity level=witness variant=full sha256=" + hashlib.sha256(raw).hexdigest()
                           + "\nBENCH_SUMMARY " + json.dumps(dict(level="witness", total_vertices=3, visible_vertices=3,
                               draw_calls=1, vbo_bytes=12, index_bytes=12, material_changes=1)) + "\n")
            for payload in (b"x" + header[1:], header + b"x"):
                with self.subTest(size=len(payload)), zipfile.ZipFile(package, "w") as archive:
                    archive.writestr("manifest.json", raw)
                    archive.writestr("atlas.ktx2", payload)
                with self.assertRaisesRegex(ValueError, "Unverified package entry|entry size differs"):
                    BUDGET.inspect(package, log)


if __name__ == "__main__":
    unittest.main()
