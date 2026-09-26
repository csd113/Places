"""Minimal, dependency-free GLB (binary glTF 2.0) reader/writer for the prop pack.

The default writer path emits the single, deliberately narrow shape every
existing prop uses and the validator enforces:

* one scene, one node, one mesh, one primitive, one material, one texture;
* an embedded PNG image stored in a bufferView (no external files);
* ``POSITION`` (float32 vec3), ``TEXCOORD_0`` (float32 vec2),
  ``COLOR_0`` (normalized uint8 vec4) and 16-bit triangle indices;
* a CLAMP_TO_EDGE sampler with mipmapped linear filtering;
* no glTF extensions and no morph targets.

A model may opt into the extended path (see :func:`write_glb`) for the pieces
the pack has since needed: one glTF primitive per contiguous material group,
several materials over one atlas, ``emissiveFactor`` with the
``KHR_materials_emissive_strength`` extension, node hierarchies, named meshes
and LINEAR/STEP animation clips. An extended document stamps
``asset.extras.places_props_toolkit = 1`` so ``build.py`` can tell a
toolkit-authored animated model (the wall switch) from a hand-authored skinned
entity. Skins and clips are read through to the mesh data unchanged in the
default path: the runtime poses a placed skinned or animated model through its
character path, so the validator must accept the same container the loader
does (the Rust loader's shipped-asset test is the authority on the
skin/animation structure itself).

The reader exists so tools (the preview renderer, tests) can round-trip the
same files without pulling in a third-party glTF library. It merges every
scene node's meshes at their rest transforms, so a multi-mesh model reads back
as the flat pose the previews and budgets are about.
"""

from __future__ import annotations

import base64
import json
import math
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


def _write_single_glb(mesh, texture_png: bytes, name: str = "prop") -> bytes:
    """The legacy single-node/single-primitive/single-material writer.

    Kept verbatim as the default path of :func:`write_glb` so every existing
    builder keeps byte-identical output.
    """
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


def write_glb(
    mesh,
    texture_png: bytes,
    name: str = "prop",
    materials=None,
    submeshes=None,
    nodes=None,
    animations=None,
) -> bytes:
    """Packs a :class:`mesh.Mesh` plus one PNG into a self-contained GLB.

    With no materials, submeshes, nodes or animations this is exactly
    :func:`_write_single_glb`: one node, one mesh, one primitive, one material,
    byte-identical for every existing builder. Supplying any of them emits the
    extended document the newer props need:

    * materials: material-slot dicts (see :meth:`mesh.Mesh.material`); every
      contiguous same-slot run of triangles becomes one glTF primitive, so a
      model can carry a luminous face beside its housing;
    * submeshes: optional ``{"name", "first_index"}`` markers splitting the
      index buffer into named glTF meshes a node can reference;
    * nodes: ``{"name", "mesh", "translation"/"rotation"/"scale"/"matrix",
      "children"}`` dicts; the scene roots are the nodes no other node lists;
    * animations: ``{"name", "channels": [{"node", "path", "times", "values",
      "interpolation"}]}`` clips; LINEAR/STEP samplers are packed as float32
      accessors.

    An extended document carries ``asset.extras.places_props_toolkit = 1`` so
    ``build.py`` can tell a toolkit-authored animated model from a
    hand-authored skinned one.
    """
    material_list = list(materials) if materials is not None else list(getattr(mesh, "materials", ()) or ())
    submesh_list = list(submeshes) if submeshes is not None else list(getattr(mesh, "submeshes", ()) or ())
    node_list = list(nodes or ())
    animation_list = list(animations or ())
    if not material_list and not submesh_list and not node_list and not animation_list:
        return _write_single_glb(mesh, texture_png, name)
    return _write_extended_glb(mesh, texture_png, name, material_list, submesh_list, node_list, animation_list)


def _write_extended_glb(mesh, texture_png: bytes, name: str, materials: List[dict],
                        submeshes: List[dict], nodes: List[dict], animations: List[dict]) -> bytes:
    """Multi-primitive/multi-material/nodes/animations writer (see write_glb)."""
    if mesh.vertex_count > 65535:
        raise GltfError(f"mesh has {mesh.vertex_count} vertices; the pack uses 16-bit indices")
    if not texture_png.startswith(b"\x89PNG"):
        raise GltfError("prop textures must be PNG")
    if not mesh.indices or len(mesh.indices) % 3 != 0:
        raise GltfError("mesh must carry a whole number of triangles")
    for u, v in mesh.uvs:
        if u < 0.0 or u > 1.0 or v < 0.0 or v > 1.0:
            raise GltfError(f"UV {u:.3f},{v:.3f} is outside 0..1; props use non-tiling UVs")

    # Named submeshes split the index buffer; without markers the whole prop is
    # one mesh. Boundaries must ascend and fall on triangle edges.
    marks: List[dict] = []
    previous = -1
    for marker in submeshes:
        start = int(marker.get("first_index", 0))
        if start <= previous or start > len(mesh.indices) or start % 3 != 0:
            raise GltfError("submesh markers must ascend on triangle boundaries")
        previous = start
        marks.append({"name": marker.get("name"), "first_index": start})
    ranges: List[tuple] = []
    for index, mark in enumerate(marks):
        end = marks[index + 1]["first_index"] if index + 1 < len(marks) else len(mesh.indices)
        if end > mark["first_index"]:
            ranges.append((mark["name"], mark["first_index"], end))
    if not ranges:
        ranges = [(None, 0, len(mesh.indices))]

    mesh_entries: List[dict] = []
    for index, (range_name, start, end) in enumerate(ranges):
        entry_name = range_name or (name if len(ranges) == 1 else f"{name}_{index}")
        mesh_entries.append({"name": entry_name, "groups": mesh.groups_for_range(start, end)})
    mesh_names = {entry["name"]: index for index, entry in enumerate(mesh_entries)}
    if len(mesh_names) != len(mesh_entries):
        raise GltfError("submesh names must be unique")

    # One implicit opaque material keeps a tagged-but-unregistered model valid.
    if not materials:
        materials = [{"name": f"{name}_mat", "emissive": None, "strength": 1.0, "color": None}]
    for entry in mesh_entries:
        for group in entry["groups"]:
            if group["material"] >= len(materials):
                raise GltfError(f"primitive uses material slot {group['material']} but only "
                                f"{len(materials)} material(s) are registered")

    # Nodes: strings name a submesh, integers index one directly.
    normalized: List[dict] = []
    for node in nodes:
        out: Dict[str, Any] = {"name": str(node.get("name", ""))}
        reference = node.get("mesh")
        if reference is not None:
            if isinstance(reference, str):
                if reference not in mesh_names:
                    raise GltfError(f"node {out['name']!r} references unknown mesh {reference!r}")
                out["mesh"] = mesh_names[reference]
            else:
                mesh_index = int(reference)
                if mesh_index < 0 or mesh_index >= len(mesh_entries):
                    raise GltfError(f"node {out['name']!r} references mesh {mesh_index} outside the model")
                out["mesh"] = mesh_index
        if node.get("matrix") is not None:
            matrix = [float(value) for value in node["matrix"]]
            if len(matrix) != 16 or not all(math.isfinite(value) for value in matrix):
                raise GltfError(f"node {out['name']!r} matrix must have 16 finite elements")
            out["matrix"] = matrix
        else:
            for key in ("translation", "rotation", "scale"):
                if node.get(key) is not None:
                    values = [float(value) for value in node[key]]
                    if not all(math.isfinite(value) for value in values):
                        raise GltfError(f"node {out['name']!r} {key} must be finite")
                    out[key] = values
        if node.get("children") is not None:
            out["children"] = [int(index) for index in node["children"]]
        normalized.append(out)
    if not normalized:
        normalized = [{"name": entry["name"], "mesh": index} for index, entry in enumerate(mesh_entries)]
    for node in normalized:
        for child in node.get("children", ()):
            if child < 0 or child >= len(normalized):
                raise GltfError(f"node {node['name']!r} references node {child} outside the model")
    referenced = {child for node in normalized for child in node.get("children", ())}
    roots = [index for index in range(len(normalized)) if index not in referenced]
    if not roots:
        raise GltfError("node hierarchy has no root node (cycle?)")

    # Binary layout: mesh data first (the legacy order), then animation data.
    # Every primitive carries its own compacted vertex arrays (only the
    # vertices its indices reference, in first-use order), so a vertex the
    # model does not draw is never shipped and the runtime's per-primitive
    # vertex expansion stays a sharing-preserving subset of the flat list.
    blob = bytearray()
    views: List[Dict[str, Any]] = []

    def add_view(payload: bytes, target: Optional[int]) -> int:
        while len(blob) % 4 != 0:
            blob.append(0)
        offset = len(blob)
        blob.extend(payload)
        view: Dict[str, Any] = {"buffer": 0, "byteOffset": offset, "byteLength": len(payload)}
        if target is not None:
            view["target"] = target
        views.append(view)
        return len(views) - 1

    image_view = add_view(texture_png, None)
    accessors: List[Dict[str, Any]] = []

    meshes_json: List[Dict[str, Any]] = []
    for entry in mesh_entries:
        primitives = []
        for group in entry["groups"]:
            run_start = group["first_index"]
            run_end = run_start + group["index_count"]
            remap: Dict[int, int] = {}
            local_indices: List[int] = []
            for index in mesh.indices[run_start:run_end]:
                slot = remap.get(index)
                if slot is None:
                    slot = len(remap)
                    remap[index] = slot
                local_indices.append(slot)
            order = list(remap.keys())
            if not order:
                continue
            index_view = add_view(
                b"".join(struct.pack("<H", index) for index in local_indices), 34963
            )
            position_view = add_view(
                b"".join(struct.pack("<3f", *mesh.positions[index]) for index in order), 34962
            )
            uv_view = add_view(
                b"".join(struct.pack("<2f", *mesh.uvs[index]) for index in order), 34962
            )
            color_view = add_view(
                b"".join(
                    struct.pack("<4B", mesh.colors[index][0], mesh.colors[index][1],
                                mesh.colors[index][2], 255)
                    for index in order
                ),
                34962,
            )
            local_low = [min(mesh.positions[index][axis] for index in order) for axis in range(3)]
            local_high = [max(mesh.positions[index][axis] for index in order) for axis in range(3)]
            accessors.append(
                {
                    "bufferView": index_view,
                    "componentType": COMPONENT_USHORT,
                    "count": len(local_indices),
                    "type": "SCALAR",
                }
            )
            accessors.append(
                {
                    "bufferView": position_view,
                    "componentType": COMPONENT_FLOAT,
                    "count": len(order),
                    "type": "VEC3",
                    "min": [round(value, 5) for value in local_low],
                    "max": [round(value, 5) for value in local_high],
                }
            )
            accessors.append(
                {
                    "bufferView": uv_view,
                    "componentType": COMPONENT_FLOAT,
                    "count": len(order),
                    "type": "VEC2",
                }
            )
            accessors.append(
                {
                    "bufferView": color_view,
                    "componentType": COMPONENT_UBYTE,
                    "normalized": True,
                    "count": len(order),
                    "type": "VEC4",
                }
            )
            primitive_index = len(accessors) - 4
            primitives.append(
                {
                    "attributes": {
                        "POSITION": primitive_index + 1,
                        "TEXCOORD_0": primitive_index + 2,
                        "COLOR_0": primitive_index + 3,
                    },
                    "indices": primitive_index,
                    "material": group["material"],
                    "mode": 4,
                }
            )
        meshes_json.append({"name": entry["name"], "primitives": primitives})

    animations_json: List[Dict[str, Any]] = []
    arity = {"translation": 3, "rotation": 4, "scale": 3}
    for clip in animations:
        clip_samplers: List[Dict[str, Any]] = []
        channels_json = []
        for channel in clip.get("channels", ()):
            node_index = int(channel["node"])
            if node_index < 0 or node_index >= len(normalized):
                raise GltfError(f"animation {clip.get('name', '')!r} references node {node_index} outside the model")
            path = str(channel.get("path", ""))
            if path not in arity:
                raise GltfError(f"animation {clip.get('name', '')!r} path {path!r} is not a TRS channel")
            times = [float(value) for value in channel["times"]]
            values = [[float(component) for component in value] for value in channel["values"]]
            if not times or len(times) != len(values):
                raise GltfError(f"animation {clip.get('name', '')!r} channel key counts do not match")
            for index in range(len(times)):
                if not math.isfinite(times[index]):
                    raise GltfError(f"animation {clip.get('name', '')!r} key times must be finite")
                if index and times[index] <= times[index - 1]:
                    raise GltfError(f"animation {clip.get('name', '')!r} key times must strictly increase")
                if len(values[index]) != arity[path]:
                    raise GltfError(f"animation {clip.get('name', '')!r} {path} keys must have {arity[path]} values")
                if not all(math.isfinite(component) for component in values[index]):
                    raise GltfError(f"animation {clip.get('name', '')!r} key values must be finite")
            input_view = add_view(b"".join(struct.pack("<f", value) for value in times), None)
            output_view = add_view(b"".join(struct.pack("<f", component) for value in values for component in value), None)
            accessors.append(
                {
                    "bufferView": input_view,
                    "componentType": COMPONENT_FLOAT,
                    "count": len(times),
                    "type": "SCALAR",
                    "min": [min(times)],
                    "max": [max(times)],
                }
            )
            accessors.append(
                {
                    "bufferView": output_view,
                    "componentType": COMPONENT_FLOAT,
                    "count": len(values),
                    "type": "VEC3" if arity[path] == 3 else "VEC4",
                }
            )
            interpolation = str(channel.get("interpolation", "LINEAR")).upper()
            if interpolation not in ("LINEAR", "STEP"):
                raise GltfError(f"animation interpolation {interpolation!r} is not LINEAR or STEP")
            clip_samplers.append(
                {"input": len(accessors) - 2, "output": len(accessors) - 1, "interpolation": interpolation}
            )
            channels_json.append({"sampler": len(clip_samplers) - 1, "target": {"node": node_index, "path": path}})
        animations_json.append(
            {"name": str(clip.get("name", "")), "samplers": clip_samplers, "channels": channels_json}
        )

    materials_json: List[Dict[str, Any]] = []
    uses_emissive_strength = False
    for index, entry in enumerate(materials):
        pbr: Dict[str, Any] = {
            "baseColorTexture": {"index": 0},
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0,
        }
        if entry.get("color") is not None:
            pbr["baseColorFactor"] = [round(float(channel) / 255.0, 6) for channel in entry["color"]] + [1.0]
        material: Dict[str, Any] = {
            "name": str(entry.get("name") or f"{name}_mat_{index}"),
            "pbrMetallicRoughness": pbr,
        }
        emissive = entry.get("emissive")
        if emissive is not None:
            material["emissiveFactor"] = [round(float(component), 6) for component in emissive]
        strength = float(entry.get("strength", 1.0))
        if strength != 1.0:
            uses_emissive_strength = True
            material["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": round(strength, 6)}}
        material["doubleSided"] = True
        materials_json.append(material)

    gltf: Dict[str, Any] = {
        "asset": {
            "version": "2.0",
            "generator": "places props toolkit (tools/props)",
            "extras": {"places_props_toolkit": 1},
        },
    }
    if uses_emissive_strength:
        gltf["extensionsUsed"] = ["KHR_materials_emissive_strength"]
    gltf.update(
        {
            "scene": 0,
            "scenes": [{"name": name, "nodes": roots}],
            "nodes": normalized,
            "meshes": meshes_json,
            "accessors": accessors,
            "bufferViews": views,
            "buffers": [{"byteLength": len(blob)}],
            "images": [{"bufferView": image_view, "mimeType": "image/png", "name": f"{name}_diffuse"}],
            "samplers": [
                {
                    "magFilter": 9729,
                    "minFilter": 9987,
                    "wrapS": 33071,
                    "wrapT": 33071,
                }
            ],
            "textures": [{"sampler": 0, "source": 0}],
            "materials": materials_json,
        }
    )
    if animations_json:
        gltf["animations"] = animations_json

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
    """Mesh data as read from a GLB, already expanded to interleaved vertices.

    Every scene node's meshes are merged at their rest transforms, so a model
    that hangs its rocker off a child node reads back as the flat rest pose the
    preview and the budget checks are about. The first texture is kept for the
    preview; names stay available for diagnostics and tests.
    """

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
        # Identity metadata: node, mesh and clip names in asset order.
        self.node_names: List[str] = []
        self.mesh_names: List[str] = []
        self.animation_names: List[str] = []
        # Names of the nodes whose meshes were merged, in merge order.
        self.used_nodes: List[str] = []

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


_IDENTITY_MATRIX = (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)


def _mat_mul(a, b):
    """Column-major 4x4 product ``a * b`` (glTF matrix convention)."""
    out: List[float] = []
    for column in range(4):
        for row in range(4):
            total = 0.0
            for k in range(4):
                total += a[k * 4 + row] * b[column * 4 + k]
            out.append(total)
    return tuple(out)


def _trs_matrix(node: Dict[str, Any]):
    """Column-major matrix for a node's ``matrix`` or T*R*S transform."""
    if node.get("matrix") is not None:
        matrix = [float(value) for value in node["matrix"]]
        if len(matrix) != 16:
            raise GltfError("node matrix must have 16 elements")
        return tuple(matrix)
    translation = [float(value) for value in node.get("translation", (0.0, 0.0, 0.0))]
    rotation = [float(value) for value in node.get("rotation", (0.0, 0.0, 0.0, 1.0))]
    scale = [float(value) for value in node.get("scale", (1.0, 1.0, 1.0))]
    x, y, z, w = rotation
    columns = (
        (1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w), 0.0),
        (2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w), 0.0),
        (2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y), 0.0),
        (translation[0], translation[1], translation[2], 1.0),
    )
    return tuple(
        component * (scale[column] if column < 3 and row < 3 else 1.0)
        for column, column_values in enumerate(columns)
        for row, component in enumerate(column_values)
    )


def _transform_point(matrix, point):
    x, y, z = point
    return (
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    )


def _node_rest_matrices(gltf: Dict[str, Any]):
    """Returns ``(world_matrices, scene_roots)`` for the document's nodes."""
    nodes = gltf["nodes"]
    parents: Dict[int, int] = {}
    for index, node in enumerate(nodes):
        for child in node.get("children", ()):
            child = int(child)
            if child < 0 or child >= len(nodes):
                raise GltfError(f"node {index} lists child {child} outside the model")
            if child in parents:
                raise GltfError(f"node {child} is a child of more than one node")
            parents[child] = index

    world: List[Optional[tuple]] = [None] * len(nodes)
    visiting: set[int] = set()

    def resolve(index: int):
        if world[index] is not None:
            return world[index]
        if index in visiting:
            raise GltfError("node hierarchy has a cycle")
        visiting.add(index)
        local = _trs_matrix(nodes[index])
        parent = parents.get(index)
        world[index] = local if parent is None else _mat_mul(resolve(parent), local)
        visiting.discard(index)
        return world[index]

    for index in range(len(nodes)):
        resolve(index)

    scenes = gltf.get("scenes") or []
    roots: Optional[List[int]] = None
    if scenes:
        scene_index = int(gltf.get("scene", 0))
        if scene_index < 0 or scene_index >= len(scenes):
            scene_index = 0
        declared = scenes[scene_index].get("nodes")
        if declared:
            roots = [int(index) for index in declared]
    if roots is None:
        roots = [index for index in range(len(nodes)) if index not in parents]
    return world, roots


def read_glb(data: bytes) -> ReadMesh:
    """Parses a GLB and returns expanded mesh data (raises :class:`GltfError`).

    Every scene node's meshes are merged at their rest transforms; a skinned or
    animated model reads back its static bind pose, which is what the toolkit's
    previews and budgets are about. The runtime validates skins and clips.
    """
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
    # this reader only expands the primitive mesh data the budgets are about,
    # merged at every mesh node's rest transform.
    mesh.node_names = [node.get("name", "") for node in gltf.get("nodes", ())]
    mesh.mesh_names = [document_mesh.get("name", "") for document_mesh in gltf.get("meshes", ())]
    mesh.animation_names = [clip.get("name", "") for clip in gltf.get("animations", ())]

    meshes = gltf.get("meshes", [])
    if not meshes:
        raise GltfError("file has no meshes")
    nodes = gltf.get("nodes") or []
    if nodes:
        world, roots = _node_rest_matrices(gltf)
        placements: List[tuple] = []
        visited: set[int] = set()

        def walk(index: int) -> None:
            if index in visited:
                return
            visited.add(index)
            node = nodes[index]
            reference = node.get("mesh")
            if reference is not None:
                mesh_index = int(reference)
                if mesh_index < 0 or mesh_index >= len(meshes):
                    raise GltfError(f"node {index} references mesh {mesh_index} outside the model")
                placements.append((mesh_index, world[index], node.get("name", "")))
            for child in node.get("children", ()):
                walk(int(child))

        for root in roots:
            walk(root)
    else:
        placements = [(index, _IDENTITY_MATRIX, "") for index in range(len(meshes))]
    if not placements:
        raise GltfError("no scene node carries a mesh")

    for mesh_index, matrix, node_name in placements:
        primitives = meshes[mesh_index].get("primitives", [])
        if not primitives:
            raise GltfError(f"mesh {mesh_index} has no primitives")
        mesh.used_nodes.append(node_name)
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
            if matrix != _IDENTITY_MATRIX:
                positions = [_transform_point(matrix, position) for position in positions]
            uvs = _read_accessor(gltf, blob, attributes["TEXCOORD_0"])
            if "COLOR_0" in attributes:
                colors = _read_accessor(gltf, blob, attributes["COLOR_0"])
                if len(colors[0]) == 3:
                    colors = [tuple(list(color) + [1.0]) for color in colors]
            else:
                colors = [(1.0, 1.0, 1.0, 1.0)] * len(positions)
            # glTF 2.0 requires every attribute accessor of a primitive to have
            # the same count as POSITION; the runtime rejects a primitive that
            # breaks it, so the tool validator must reject it too (a
            # merged/upgraded model can otherwise leave a stale COLOR_0
            # accessor behind unnoticed).
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

            # Compact the primitive to the vertices it actually draws. A mesh
            # may host several primitives over one shared accessor (our own
            # multi-material models do), so merging raw attribute arrays would
            # drag another node's vertices through this node's transform.
            base = len(mesh.positions)
            remap: Dict[int, int] = {}
            for index in indices:
                slot = remap.get(index)
                if slot is None:
                    slot = len(mesh.positions) - base
                    remap[index] = slot
                    mesh.positions.append(positions[index])
                    mesh.uvs.append(uvs[index])
                    mesh.colors.append(colors[index])
                mesh.indices.append(base + slot)

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
