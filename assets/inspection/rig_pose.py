"""Read-only skeletal pose sampling for checked-in GLB regression cases."""
from __future__ import annotations

import math
import struct
from pathlib import Path
from typing import Dict, List, Sequence, Tuple

from .glb import read_glb_chunks


def quat_identity() -> Tuple[float, float, float, float]:
    return IDENTITY_QUAT


def quat_normalize(quat: Tuple[float, float, float, float]) -> Tuple[float, float, float, float]:
    length = math.sqrt(sum(component * component for component in quat))
    if length <= 1.0e-12:
        return quat_identity()
    return tuple(component / length for component in quat)


def _mat_identity() -> List[float]:
    return [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]


def _mat_mul(a: Sequence[float], b: Sequence[float]) -> List[float]:
    """Column-major 4x4 product ``a * b``."""
    out = [0.0] * 16
    for column in range(4):
        for row in range(4):
            total = 0.0
            for k in range(4):
                total += a[k * 4 + row] * b[column * 4 + k]
            out[column * 4 + row] = total
    return out


def _mat_trs(
    translation: Sequence[float],
    rotation: Sequence[float],
    scale: Sequence[float],
) -> List[float]:
    x, y, z, w = rotation
    xx, yy, zz = x * x, y * y, z * z
    xy, xz, yz = x * y, x * z, y * z
    wx, wy, wz = w * x, w * y, w * z
    sx, sy, sz = scale
    return [
        (1 - 2 * (yy + zz)) * sx,
        (2 * (xy + wz)) * sx,
        (2 * (xz - wy)) * sx,
        0.0,
        (2 * (xy - wz)) * sy,
        (1 - 2 * (xx + zz)) * sy,
        (2 * (yz + wx)) * sy,
        0.0,
        (2 * (xz + wy)) * sz,
        (2 * (yz - wx)) * sz,
        (1 - 2 * (xx + yy)) * sz,
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]


_COMPONENT = {
    5120: ("b", 1),
    5121: ("B", 1),
    5122: ("h", 2),
    5123: ("H", 2),
    5125: ("I", 4),
    5126: ("f", 4),
}
_TYPE_COUNT = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


class Model:
    """The decoded inputs the sweep needs, straight from one GLB."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.document, self.binary = read_glb_chunks(path.read_bytes())
        self.nodes = self.document.get("nodes", [])
        self.accessors = self.document.get("accessors", [])
        self.skins = self.document.get("skins", [])
        self.animations = self.document.get("animations", [])
        self.rest_translation = [list(node.get("translation", [0.0, 0.0, 0.0])) for node in self.nodes]
        self.rest_rotation = [list(node.get("rotation", [0.0, 0.0, 0.0, 1.0])) for node in self.nodes]
        self.rest_scale = [list(node.get("scale", [1.0, 1.0, 1.0])) for node in self.nodes]
        self.parent = [None] * len(self.nodes)
        for index, node in enumerate(self.nodes):
            for child in node.get("children", []):
                self.parent[child] = index
        skin = self.skins[0]
        self.joints = list(skin["joints"])
        self.inverse_bind = self._read_accessor(skin["inverseBindMatrices"], "MAT4")
        self.parent_of_joint = [self.parent[node] for node in self.joints]
        self.order = self._topological_order()
        # Include every skinned primitive: Spoonerman separates body and paws.
        self.positions = []
        self.vertex_joints = []
        self.vertex_weights = []
        self.indices = []
        for mesh in self.document["meshes"]:
            for primitive in mesh["primitives"]:
                attributes = primitive["attributes"]
                if "JOINTS_0" not in attributes:
                    continue
                base = len(self.positions)
                self.positions.extend(self._read_accessor(attributes["POSITION"], "VEC3"))
                self.vertex_joints.extend(self._read_accessor(attributes["JOINTS_0"], "VEC4"))
                self.vertex_weights.extend(self._read_accessor(attributes["WEIGHTS_0"], "VEC4"))
                flat = self._read_accessor(primitive["indices"], "SCALAR")
                self.indices.extend(base + int(value[0]) for value in flat)
        # Sample the bind pose once: the reference for displacement/stretch.
        identity = self._identity_globals()
        self.bind_skin = self._skin(identity)
        self.bind_edges = self._edge_lengths(self.bind_skin)
        self.marker = (
            self.document.get("asset", {}).get("extras", {}).get("places_entity_clips", {})
        )
        self.clips = self._read_clips()

    # -- accessors ------------------------------------------------------

    def _read_accessor(self, index: int, expected: str):
        accessor = self.accessors[index]
        kind = accessor["type"]
        if kind != expected:
            raise SystemExit(f"{self.path}: accessor {index} is {kind}, expected {expected}")
        component = accessor["componentType"]
        if component not in _COMPONENT:
            raise SystemExit(f"{self.path}: accessor {index} has componentType {component}")
        fmt, size = _COMPONENT[component]
        count = accessor["count"]
        components = _TYPE_COUNT[kind]
        view = self.document["bufferViews"][accessor["bufferView"]]
        offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        stride = view.get("byteStride") or (size * components)
        normalized = bool(accessor.get("normalized"))
        scale = 255.0 if normalized and component == 5121 else 1.0
        out = []
        for element in range(count):
            base = offset + element * stride
            row = []
            for component_index in range(components):
                (raw,) = struct.unpack_from(
                    "<" + fmt, self.binary, base + component_index * size
                )
                row.append(raw / scale if scale != 1.0 else raw)
            out.append(row)
        return out

    def _identity_globals(self) -> List[List[float]]:
        globals_: List[List[float]] = [_mat_identity() for _ in self.nodes]
        for index in self._topological_order():
            local = _mat_trs(
                self.rest_translation[index], self.rest_rotation[index], self.rest_scale[index]
            )
            parent = self.parent[index]
            globals_[index] = (
                _mat_mul(globals_[parent], local)
                if parent is not None
                else local
            )
        return globals_

    def _topological_order(self) -> List[int]:
        order = []
        emitted = [False] * len(self.nodes)
        while len(order) < len(self.nodes):
            progressed = False
            for index in range(len(self.nodes)):
                if emitted[index]:
                    continue
                parent = self.parent[index]
                if parent is None or emitted[parent]:
                    order.append(index)
                    emitted[index] = True
                    progressed = True
            if not progressed:
                raise SystemExit(f"{self.path}: node hierarchy has a cycle")
        return order

    # -- clips ----------------------------------------------------------

    def _read_clips(self) -> List[dict]:
        clips = []
        for animation in self.animations:
            channels = []
            duration = 0.0
            for channel in animation["channels"]:
                sampler = animation["samplers"][channel["sampler"]]
                times = [value[0] for value in self._read_accessor(sampler["input"], "SCALAR")]
                target = channel["target"]
                path = target["path"]
                kind = {"rotation": "VEC4", "translation": "VEC3", "scale": "VEC3"}.get(path)
                if kind is None:
                    continue
                values = self._read_accessor(sampler["output"], kind)
                duration = max(duration, times[-1] if times else 0.0)
                channels.append({"node": target["node"], "path": path, "times": times, "values": values,
                                 "interpolation": sampler.get("interpolation", "LINEAR")})
            clips.append({"name": animation.get("name", ""), "duration": duration, "channels": channels})
        # Attach the marker metadata by name.
        by_name = {
            clip.get("name"): clip
            for clip in (self.marker.get("clips", []) if isinstance(self.marker, dict) else [])
        }
        for clip in clips:
            meta = by_name.get(clip["name"], {})
            clip["loop"] = bool(meta.get("loop", True))
            speed = meta.get("reference_speed_mps")
            clip["reference_speed_mps"] = (
                float(speed) if isinstance(speed, (int, float)) and speed > 0 else None
            )
            clip["kind"] = meta.get("kind")
        return clips

    # -- pose evaluation ------------------------------------------------

    def _interpolate(self, keys_times: Sequence[float], values: Sequence[list], time: float, looping: bool,
                     *, rotation=False, interpolation="LINEAR"):
        if not keys_times:
            return values[0] if values else None
        duration = keys_times[-1]
        if duration <= 0.0:
            return values[0]
        local = time % duration if looping else min(max(time, 0.0), duration)
        if local <= keys_times[0]:
            return values[0]
        for index in range(1, len(keys_times)):
            if local <= keys_times[index]:
                span = keys_times[index] - keys_times[index - 1]
                t = 0.0 if span <= 0.0 else (local - keys_times[index - 1]) / span
                if interpolation == "STEP":
                    return values[index] if t == 1.0 else values[index - 1]
                if interpolation != "LINEAR":
                    raise ValueError(f"entity sweep does not implement {interpolation} interpolation")
                if rotation:
                    return _slerp(values[index - 1], values[index], t)
                return _lerp(values[index - 1], values[index], t)
        return values[-1]

    def _pose_globals(self, clip: dict, time: float) -> List[List[float]]:
        channels_by_node: Dict[int, Dict[str, list]] = {}
        for channel in clip["channels"]:
            channels_by_node.setdefault(channel["node"], {})[channel["path"]] = channel
        globals_: List[List[float]] = [_mat_identity() for _ in self.nodes]
        for index in self.order:
            channels = channels_by_node.get(index)
            translation = list(self.rest_translation[index])
            rotation = list(self.rest_rotation[index])
            scale = list(self.rest_scale[index])
            if channels:
                channel = channels.get("translation")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"],
                                              interpolation=channel["interpolation"])
                    translation = list(value) if value else translation
                channel = channels.get("rotation")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"],
                                              rotation=True, interpolation=channel["interpolation"])
                    rotation = list(value) if value else rotation
                channel = channels.get("scale")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"],
                                              interpolation=channel["interpolation"])
                    scale = list(value) if value else scale
            local = _mat_trs(translation, rotation, scale)
            parent = self.parent[index]
            globals_[index] = (
                _mat_mul(globals_[parent], local) if parent is not None else local
            )
        return globals_

    def _skin(self, globals_: List[List[float]]) -> List[Tuple[float, float, float]]:
        matrices = [
            _mat_mul(globals_[joint], self.inverse_bind[slot])
            for slot, joint in enumerate(self.joints)
        ]
        out = []
        for vertex, position in enumerate(self.positions):
            slots = self.vertex_joints[vertex]
            weights = self.vertex_weights[vertex]
            blended = [0.0, 0.0, 0.0]
            total = 0.0
            for slot, weight in zip(slots, weights):
                if weight <= 0.0:
                    continue
                matrix = matrices[int(slot)]
                x, y, z = position
                blended[0] += (matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12]) * weight
                blended[1] += (matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13]) * weight
                blended[2] += (matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14]) * weight
                total += weight
            if total <= 0.0:
                out.append((position[0], position[1], position[2]))
            else:
                out.append((blended[0] / total, blended[1] / total, blended[2] / total))
        return out

    def _edge_lengths(self, skinned: Sequence[Sequence[float]]) -> List[float]:
        lengths = []
        for index in range(0, len(self.indices) - 2, 3):
            a, b, c = self.indices[index], self.indices[index + 1], self.indices[index + 2]
            for first, second in ((a, b), (b, c), (c, a)):
                p = skinned[first]
                q = skinned[second]
                lengths.append(math.dist(p, q))
        return lengths


def _lerp(a: Sequence[float], b: Sequence[float], t: float) -> List[float]:
    return [x + (y - x) * t for x, y in zip(a, b)]


def _slerp(a: Sequence[float], b: Sequence[float], t: float) -> List[float]:
    """Shortest-arc normalized rotation, matching character.rs LINEAR keys."""
    first = quat_normalize(tuple(a))
    second = quat_normalize(tuple(b))
    cosine = sum(x * y for x, y in zip(first, second))
    if cosine < 0:
        second = tuple(-value for value in second)
        cosine = -cosine
    if cosine > .9995:
        return list(quat_normalize(tuple(_lerp(first, second, t))))
    angle = math.acos(min(1.0, cosine))
    denominator = math.sin(angle)
    result = [x * math.sin((1 - t) * angle) / denominator + y * math.sin(t * angle) / denominator
              for x, y in zip(first, second)]
    return list(quat_normalize(tuple(result)))


def verify_report(report: dict) -> List[str]:
    """Independent sanity gates over the merged sweep."""
    problems = []
    by_path = {asset["path"]: asset for asset in report["assets"]}
    for asset in report["assets"]:
        problems.extend(f"{asset['path']}: {problem}" for problem in asset["weight_problems"])
        if asset["vertices"] <= 0 or asset["triangles"] <= 0 or asset["joints"] <= 0:
            problems.append(f"{asset['path']}: empty geometry or rig")
    for clip in report["clips"]:
        label = f"{clip['asset']} clip {clip['name']}"
        if Path(clip["asset"]).stem in ("beach_seagull", "beach_crab", "beach_fish"):
            asset = by_path[clip["asset"]]
            # character.rs expands the transformed bind half-extents by 1.15
            # plus 5 cm. Local flying/swimming motion must stay in that box;
            # root translation does not update the runtime lighting anchor.
            for axis in range(3):
                low, high = asset["bind_min_m"][axis], asset["bind_max_m"][axis]
                center = (low + high) * 0.5
                half = (high - low) * 0.5 * 1.15 + 0.05
                if (clip["posed_min_m"][axis] < center - half - 1e-5
                        or clip["posed_max_m"][axis] > center + half + 1e-5):
                    problems.append(f"{label}: posed axis {axis} exceeds the runtime bind culling/lighting envelope")
        if clip["min_y_m"] < -0.03:
            problems.append(f"{label}: penetrates the floor by {-clip['min_y_m']:.4f} m")
        if clip["max_edge_stretch"] > 2.0:
            problems.append(f"{label}: edge stretch {clip['max_edge_stretch']:.3f}x")
        if clip["duration"] > 0.0 and clip["contact_frames"] == 0 and clip["kind"] in ("walk", "run"):
            problems.append(f"{label}: a {clip['kind']} clip never touches the floor")
    return problems


def quat_slerp(a: Sequence[float], b: Sequence[float], t: float) -> Tuple[float, float, float, float]:
    """Interpolate glTF rotation keys along the shortest arc."""
    dot = sum(x * y for x, y in zip(a, b))
    if dot < 0.0:
        b = tuple(-component for component in b)
        dot = -dot
    if dot > 0.9995:
        blended = tuple(a[i] + (b[i] - a[i]) * t for i in range(4))
    else:
        theta = math.acos(min(1.0, max(-1.0, dot)))
        sin_theta = math.sin(theta)
        if abs(sin_theta) < 1.0e-9:
            return tuple(a)  # type: ignore[return-value]
        start_weight = math.sin((1.0 - t) * theta) / sin_theta
        end_weight = math.sin(t * theta) / sin_theta
        blended = tuple(a[i] * start_weight + b[i] * end_weight for i in range(4))
    length = math.sqrt(sum(component * component for component in blended))
    return tuple(component / length for component in blended)  # type: ignore[return-value]
