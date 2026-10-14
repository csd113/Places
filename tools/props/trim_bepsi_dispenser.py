#!/usr/bin/env python3
"""Trim v36 Bepsi's lower print around its coplanar dispenser rim.

Offline geometry correction only: PNGs, other primitives, bounds and collision
remain unchanged. Reapply to the original delivered GLB after a master export;
the input hash prevents applying this recipe to an unrelated rebuild.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
MODEL = ROOT / "assets/environments/arcade/props/vending_machine.glb"
SOURCE_SHA256 = "2180912882e9aa23f464484ee41cae51570b29a20a47eb66203c5448d538f721"


def trim(source: bytes) -> bytes:
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("expected the original v36 Bepsi export")
    data = bytearray(source)
    json_size = struct.unpack_from("<I", data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    primitive = document["meshes"][0]["primitives"][2]
    if document["materials"][primitive["material"]]["name"] != "arcade_vending_lower_lit":
        raise ValueError("unexpected lower-print primitive")
    binary_start = 28 + json_size

    def accessor(attribute: str, components: int) -> tuple[int, int]:
        record = document["accessors"][primitive["attributes"][attribute]]
        view = document["bufferViews"][record["bufferView"]]
        if record["count"] != 16 or record["componentType"] != 5126:
            raise ValueError("unexpected lower-print accessor")
        offset = binary_start + view.get("byteOffset", 0) + record.get("byteOffset", 0)
        return offset, view.get("byteStride", 4 * components)

    positions, position_stride = accessor("POSITION", 3)
    uvs, uv_stride = accessor("TEXCOORD_0", 2)
    # The physical rim spans +/-0.247 and Y0.211..0.489. A 0.2mm print
    # clearance gives each surface an exclusive domain without a depth bias.
    changes = {
        0: (None, .4892), 1: (None, .4892),
        6: (None, .2108), 7: (None, .2108),
        8: (None, .2108), 9: (-.2472, .2108),
        10: (None, .4892), 11: (-.2472, .4892),
        12: (.2472, .2108), 13: (None, .2108),
        14: (.2472, .4892), 15: (None, .4892),
    }
    for index, (new_x, new_y) in changes.items():
        position_offset = positions + index * position_stride
        x, y, z = struct.unpack_from("<3f", data, position_offset)
        if abs(z - .432) > 1e-6:
            raise ValueError("unexpected lower-print depth")
        target_x = x if new_x is None else new_x
        u, v = struct.unpack_from("<2f", data, uvs + index * uv_stride)
        # Retain the original artwork's affine mapping; trim, never stretch.
        u += (target_x - x) * (.4921875 / .9)
        v += (new_y - y) * (-.3046875 / .58)
        struct.pack_into("<3f", data, position_offset, target_x, new_y, z)
        struct.pack_into("<2f", data, uvs + index * uv_stride, u, v)
    return bytes(data)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="original v36 runtime GLB")
    parser.add_argument("--out", type=Path, default=MODEL)
    parser.add_argument("--check", action="store_true", help="compare without writing")
    args = parser.parse_args()
    corrected = trim(args.source.read_bytes())
    if args.check:
        if args.out.read_bytes() != corrected:
            raise SystemExit("Bepsi dispenser correction differs from the recipe")
    else:
        args.out.write_bytes(corrected)
    print(json.dumps({"source_sha256": SOURCE_SHA256,
                      "sha256": hashlib.sha256(corrected).hexdigest(),
                      "bytes": len(corrected), "changed_vertices": 12,
                      "png_changes": 0, "other_primitive_changes": 0}))


if __name__ == "__main__":
    main()
