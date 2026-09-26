#!/usr/bin/env python3
"""Uniformly resize the PNGs embedded in a GLB, in place, deterministically.

The asset rules in ``docs/ASSET_SPECIFICATION.md`` cap every PNG at 1024 px per
edge and ship prop/entity textures at their 256 px native size. A GLB exported
from a modelling tool can arrive above that; model UVs are normalized, so a
uniform resize preserves the mapping and is the safe conversion (repacking or
re-laying-out a model texture is not).

The reader/writer below is deliberately minimal: it rewrites only the JSON
chunk's ``bufferViews``/``images`` bookkeeping for the resized images and the
BIN chunk bytes, so a resize is a pure asset conversion — no geometry, node,
skin, animation or material data is touched.

Usage:
    python3 tools/props/resize_embedded_textures.py MODEL.glb [--size 256]

The script is idempotent: a second run on an already-conformant GLB leaves the
file byte-identical. It refuses non-PNG images and non-GLB input.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def read_glb(path: str) -> tuple[dict, bytearray]:
    with open(path, "rb") as handle:
        data = handle.read()
    if data[:4] != b"glTF":
        raise SystemExit(f"{path}: not a GLB")
    version, total = struct.unpack_from("<II", data, 4)
    if version != 2:
        raise SystemExit(f"{path}: unsupported glTF version {version}")
    offset, document, binary = 12, None, None
    while offset < total:
        length, kind = struct.unpack_from("<I4s", data, offset)
        chunk = data[offset + 8 : offset + 8 + length]
        if kind == b"JSON":
            document = json.loads(chunk)
        elif kind == b"BIN\x00":
            binary = bytearray(chunk)
        offset += 8 + length
    if document is None or binary is None:
        raise SystemExit(f"{path}: missing JSON or BIN chunk")
    return document, binary


def write_glb(path: str, document: dict, binary: bytearray) -> None:
    json_bytes = json.dumps(document, separators=(",", ":"), ensure_ascii=False).encode()
    while len(json_bytes) % 4:
        json_bytes += b" "
    while len(binary) % 4:
        binary += b"\x00"
    total = 12 + 8 + len(json_bytes) + 8 + len(binary)
    out = bytearray()
    out += b"glTF"
    out += struct.pack("<II", 2, total)
    out += struct.pack("<I4s", len(json_bytes), b"JSON")
    out += json_bytes
    out += struct.pack("<I4s", len(binary), b"BIN\x00")
    out += binary
    with open(path, "wb") as handle:
        handle.write(out)


def decode_png(data: bytes) -> tuple[int, int, bytearray]:
    """Decode an 8-bit RGB/RGBA non-interlaced PNG to RGBA8."""
    if data[:8] != PNG_SIGNATURE:
        raise ValueError("not a PNG")
    offset, width, height, colour, depth, interlace = 8, 0, 0, 0, 0, 0
    idat = bytearray()
    while offset < len(data):
        length, kind = struct.unpack_from(">I4s", data, offset)
        payload = data[offset + 8 : offset + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
        elif kind == b"IDAT":
            idat += payload
        elif kind == b"IEND":
            break
        offset += 12 + length
    if depth != 8 or interlace != 0 or colour not in (2, 6):
        raise ValueError(f"unsupported PNG (depth {depth}, colour {colour})")
    channels = 3 if colour == 2 else 4
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(width * height * 4)
    previous = bytearray(stride)
    pos = 0
    for y in range(height):
        filter_kind = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for x in range(stride):
            left = line[x - channels] if x >= channels else 0
            up = previous[x]
            up_left = previous[x - channels] if x >= channels else 0
            if filter_kind == 1:
                line[x] = (line[x] + left) & 0xFF
            elif filter_kind == 2:
                line[x] = (line[x] + up) & 0xFF
            elif filter_kind == 3:
                line[x] = (line[x] + (left + up) // 2) & 0xFF
            elif filter_kind == 4:
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                predictor = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[x] = (line[x] + predictor) & 0xFF
        previous = line
        for x in range(width):
            src = x * channels
            dst = (y * width + x) * 4
            out[dst] = line[src]
            out[dst + 1] = line[src + 1]
            out[dst + 2] = line[src + 2]
            out[dst + 3] = line[src + 3] if channels == 4 else 255
    return width, height, out


def encode_png(width: int, height: int, rgba: bytes) -> bytes:
    """Encode RGBA8 as a non-interlaced PNG with filter 0 rows."""
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw += rgba[y * stride : (y + 1) * stride]

    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        PNG_SIGNATURE
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def box_downsample(width: int, height: int, rgba: bytearray, size: int) -> tuple[int, int, bytearray]:
    """Fractional box-filter down to exactly ``size`` on the longest edge.

    Integer stride dropping would round a 1254 px edge down to 250 and lose the
    native 256 size; the fractional filter keeps the aspect ratio, the power-of-
    two target and every source pixel's contribution.
    """
    if width <= size and height <= size:
        return width, height, rgba
    scale = max(width, height) / size
    out_w = max(1, round(width / scale))
    out_h = max(1, round(height / scale))
    out = bytearray(out_w * out_h * 4)
    for y in range(out_h):
        y0 = y * height / out_h
        y1 = (y + 1) * height / out_h
        sy0, sy1 = int(y0), max(int(y0) + 1, min(height, int(y1 + 0.999999)))
        for x in range(out_w):
            x0 = x * width / out_w
            x1 = (x + 1) * width / out_w
            sx0, sx1 = int(x0), max(int(x0) + 1, min(width, int(x1 + 0.999999)))
            sums = [0, 0, 0, 0]
            count = (sy1 - sy0) * (sx1 - sx0)
            for sy in range(sy0, sy1):
                row = sy * width * 4
                for sx in range(sx0, sx1):
                    base = row + sx * 4
                    sums[0] += rgba[base]
                    sums[1] += rgba[base + 1]
                    sums[2] += rgba[base + 2]
                    sums[3] += rgba[base + 3]
            dst = (y * out_w + x) * 4
            for channel in range(4):
                out[dst + channel] = sums[channel] // count
    return out_w, out_h, out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", help="GLB file to rewrite in place")
    parser.add_argument("--size", type=int, default=256, help="target longest edge (default 256)")
    args = parser.parse_args()
    if args.size <= 0:
        raise SystemExit("--size must be positive")

    document, binary = read_glb(args.model)
    images = document.get("images", [])
    if not images:
        raise SystemExit(f"{args.model}: no embedded images")
    views = document["bufferViews"]
    spans = []
    for index, view in enumerate(views):
        offset = view.get("byteOffset", 0)
        spans.append((offset, offset + view["byteLength"], index))
    for (_, end, left), (start, _, right) in zip(spans, spans[1:]):
        if end > start:
            raise SystemExit(
                f"{args.model}: bufferViews {left} and {right} overlap; cannot rewrite safely"
            )
    replacements: dict[int, bytes] = {}
    for image in images:
        if "uri" in image:
            raise SystemExit(f"{args.model}: external image URI is not supported")
        view_index = image["bufferView"]
        view = views[view_index]
        offset = view.get("byteOffset", 0)
        original = bytes(binary[offset : offset + view["byteLength"]])
        width, height, rgba = decode_png(original)
        new_w, new_h, new_rgba = box_downsample(width, height, rgba, args.size)
        if (new_w, new_h) == (width, height):
            continue
        replacements[view_index] = encode_png(new_w, new_h, new_rgba)
        print(f"image {image.get('name', view_index)}: {width}x{height} -> {new_w}x{new_h}")
    if not replacements:
        print(f"{args.model}: already within {args.size} px; nothing to do")
        return 0
    # Rebuild the BIN chunk in bufferView order, keeping 4-byte alignment and
    # rewriting every offset. Nothing but image bytes changes.
    rebuilt = bytearray()
    for index, view in enumerate(views):
        payload = replacements.get(index)
        if payload is None:
            offset = view.get("byteOffset", 0)
            payload = bytes(binary[offset : offset + view["byteLength"]])
        while len(rebuilt) % 4:
            rebuilt += b"\x00"
        view["byteOffset"] = len(rebuilt)
        view["byteLength"] = len(payload)
        rebuilt += payload
    buffers = document.get("buffers")
    if buffers:
        buffers[0]["byteLength"] = len(rebuilt)
    write_glb(args.model, document, rebuilt)
    print(f"{args.model}: resized {len(replacements)} image(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
