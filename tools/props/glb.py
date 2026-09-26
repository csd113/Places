"""Minimal, dependency-free GLB (binary glTF 2.0) reader/writer for the prop pack.

The writer emits the single, deliberately narrow shape the game loader
supports and the validator enforces:

* one scene, one node, one mesh, one primitive, one material, one texture;
* an embedded PNG image stored in a bufferView (no external files);
* ``POSITION`` (float32 vec3), ``TEXCOORD_0`` (float32 vec2),
  ``COLOR_0`` (normalized uint8 vec4) and 16-bit triangle indices;
* a CLAMP_TO_EDGE sampler with mipmapped linear filtering;
* no glTF extensions and no morph targets. Skins and animation clips are read
  through to the mesh data unchanged: the runtime poses a placed skinned or
  animated model through its character path, so the validator must accept the
  same container the loader does (the Rust loader's shipped-asset test is the
  authority on the skin/animation structure itself).

The reader exists so tools (the preview renderer, tests) can round-trip the
same files without pulling in a third-party glTF library.
"""

from __future__ import annotations

import base64
import json
import struct
from typing import Any, Dict, List, Optional

GLB_MAGIC = 0x46546C67
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942

COMPONENT_FLOAT = 5126
COMPONENT_UINT = 5125
COMPONENT_USHORT = 5123
COMPONENT_UBYTE = 5121

TYPE_COUNTS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
COMPONENT_SIZES = {COMPONENT_FLOAT: 4, COMPONENT_UINT: 4, COMPONENT_USHORT: 2, COMPONENT_UBYTE: 1}


class GltfError(ValueError):
    """Raised when a GLB file cannot be read or does not match the pack rules."""


def _align(value: int, alignment: int = 4) -> int:
    remainder = value % alignment
    return value if remainder == 0 else value + (alignment - remainder)


# --------------------------------------------------------------------- writer


def write_glb(mesh, texture_png: bytes, name: str = "prop") -> bytes:
    """Packs a :class:`mesh.Mesh` plus one PNG into a self-contained GLB."""
    if mesh.vertex_count > 65535:
        raise GltfError(f"mesh has {mesh.vertex_count} vertices; the pack uses 16-bit indices")
    if not texture_png.startswith(b"\x89PNG"):
        raise GltfError("prop textures must be PNG")

    index_bytes = b"".join(struct.pack("<H", index) for index in mesh.indices)
    position_bytes = b"".join(struct.pack("<3f", *position) for position in mesh.positions)
    uv_bytes = b"".join(struct.pack("<2f", *uv) for uv in mesh.uvs)
    color_bytes = b"".join(struct.pack("<4B", color[0], color[1], color[2], 255) for color in mesh.colors)

    low, high = mesh.bounds()

    blob = bytearray()
    views: List[Dict[str, Any]] = []

    def add_view(payload: bytes, target: Optional[int]) -> int:
        global_alignment = 4
        while len(blob) % global_alignment != 0:
            blob.append(0)
        offset = len(blob)
        blob.extend(payload)
        view: Dict[str, Any] = {"buffer": 0, "byteOffset": offset, "byteLength": len(payload)}
        if target is not None:
            view["target"] = target
        views.append(view)
        return len(views) - 1

    index_view = add_view(index_bytes, 34963)
    position_view = add_view(position_bytes, 34962)
    uv_view = add_view(uv_bytes, 34962)
    color_view = add_view(color_bytes, 34962)
    image_view = add_view(texture_png, None)

    gltf: Dict[str, Any] = {
        "asset": {"version": "2.0", "generator": "places props toolkit (tools/props)"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": [0]}],
        "nodes": [{"name": name, "mesh": 0}],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {"POSITION": 1, "TEXCOORD_0": 2, "COLOR_0": 3},
                        "indices": 0,
                        "material": 0,
                        "mode": 4,
                    }
                ],
            }
        ],
        "accessors": [
            {
                "bufferView": index_view,
                "componentType": COMPONENT_USHORT,
                "count": len(mesh.indices),
                "type": "SCALAR",
            },
            {
                "bufferView": position_view,
                "componentType": COMPONENT_FLOAT,
                "count": mesh.vertex_count,
                "type": "VEC3",
                "min": [round(value, 5) for value in low],
                "max": [round(value, 5) for value in high],
            },
            {
                "bufferView": uv_view,
                "componentType": COMPONENT_FLOAT,
                "count": len(mesh.uvs),
                "type": "VEC2",
            },
            {
                "bufferView": color_view,
                "componentType": COMPONENT_UBYTE,
                "normalized": True,
                "count": len(mesh.colors),
                "type": "VEC4",
            },
        ],
        "bufferViews": views,
        "buffers": [{"byteLength": len(blob)}],
        "images": [{"bufferView": image_view, "mimeType": "image/png", "name": f"{name}_diffuse"}],
        "samplers": [
            {
                "magFilter": 9729,  # LINEAR
                "minFilter": 9987,  # LINEAR_MIPMAP_LINEAR
                "wrapS": 33071,  # CLAMP_TO_EDGE
                "wrapT": 33071,
            }
        ],
        "textures": [{"sampler": 0, "source": 0}],
        "materials": [
            {
                "name": f"{name}_mat",
                "pbrMetallicRoughness": {
                    "baseColorTexture": {"index": 0},
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0,
                },
                "doubleSided": True,
            }
        ],
    }

    json_chunk = json.dumps(gltf, separators=(",", ":")).encode("utf-8")
    json_chunk += b" " * (_align(len(json_chunk)) - len(json_chunk))
    bin_chunk = bytes(blob)
    bin_chunk += b"\x00" * (_align(len(bin_chunk)) - len(bin_chunk))

    total = 12 + 8 + len(json_chunk) + 8 + len(bin_chunk)
    header = struct.pack("<III", GLB_MAGIC, 2, total)
    return (
        header
        + struct.pack("<II", len(json_chunk), CHUNK_JSON)
        + json_chunk
        + struct.pack("<II", len(bin_chunk), CHUNK_BIN)
        + bin_chunk
    )


# --------------------------------------------------------------------- reader


class ReadMesh:
    """Mesh data as read from a GLB, already expanded to interleaved vertices."""

    def __init__(self) -> None:
        self.positions: List[tuple] = []
        self.uvs: List[tuple] = []
        self.colors: List[tuple] = []
        self.indices: List[int] = []
        self.texture_png: bytes = b""
        self.texture_name: str = ""
        self.material_count: int = 0
        self.material_names: List[str] = []
        self.extensions_used: List[str] = []
        self.modes: List[int] = []
        self.json: Dict[str, Any] = {}

    @property
    def triangle_count(self) -> int:
        return len(self.indices) // 3

    def bounds(self):
        xs = [p[0] for p in self.positions]
        ys = [p[1] for p in self.positions]
        zs = [p[2] for p in self.positions]
        return (min(xs), min(ys), min(zs)), (max(xs), max(ys), max(zs))


def _read_accessor(gltf: Dict[str, Any], blob: bytes, index: int) -> List[tuple]:
    accessor = gltf["accessors"][index]
    count = accessor["count"]
    components = TYPE_COUNTS[accessor["type"]]
    component_type = accessor["componentType"]
    component_size = COMPONENT_SIZES[component_type]
    normalized = bool(accessor.get("normalized"))
    view = gltf["bufferViews"][accessor["bufferView"]]
    base = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride") or components * component_size

    fmt = {COMPONENT_FLOAT: "f", COMPONENT_USHORT: "H", COMPONENT_UBYTE: "B", COMPONENT_UINT: "I"}[component_type]
    out: List[tuple] = []
    for element in range(count):
        offset = base + element * stride
        raw = struct.unpack_from("<" + fmt * components, blob, offset)
        if component_type == COMPONENT_FLOAT:
            out.append(tuple(float(value) for value in raw))
        elif normalized and component_type == COMPONENT_UBYTE:
            out.append(tuple(value / 255.0 for value in raw))
        elif normalized and component_type == COMPONENT_USHORT:
            out.append(tuple(value / 65535.0 for value in raw))
        else:
            out.append(tuple(float(value) for value in raw))
    return out


def read_glb(data: bytes) -> ReadMesh:
    """Parses a GLB and returns expanded mesh data (raises :class:`GltfError`)."""
    if len(data) < 12:
        raise GltfError("file is too small to be a GLB")
    magic, version, total = struct.unpack_from("<III", data, 0)
    if magic != GLB_MAGIC:
        raise GltfError("not a GLB file (bad magic)")
    if version != 2:
        raise GltfError(f"unsupported glTF container version {version}")
    if total > len(data):
        raise GltfError("GLB header length exceeds the file size")

    offset = 12
    json_chunk: Optional[bytes] = None
    blob = b""
    while offset + 8 <= total:
        length, kind = struct.unpack_from("<II", data, offset)
        payload = data[offset + 8 : offset + 8 + length]
        if len(payload) != length:
            raise GltfError("GLB chunk is truncated")
        if kind == CHUNK_JSON:
            json_chunk = payload
        elif kind == CHUNK_BIN:
            blob = payload
        offset += 8 + length
    if json_chunk is None:
        raise GltfError("GLB has no JSON chunk")

    gltf = json.loads(json_chunk.decode("utf-8"))
    mesh = ReadMesh()
    mesh.json = gltf
    mesh.extensions_used = list(gltf.get("extensionsUsed", []))

    # A skinned or animated model is a supported placed asset: the static prop
    # path bakes the bind pose and the runtime re-poses the placement through
    # the character path (see docs/MAP_AUTHORING_GUIDE.md, "Props and Models").
    # Its skin and clips are validated by the Rust loader, not duplicated here;
    # this reader only expands the primitive mesh data the budgets are about.
    meshes = gltf.get("meshes", [])
    if len(meshes) != 1:
        raise GltfError(f"expected exactly one mesh, found {len(meshes)}")
    primitives = meshes[0].get("primitives", [])
    if not primitives:
        raise GltfError("mesh has no primitives")

    for primitive in primitives:
        mode = primitive.get("mode", 4)
        mesh.modes.append(mode)
        if mode != 4:
            raise GltfError(f"primitive mode {mode} is not TRIANGLES (4)")
        attributes = primitive.get("attributes", {})
        if "POSITION" not in attributes:
            raise GltfError("primitive has no POSITION attribute")
        if "TEXCOORD_0" not in attributes:
            raise GltfError("primitive has no TEXCOORD_0 attribute (props must be UV mapped)")
        positions = _read_accessor(gltf, blob, attributes["POSITION"])
        uvs = _read_accessor(gltf, blob, attributes["TEXCOORD_0"])
        if "COLOR_0" in attributes:
            colors = _read_accessor(gltf, blob, attributes["COLOR_0"])
            if len(colors[0]) == 3:
                colors = [tuple(list(color) + [1.0]) for color in colors]
        else:
            colors = [(1.0, 1.0, 1.0, 1.0)] * len(positions)
        # glTF 2.0 requires every attribute accessor of a primitive to have the
        # same count as POSITION; the runtime rejects a primitive that breaks
        # it, so the tool validator must reject it too (a merged/upgraded model
        # can otherwise leave a stale COLOR_0 accessor behind unnoticed).
        if len(uvs) != len(positions):
            raise GltfError(
                f"POSITION has {len(positions)} vertices but TEXCOORD_0 has {len(uvs)}"
            )
        if len(colors) != len(positions):
            raise GltfError(
                f"POSITION has {len(positions)} vertices but COLOR_0 has {len(colors)}"
            )
        indices = [int(value[0]) for value in _read_accessor(gltf, blob, primitive["indices"])] if "indices" in primitive else list(range(len(positions)))
        if indices and (min(indices) < 0 or max(indices) >= len(positions)):
            raise GltfError(
                f"index {max(indices)} lies outside the primitive's {len(positions)} vertices"
            )

        base = len(mesh.positions)
        mesh.positions.extend(positions)
        mesh.uvs.extend(uvs)
        mesh.colors.extend(colors)
        mesh.indices.extend(base + index for index in indices)

    materials = gltf.get("materials", [])
    mesh.material_count = len(materials)
    mesh.material_names = [material.get("name", "") for material in materials]

    textures = gltf.get("textures", [])
    images = gltf.get("images", [])
    if textures:
        source = textures[0].get("source")
        if source is None:
            raise GltfError("texture has no image source")
        image = images[source]
        if "uri" in image:
            uri = image["uri"]
            if uri.startswith("data:"):
                mesh.texture_png = base64.b64decode(uri.split(",", 1)[1])
            else:
                raise GltfError("external image files are not allowed; props must embed their PNG")
        else:
            view = gltf["bufferViews"][image["bufferView"]]
            start = view.get("byteOffset", 0)
            mesh.texture_png = blob[start : start + view["byteLength"]]
    return mesh
