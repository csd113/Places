#!/usr/bin/env python3
"""Author the playable animation clips into the shipped Spooner-Man GLB.

The committed ``assets/entities/spooner-man/model/spooner-man.glb`` is a
hand-authored Blender export: one ``cat_rig`` skin (26 joints), three skinned
primitives, three embedded textures - and no clips. Its mesh, textures, skin,
inverse bind matrices and node hierarchy are the canonical asset and this tool
never rewrites them. It *appends* a small, deterministic set of animation
clips so the engine's character path can play real poses:

    idle       4.0 s loop   breathing, tail sway, a still head
    walk       0.6 s loop   a slow diagonal-pair gait for the route speed
    sit_down   1.6 s once   stand -> sit, holding the seated pose at the end
    sit_idle   5.0 s loop   the seated pose with restrained breathing
    stand_up   1.4 s once   sit -> stand

The clips are authored from the rest skeleton with the same axis convention
the engine uses at runtime (``src/render/common/character.rs``): a rotation of
``angle`` degrees about the model-space X (lateral) or Y (up) axis, expressed
in the joint's parent frame, then composed with the joint's rest rotation.
Translations are authored in model space and converted into the parent frame.
The seated pose's pelvis height is solved against the actual skinned mesh so
the lowest posed vertex touches the floor plane exactly.

Structural rules the writer guarantees, so the engine's GLB reader
(``src/gltf.rs``) accepts the result:

* every existing ``bufferView`` keeps its byte offset and length; new views
  are appended after the recorded base length, and the BIN chunk grows;
* animations are LINEAR samplers over float32 accessors, one shared input
  accessor per clip and one output accessor per channel;
* a ``places_entity_clips`` marker in ``asset.extras`` records the base BIN
  length and clip table, so re-running the tool regenerates its own output
  from the pristine bytes but refuses to overwrite a foreign animation set.

    python3 tools/props/animate_spooner_man.py            # author (idempotent)
    python3 tools/props/animate_spooner_man.py --check    # verify only
    python3 tools/props/animate_spooner_man.py --report   # prints clip detail
    python3 tools/props/animate_spooner_man.py --force    # replace foreign clips
"""

from __future__ import annotations

import argparse
import json
import math
import os
import struct
import sys
from typing import Callable, Dict, List, Optional, Sequence, Tuple

# --------------------------------------------------------------------- model

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GLB_PATH = os.path.join(
    REPO_ROOT, "assets", "entities", "spooner-man", "model", "spooner-man.glb"
)

CLIP_MARKER = "places_entity_clips"
CLIP_VERSION = 1

CHUNK_JSON = 0x4E4F_534A
CHUNK_BIN = 0x004E_4942

# One cycle moves the front-left paw about 0.120 m over 0.6 s. The runtime
# scales clip playback to its route speed using this measured reference.
WALK_REFERENCE_SPEED = 0.20

# Engine limits this tool must respect (src/level.rs).
MAX_ANIMATIONS = 64
MAX_CHANNELS = 4096

Pose = Tuple[Dict[str, float], Dict[str, float], Dict[str, Sequence[float]]]

# Joint families, in the shipped rig.
JOINTS = (
    "leg_rl_paw", "leg_rl_lower", "leg_rl_upper",
    "leg_rr_paw", "leg_rr_lower", "leg_rr_upper",
    "leg_fl_paw", "leg_fl_lower", "leg_fl_upper",
    "leg_fr_paw", "leg_fr_lower", "leg_fr_upper",
    "head", "neck", "chest", "spine",
    "tail_01", "tail_02", "tail_03", "tail_04",
    "tail_05", "tail_06", "tail_07", "tail_08",
    "pelvis", "root",
)


class GlbError(RuntimeError):
    """A malformed or unsupported GLB structure."""


class Model:
    """Read-only view of the shipped GLB plus the rest-pose maths."""

    def __init__(self, path: str) -> None:
        with open(path, "rb") as handle:
            raw = handle.read()
        self.raw = raw
        magic, version, length = struct.unpack_from("<III", raw, 0)
        if magic != 0x4654_6C67 or version != 2 or length != len(raw):
            raise GlbError("asset is not a glTF 2.0 GLB")
        off = 12
        self.json = None
        self.bin = b""
        while off < length:
            chunk_len, chunk_type = struct.unpack_from("<II", raw, off)
            off += 8
            payload = raw[off:off + chunk_len]
            off += chunk_len
            if chunk_type == CHUNK_JSON:
                self.json = json.loads(payload.decode("utf-8"))
            elif chunk_type == CHUNK_BIN:
                self.bin = payload
        if self.json is None:
            raise GlbError("GLB has no JSON chunk")
        self.nodes = self.json.get("nodes") or []
        self.names = [node.get("name", "") for node in self.nodes]
        self.index_of = {name: index for index, name in enumerate(self.names)}
        missing = [name for name in JOINTS if name not in self.index_of]
        if missing:
            raise GlbError(f"the rig is missing joints: {', '.join(missing)}")
        self.parent: Dict[int, int] = {}
        for index, node in enumerate(self.nodes):
            for child in node.get("children") or []:
                self.parent[child] = index
        skin = (self.json.get("skins") or [None])[0]
        if skin is None or len(skin.get("joints") or []) != 26:
            raise GlbError("expected exactly one 26-joint cat_rig skin")
        self.skin = skin
        self.joint_slots = skin["joints"]
        self._rest_global: Dict[int, Tuple[List[List[float]], List[float]]] = {}
        self._rest_rotation: Dict[int, List[List[float]]] = {}
        self.skin_primitives: List[Tuple[int, int, int, int]] = []
        self._vertices = self._read_skin_vertices()

    # -- accessors ---------------------------------------------------------

    def read_accessor(self, index: int, components: int, fmt: str = "f") -> list:
        accessor = self.json["accessors"][index]
        view = self.json["bufferViews"][accessor["bufferView"]]
        offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        size = struct.calcsize("<" + fmt * components)
        stride = view.get("byteStride") or size
        out = []
        for i in range(accessor["count"]):
            out.append(struct.unpack_from("<" + fmt * components, self.bin, offset + i * stride))
        return out

    def _read_skin_vertices(self) -> list:
        vertices = []
        for mesh in self.json.get("meshes") or []:
            for primitive in mesh.get("primitives") or []:
                attributes = primitive.get("attributes") or {}
                if "JOINTS_0" not in attributes:
                    continue
                positions = self.read_accessor(attributes["POSITION"], 3)
                joints = self.read_accessor(attributes["JOINTS_0"], 4, "B")
                weights = self.read_accessor(attributes["WEIGHTS_0"], 4)
                self.skin_primitives.append(
                    (attributes["JOINTS_0"], attributes["WEIGHTS_0"],
                     len(vertices), len(positions))
                )
                for index, position in enumerate(positions):
                    vertices.append((position, joints[index], weights[index]))
        return vertices

    # -- skin weight repair ------------------------------------------------

    def weight_families(self) -> Dict[str, float]:
        """Total skin weight per bone family over every vertex."""
        totals = {"leg": 0.0, "tail": 0.0, "body": 0.0}
        for _position, joints, weights in self._vertices:
            for k in range(4):
                weight = weights[k]
                if weight <= 0.0:
                    continue
                name = self.names[self.joint_slots[joints[k]]]
                if name.startswith("leg_"):
                    totals["leg"] += weight
                elif name.startswith("tail_"):
                    totals["tail"] += weight
                else:
                    totals["body"] += weight
        return totals

    def _joint_segments(self) -> List[Tuple[int, list, list]]:
        """Each joint slot's bind segment in mesh space: parent head -> head."""
        slots = set(self.joint_slots)
        segments = []
        for slot, node in enumerate(self.joint_slots):
            head = self.rest_global(node)[1]
            ancestor = self.parent.get(node)
            parent_head = None
            while ancestor is not None:
                if ancestor in slots:
                    parent_head = self.rest_global(ancestor)[1]
                    break
                ancestor = self.parent.get(ancestor)
            if parent_head is None:
                outer = self.parent.get(node)
                parent_head = (self.rest_global(outer)[1] if outer is not None
                               else [0.0, 0.0, 0.0])
            segments.append((slot, parent_head, head))
        return segments

    @staticmethod
    def _segment_distance(point: Sequence[float], a: Sequence[float],
                          b: Sequence[float]) -> float:
        ab = [b[i] - a[i] for i in range(3)]
        length_sq = sum(value * value for value in ab)
        if length_sq <= 1.0e-12:
            return math.dist(point, a)
        t = sum((point[i] - a[i]) * ab[i] for i in range(3)) / length_sq
        t = max(0.0, min(1.0, t))
        nearest = [a[i] + ab[i] * t for i in range(3)]
        return math.dist(point, nearest)

    def repair_weights_if_needed(self, force: bool = False) -> Dict[str, object]:
        """Rebuilds degenerate skin weights with a nearest-segment bind.

        The shipped hand-authored export binds the paw geometry to the body
        chain (measured below: the leg bones carry under 1% of the vertex
        weight), so no leg animation can deform it. This pass detects that and
        rebinds every vertex to the closest one or two joint segments with a
        smooth two-bone blend. It rewrites only the JOINTS_0/WEIGHTS_0 bytes in
        place: positions, UVs, textures, the node hierarchy, the joint list,
        the inverse bind matrices and the rest pose are untouched.
        """
        before = self.weight_families()
        total = sum(before.values())
        if not force and (total <= 0.0 or before["leg"] / total >= 0.15):
            return {"repaired": False, "before": before}
        sigma = 0.02
        root_slot = next(
            (slot for slot, node in enumerate(self.joint_slots)
             if self.names[node] == "root"),
            None,
        )
        segments = [
            segment for segment in self._joint_segments() if segment[0] != root_slot
        ]
        new_joints: List[Tuple[int, int, int, int]] = []
        new_weights: List[Tuple[float, float, float, float]] = []
        for position, _joints, _weights in self._vertices:
            dists = sorted(
                (self._segment_distance(position, a, b), slot)
                for slot, a, b in segments
            )
            best = dists[0]
            second = dists[1]
            w1 = math.exp(-best[0] / sigma)
            w2 = math.exp(-second[0] / sigma)
            blended = w2 / (w1 + w2)
            pairs = [(best[1], 1.0)]
            if blended >= 0.12:
                pairs = [(best[1], 1.0 - blended), (second[1], blended)]
            packed_joints = [0, 0, 0, 0]
            packed_weights = [0.0, 0.0, 0.0, 0.0]
            for i, (slot, weight) in enumerate(pairs[:4]):
                packed_joints[i] = slot
                packed_weights[i] = weight
            new_joints.append(tuple(packed_joints))
            new_weights.append(tuple(packed_weights))
        for accessor, values in self._pack_skin_accessors(new_joints, new_weights):
            self._overwrite_accessor(accessor, values)
        repaired_vertices = []
        for index, (_position, _joints, _weights) in enumerate(self._vertices):
            repaired_vertices.append(
                (self._vertices[index][0], new_joints[index], new_weights[index])
            )
        self._vertices = repaired_vertices
        after = self.weight_families()
        return {
            "repaired": True,
            "before": before,
            "after": after,
            "sigma_m": sigma,
        }

    def _pack_skin_accessors(self, new_joints: list, new_weights: list) -> List[Tuple[int, list]]:
        """The (accessor, tuples) pairs to overwrite, per primitive."""
        packed = []
        for joints_accessor, weights_accessor, start, count in self.skin_primitives:
            packed.append((joints_accessor, new_joints[start:start + count]))
            packed.append((weights_accessor, new_weights[start:start + count]))
        return packed

    def polish_weight_junctions(self) -> None:
        """Smooth repaired skin junctions once without touching the cat's mesh."""
        extras = self.json.setdefault("asset", {}).setdefault("extras", {})
        if extras.get("places_weight_polish") == 1:
            return
        samples = {}
        for position, joints, weights in self._vertices:
            samples.setdefault(tuple(round(v, 7) for v in position), (position, joints, weights))
        new_joints, new_weights = [], []
        for point, original_joints, original_weights in self._vertices:
            totals = {}
            for position, joints, weights in samples.values():
                distance = math.dist(point, position)
                if distance > 0.10:
                    continue
                proximity = math.exp(-((distance / 0.040) ** 2))
                for joint, weight in zip(joints, weights):
                    totals[joint] = totals.get(joint, 0.0) + proximity * weight
            # The nearest-segment repair can bind a haunch to a lower leg.
            # Ease those influences up the chain through the shoulder/hip mass.
            blend = smoothstep((point[1] - 0.08) / 0.09)
            for slot, weight in list(totals.items()):
                name = self.names[self.joint_slots[slot]]
                if name.startswith("leg_") and name.endswith(("_lower", "_paw")):
                    upper = name.rsplit("_", 1)[0] + "_upper"
                    upper_slot = self.joint_slots.index(self.index_of[upper])
                    totals[slot] -= weight * blend
                    totals[upper_slot] = totals.get(upper_slot, 0.0) + weight * blend
            strongest = sorted(((joint, weight ** 1.5) for joint, weight in totals.items()),
                               key=lambda item: (-item[1], item[0]))[:4]
            total = sum(weight for _, weight in strongest)
            slots = [joint for joint, _ in strongest]
            shares = [weight / total for _, weight in strongest]
            new_joints.append(tuple(slots + [0] * (4 - len(slots))))
            new_weights.append(tuple(shares + [0.0] * (4 - len(shares))))
        for accessor, values in self._pack_skin_accessors(new_joints, new_weights):
            self._overwrite_accessor(accessor, values)
        self._vertices = [(vertex[0], new_joints[i], new_weights[i])
                          for i, vertex in enumerate(self._vertices)]
        extras["places_weight_polish"] = 1

    def _accessor_layout(self, index: int) -> Tuple[int, int, int, int]:
        accessor = self.json["accessors"][index]
        view = self.json["bufferViews"][accessor["bufferView"]]
        offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        components = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[accessor["type"]]
        size = components * 1 if accessor["componentType"] == 5121 else components * 4
        stride = view.get("byteStride") or size
        return offset, accessor["count"], stride, components

    def _overwrite_accessor(self, index: int, tuples: list) -> None:
        offset, count, stride, components = self._accessor_layout(index)
        if count != len(tuples) or stride != components * (1 if self.json["accessors"][index]["componentType"] == 5121 else 4):
            raise GlbError(f"accessor {index} is not tightly packed; cannot repair in place")
        buffer = bytearray(self.bin)
        for i, values in enumerate(tuples):
            if self.json["accessors"][index]["componentType"] == 5121:
                struct.pack_into("<" + "B" * components, buffer, offset + i * stride, *values)
            else:
                struct.pack_into("<" + "f" * components, buffer, offset + i * stride, *values)
        self.bin = bytes(buffer)

    # -- rest pose ---------------------------------------------------------

    def _local(self, index: int) -> Tuple[list, list, list]:
        node = self.nodes[index]
        return (
            node.get("translation", [0.0, 0.0, 0.0]),
            node.get("rotation", [0.0, 0.0, 0.0, 1.0]),
            node.get("scale", [1.0, 1.0, 1.0]),
        )

    def rest_global(self, index: int) -> Tuple[List[List[float]], List[float]]:
        if index in self._rest_global:
            return self._rest_global[index]
        translation, rotation, scale = self._local(index)
        rotation_matrix = quat_matrix(rotation)
        parent = self.parent.get(index)
        if parent is None:
            matrix = rotation_matrix
            position = list(translation)
        else:
            parent_matrix, parent_position = self.rest_global(parent)
            matrix = mat_mul(parent_matrix, rotation_matrix)
            position = vec_add(parent_position, mat_vec(parent_matrix, translation))
        self._rest_global[index] = (matrix, position)
        return self._rest_global[index]

    def axis_in_parent(self, index: int, axis: Sequence[float]) -> List[float]:
        parent = self.parent.get(index)
        if parent is None:
            return list(axis)
        rotation, _ = self.rest_global(parent)
        return [sum(rotation[k][d] * axis[k] for k in range(3)) for d in range(3)]

    def local_pose(self, index: int, lat: float, up: float,
                   delta: Optional[Sequence[float]],
                   parent_rotation: Optional[List[List[float]]] = None) -> Tuple[list, list]:
        """The animated local translation and rotation for one joint."""
        translation, rotation, _ = self._local(index)
        net = quat_mul(
            quat_mul(
                axis_quat(self.axis_in_parent(index, (0.0, 1.0, 0.0)), up),
                axis_quat(self.axis_in_parent(index, (1.0, 0.0, 0.0)), lat),
            ),
            rotation,
        )
        if delta is None:
            return list(translation), net
        parent = self.parent.get(index)
        if parent is None:
            delta_local = list(delta)
        else:
            if parent_rotation is None:
                parent_rotation, _ = self.rest_global(parent)
            delta_local = [
                sum(parent_rotation[k][d] * delta[k] for k in range(3)) for d in range(3)
            ]
        moved = [translation[k] + delta_local[k] for k in range(3)]
        return moved, net

    # -- skinning ----------------------------------------------------------

    def skinned_points(self, pose: Pose) -> List[List[float]]:
        lat, up, trans = pose
        mesh_node = self.index_of["Spooner_Watertight"]
        matrix, position = self._posed_global(mesh_node, lat, up, trans)
        inverse = invert_affine(matrix, position)
        joint_world = [
            self._posed_global(slot, lat, up, trans) for slot in self.joint_slots
        ]
        bind = self.read_accessor(self.skin["inverseBindMatrices"], 16)
        points = []
        for raw, joints, weights in self._vertices:
            acc = [0.0, 0.0, 0.0]
            for k in range(4):
                weight = weights[k]
                if weight <= 0.0:
                    continue
                slot = min(joints[k], len(joint_world) - 1)
                rj, tj = joint_world[slot]
                ib = bind[slot]
                r4 = [[rj[r][c] for c in range(3)] + [tj[r]] for r in range(3)] + [[0.0, 0.0, 0.0, 1.0]]
                ib4 = [
                    [ib[col * 4 + row] for col in range(4)] for row in range(4)
                ]
                m2 = mat_mul4(r4, ib4)
                m3 = mat_mul4(inverse, m2)
                point = [
                    sum(m3[r][c] * raw[c] for c in range(3)) + m3[r][3]
                    for r in range(3)
                ]
                for d in range(3):
                    acc[d] += weight * point[d]
            points.append(acc)
        return points

    def _posed_global(self, index: int, lat: Dict[str, float], up: Dict[str, float],
                      trans: Dict[str, Sequence[float]]) -> Tuple[List[List[float]], List[float]]:
        translation, rotation, _ = self._local(index)
        parent = self.parent.get(index)
        if parent is None:
            parent_matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
            parent_position = [0.0, 0.0, 0.0]
        else:
            parent_matrix, parent_position = self._posed_global(parent, lat, up, trans)
        name = self.names[index]
        net = quat_mul(
            quat_mul(
                axis_quat(self.axis_in_parent(index, (0.0, 1.0, 0.0)), up.get(name, 0.0)),
                axis_quat(self.axis_in_parent(index, (1.0, 0.0, 0.0)), lat.get(name, 0.0)),
            ),
            rotation,
        )
        delta = trans.get(name)
        if delta is None:
            delta_local = [0.0, 0.0, 0.0]
        else:
            delta_local = [
                sum(parent_matrix[k][d] * delta[k] for k in range(3)) for d in range(3)
            ]
        world_matrix = mat_mul(parent_matrix, quat_matrix(net))
        world_position = vec_add(
            parent_position,
            mat_vec(parent_matrix, [translation[k] + delta_local[k] for k in range(3)]),
        )
        return world_matrix, world_position


# ------------------------------------------------------------------ 3x3 maths


def quat_mul(a: Sequence[float], b: Sequence[float]) -> Tuple[float, ...]:
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def axis_quat(axis: Sequence[float], degrees: float) -> Tuple[float, ...]:
    x, y, z = axis
    length = math.sqrt(x * x + y * y + z * z)
    if length < 1e-12:
        return (0.0, 0.0, 0.0, 1.0)
    x, y, z = x / length, y / length, z / length
    half = math.radians(degrees) * 0.5
    sine = math.sin(half)
    return (x * sine, y * sine, z * sine, math.cos(half))


def quat_matrix(q: Sequence[float]) -> List[List[float]]:
    x, y, z, w = q
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]


def mat_mul(a: List[List[float]], b: List[List[float]]) -> List[List[float]]:
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def mat_mul4(a: List[List[float]], b: List[List[float]]) -> List[List[float]]:
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def mat_vec(a: List[List[float]], v: Sequence[float]) -> List[float]:
    return [sum(a[i][k] * v[k] for k in range(3)) for i in range(3)]


def vec_add(a: Sequence[float], b: Sequence[float]) -> List[float]:
    return [a[i] + b[i] for i in range(3)]


def invert_affine(rotation: List[List[float]], position: List[float]) -> List[List[float]]:
    """Inverse of a rigid transform as a full 4x4 row-major matrix."""
    inverse = [[0.0] * 4 for _ in range(4)]
    for i in range(3):
        for j in range(3):
            inverse[i][j] = rotation[j][i]
        inverse[i][3] = -sum(rotation[j][i] * position[j] for j in range(3))
    inverse[3][3] = 1.0
    return inverse


# --------------------------------------------------------------------- poses


def blended(stand: Pose, sit: Pose, t: float) -> Pose:
    """Smoothstep blend between two poses, channel by channel."""
    eased = t * t * (3.0 - 2.0 * t)
    lat: Dict[str, float] = {}
    up: Dict[str, float] = {}
    trans: Dict[str, Sequence[float]] = {}
    for name in set(stand[0]) | set(sit[0]):
        lat[name] = stand[0].get(name, 0.0) + (sit[0].get(name, 0.0) - stand[0].get(name, 0.0)) * eased
    for name in set(stand[1]) | set(sit[1]):
        up[name] = stand[1].get(name, 0.0) + (sit[1].get(name, 0.0) - stand[1].get(name, 0.0)) * eased
    for name in set(stand[2]) | set(sit[2]):
        a = stand[2].get(name, (0.0, 0.0, 0.0))
        b = sit[2].get(name, (0.0, 0.0, 0.0))
        trans[name] = [a[i] + (b[i] - a[i]) * eased for i in range(3)]
    return lat, up, trans


def stand_pose() -> Pose:
    return {}, {}, {}


# Seated pose angles, in degrees: the torso folds nose-up over the dropped
# pelvis, both hind legs fold flat under the haunch, the front legs stay
# planted, and the tail curls around the right flank. The pelvis height is
# solved against the skinned mesh below; `SIT_DROP_M` is the provisional drop
# that the solver corrects.
SIT_LAT = {
    "pelvis": -20.0,
    "spine": -6.0,
    "chest": -5.0,
    "neck": 4.0,
    "head": 8.0,
    "leg_fl_upper": -7.0,
    "leg_fl_lower": -5.0,
    "leg_fl_paw": 0.0,
    "leg_fr_upper": -7.0,
    "leg_fr_lower": -5.0,
    "leg_fr_paw": 0.0,
    "leg_rl_upper": -45.0,
    "leg_rl_lower": -27.0,
    "leg_rl_paw": -12.0,
    "leg_rr_upper": -45.0,
    "leg_rr_lower": -27.0,
    "leg_rr_paw": -12.0,
}
SIT_UP: Dict[str, float] = {}
SIT_DROP_M = -0.1323


def sit_pose(drop: float) -> Pose:
    lat = dict(SIT_LAT)
    up = dict(SIT_UP)
    return lat, up, {"pelvis": (0.0, drop, 0.0)}


def smoothstep(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)


def idle_pose(seconds: float, period: float, amplitude: float = 0.8) -> Pose:
    lat: Dict[str, float] = {}
    up: Dict[str, float] = {}
    t = seconds / period
    for name in ("spine", "chest", "neck", "head"):
        phase = {"spine": 0.0, "chest": 0.12, "neck": 0.24, "head": 0.36}[name]
        lat[name] = (math.sin(math.tau * (t + phase)) - math.sin(math.tau * phase)) * amplitude
    for index in range(1, 9):
        name = f"tail_0{index}"
        up[name] = (math.sin(math.tau * (t + index * 0.08)) - math.sin(math.tau * index * 0.08)) * 3.5
        # Every component must meet its first key at the loop boundary.
        lat[name] = (math.sin(math.tau * (t + index * 0.05)) - math.sin(math.tau * index * 0.05)) * 1.0
    return lat, up, {}


def plant_front_paws(model: Model, pose: Pose, reference: Pose) -> Pose:
    """Counter the breathing chest so both front paws keep their reference pose."""
    lat, up, trans = pose
    lat = dict(lat)
    trans = dict(trans)
    reference_lat, reference_up, reference_trans = reference
    breath = (lat.get("spine", 0.0) - reference_lat.get("spine", 0.0)
              + lat.get("chest", 0.0) - reference_lat.get("chest", 0.0))
    for side in ("fl", "fr"):
        upper = f"leg_{side}_upper"
        paw = model.index_of[f"leg_{side}_paw"]
        lat[upper] = lat.get(upper, 0.0) - breath
        target = model._posed_global(
            paw, reference_lat, reference_up, reference_trans
        )[1]
        current = model._posed_global(paw, lat, up, trans)[1]
        trans[upper] = tuple(target[axis] - current[axis] for axis in range(3))
    return lat, up, trans


def walk_pose(phase: float) -> Pose:
    """One slow diagonal-pair gait cycle; `phase` runs 0..1."""
    lat: Dict[str, float] = {}
    up: Dict[str, float] = {}
    for side, pair in (("fl", 0.0), ("rr", 0.0), ("fr", 0.5), ("rl", 0.5)):
        swing = math.sin(math.tau * (phase + pair))
        front = side[0] == "f"
        lat[f"leg_{side}_upper"] = swing * (24.0 if front else 19.0)
        # Bend during the forward swing, then extend for the backward plant.
        lift = max(0.0, swing) ** 2
        lat[f"leg_{side}_lower"] = -lift * (14.0 if front else 17.0)
        lat[f"leg_{side}_paw"] = -swing * 4.0 + lift * 5.0
    for index in range(1, 9):
        up[f"tail_0{index}"] = math.sin(math.tau * (phase + index * 0.1)) * 5.0
    lat["head"] = math.sin(math.tau * (phase + 0.25)) * 2.0
    up["spine"] = math.sin(math.tau * phase) * 2.0
    bob = -(1.0 - math.cos(math.tau * 2.0 * phase)) * 0.002
    return lat, up, {"pelvis": (0.0, bob, 0.0)}


# -------------------------------------------------------------------- writing


def append_accessor(doc: dict, bin_parts: List[bytes], byte_length: List[int],
                    values: Sequence[float], kind: str, count: int,
                    minmax: Optional[Tuple[float, float]] = None) -> int:
    """Appends one tightly packed float32 accessor; returns its index.

    `minmax` adds the `min`/`max` bounds a sampler's input accessor is
    required to declare by the specification.
    """
    payload = struct.pack("<" + "f" * len(values), *values)
    while byte_length[0] % 4:
        bin_parts.append(b"\x00")
        byte_length[0] += 1
    offset = byte_length[0]
    bin_parts.append(payload)
    byte_length[0] += len(payload)
    doc["bufferViews"].append({
        "buffer": 0,
        "byteOffset": offset,
        "byteLength": len(payload),
    })
    view = len(doc["bufferViews"]) - 1
    accessor = {
        "bufferView": view,
        "componentType": 5126,
        "count": count,
        "type": kind,
    }
    if minmax is not None:
        accessor["min"] = [minmax[0]]
        accessor["max"] = [minmax[1]]
    doc["accessors"].append(accessor)
    return len(doc["accessors"]) - 1


def write_clips(model: Model, prefix_views: int, prefix_accessors: int,
                base_bin_bytes: int) -> dict:
    """Builds the five clips into a fresh JSON with an appended BIN region."""
    doc = json.loads(json.dumps(model.json))
    doc["bufferViews"] = doc["bufferViews"][:prefix_views]
    doc["accessors"] = doc["accessors"][:prefix_accessors]
    doc.pop("animations", None)
    base_bin = model.bin[:base_bin_bytes]
    bin_parts: List[bytes] = [base_bin]
    byte_length = [len(base_bin)]

    # Solve the seated pelvis height so the lowest posed vertex rests on y=0.
    drop = SIT_DROP_M
    for _ in range(3):
        points = model.skinned_points(sit_pose(drop))
        lowest = min(point[1] for point in points)
        if abs(lowest) < 1.0e-4:
            break
        drop -= lowest

    clips = build_clip_table(model, drop)
    animations = []
    total_channels = 0
    for name, duration, samples in clips:
        times = [duration * i / (len(samples) - 1) for i in range(len(samples))]
        time_accessor = append_accessor(
            doc, bin_parts, byte_length, times, "SCALAR", len(times),
            minmax=(min(times), max(times)),
        )
        channels = []
        for joint in JOINTS:
            rotations = [model.local_pose(model.index_of[joint], lat.get(joint, 0.0),
                                          up.get(joint, 0.0), None)[1]
                         for lat, up, _trans in samples]
            values = [component for quat in rotations for component in quat]
            output = append_accessor(doc, bin_parts, byte_length, values, "VEC4", len(rotations))
            channels.append((joint, "rotation", output))
            if any(_trans.get(joint) for _lat, _up, _trans in samples):
                translations = []
                for lat, up, trans in samples:
                    parent = model.parent.get(model.index_of[joint])
                    parent_rotation = (model._posed_global(parent, lat, up, trans)[0]
                                       if parent is not None else None)
                    translations.append(model.local_pose(
                        model.index_of[joint], lat.get(joint, 0.0),
                        up.get(joint, 0.0), trans.get(joint), parent_rotation,
                    )[0])
                flat = [component for point in translations for component in point]
                output = append_accessor(doc, bin_parts, byte_length, flat, "VEC3", len(translations))
                channels.append((joint, "translation", output))
        total_channels += len(channels)
        if total_channels > MAX_CHANNELS:
            raise GlbError(f"clip table exceeds the {MAX_CHANNELS}-channel engine ceiling")
        samplers = []
        gltf_channels = []
        for name_joint, path, output in channels:
            target = model.index_of[name_joint]
            samplers.append({
                "input": time_accessor,
                "interpolation": "LINEAR",
                "output": output,
            })
            gltf_channels.append({
                "sampler": len(samplers) - 1,
                "target": {"node": target, "path": path},
            })
        animations.append({"name": name, "samplers": samplers, "channels": gltf_channels})
    if len(animations) > MAX_ANIMATIONS:
        raise GlbError("too many clips")

    doc["animations"] = animations
    doc["buffers"] = [{"byteLength": byte_length[0]}]
    extras = doc.setdefault("asset", {}).setdefault("extras", {})
    extras[CLIP_MARKER] = {
        "version": CLIP_VERSION,
        "generator": "tools/props/animate_spooner_man.py",
        "base_bin_bytes": len(base_bin),
        "base_buffer_views": prefix_views,
        "base_accessors": prefix_accessors,
        "clips": [{"name": name, "duration": duration, "samples": len(samples)}
                  for name, duration, samples in clips],
        "walk_reference_speed": WALK_REFERENCE_SPEED,
    }
    return {"document": doc, "bin": b"".join(bin_parts), "drop": drop}


def build_clip_table(model: Model, drop: float) -> List[Tuple[str, float, List[Pose]]]:
    """The five clips as (name, duration, sampled poses)."""
    standing = stand_pose()
    idle = [plant_front_paws(model, idle_pose(4.0 * i / 48, 4.0), standing)
            for i in range(49)]
    walk = [walk_pose(i / 48) for i in range(49)]
    sit = sit_pose(drop)

    def transition(amount: float) -> Pose:
        # A small pelvis lift at mid-transition keeps the folding legs from
        # scraping the floor while the pose interpolates.
        lat, up, trans = blended(stand_pose(), sit, amount)
        bump = 0.035 * math.sin(math.pi * amount) ** 2
        trans = dict(trans)
        base = trans.get("pelvis", (0.0, 0.0, 0.0))
        trans["pelvis"] = (base[0], base[1] + bump, base[2])
        return lat, up, trans

    sit_down = [transition(i / 48) for i in range(49)]
    sit_idle = []
    for i in range(61):
        seconds = 5.0 * i / 60
        lat, up, _ = idle_pose(seconds, 5.0, amplitude=0.45)
        base_lat, base_up, base_trans = sit
        merged_lat = dict(base_lat)
        merged_up = dict(base_up)
        for name, value in lat.items():
            merged_lat[name] = merged_lat.get(name, 0.0) + value
        for name, value in up.items():
            merged_up[name] = merged_up.get(name, 0.0) + value
        sit_idle.append(plant_front_paws(
            model, (merged_lat, merged_up, dict(base_trans)), sit
        ))

    def reverse_transition(amount: float) -> Pose:
        lat, up, trans = blended(sit, stand_pose(), amount)
        bump = 0.035 * math.sin(math.pi * amount) ** 2
        trans = dict(trans)
        base = trans.get("pelvis", (0.0, 0.0, 0.0))
        trans["pelvis"] = (base[0], base[1] + bump, base[2])
        return lat, up, trans

    stand_up = [reverse_transition(i / 42) for i in range(43)]
    return [
        ("idle", 4.0, idle),
        ("walk", 0.6, walk),
        ("sit_down", 1.6, sit_down),
        ("sit_idle", 5.0, sit_idle),
        ("stand_up", 1.4, stand_up),
    ]


def glb_document_bytes(document: dict, bin_bytes: bytes) -> bytes:
    json_bytes = json.dumps(document, separators=(",", ":")).encode("utf-8")
    while len(json_bytes) % 4:
        json_bytes += b" "
    while len(bin_bytes) % 4:
        bin_bytes += b"\x00"
    length = 12 + 8 + len(json_bytes) + 8 + len(bin_bytes)
    out = bytearray()
    out += struct.pack("<III", 0x4654_6C67, 2, length)
    out += struct.pack("<II", len(json_bytes), CHUNK_JSON)
    out += json_bytes
    out += struct.pack("<II", len(bin_bytes), CHUNK_BIN)
    out += bin_bytes
    return bytes(out)


# ----------------------------------------------------------------------- main


def check(model: Model) -> int:
    marker = (model.json.get("asset", {}).get("extras") or {}).get(CLIP_MARKER)
    if marker is None:
        print("no authored clips: run tools/props/animate_spooner_man.py")
        return 1
    clips = {entry["name"]: entry for entry in marker.get("clips", [])}
    expected = {"idle", "walk", "sit_down", "sit_idle", "stand_up"}
    missing = expected - set(clips)
    if missing:
        print(f"missing clips: {', '.join(sorted(missing))}")
        return 1
    animations = {animation["name"]: animation for animation in model.json.get("animations") or []}
    for name in sorted(expected):
        animation = animations.get(name)
        if animation is None:
            print(f"clip {name} is not in the GLB animation list")
            return 1
        if name in {"idle", "walk", "sit_idle"}:
            for channel in animation["channels"]:
                sampler = animation["samplers"][channel["sampler"]]
                output = model.json["accessors"][sampler["output"]]
                components = 4 if output["type"] == "VEC4" else 3
                values = model.read_accessor(sampler["output"], components)
                if any(abs(first - last) > 1.0e-5
                       for first, last in zip(values[0], values[-1])):
                    print(f"clip {name} has a discontinuous loop on node "
                          f"{channel['target']['node']}")
                    return 1
    if len(model.bin) < int(marker.get("base_bin_bytes", 0)):
        print("BIN chunk shrank below the pristine mesh/texture region")
        return 1
    print(f"clips present: {', '.join(sorted(expected))}")
    print(f"base BIN bytes {marker['base_bin_bytes']}, total BIN bytes {len(model.bin)}")
    return 0


def report(model: Model, drop: float) -> None:
    print(f"seated pelvis drop: {drop:+.4f} m (solved against the skinned mesh)")
    for name, duration, samples in build_clip_table(model, drop):
        lowest = 1e9
        highest = -1e9
        for pose in samples:
            points = model.skinned_points(pose)
            lowest = min(lowest, min(point[1] for point in points))
            highest = max(highest, max(point[1] for point in points))
        print(f"  {name:9s} {duration:.1f}s {len(samples):3d} keys  y [{lowest:+.3f}, {highest:+.3f}]")
    # The slow-walk stride: how far the front-left ankle travels forward and
    # back across the cycle. The route speed is authored to match this.
    forwards = []
    for i in range(24):
        lat, up, trans = walk_pose(i / 24)
        _matrix, position = model._posed_global(
            model.index_of["leg_fl_paw"], lat, up, trans)
        forwards.append(position[2])
    stride = max(forwards) - min(forwards)
    print(f"walk stride {stride:.3f} m per cycle -> reference speed "
          f"{stride / 0.6:.3f} m/s")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the authored clips")
    parser.add_argument("--report", action="store_true", help="print clip detail")
    parser.add_argument("--force", action="store_true",
                        help="replace a foreign animation set")
    parser.add_argument("--repair-skin", action="store_true",
                        help="force a skin-weight rebind even if the current "
                             "weights look healthy")
    args = parser.parse_args(argv)

    model = Model(GLB_PATH)
    marker = (model.json.get("asset", {}).get("extras") or {}).get(CLIP_MARKER)
    if args.check:
        return check(model)
    repair = model.repair_weights_if_needed(force=args.repair_skin)
    if repair["repaired"]:
        model.json.setdefault("asset", {}).setdefault("extras", {}).pop("places_weight_polish", None)
    model.polish_weight_junctions()
    if repair["repaired"]:
        before = repair["before"]
        after = repair["after"]
        print(
            "skin repair: rebound vertices to nearest joint segments "
            f"(leg share {before['leg'] / sum(before.values()):.3f} -> "
            f"{after['leg'] / sum(after.values()):.3f})"
        )
    else:
        print("skin weights: healthy (no repair needed)")
    if marker is None and model.json.get("animations") and not args.force:
        print("the GLB carries animations this tool did not write; pass --force to replace them")
        return 1
    prefix_views = (int(marker["base_buffer_views"]) if marker
                    else len(model.json.get("bufferViews") or []))
    prefix_accessors = (int(marker["base_accessors"]) if marker
                        else len(model.json.get("accessors") or []))
    base_bin_bytes = (int(marker["base_bin_bytes"]) if marker else len(model.bin))
    built = write_clips(model, prefix_views, prefix_accessors, base_bin_bytes)
    document, payload, drop = built["document"], built["bin"], built["drop"]
    # The marker must not deny a repair the file still contains: a re-run over
    # healthy weights keeps the previous "repaired" record.
    if (not repair["repaired"] and marker
            and (marker.get("skin_repair") or {}).get("repaired")):
        repair_entry = marker["skin_repair"]
    else:
        repair_entry = {
            "repaired": repair["repaired"],
            "method": "nearest-segment-2bone" if repair["repaired"] else "none",
        }
    document["asset"]["extras"][CLIP_MARKER]["skin_repair"] = repair_entry
    if args.report:
        report(model, drop)
    encoded = glb_document_bytes(document, payload)
    with open(GLB_PATH, "wb") as handle:
        handle.write(encoded)
    channels = sum(len(animation["channels"]) for animation in document["animations"])
    print(f"wrote {len(encoded)} bytes ({channels} channels, "
          f"{len(document['animations'])} clips) to {os.path.relpath(GLB_PATH, REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
