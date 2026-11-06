"""Losslessly consolidate the verified Feltling delivery for Places' one-mesh-node skin contract.

Only the JSON scene bindings change. Geometry, UVs, textures, weights, inverse
binds, animation output samples and the complete BIN chunk stay byte-identical.
The approved originals and the delivered Library file are never overwritten.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

SOURCE_SHA256 = "d13ccb3fa1c86e132914e9ee549c228e1ff9b055ded6b824bf2886197d4bbb0a"


def normalize(source):
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("Use the exact verified Places-compatible Feltling delivery")
    magic, version, size = struct.unpack_from("<4sII", source)
    if (magic, version, size) != (b"glTF", 2, len(source)):
        raise ValueError("Invalid GLB container")
    json_size, kind = struct.unpack_from("<II", source, 12)
    if kind != 0x4E4F534A:
        raise ValueError("Expected first JSON chunk")
    document = json.loads(source[20:20 + json_size])
    before = json.loads(json.dumps(document))
    binary_chunk = source[20 + json_size:]
    nodes = document["nodes"]
    selected = [index for index, node in enumerate(nodes) if "mesh" in node]
    if selected != [16, 17, 18, 19, 20]:
        raise ValueError("Unexpected delivered mesh-node identity")
    if any("translation" in nodes[i] or "rotation" in nodes[i] or "scale" in nodes[i]
           or "matrix" in nodes[i] or "children" in nodes[i] or nodes[i].get("skin") != 0 for i in selected):
        raise ValueError("Only identity-transform nodes with the common skin may consolidate")
    parents = [[i for i, node in enumerate(nodes) if index in node.get("children", [])] for index in selected]
    if any(parent != [21] for parent in parents):
        raise ValueError("Mesh nodes must share the unchanged armature parent")
    if any(channel["target"]["node"] in selected for clip in document["animations"] for channel in clip["channels"]):
        raise ValueError("Independently animated mesh nodes cannot consolidate")
    primitives = [primitive for index in selected for primitive in document["meshes"][nodes[index]["mesh"]]["primitives"]]
    if len(primitives) != 5:
        raise ValueError("Expected five delivered material primitives")
    document["meshes"] = [{"name": "Feltling_Runtime", "primitives": primitives}]
    for index in selected:
        nodes[index].pop("mesh")
        nodes[index].pop("skin")
    nodes[selected[0]]["mesh"] = 0
    nodes[selected[0]]["skin"] = 0
    document["asset"]["extras"]["places_import_normalization"] = {
        "source_sha256": SOURCE_SHA256, "operation": "identity sibling primitives consolidated into one skinned mesh node",
        "binary_chunk_unchanged": True, "joint_nodes_and_animation_targets_unchanged": True,
    }
    for key in ("accessors", "bufferViews", "buffers", "skins", "animations", "materials", "images", "textures"):
        if document[key] != before[key]:
            raise ValueError(f"Unexpected {key} change")
    original_primitives = [p for mesh in before["meshes"] for p in mesh["primitives"]]
    if primitives != original_primitives:
        raise ValueError("Primitive attributes or ordering changed")
    encoded = json.dumps(document, separators=(",", ":")).encode()
    encoded += b" " * (-len(encoded) % 4)
    total = 12 + 8 + len(encoded) + len(binary_chunk)
    result = struct.pack("<4sII", b"glTF", 2, total) + struct.pack("<II", len(encoded), kind) + encoded + binary_chunk
    return result, {"source_sha256": SOURCE_SHA256, "runtime_sha256": hashlib.sha256(result).hexdigest(),
                    "runtime_bytes": len(result), "unchanged_bin_chunk_sha256": hashlib.sha256(binary_chunk).hexdigest(),
                    "unchanged": ["geometry", "UVs", "weights", "inverse binds", "joint hierarchy", "animation outputs/times", "PNG pixels", "materials"],
                    "skinned_mesh_nodes_before": 5, "skinned_mesh_nodes_after": 1}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path, help="New runtime GLB; never overwrites a delivery")
    args = parser.parse_args()
    result, report = normalize(args.source.read_bytes())
    with args.out.open("xb") as output:
        output.write(result)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
