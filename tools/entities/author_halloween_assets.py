#!/usr/bin/env python3
"""Author the shipped Halloween entity material contracts.

The four Halloween entity GLBs (``carved-pumpkin``, ``sheet-ghost``,
``sheet-ghost-cat``, ``pumpkin-skeleton``) were modelled and rigged outside the
repository's toolkit. They are structurally complete but their materials do not
yet carry the contracts the runtime needs:

* ``pumpkin-skeleton`` draws its whole body with one opaque material, so the
  pumpkin head cannot glow independently. The head triangles (every vertex
  whose dominant skin joint is ``piece_pumpkin_head``) are moved into a second
  primitive that shares the same attributes and skin and binds a ``pumpkin_head``
  material with a mild warm emissive factor. Geometry, UVs, vertex colours,
  skin, inverse bind matrices and every animation channel are untouched — only
  the index set is partitioned, so the two primitives together draw exactly the
  original triangles.
* ``sheet-ghost`` gets the blended-material contract (``alphaMode: "BLEND"``)
  the translucent character route draws, plus the mild cyan emissive factor
  that makes the cloth glow eerily. Its painted face (eyes, nose and mouth,
  carried in the vertex colours) becomes a second primitive with a much dimmer
  emissive factor, so the features stay readable as darker holes instead of
  being flooded by the glow. Geometry, UVs, vertex colours, skin, inverse bind
  matrices and every animation channel are untouched — only the index set is
  partitioned, so the two primitives together draw exactly the original
  triangles.
* ``sheet-ghost-cat`` is the sheet ghost's small companion. It ships a single
  material for the whole sheet, so it takes the same ``alphaMode: "BLEND"`` and
  the same cyan emissive family directly; there is no face partition and no
  geometry edit at all.
* ``carved-pumpkin`` is already complete: its ``candle_flame`` primitive
  carries the warm emissive factor and the carved shell samples the same shared
  surface sheet. The tool only verifies that contract.

The tool is pure stdlib and deterministic: the same input GLB yields
byte-identical output. ``--check`` runs the structural, origin and
bind-inverse verification **and** compares the shipped bytes against a pinned
SHA-256, so any edit to a shipped GLB (even one that preserves every invariant)
fails the repository check instead of silently drifting. Re-authoring an asset
in write mode prints its new hash for :data:`AUTHORED_SHA256`.

    python3 tools/entities/author_halloween_assets.py            # write
    python3 tools/entities/author_halloween_assets.py --check    # verify
    python3 tools/entities/author_halloween_assets.py --glb path # one asset
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT / "tools" / "entities") not in sys.path:
    sys.path.insert(0, str(REPO_ROOT / "tools" / "entities"))

import rig  # noqa: E402  (tools/entities on sys.path)

GLB_MAGIC = 0x46546C67
GLB_VERSION = 2
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942

COMPONENT_UNSIGNED_SHORT = 5123
COMPONENT_FLOAT = 5126
TYPE_SCALAR = "SCALAR"
TYPE_VEC3 = "VEC3"
TYPE_VEC4 = "VEC4"

# Origin-convention tolerances, mirroring the shipped-asset budget suite: the
# horizontal bind-pose centre must lie within 20 mm of the origin and the
# lowest vertex within 12 mm of y = 0 (a hovering entity is normalized so its
# hem rests on the origin and its placements lift it).
ORIGIN_CENTRE_TOLERANCE_M = 0.02
ORIGIN_BASE_TOLERANCE_M = 0.012

PUMPKIN_HEAD_JOINT = "piece_pumpkin_head"
PUMPKIN_HEAD_MATERIAL = "pumpkin_head"
PUMPKIN_HEAD_EMISSIVE = [0.6, 0.24, 0.07]
GHOST_EMISSIVE = [0.14, 0.5, 0.58]
GHOST_FACE_MATERIAL = "sheet_face"
# The painted face features stay much dimmer than the cloth, so the sheet glows
# while the eyes, nose and mouth read as darker holes instead of being flooded.
GHOST_FACE_EMISSIVE = [0.03, 0.1, 0.12]
# Per-vertex mean RGB below this is a painted face feature (eyes/nose/mouth).
GHOST_FACE_LUMA_THRESHOLD = 0.35

# The shipped assets this tool owns, relative to the repository root.
ASSETS: Dict[str, str] = {
    "carved-pumpkin": "assets/entities/carved-pumpkin/model/carved-pumpkin.glb",
    "pumpkin-skeleton": "assets/entities/pumpkin-skeleton/model/pumpkin-skeleton.glb",
    "sheet-ghost": "assets/entities/sheet-ghost/model/sheet-ghost.glb",
    "sheet-ghost-cat": "assets/entities/sheet-ghost-cat/model/sheet-ghost-cat.glb",
}

# Assets whose bind pose is authored to hover: their hem stays above the
# origin plane and a placement's own `y` lifts them further. Every other
# asset is grounded so its lowest vertex rests on the origin plane.
HOVERING_ASSETS = frozenset({"sheet-ghost", "sheet-ghost-cat"})

# SHA-256 of the authored bytes, pinned so `--check` detects any tampering with
# the shipped GLBs (a hand edit that preserves every structural invariant is
# still a drift the repository gate must see). Re-authoring an asset prints the
# new hash for this table; an idempotent re-run keeps it unchanged.
AUTHORED_SHA256: Dict[str, str] = {
    "carved-pumpkin": "b6f76aec463625bc09906ee9c17f7168caca34b80a5745144d5de05b2440d285",
    "pumpkin-skeleton": "5a7511b0b1c22311c43d2ef50f3762458a86c68c42ffc9f9161dda6e00653953",
    "sheet-ghost": "f0344ea2dc98fff90b8e4c9971ce88860eee90b3044feee9165f0b62ce7f332a",
    "sheet-ghost-cat": "f3007576af73bd002121a4af59fd95dc4715db0bd7795ec57ccfc5fa77619a5f",
}


class AuthoringError(ValueError):
    """A GLB that cannot carry the Halloween material contract."""


# ------------------------------------------------------------------ GLB io


def read_glb(path: Path) -> Tuple[dict, bytes]:
    """Reads a GLB into its JSON document and binary chunk, strictly."""
    data = path.read_bytes()
    if len(data) < 12:
        raise AuthoringError(f"{path}: file is shorter than a GLB header")
    magic, version, length = struct.unpack_from("<III", data, 0)
    if magic != GLB_MAGIC:
        raise AuthoringError(f"{path}: not a GLB (magic {magic:#x})")
    if version != GLB_VERSION:
        raise AuthoringError(f"{path}: GLB version {version}, expected {GLB_VERSION}")
    if length != len(data):
        raise AuthoringError(f"{path}: header length {length} != file length {len(data)}")
    offset = 12
    document: Optional[dict] = None
    binary = b""
    while offset < length:
        if offset + 8 > length:
            raise AuthoringError(f"{path}: truncated chunk header")
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        start = offset + 8
        end = start + chunk_length
        if end > length:
            raise AuthoringError(f"{path}: chunk overruns the file")
        chunk = data[start:end]
        if chunk_type == CHUNK_JSON:
            if document is not None:
                raise AuthoringError(f"{path}: more than one JSON chunk")
            document = json.loads(chunk)
        elif chunk_type == CHUNK_BIN:
            if binary:
                raise AuthoringError(f"{path}: more than one BIN chunk")
            binary = chunk
        offset = end + ((4 - chunk_length % 4) % 4)
    if document is None:
        raise AuthoringError(f"{path}: no JSON chunk")
    return document, binary


def _align(value: int, alignment: int = 4) -> int:
    return value + ((alignment - value % alignment) % alignment)


def write_glb(document: dict, binary: bytes) -> bytes:
    """Serialises a document and binary chunk into deterministic GLB bytes."""
    json_bytes = json.dumps(document, separators=(",", ":"), sort_keys=False).encode("utf-8")
    json_padding = b" " * ((4 - len(json_bytes) % 4) % 4)
    bin_padding = b"\x00" * ((4 - len(binary) % 4) % 4)
    total = 12 + 8 + len(json_bytes) + len(json_padding)
    if binary:
        total += 8 + len(binary) + len(bin_padding)
    header = struct.pack("<III", GLB_MAGIC, GLB_VERSION, total)
    chunks = struct.pack("<II", len(json_bytes) + len(json_padding), CHUNK_JSON)
    chunks += json_bytes + json_padding
    if binary:
        chunks += struct.pack("<II", len(binary) + len(bin_padding), CHUNK_BIN)
        chunks += binary + bin_padding
    return header + chunks


# ------------------------------------------------------------- accessors


def accessor(document: dict, binary: bytes, index: int) -> Tuple[list, int, int]:
    """Reads one accessor into (values, component_type, components_per_element)."""
    accessors = document.get("accessors", [])
    if not 0 <= index < len(accessors):
        raise AuthoringError(f"accessor {index} does not exist")
    entry = accessors[index]
    component_type = entry["componentType"]
    packs = {
        5120: "b",
        5121: "B",
        5122: "h",
        5123: "H",
        5125: "I",
        5126: "f",
    }
    pack = packs.get(component_type)
    if pack is None:
        raise AuthoringError(f"accessor {index}: unsupported component type {component_type}")
    normalized = bool(entry.get("normalized", False))
    scale = 1.0
    if normalized and pack in ("b", "B", "h", "H"):
        scale = float((1 << (8 * struct.calcsize(pack))) - 1)
    components = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[entry["type"]]
    view = document["bufferViews"][entry["bufferView"]]
    start = view.get("byteOffset", 0) + entry.get("byteOffset", 0)
    stride = view.get("byteStride") or struct.calcsize("<" + pack * components)
    size = struct.calcsize("<" + pack * components)
    values = []
    for item in range(entry["count"]):
        position = start + item * stride
        if position + size > len(binary):
            raise AuthoringError(f"accessor {index}: reads past the binary chunk")
        unpacked = struct.unpack_from("<" + pack * components, binary, position)
        if scale != 1.0:
            unpacked = tuple(value / scale for value in unpacked)
        values.append(unpacked)
    return values, component_type, components


def node_names(document: dict) -> List[str]:
    return [node.get("name", f"node_{index}") for index, node in enumerate(document.get("nodes", []))]


def dominant_joint_names(document: dict, binary: bytes, primitive: dict) -> List[str]:
    """Dominant skin-joint name per vertex of one skinned primitive."""
    attributes = primitive.get("attributes", {})
    for required in ("JOINTS_0", "WEIGHTS_0"):
        if required not in attributes:
            raise AuthoringError(f"primitive lacks {required}")
    skins = document.get("skins", [])
    if len(skins) != 1:
        raise AuthoringError(f"expected exactly one skin, found {len(skins)}")
    joints = skins[0]["joints"]
    names = node_names(document)
    joint_slots, _, _ = accessor(document, binary, attributes["JOINTS_0"])
    weights, _, _ = accessor(document, binary, attributes["WEIGHTS_0"])
    if len(joint_slots) != len(weights):
        raise AuthoringError("JOINTS_0 and WEIGHTS_0 element counts differ")
    dominant: List[str] = []
    for slots, influence in zip(joint_slots, weights):
        best = max(range(len(influence)), key=lambda index: influence[index])
        slot = int(slots[best])
        if not 0 <= slot < len(joints):
            raise AuthoringError(f"joint slot {slot} outside the skin")
        dominant.append(names[joints[slot]])
    return dominant


def indices_of(document: dict, binary: bytes, primitive: dict) -> List[int]:
    if "indices" not in primitive:
        raise AuthoringError("primitive has no index accessor")
    values, component_type, components = accessor(document, binary, primitive["indices"])
    if component_type != COMPONENT_UNSIGNED_SHORT or components != 1:
        raise AuthoringError("primitive indices must be unsigned short scalars")
    if len(values) % 3 != 0:
        raise AuthoringError("primitive indices are not whole triangles")
    return [int(value[0]) for value in values]


def _combined_position_bounds(document: dict, binary: bytes) -> Tuple[List[float], List[float]]:
    """Bind-pose POSITION bounds over every distinct accessor."""
    mins = [float("inf")] * 3
    maxs = [float("-inf")] * 3
    seen = set()
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            index = primitive.get("attributes", {}).get("POSITION")
            if index is None or index in seen:
                continue
            seen.add(index)
            values, _, _ = accessor(document, binary, index)
            for value in values:
                for axis in range(3):
                    mins[axis] = min(mins[axis], value[axis])
                    maxs[axis] = max(maxs[axis], value[axis])
    if not seen:
        raise AuthoringError("model has no POSITION accessor")
    return mins, maxs


def _position_accessors(document: dict) -> List[int]:
    seen: List[int] = []
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            index = primitive.get("attributes", {}).get("POSITION")
            if index is not None and index not in seen:
                seen.append(index)
    return seen


def _shift_accessor(document: dict, buffer: bytearray, index: int, delta: Sequence[float]) -> None:
    """Adds `delta` to every VEC3 element of a float accessor, in place."""
    entry = document["accessors"][index]
    if entry["componentType"] != COMPONENT_FLOAT or entry["type"] != TYPE_VEC3:
        raise AuthoringError(f"accessor {index} is not a float VEC3")
    view = document["bufferViews"][entry["bufferView"]]
    start = view.get("byteOffset", 0) + entry.get("byteOffset", 0)
    stride = view.get("byteStride") or 12
    for element in range(entry["count"]):
        offset = start + element * stride
        values = struct.unpack_from("<fff", buffer, offset)
        struct.pack_into(
            "<fff",
            buffer,
            offset,
            values[0] + delta[0],
            values[1] + delta[1],
            values[2] + delta[2],
        )


def _root_joints(document: dict) -> List[int]:
    """Root nodes that are skin joints (the nodes the whole rig hangs from)."""
    child = set()
    for node in document.get("nodes", []):
        child.update(node.get("children", []))
    roots = [index for index in range(len(document.get("nodes", []))) if index not in child]
    joints = set()
    for skin in document.get("skins", []):
        joints.update(skin.get("joints", []))
    return [index for index in roots if index in joints]


def _shift_ibm_translations(
    document: dict, buffer: bytearray, roots: Sequence[int], delta: Sequence[float]
) -> None:
    """Applies `IBM := IBM @ T(-delta)` to every joint under the shifted roots.

    A root joint's rest translation gained `delta`, so every joint in its
    subtree gains the same translation in rest space. Correcting each inverse
    bind matrix by the same translation keeps `rest_global @ IBM` the identity,
    so the flattened and skinned rest poses stay exactly where the shifted
    vertices put them.
    """
    nodes = document.get("nodes", [])
    parent: Dict[int, int] = {}
    for index, node in enumerate(nodes):
        for child in node.get("children", []):
            parent[child] = index
    shifted = set()
    for skin in document.get("skins", []):
        for joint in skin.get("joints", []):
            cursor = joint
            while cursor in parent and cursor not in roots:
                cursor = parent[cursor]
            if cursor in roots:
                shifted.add(joint)
    if not shifted:
        raise AuthoringError("no joint lies under a shifted root")
    for skin in document.get("skins", []):
        joints = skin.get("joints", [])
        accessor_index = skin.get("inverseBindMatrices")
        if accessor_index is None:
            raise AuthoringError("skin has no inverse bind matrices")
        entry = document["accessors"][accessor_index]
        view = document["bufferViews"][entry["bufferView"]]
        start = view.get("byteOffset", 0) + entry.get("byteOffset", 0)
        stride = view.get("byteStride") or 64
        for slot, joint in enumerate(joints):
            if joint not in shifted:
                continue
            offset = start + slot * stride
            matrix = list(struct.unpack_from("<16f", buffer, offset))
            dx, dy, dz = delta
            matrix[12] -= matrix[0] * dx + matrix[4] * dy + matrix[8] * dz
            matrix[13] -= matrix[1] * dx + matrix[5] * dy + matrix[9] * dz
            matrix[14] -= matrix[2] * dx + matrix[6] * dy + matrix[10] * dz
            struct.pack_into("<16f", buffer, offset, *matrix)


def normalize_model_origin(
    document: dict, binary: bytes, label: str, ground_base: bool = True
) -> Tuple[dict, bytes]:
    """Puts the bind-pose mesh on the repository's model origin convention.

    The repository requires the horizontal bind-pose centre within
    [`ORIGIN_CENTRE_TOLERANCE_M`] of the origin and, for a grounded model, the
    lowest vertex within [`ORIGIN_BASE_TOLERANCE_M`] of `y = 0`. The externally
    modelled Halloween assets are off-centre; the sheet ghost is also authored
    to hover (`ground_base: false` keeps that hover: its hem stays above the
    origin and its placement `y` is zero). The whole skinned model is
    translated: POSITION values and accessor bounds, the root joint's rest
    translation, every root `translation` animation key and every joint's
    inverse bind matrix move by the same delta. The rig-relative geometry, UVs
    and clips are untouched, so an attached socket keeps its offset.
    """
    mins, maxs = _combined_position_bounds(document, binary)
    centre_x = (mins[0] + maxs[0]) / 2.0
    centre_z = (mins[2] + maxs[2]) / 2.0
    delta = [-centre_x, 0.0, -centre_z]
    if ground_base and abs(mins[1]) > ORIGIN_BASE_TOLERANCE_M:
        delta[1] = -mins[1]
    if all(abs(value) <= 1.0e-6 for value in delta):
        return document, binary
    buffer = bytearray(binary)
    shifted = set()
    for index in _position_accessors(document):
        shifted.add(index)
        _shift_accessor(document, buffer, index, delta)
        entry = document["accessors"][index]
        if "min" in entry:
            entry["min"] = [value + delta[axis] for axis, value in enumerate(entry["min"])]
        if "max" in entry:
            entry["max"] = [value + delta[axis] for axis, value in enumerate(entry["max"])]
    roots = _root_joints(document)
    if not roots:
        raise AuthoringError(f"{label}: no root joint to carry the origin shift")
    for index in roots:
        node = document["nodes"][index]
        translation = node.get("translation", [0.0, 0.0, 0.0])
        node["translation"] = [value + delta[axis] for axis, value in enumerate(translation)]
    _shift_ibm_translations(document, buffer, roots, delta)
    for animation in document.get("animations", []):
        for channel in animation.get("channels", []):
            target = channel.get("target", {})
            if target.get("path") != "translation" or target.get("node") not in roots:
                continue
            sampler = animation["samplers"][channel["sampler"]]
            output = sampler["output"]
            if output in shifted:
                continue
            shifted.add(output)
            _shift_accessor(document, buffer, output, delta)
    new_min = [value + delta[axis] for axis, value in enumerate(mins)]
    new_max = [value + delta[axis] for axis, value in enumerate(maxs)]
    print(
        f"{label}: origin shift ({delta[0]:+.4f}, {delta[1]:+.4f}, {delta[2]:+.4f}) "
        f"-> centre ({((new_min[0] + new_max[0]) / 2.0):+.4f}, {((new_min[2] + new_max[2]) / 2.0):+.4f}), "
        f"base y={new_min[1]:+.4f}"
    )
    return document, bytes(buffer)


# ------------------------------------------------------- asset contracts


def is_partitioned(document: dict, material_name: str) -> bool:
    """True when the single mesh already carries the named second material."""
    meshes = document.get("meshes", [])
    return (
        len(meshes) == 1
        and len(meshes[0].get("primitives", [])) == 2
        and len(document.get("materials", [])) == 2
        and document["materials"][1].get("name") == material_name
    )


def _single_primitive(document: dict, label: str) -> dict:
    meshes = document.get("meshes", [])
    if len(meshes) != 1 or len(meshes[0].get("primitives", [])) != 1:
        raise AuthoringError(f"{label}: expected one mesh with one primitive")
    primitive = meshes[0]["primitives"][0]
    if primitive.get("material") != 0:
        raise AuthoringError(f"{label}: the body primitive must bind material 0")
    return primitive


def _mirrored_material(body: dict, name: str, emissive: List[float], alpha_mode: str) -> dict:
    """A second material sharing the body's surface but with its own emission."""
    material = {
        "name": name,
        "pbrMetallicRoughness": json.loads(json.dumps(body.get("pbrMetallicRoughness", {}))),
        "emissiveFactor": list(emissive),
        "alphaMode": alpha_mode,
    }
    if "doubleSided" in body:
        material["doubleSided"] = body["doubleSided"]
    return material


def vertex_colours(document: dict, binary: bytes, primitive: dict) -> List[tuple]:
    """Normalised `COLOR_0` per vertex of one primitive."""
    if "COLOR_0" not in primitive.get("attributes", {}):
        raise AuthoringError("primitive lacks COLOR_0")
    values, _, _ = accessor(document, binary, primitive["attributes"]["COLOR_0"])
    return values


def partition_primitive(
    document: dict,
    binary: bytes,
    label: str,
    wanted,
    new_material: dict,
) -> Tuple[dict, bytes]:
    """Splits the single mesh's triangles into a body and a wanted part.

    `wanted(triangle)` receives three vertex indices and decides which
    primitive the triangle belongs to. Both index ranges are written into one
    new buffer view at the end of the binary chunk; the original index accessor
    is repointed at the body range and one new accessor holds the parted range,
    so the accessor list gains one entry and nothing stale is left referencing
    the old interleaved range. Geometry, skin, inverse bind matrices and every
    animation channel are byte-identical: only the triangle partition and the
    material list change.
    """
    meshes = document.get("meshes", [])
    primitive = _single_primitive(document, label)
    attributes = primitive["attributes"]
    indices = indices_of(document, binary, primitive)
    original_accessor = primitive["indices"]
    if document["accessors"][original_accessor].get("componentType") != COMPONENT_UNSIGNED_SHORT:
        raise AuthoringError(f"{label}: body indices are not unsigned shorts")

    body: List[int] = []
    parted: List[int] = []
    for start in range(0, len(indices), 3):
        triangle = indices[start : start + 3]
        (parted if wanted(triangle) else body).append(start)
    if not parted or not body:
        raise AuthoringError(
            f"{label}: partition produced {len(body)} body and {len(parted)} part triangles"
        )
    parted_indices = [indices[start + offset] for start in parted for offset in range(3)]
    body_indices = [indices[start + offset] for start in body for offset in range(3)]
    if len(parted_indices) + len(body_indices) != len(indices):
        raise AuthoringError(f"{label}: the partition lost indices")

    body_bytes = struct.pack(f"<{len(body_indices)}H", *body_indices)
    parted_bytes = struct.pack(f"<{len(parted_indices)}H", *parted_indices)
    byte_offset = _align(len(binary))
    padded = binary + b"\x00" * (byte_offset - len(binary)) + body_bytes + parted_bytes
    document["bufferViews"].append(
        {
            "buffer": 0,
            "byteOffset": byte_offset,
            "byteLength": len(body_bytes) + len(parted_bytes),
        }
    )
    shared_view = len(document["bufferViews"]) - 1
    document["accessors"][original_accessor] = {
        "bufferView": shared_view,
        "componentType": COMPONENT_UNSIGNED_SHORT,
        "count": len(body_indices),
        "type": TYPE_SCALAR,
        "min": [min(body_indices)],
        "max": [max(body_indices)],
    }
    document["accessors"].append(
        {
            "bufferView": shared_view,
            "byteOffset": len(body_bytes),
            "componentType": COMPONENT_UNSIGNED_SHORT,
            "count": len(parted_indices),
            "type": TYPE_SCALAR,
            "min": [min(parted_indices)],
            "max": [max(parted_indices)],
        }
    )
    parted_accessor = len(document["accessors"]) - 1

    document["materials"].append(new_material)
    parted_material = len(document["materials"]) - 1
    meshes[0]["primitives"] = [
        {"attributes": dict(attributes), "indices": original_accessor, "material": 0},
        {
            "attributes": dict(attributes),
            "indices": parted_accessor,
            "material": parted_material,
        },
    ]
    document["buffers"] = [{"byteLength": len(padded)}]
    return document, padded


def split_pumpkin_head(document: dict, binary: bytes) -> Tuple[dict, bytes]:
    """Moves the pumpkin-head triangles into a second emissive primitive."""
    primitive = _single_primitive(document, "pumpkin-skeleton")
    dominant = dominant_joint_names(document, binary, primitive)

    def wanted(triangle: Sequence[int]) -> bool:
        votes = sum(1 for vertex in triangle if dominant[vertex] == PUMPKIN_HEAD_JOINT)
        return votes >= 2

    material = _mirrored_material(
        document["materials"][0], PUMPKIN_HEAD_MATERIAL, PUMPKIN_HEAD_EMISSIVE, "OPAQUE"
    )
    return partition_primitive(document, binary, "pumpkin-skeleton", wanted, material)


def split_ghost_face(document: dict, binary: bytes) -> Tuple[dict, bytes]:
    """Moves the painted face features into a second, dimmer emissive primitive.

    The face (eyes, nose and mouth) is painted into the vertex colours, not the
    texture. A uniformly emissive sheet would flood those features at night, so
    the dark triangles get their own material with a much dimmer cyan emissive
    factor: the cloth still glows while the face stays readable as darker
    holes. The two primitives together draw exactly the original triangles.
    """
    primitive = _single_primitive(document, "sheet-ghost")
    colours = vertex_colours(document, binary, primitive)

    def wanted(triangle: Sequence[int]) -> bool:
        votes = 0
        for vertex in triangle:
            colour = colours[vertex]
            if (colour[0] + colour[1] + colour[2]) / 3.0 < GHOST_FACE_LUMA_THRESHOLD:
                votes += 1
        return votes >= 2

    material = _mirrored_material(
        document["materials"][0], GHOST_FACE_MATERIAL, GHOST_FACE_EMISSIVE, "BLEND"
    )
    return partition_primitive(document, binary, "sheet-ghost", wanted, material)


def blend_ghost_material(document: dict) -> dict:
    """Installs the ghost's blended cyan material contract on both primitives."""
    materials = document.get("materials", [])
    if len(materials) != 2:
        raise AuthoringError(f"sheet-ghost: expected two materials, found {len(materials)}")
    cloth, face = materials
    cloth["alphaMode"] = "BLEND"
    cloth["emissiveFactor"] = list(GHOST_EMISSIVE)
    if face.get("name") != GHOST_FACE_MATERIAL:
        raise AuthoringError(f"sheet-ghost: the second material is not {GHOST_FACE_MATERIAL}")
    if face.get("alphaMode") != "BLEND":
        raise AuthoringError("sheet-ghost: the face material must keep the blend contract")
    return document


def blend_ghost_cat_material(document: dict) -> dict:
    """Installs the ghost cat's blended cyan material contract.

    The cat ships one material for the whole sheet, so there is no face
    partition to keep dimmer: the single material takes the same
    ``alphaMode: "BLEND"`` and the same mild cyan emissive family as the sheet
    ghost's cloth, which is exactly the pair the translucent, fading character
    route expects. Geometry, UVs, vertex colours, skin, inverse bind matrices
    and every animation channel are untouched.
    """
    materials = document.get("materials", [])
    if len(materials) != 1:
        raise AuthoringError(
            f"sheet-ghost-cat: expected one material, found {len(materials)}"
        )
    material = materials[0]
    material["alphaMode"] = "BLEND"
    material["emissiveFactor"] = list(GHOST_EMISSIVE)
    return document


# --------------------------------------------------------------- checks


def check_carved_pumpkin(document: dict, path: Path) -> None:
    """The carved pumpkin ships complete; fail loudly if that changes."""
    materials = document.get("materials", [])
    if len(materials) < 2:
        raise AuthoringError(f"{path}: expected the shell and candle_flame materials")
    flame = next(
        (material for material in materials if material.get("name") == "candle_flame"), None
    )
    if flame is None:
        raise AuthoringError(f"{path}: the candle_flame material is missing")
    emissive = flame.get("emissiveFactor", [0.0, 0.0, 0.0])
    if max(emissive) <= 0.0:
        raise AuthoringError(f"{path}: candle_flame has no emissive factor")
    primitives = [
        primitive
        for mesh in document.get("meshes", [])
        for primitive in mesh.get("primitives", [])
    ]
    if not any(
        materials[primitive.get("material", 0)].get("name") == "candle_flame"
        for primitive in primitives
    ):
        raise AuthoringError(f"{path}: no primitive binds the candle_flame material")


def planned_bytes(name: str, path: Path) -> bytes:
    """The exact bytes the shipped asset must have, derived from the source.

    Idempotent: origin normalization and the material contracts are applied on
    every run, and an already-authored partition is not re-partitioned. The
    carved pumpkin is complete as shipped and is never rewritten. To change an
    authored contract, restore the raw GLB from git and re-run.
    """
    document, binary = read_glb(path)
    if name == "carved-pumpkin":
        # Already inside every contract (emissive flame, origin convention);
        # never rewrite an asset whose bytes this tool did not author.
        check_carved_pumpkin(document, path)
        return path.read_bytes()
    document, binary = normalize_model_origin(
        document, binary, name, ground_base=(name not in HOVERING_ASSETS)
    )
    if name == "pumpkin-skeleton":
        if not is_partitioned(document, PUMPKIN_HEAD_MATERIAL):
            document, binary = split_pumpkin_head(document, binary)
    elif name == "sheet-ghost":
        if not is_partitioned(document, GHOST_FACE_MATERIAL):
            document, binary = split_ghost_face(document, binary)
        document = blend_ghost_material(document)
    elif name == "sheet-ghost-cat":
        document = blend_ghost_cat_material(document)
    else:
        raise AuthoringError(f"unknown asset {name}")
    return write_glb(document, binary)


def _verify_partition(
    document: dict,
    binary: bytes,
    path: Path,
    label: str,
    material_name: str,
    expected_emissive: List[float],
    wanted,
) -> None:
    """Shared structural checks for a body/part triangle partition."""
    meshes = document.get("meshes", [])
    if len(meshes) != 1 or len(meshes[0].get("primitives", [])) != 2:
        raise AuthoringError(f"{path}: expected the two-primitive {label} partition")
    primitives = meshes[0]["primitives"]
    parted_material = document["materials"][primitives[1].get("material", 0)]
    if parted_material.get("name") != material_name:
        raise AuthoringError(f"{path}: the second material is not {material_name}")
    emissive = parted_material.get("emissiveFactor")
    if emissive != expected_emissive:
        raise AuthoringError(f"{path}: {material_name} emissive {emissive} != {expected_emissive}")
    vertex_count = document["accessors"][primitives[0]["attributes"]["POSITION"]]["count"]
    body = indices_of(document, binary, primitives[0])
    parted = indices_of(document, binary, primitives[1])
    for part_label, values, expected_part in (
        ("body", body, False),
        ("part", parted, True),
    ):
        if not values:
            raise AuthoringError(f"{path}: the {part_label} primitive is empty")
        if max(values) >= vertex_count or min(values) < 0:
            raise AuthoringError(f"{path}: {part_label} indices exceed the vertex count")
        for start in range(0, len(values), 3):
            if wanted(values[start : start + 3]) != expected_part:
                raise AuthoringError(
                    f"{path}: a {part_label} triangle crosses the {label} partition"
                )
    # The original index bufferView is left unreferenced at its old byte
    # length; the partition must cover exactly those triangles.
    shared_view = document["accessors"][primitives[1]["indices"]]["bufferView"]
    original_bytes = [
        view.get("byteLength", 0)
        for index, view in enumerate(document["bufferViews"])
        if index != shared_view and view.get("byteLength", 0) == 2 * (len(body) + len(parted))
    ]
    if not original_bytes:
        raise AuthoringError(
            f"{path}: the partition has {len(body) + len(parted)} indices but no "
            "original index buffer view matches"
        )


def _rest_global_matrices(document: dict) -> Dict[int, List[List[float]]]:
    """Composed rest transforms for every node (row-major 4x4)."""
    nodes = document.get("nodes", [])
    parent: Dict[int, int] = {}
    for index, node in enumerate(nodes):
        for child in node.get("children", []):
            parent[child] = index

    def local(node: dict) -> List[List[float]]:
        x, y, z, w = node.get("rotation", [0.0, 0.0, 0.0, 1.0])
        matrix = [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w), 0.0],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w), 0.0],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y), 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
        scale = node.get("scale", [1.0, 1.0, 1.0])
        for row in range(3):
            for column in range(3):
                matrix[row][column] *= scale[column]
        translation = node.get("translation", [0.0, 0.0, 0.0])
        for axis in range(3):
            matrix[axis][3] = translation[axis]
        return matrix

    def multiply(a, b):
        return [
            [sum(a[row][k] * b[k][column] for k in range(4)) for column in range(4)]
            for row in range(4)
        ]

    globals_: Dict[int, List[List[float]]] = {}
    for index in range(len(nodes)):
        matrix = local(nodes[index])
        cursor = index
        while cursor in parent:
            cursor = parent[cursor]
            matrix = multiply(local(nodes[cursor]), matrix)
        globals_[index] = matrix
    return globals_


def _verify_bind_inverses(document: dict, binary: bytes, path: Path) -> None:
    """Every joint's rest transform composed with its IBM must be the identity."""
    globals_ = _rest_global_matrices(document)
    for skin in document.get("skins", []):
        accessor_index = skin.get("inverseBindMatrices")
        if accessor_index is None:
            raise AuthoringError(f"{path}: skin has no inverse bind matrices")
        values, _, _ = accessor(document, binary, accessor_index)
        for slot, node in enumerate(skin.get("joints", [])):
            if slot >= len(values):
                raise AuthoringError(f"{path}: skin has fewer IBMs than joints")
            ibm = values[slot]
            matrix = [[ibm[column * 4 + row] for column in range(4)] for row in range(4)]
            product = [
                [
                    sum(globals_[node][row][k] * matrix[k][column] for k in range(4))
                    for column in range(4)
                ]
                for row in range(4)
            ]
            for row in range(4):
                for column in range(4):
                    expected = 1.0 if row == column else 0.0
                    if abs(product[row][column] - expected) > 1.0e-3:
                        raise AuthoringError(
                            f"{path}: joint {node} rest transform * IBM is not the identity"
                        )


def verify(name: str, path: Path, report: dict) -> List[str]:
    problems: List[str] = []
    document, binary = read_glb(path)
    try:
        _verify_bind_inverses(document, binary, path)
    except AuthoringError as error:
        problems.append(str(error))
    mins, maxs = _combined_position_bounds(document, binary)
    centre_x = (mins[0] + maxs[0]) / 2.0
    centre_z = (mins[2] + maxs[2]) / 2.0
    if abs(centre_x) > ORIGIN_CENTRE_TOLERANCE_M or abs(centre_z) > ORIGIN_CENTRE_TOLERANCE_M:
        problems.append(
            f"{path}: bind-pose centre ({centre_x:.3f}, {centre_z:.3f}) is outside "
            f"the +/-{ORIGIN_CENTRE_TOLERANCE_M} m origin tolerance"
        )
    if not -ORIGIN_BASE_TOLERANCE_M <= mins[1] <= ORIGIN_BASE_TOLERANCE_M:
        # A grounded model's base sits on the origin plane; a hovering model
        # (the sheet ghosts, `ground_base: false`) must keep its lowest vertex
        # above it and inside its bound height.
        hovering = name in HOVERING_ASSETS
        if not (hovering and mins[1] > 0.0 and mins[1] <= maxs[1]):
            problems.append(
                f"{path}: lowest vertex sits at y={mins[1]:.3f}, outside "
                f"+/-{ORIGIN_BASE_TOLERANCE_M} m of the origin plane"
            )
    if name == "pumpkin-skeleton":
        try:
            meshes = document.get("meshes", [])
            primitives = meshes[0].get("primitives", []) if len(meshes) == 1 else []
            if len(primitives) != 2:
                raise AuthoringError(f"{path}: expected the two-primitive pumpkin-head partition")
            dominant = dominant_joint_names(document, binary, primitives[0])

            def wanted(triangle: Sequence[int]) -> bool:
                votes = sum(1 for vertex in triangle if dominant[vertex] == PUMPKIN_HEAD_JOINT)
                return votes >= 2

            _verify_partition(
                document,
                binary,
                path,
                "pumpkin-head",
                PUMPKIN_HEAD_MATERIAL,
                PUMPKIN_HEAD_EMISSIVE,
                wanted,
            )
        except AuthoringError as error:
            problems.append(str(error))
    elif name == "sheet-ghost":
        try:
            meshes = document.get("meshes", [])
            primitives = meshes[0].get("primitives", []) if len(meshes) == 1 else []
            if len(primitives) != 2:
                raise AuthoringError(f"{path}: expected the two-primitive ghost-face partition")
            colours = vertex_colours(document, binary, primitives[0])

            def wanted(triangle: Sequence[int]) -> bool:
                votes = 0
                for vertex in triangle:
                    colour = colours[vertex]
                    if (colour[0] + colour[1] + colour[2]) / 3.0 < GHOST_FACE_LUMA_THRESHOLD:
                        votes += 1
                return votes >= 2

            _verify_partition(
                document,
                binary,
                path,
                "ghost-face",
                GHOST_FACE_MATERIAL,
                GHOST_FACE_EMISSIVE,
                wanted,
            )
            cloth, face = document["materials"]
            if cloth.get("alphaMode") != "BLEND" or face.get("alphaMode") != "BLEND":
                problems.append(f"{path}: both ghost materials must keep the BLEND contract")
            if cloth.get("emissiveFactor") != GHOST_EMISSIVE:
                problems.append(
                    f"{path}: cloth emissive {cloth.get('emissiveFactor')} != {GHOST_EMISSIVE}"
                )
        except AuthoringError as error:
            problems.append(str(error))
    elif name == "sheet-ghost-cat":
        try:
            materials = document.get("materials", [])
            if len(materials) != 1:
                raise AuthoringError(
                    f"{path}: expected the single ghost-cat material, found {len(materials)}"
                )
            material = materials[0]
            if material.get("alphaMode") != "BLEND":
                problems.append(
                    f"{path}: the ghost-cat material must keep the BLEND contract"
                )
            if material.get("emissiveFactor") != GHOST_EMISSIVE:
                problems.append(
                    f"{path}: the ghost-cat emissive {material.get('emissiveFactor')} "
                    f"!= {GHOST_EMISSIVE}"
                )
        except AuthoringError as error:
            problems.append(str(error))
    elif name == "carved-pumpkin":
        try:
            check_carved_pumpkin(document, path)
        except AuthoringError as error:
            problems.append(str(error))
    stats = rig.check_model(path)
    problems.extend(f"{path}: {problem}" for problem in stats["problems"])
    report[name] = {
        "bytes": path.stat().st_size,
        "problems": problems,
        "clips": stats["clips"],
        "joints": stats["joints"],
    }
    return problems


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify instead of writing")
    parser.add_argument("--glb", action="append", default=None, help="one model directory name")
    arguments = parser.parse_args(argv)
    names = arguments.glb or list(ASSETS)
    failures: List[str] = []
    report: dict = {}
    for name in names:
        path = REPO_ROOT / ASSETS[name]
        if arguments.check:
            failures.extend(verify(name, path, report))
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            expected = AUTHORED_SHA256.get(name)
            if digest != expected:
                failures.append(
                    f"{path}: shipped bytes are not the pinned authored bytes "
                    f"(sha256 {digest}, expected {expected}); re-run the writer and "
                    "update AUTHORED_SHA256 if the change is intended"
                )
            continue
        expected = planned_bytes(name, path)
        current = path.read_bytes()
        if current == expected:
            print(f"{name}: already current ({len(current)} bytes)")
        else:
            path.write_bytes(expected)
            print(f"{name}: wrote {len(expected)} bytes (was {len(current)})")
        print(f"{name}: sha256 {hashlib.sha256(path.read_bytes()).hexdigest()}")
        failures.extend(verify(name, path, report))
    print(json.dumps(report, indent=1))
    if failures:
        for failure in failures:
            print(f"error: {failure}", file=sys.stderr)
        return 1
    print("halloween entity material contracts OK" + (" (check)" if arguments.check else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
