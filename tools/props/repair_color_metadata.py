#!/usr/bin/env python3
"""Lossless structural repair of the seven upgraded GLBs with a stale COLOR_0.

Background
----------
The production models for seven props were upgraded by appending a closed
bottom-face pass to an existing GLB: the old binary chunk is retained as a
prefix and new POSITION/TEXCOORD_0/indices accessors are appended.  The pass
failed to emit a matching COLOR_0 accessor, so the primitive still references
the old colour accessor whose count is smaller than POSITION's.  The glTF 2.0
spec requires every attribute accessor of a primitive to have the same count
as POSITION, so Places (correctly) rejects the file.

Repair
------
Append a complete colour array (count == POSITION count) to the binary chunk:

* the first ``C`` entries are byte-identical copies of the existing colour
  data, so every retained vertex keeps its exact authored colour;
* each appended vertex copies the colour of the first coincident old vertex,
  which is the convention every correctly-exported upgraded model already
  follows (verified across crate, cardboard_box, pool_chair, pool_table,
  pool_ladder and the pool guardrails);
* a fan hub with no coincident old vertex takes the per-channel mean of its
  own fan's perimeter colours (hub vertices only exist in the hidden closed
  bottoms added by the upgrade; nothing else is affected).

Nothing else changes: offsets, byte lengths and the GLB header are
recomputed, and accessor 3 is repointed to the new contiguous view.  Positions,
indices, UVs, materials, textures, transforms and every existing colour byte
are untouched.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMPONENT_UBYTE = 5121
TARGET_ARRAY_BUFFER = 34962

MODELS = (
    "assets/environment/office/props/models/desk.glb",
    "assets/environment/office/props/models/chair.glb",
    "assets/environment/office/props/models/cabinet.glb",
    "assets/environment/office/props/models/vending_machine.glb",
    "assets/environment/office/props/models/water_cooler.glb",
    "assets/environment/home/props/models/cabinet_base.glb",
    "assets/environment/home/props/models/cabinet_wall.glb",
)


def parse_glb(data: bytes):
    magic, version, length = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        raise ValueError("not a glTF 2.0 GLB")
    offset = 12
    json_doc = None
    binary = b""
    json_start = json_end = 0
    bin_start = bin_end = 0
    while offset + 8 <= min(length, len(data)):
        chunk_len, chunk_type = struct.unpack_from("<II", data, offset)
        start = offset + 8
        end = start + chunk_len
        if chunk_type == 0x4E4F534A:
            json_doc = json.loads(data[start:end].decode("utf-8").rstrip("\x00 "))
            json_start, json_end = start, end
        elif chunk_type == 0x004E4942:
            binary = data[start:end]
            bin_start, bin_end = start, end
        offset = end
    if json_doc is None:
        raise ValueError("GLB has no JSON chunk")
    return json_doc, binary, (json_start, json_end), (bin_start, bin_end)


def accessor_bytes(doc, blob, index: int, component_size: int, components: int) -> list[tuple]:
    accessor = doc["accessors"][index]
    view = doc["bufferViews"][accessor["bufferView"]]
    fmt = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}[accessor["componentType"]]
    stride = view.get("byteStride", component_size * components)
    start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    out = []
    for i in range(accessor["count"]):
        out.append(
            struct.unpack_from(f"<{components}{fmt}", blob, start + i * stride)
        )
    return out


def float_tuple(values) -> tuple[float, ...]:
    return tuple(float(v) for v in values)


def pad_to(data: bytearray, alignment: int) -> None:
    while len(data) % alignment:
        data.append(0)


def repair(path: Path, write: bool = False, quiet: bool = False) -> dict:
    data = path.read_bytes()
    doc, blob, json_span, bin_span = parse_glb(data)
    primitive = doc["meshes"][0]["primitives"][0]
    position_index = primitive["attributes"]["POSITION"]
    color_index = primitive["attributes"]["COLOR_0"]

    positions = accessor_bytes(doc, blob, position_index, 4, 3)
    colors = accessor_bytes(doc, blob, color_index, 1, 4)
    indices = [
        v[0]
        for v in accessor_bytes(doc, blob, primitive["indices"], 2, 1)
    ]

    vertex_count = len(positions)
    old_color_count = len(colors)
    if old_color_count == vertex_count:
        return {"path": str(path.relative_to(ROOT)), "status": "already valid"}

    # First coincident old vertex per position, in old index order.
    first_at: dict[tuple[float, ...], int] = {}
    for index, position in enumerate(positions[:old_color_count]):
        key = float_tuple(position)
        if key not in first_at:
            first_at[key] = index

    repaired: list[tuple[int, int, int, int]] = [tuple(c) for c in colors]
    hubs: list[int] = []
    for index in range(old_color_count, vertex_count):
        source = first_at.get(float_tuple(positions[index]))
        if source is None:
            hubs.append(index)
            repaired.append((255, 255, 255, 255))  # placeholder, filled below
        else:
            repaired.append(tuple(colors[source]))

    if hubs:
        # Hub of a fan: mean of the perimeter vertices of the same component.
        # Perimeter membership comes from the index buffer: the hub is the
        # vertex shared by every triangle of its fan; the others are the ring.
        triangles = [tuple(indices[i : i + 3]) for i in range(0, len(indices), 3)]
        for hub in hubs:
            ring: set[int] = set()
            for a, b, c in triangles:
                if hub in (a, b, c):
                    ring.update((a, b, c))
            ring.discard(hub)
            if not ring:
                continue
            channels = [
                sum(repaired[v][channel] for v in ring) / len(ring)
                for channel in range(4)
            ]
            repaired[hub] = tuple(int(round(value)) for value in channels)

    if not quiet:
        print(
            f"{path.name}: POSITION={vertex_count} old COLOR_0={old_color_count} "
            f"appended={vertex_count - old_color_count} hubs={len(hubs)}"
        )
    if not write:
        return {
            "path": str(path.relative_to(ROOT)),
            "status": "needs repair",
            "vertices": vertex_count,
            "old_colors": old_color_count,
            "hubs": len(hubs),
        }

    # Append the complete colour array to the binary chunk.
    new_blob = bytearray(blob)
    pad_to(new_blob, 4)
    color_view_offset = len(new_blob)
    for color in repaired:
        new_blob.extend(bytes(color))
    color_view_index = len(doc["bufferViews"])
    doc["bufferViews"].append(
        {
            "buffer": 0,
            "byteOffset": color_view_offset,
            "byteLength": len(repaired) * 4,
            "target": TARGET_ARRAY_BUFFER,
        }
    )
    doc["accessors"][color_index] = {
        "bufferView": color_view_index,
        "componentType": COMPONENT_UBYTE,
        "normalized": True,
        "count": vertex_count,
        "type": "VEC4",
    }
    doc["buffers"][0]["byteLength"] = len(new_blob)

    json_bytes = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    while len(json_bytes) % 4:
        json_bytes += b" "
    while len(new_blob) % 4:
        new_blob.append(0)

    header = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(json_bytes) + 8 + len(new_blob))
    payload = (
        header
        + struct.pack("<II", len(json_bytes), 0x4E4F534A)
        + json_bytes
        + struct.pack("<II", len(new_blob), 0x004E4942)
        + bytes(new_blob)
    )
    path.write_bytes(payload)
    return {
        "path": str(path.relative_to(ROOT)),
        "status": "repaired",
        "vertices": vertex_count,
        "old_colors": old_color_count,
        "hubs": len(hubs),
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write the repaired GLBs")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("models", nargs="*", default=None)
    args = parser.parse_args(argv)
    models = args.models or MODELS
    failures = 0
    for rel in models:
        path = ROOT / rel
        try:
            result = repair(path, write=args.write, quiet=args.quiet)
        except Exception as error:  # noqa: BLE001 - report and continue
            print(f"FAIL {rel}: {error}")
            failures += 1
            continue
        if not args.quiet:
            print(f"  -> {result['status']}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
