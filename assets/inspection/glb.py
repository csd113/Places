"""Read-only inspection of GLB containers, accessors, and static rest meshes."""
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


def read_glb_chunks(data: bytes) -> tuple[dict, bytes]:
    """Read the container without expanding or writing model geometry."""
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
    return gltf, blob


class ReadMesh:
    """Mesh data as read from a GLB, already expanded to interleaved vertices.

    Every scene node's meshes are merged at their rest transforms, so a model
    that hangs its rocker off a child node reads back as the flat rest pose the
    preview and the budget checks are about. All texture slots and triangle
    material assignments are retained for inspection; the legacy first-texture
    field stays available to existing budget readers.
    """

    def __init__(self) -> None:
        self.positions: List[tuple] = []
        self.uvs: List[tuple] = []
        self.colors: List[tuple] = []
        self.indices: List[int] = []
        self.texture_png: bytes = b""
        self.texture_pngs: List[bytes] = []
        self.triangle_materials: List[Optional[int]] = []
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


def _require_non_negative_int(value: Any, what: str) -> int:
    """A glTF byte field: an integer >= 0, never a bool or a float."""
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise GltfError(f"{what} must be a non-negative integer, got {value!r}")
    return value


def _read_accessor(gltf: Dict[str, Any], blob: bytes, index: int) -> List[tuple]:
    accessor = gltf["accessors"][index]
    count = _require_non_negative_int(accessor["count"], f"accessor {index} count")
    if count == 0:
        raise GltfError(
            f"accessor {index} declares zero elements; an accessor must hold at least one"
        )
    components = TYPE_COUNTS[accessor["type"]]
    component_type = accessor["componentType"]
    component_size = COMPONENT_SIZES[component_type]
    element_size = components * component_size
    normalized = bool(accessor.get("normalized"))
    view = gltf["bufferViews"][accessor["bufferView"]]
    view_offset = _require_non_negative_int(
        view.get("byteOffset", 0), f"accessor {index} bufferView byteOffset"
    )
    view_length = _require_non_negative_int(
        view["byteLength"], f"accessor {index} bufferView byteLength"
    )
    # A bufferView's byteLength is relative to its own byteOffset: the view is
    # blob[view_offset : view_offset + view_length], and the binary chunk
    # continuing past it does not make a crossing accessor valid.
    view_end = view_offset + view_length
    if view_end > len(blob):
        raise GltfError(
            f"accessor {index} bufferView [{view_offset}, {view_end}) extends past the "
            f"{len(blob)}-byte binary chunk; the GLB is truncated"
        )
    accessor_offset = _require_non_negative_int(
        accessor.get("byteOffset", 0), f"accessor {index} byteOffset"
    )
    declared_stride = view.get("byteStride")
    if declared_stride is None:
        stride = element_size
    else:
        stride = _require_non_negative_int(
            declared_stride, f"accessor {index} bufferView byteStride"
        )
        if count > 1:
            # A zero stride would read the same bytes for every element; glTF
            # requires at least the element size (and a multiple of 4) when
            # byteStride is authored at all.
            if stride < element_size:
                raise GltfError(
                    f"accessor {index} declares byteStride {stride} smaller than its "
                    f"{element_size}-byte elements"
                )
            if stride % 4 != 0:
                raise GltfError(
                    f"accessor {index} declares byteStride {stride}; glTF requires a multiple of 4"
                )
    # The accessor's byteOffset is relative to the view: every element it
    # addresses has to fit inside the declared view.
    relative_end = accessor_offset + (count - 1) * stride + element_size
    if relative_end > view_length:
        raise GltfError(
            f"accessor {index} declares {count} elements but its bufferView is too small "
            f"({relative_end} bytes needed, {view_length} declared)"
        )
    base = view_offset + accessor_offset

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
    gltf, blob = read_glb_chunks(data)
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
            mesh.triangle_materials.extend([primitive.get("material")] * (len(indices) // 3))

    materials = gltf.get("materials", [])
    mesh.material_count = len(materials)
    mesh.material_names = [material.get("name", "") for material in materials]

    textures = gltf.get("textures", [])
    images = gltf.get("images", [])
    for texture in textures:
        source = texture.get("source")
        if source is None:
            raise GltfError("texture has no image source")
        image = images[source]
        if "uri" in image:
            uri = image["uri"]
            if uri.startswith("data:"):
                payload = base64.b64decode(uri.split(",", 1)[1])
            else:
                raise GltfError("external image files are not allowed; props must embed their PNG")
        else:
            view = gltf["bufferViews"][image["bufferView"]]
            start = view.get("byteOffset", 0)
            payload = blob[start : start + view["byteLength"]]
        mesh.texture_pngs.append(payload)
    if mesh.texture_pngs:
        mesh.texture_png = mesh.texture_pngs[0]
    return mesh
