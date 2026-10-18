#!/usr/bin/env python3
"""Adapt the approved duck's optional specular extension to Places core PBR.

The immutable source is hash guarded. Only JSON extension metadata changes;
geometry, fitted UVs, normals, images and all BIN bytes remain exact.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

SOURCE_SHA256 = "51e672fcd095e9fcd78c64d4eeae04cad9247089144f2653e11d07f555ce1c79"


def adapt(source: bytes) -> bytes:
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("source differs from the approved rubber duck export")
    length = struct.unpack_from("<I", source, 12)[0]
    document = json.loads(source[20:20 + length])
    binary = source[28 + length:]
    for material in document["materials"]:
        extensions = material["extensions"]
        specular = extensions.pop("KHR_materials_specular")
        if abs(specular["specularFactor"] - 0.64) > 1e-5:
            raise ValueError("unexpected source specular factor")
        if not extensions:
            del material["extensions"]
    for key in ("extensionsUsed", "extensionsRequired"):
        if key in document:
            document[key] = [name for name in document[key] if name != "KHR_materials_specular"]
            if not document[key]:
                del document[key]
    encoded = json.dumps(document, separators=(",", ":")).encode()
    encoded += b" " * (-len(encoded) % 4)
    return (struct.pack("<III", 0x46546C67, 2, 28 + len(encoded) + len(binary))
            + struct.pack("<II", len(encoded), 0x4E4F534A) + encoded
            + struct.pack("<II", len(binary), 0x004E4942) + binary)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = adapt(args.source.read_bytes())
    if args.output.resolve() == args.source.resolve():
        parser.error("preserve the original source; choose a separate output")
    args.output.write_bytes(result)
    print(hashlib.sha256(result).hexdigest())


if __name__ == "__main__":
    main()
