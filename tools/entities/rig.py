"""Rigged entity GLB writer: skins, joints and LINEAR clips in pure Python.

This is the entity sibling of ``tools/props/glb.py``. Props are static,
single-node GLBs; entities are skinned characters with a real joint hierarchy
and named animation clips, so they need a writer that emits the glTF 2.0 skin
and animation structures the Rust loader (``src/gltf.rs``) reads:

* nodes with TRS locals (joints first, a dedicated mesh node last);
* one skin: ``joints``, ``skeleton`` and ``inverseBindMatrices``;
* one mesh/primitive with ``POSITION``, ``TEXCOORD_0``, ``COLOR_0``,
  ``JOINTS_0`` (unsigned byte vec4) and ``WEIGHTS_0`` (float vec4);
* named LINEAR animation clips driving node ``rotation``/``translation``;
* ``asset.extras.places_entity_clips`` metadata: per-clip loop flag and the
  measured ground speed the clip's stride is authored for, so the runtime can
  play a walk or run at the route's speed with no foot sliding.

Geometry is authored with the existing ``tools/props/mesh.py`` primitives
(boxes, cylinders, tubes, lathes) and painted with ``tools/props/tex.py``, so
entity art shares the prop pack's conventions. Weights are assigned from the
rest pose by a deterministic nearest-segment blend (up to four influences), or
explicitly per vertex.

The writer is pure stdlib and deterministic: the same builder produces
byte-identical GLBs.

    python3 tools/entities/rig.py --selftest    # write + check a tiny rig
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "tools" / "props"))

from mesh import Mesh  # noqa: E402  (tools/props on sys.path)

GLB_MAGIC = 0x46546C67
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942

COMPONENT_BYTE = 5120
COMPONENT_UBYTE = 5121
COMPONENT_SHORT = 5122
COMPONENT_USHORT = 5123
COMPONENT_UINT = 5125
COMPONENT_FLOAT = 5126

TARGET_ARRAY = 34962
TARGET_ELEMENT = 34963

WALK_REFERENCE_SPEED_MPS = 0.35
RUN_REFERENCE_SPEED_MPS = 1.20

IDENTITY_QUAT = (0.0, 0.0, 0.0, 1.0)


class RigError(ValueError):
    """Raised when a rig or clip is malformed before it reaches the disk."""


# --------------------------------------------------------------------- maths


def quat_identity() -> Tuple[float, float, float, float]:
    return IDENTITY_QUAT


def quat_from_axis_angle(axis: Sequence[float], degrees: float) -> Tuple[float, float, float, float]:
    """Quaternion for a rotation of ``degrees`` about ``axis`` (right-handed)."""
    length = math.sqrt(axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2])
    if length <= 1.0e-12:
        raise RigError("rotation axis must be non-zero")
    half = math.radians(degrees) * 0.5
    s = math.sin(half) / length
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(half))


def quat_rot_x(degrees: float) -> Tuple[float, float, float, float]:
    return quat_from_axis_angle((1.0, 0.0, 0.0), degrees)


def quat_rot_y(degrees: float) -> Tuple[float, float, float, float]:
    return quat_from_axis_angle((0.0, 1.0, 0.0), degrees)


def quat_rot_z(degrees: float) -> Tuple[float, float, float, float]:
    return quat_from_axis_angle((0.0, 0.0, 1.0), degrees)


def quat_compose(
    a: Tuple[float, float, float, float], b: Tuple[float, float, float, float]
) -> Tuple[float, float, float, float]:
    """Rotation ``a`` followed by ``b`` in the same parent frame (a * b)."""
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def quat_mul_many(*quats: Tuple[float, float, float, float]) -> Tuple[float, float, float, float]:
    result = quat_identity()
    for quat in quats:
        result = quat_compose(result, quat)
    return result


def quat_normalize(quat: Tuple[float, float, float, float]) -> Tuple[float, float, float, float]:
    length = math.sqrt(sum(component * component for component in quat))
    if length <= 1.0e-12:
        return quat_identity()
    return tuple(component / length for component in quat)  # type: ignore[return-value]


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


def _mat_inverse(m: Sequence[float]) -> List[float]:
    """General 4x4 inverse (column-major), sufficient for affine bind matrices."""
    inv = [0.0] * 16
    a = [m[0], m[1], m[2], m[3]]
    b = [m[4], m[5], m[6], m[7]]
    c = [m[8], m[9], m[10], m[11]]
    d = [m[12], m[13], m[14], m[15]]

    s0 = a[0] * b[1] - a[1] * b[0]
    s1 = a[0] * b[2] - a[2] * b[0]
    s2 = a[0] * b[3] - a[3] * b[0]
    s3 = a[1] * b[2] - a[2] * b[1]
    s4 = a[1] * b[3] - a[3] * b[1]
    s5 = a[2] * b[3] - a[3] * b[2]

    c5 = c[2] * d[3] - c[3] * d[2]
    c4 = c[1] * d[3] - c[3] * d[1]
    c3 = c[1] * d[2] - c[2] * d[1]
    c2 = c[0] * d[3] - c[3] * d[0]
    c1 = c[0] * d[2] - c[2] * d[0]
    c0 = c[0] * d[1] - c[1] * d[0]

    determinant = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0
    if abs(determinant) <= 1.0e-12 or not math.isfinite(determinant):
        raise RigError("bind matrix is singular")

    inv[0] = (b[1] * c5 - b[2] * c4 + b[3] * c3) / determinant
    inv[1] = (-a[1] * c5 + a[2] * c4 - a[3] * c3) / determinant
    inv[2] = (d[1] * s5 - d[2] * s4 + d[3] * s3) / determinant
    inv[3] = (-c[1] * s5 + c[2] * s4 - c[3] * s3) / determinant
    inv[4] = (-b[0] * c5 + b[2] * c2 - b[3] * c1) / determinant
    inv[5] = (a[0] * c5 - a[2] * c2 + a[3] * c1) / determinant
    inv[6] = (-d[0] * s5 + d[2] * s2 - d[3] * s1) / determinant
    inv[7] = (c[0] * s5 - c[2] * s2 + c[3] * s1) / determinant
    inv[8] = (b[0] * c4 - b[1] * c2 + b[3] * c0) / determinant
    inv[9] = (-a[0] * c4 + a[1] * c2 - a[3] * c0) / determinant
    inv[10] = (d[0] * s4 - d[1] * s2 + d[3] * s0) / determinant
    inv[11] = (-c[0] * s4 + c[1] * s2 - c[3] * s0) / determinant
    inv[12] = (-b[0] * c3 + b[1] * c1 - b[2] * c0) / determinant
    inv[13] = (a[0] * c3 - a[1] * c1 + a[2] * c0) / determinant
    inv[14] = (-d[0] * s3 + d[1] * s1 - d[2] * s0) / determinant
    inv[15] = (c[0] * s3 - c[1] * s1 + c[2] * s0) / determinant
    return inv


def _mat_translation(translation: Sequence[float]) -> List[float]:
    return _mat_trs(translation, IDENTITY_QUAT, (1.0, 1.0, 1.0))


def transform_point(matrix: Sequence[float], point: Sequence[float]) -> Tuple[float, float, float]:
    x, y, z = point
    return (
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    )


# ----------------------------------------------------------------------- rig


@dataclass
class Joint:
    name: str
    parent: Optional[int]
    translation: Tuple[float, float, float]
    rotation: Tuple[float, float, float, float] = IDENTITY_QUAT
    scale: Tuple[float, float, float] = (1.0, 1.0, 1.0)
    children: List[int] = field(default_factory=list)
    # Rest global transform, filled by :meth:`Rig.finish`.
    global_rest: List[float] = field(default_factory=_mat_identity)
    # Extra local point marking the tip of this joint's bone segment, used for
    # automatic weighting when the joint has no children (e.g. a head).
    tail: Optional[Tuple[float, float, float]] = None


class Rig:
    """A joint hierarchy in rest pose: names, parents, local translations."""

    def __init__(self) -> None:
        self.joints: List[Joint] = []
        self._finished = False

    def add(
        self,
        name: str,
        parent: Optional[int] = None,
        translation: Sequence[float] = (0.0, 0.0, 0.0),
        rotation: Sequence[float] = IDENTITY_QUAT,
        scale: Sequence[float] = (1.0, 1.0, 1.0),
        tail: Optional[Sequence[float]] = None,
    ) -> int:
        if self._finished:
            raise RigError("the rig is finished; add joints before writing clips")
        if parent is not None and not 0 <= parent < len(self.joints):
            raise RigError(f"joint {name!r} names parent {parent}, which does not exist yet")
        if any(joint.name == name for joint in self.joints):
            raise RigError(f"duplicate joint name {name!r}")
        index = len(self.joints)
        joint = Joint(
            name=name,
            parent=parent,
            translation=tuple(float(v) for v in translation),
            rotation=quat_normalize(tuple(float(v) for v in rotation)),
            scale=tuple(float(v) for v in scale),
            tail=tuple(float(v) for v in tail) if tail is not None else None,
        )
        self.joints.append(joint)
        if parent is not None:
            self.joints[parent].children.append(index)
        return index

    def index(self, joint: int | str) -> int:
        if isinstance(joint, int):
            if not 0 <= joint < len(self.joints):
                raise RigError(f"joint index {joint} does not exist")
            return joint
        for index, entry in enumerate(self.joints):
            if entry.name == joint:
                return index
        raise RigError(f"unknown joint {joint!r}")

    def finish(self) -> None:
        """Resolves every joint's rest global transform."""
        if self._finished:
            return
        for index, joint in enumerate(self.joints):
            if joint.parent is not None and joint.parent >= index:
                raise RigError(f"joint {joint.name!r} has a non-prior parent")
            local = _mat_trs(joint.translation, joint.rotation, joint.scale)
            joint.global_rest = (
                _mat_mul(self.joints[joint.parent].global_rest, local)
                if joint.parent is not None
                else local
            )
        self._finished = True

    def inverse_bind_matrices(self) -> List[List[float]]:
        self.finish()
        return [_mat_inverse(joint.global_rest) for joint in self.joints]

    def segment(self, index: int) -> Tuple[Tuple[float, float, float], Tuple[float, float, float]]:
        """Rest-space start and end of one joint's bone segment."""
        self.finish()
        joint = self.joints[index]
        # `global_rest` already contains this joint's own local translation, so
        # the segment start is the joint's origin in its own rest frame.
        start = transform_point(joint.global_rest, (0.0, 0.0, 0.0))
        if joint.children:
            # The segment runs through the average of the child joint origins,
            # which keeps a forked joint (a chest with two shoulders) centred.
            points = [transform_point(self.joints[c].global_rest, (0.0, 0.0, 0.0)) for c in joint.children]
            end = (
                sum(p[0] for p in points) / len(points),
                sum(p[1] for p in points) / len(points),
                sum(p[2] for p in points) / len(points),
            )
        elif joint.tail is not None:
            end = transform_point(joint.global_rest, joint.tail)
        else:
            end = transform_point(joint.global_rest, (0.0, 0.0, -0.05))
        return start, end


# --------------------------------------------------------------------- mesh


@dataclass
class SkinnedMesh:
    """A prop ``Mesh`` plus per-vertex joint indices and weights."""

    mesh: Mesh
    joints: List[List[int]] = field(default_factory=list)
    weights: List[List[float]] = field(default_factory=list)

    def ensure(self) -> None:
        count = len(self.mesh.positions)
        if not self.joints:
            self.joints = [[0, 0, 0, 0] for _ in range(count)]
        if not self.weights:
            self.weights = [[1.0, 0.0, 0.0, 0.0] for _ in range(count)]
        if len(self.joints) != count or len(self.weights) != count:
            raise RigError("joint/weight arrays must be parallel to the mesh vertices")


def _point_segment_distance(
    point: Sequence[float],
    start: Sequence[float],
    end: Sequence[float],
) -> float:
    px, py, pz = point
    ax, ay, az = start
    bx, by, bz = end
    dx, dy, dz = bx - ax, by - ay, bz - az
    length_squared = dx * dx + dy * dy + dz * dz
    if length_squared <= 1.0e-12:
        return math.dist((px, py, pz), (ax, ay, az))
    t = ((px - ax) * dx + (py - ay) * dy + (pz - az) * dz) / length_squared
    t = min(1.0, max(0.0, t))
    cx, cy, cz = ax + dx * t, ay + dy * t, az + dz * t
    return math.sqrt((px - cx) ** 2 + (py - cy) ** 2 + (pz - cz) ** 2)


def auto_weights(
    skinned: SkinnedMesh,
    rig: Rig,
    *,
    sigma: float = 0.06,
    max_influences: int = 3,
    min_share: float = 0.15,
    only: Optional[Sequence[int]] = None,
    overrides: Optional[Dict[int, Sequence[Tuple[int, float]]]] = None,
) -> None:
    """Assigns a deterministic nearest-segment blend to every vertex.

    Each vertex weights the closest ``max_influences`` bone segments by
    ``exp(-(d/sigma)^2)``; influences below ``min_share`` of the total are
    dropped and the remainder renormalised to sum to one. ``only`` restricts
    the candidate joints (useful for a tail or a skull), and ``overrides`` pins
    specific vertex indices to explicit ``(joint, weight)`` pairs.
    """
    if sigma <= 0.0 or not math.isfinite(sigma):
        raise RigError("weight sigma must be positive and finite")
    rig.finish()
    count = len(skinned.mesh.positions)
    candidates = list(range(len(rig.joints))) if only is None else [rig.index(j) for j in only]
    if not candidates:
        raise RigError("auto_weights needs at least one candidate joint")
    segments = {index: rig.segment(index) for index in candidates}
    joints: List[List[int]] = [[0, 0, 0, 0] for _ in range(count)]
    weights: List[List[float]] = [[0.0, 0.0, 0.0, 0.0] for _ in range(count)]
    override_map = overrides or {}
    for vertex, position in enumerate(skinned.mesh.positions):
        if vertex in override_map:
            pairs = list(override_map[vertex])
            if not pairs:
                raise RigError(f"vertex {vertex} has an empty weight override")
            total = sum(weight for _, weight in pairs)
            if total <= 0.0:
                raise RigError(f"vertex {vertex} weight override sums to zero")
            entries = [(rig.index(joint), weight / total) for joint, weight in pairs]
        else:
            scored = []
            for joint in candidates:
                start, end = segments[joint]
                distance = _point_segment_distance(position, start, end)
                scored.append((math.exp(-((distance / sigma) ** 2)), joint))
            scored.sort(key=lambda entry: (-entry[0], entry[1]))
            scored = scored[:max_influences]
            total = sum(score for score, _ in scored)
            if total <= 1.0e-9:
                scored = [(1.0, scored[0][1])]
                total = 1.0
            kept = [
                (score / total, joint)
                for score, joint in scored
                if score / total >= min_share
            ] or [(1.0, scored[0][1])]
            total = sum(weight for weight, _ in kept)
            entries = [(joint, weight / total) for weight, joint in kept]
        for slot, (joint, weight) in enumerate(entries[:4]):
            joints[vertex][slot] = joint
            weights[vertex][slot] = weight
    skinned.joints = joints
    skinned.weights = weights


def explicit_weights(
    skinned: SkinnedMesh,
    rig: Rig,
    pairs: Dict[int, Sequence[Tuple[int | str, float]]],
) -> None:
    """Pins specific vertices to explicit ``(joint, weight)`` pairs."""
    rig.finish()
    overrides = {
        vertex: [(rig.index(joint), weight) for joint, weight in entries]
        for vertex, entries in pairs.items()
    }
    auto_weights(skinned, rig, overrides=overrides)


def recolor(skinned: SkinnedMesh, color: Tuple[int, int, int]) -> None:
    """Sets every vertex colour, keeping the mesh's baked per-face shading."""
    skinned.mesh.colors = [color for _ in skinned.mesh.colors]


# -------------------------------------------------------------------- clips


class Clip:
    """One named LINEAR clip: keyframed local rotations and translations."""

    def __init__(
        self,
        name: str,
        *,
        loop: bool = True,
        duration: Optional[float] = None,
        reference_speed_mps: Optional[float] = None,
        kind: Optional[str] = None,
    ) -> None:
        self.name = name
        self.loop = loop
        self.duration = duration
        self.reference_speed_mps = reference_speed_mps
        self.kind = kind
        # joint -> [(time, quat)]
        self.rotations: Dict[int, List[Tuple[float, Tuple[float, float, float, float]]]] = {}
        # joint -> [(time, (dx, dy, dz) offset from rest translation)]
        self.translations: Dict[int, List[Tuple[float, Tuple[float, float, float]]]] = {}

    def joint_rotation(
        self,
        rig: Rig,
        joint: int | str,
        time: float,
        rotation: Tuple[float, float, float, float],
    ) -> "Clip":
        self.rotations.setdefault(rig.index(joint), []).append(
            (float(time), quat_normalize(rotation))
        )
        return self

    def joint_translation(
        self,
        rig: Rig,
        joint: int | str,
        time: float,
        offset: Sequence[float],
    ) -> "Clip":
        self.translations.setdefault(rig.index(joint), []).append(
            (float(time), tuple(float(v) for v in offset))
        )
        return self

    def effective_duration(self) -> float:
        times = [time for keys in self.rotations.values() for time, _ in keys]
        times += [time for keys in self.translations.values() for time, _ in keys]
        longest = max(times, default=0.0)
        if self.duration is not None:
            if self.duration + 1.0e-6 < longest:
                raise RigError(
                    f"clip {self.name!r} declares duration {self.duration} but keys reach {longest}"
                )
            return float(self.duration)
        return longest

    def keyed_rotations(self, rig: Rig) -> Dict[int, List[Tuple[float, Tuple[float, float, float, float]]]]:
        """Rotation keys as absolute local quaternions, grouped by joint."""
        resolved: Dict[int, List[Tuple[float, Tuple[float, float, float, float]]]] = {}
        for joint, keys in self.rotations.items():
            index = rig.index(joint)
            rest = rig.joints[index].rotation
            resolved[index] = sorted(
                ((time, quat_compose(rest, delta)) for time, delta in keys),
                key=lambda entry: entry[0],
            )
        return resolved

    def keyed_translations(self, rig: Rig) -> Dict[int, List[Tuple[float, Tuple[float, float, float]]]]:
        """Translation keys as absolute local translations, grouped by joint."""
        resolved: Dict[int, List[Tuple[float, Tuple[float, float, float]]]] = {}
        for joint, keys in self.translations.items():
            index = rig.index(joint)
            rest = rig.joints[index].translation
            resolved[index] = sorted(
                (
                    (time, (rest[0] + offset[0], rest[1] + offset[1], rest[2] + offset[2]))
                    for time, offset in keys
                ),
                key=lambda entry: entry[0],
            )
        return resolved


def rotation_clip(
    rig: Rig,
    name: str,
    poses: Dict[int | str, Tuple[float, float, float, float]],
    *,
    loop: bool = True,
    reference_speed_mps: Optional[float] = None,
    kind: Optional[str] = None,
    hold_seconds: float = 0.0,
) -> Clip:
    """A single-pose hold clip: every named joint keyed at 0 (and the end).

    A pose clip holds one pose for its whole duration. With ``loop`` the pose
    is keyed at both ends so the runtime wraps without a jump.
    """
    clip = Clip(name, loop=loop, reference_speed_mps=reference_speed_mps, kind=kind)
    duration = hold_seconds if hold_seconds > 0.0 else 0.0
    for joint, rotation in poses.items():
        index = rig.index(joint)
        clip.rotations.setdefault(index, []).append((0.0, quat_normalize(rotation)))
        if duration > 0.0:
            clip.rotations[index].append((duration, quat_normalize(rotation)))
    if duration > 0.0:
        clip.duration = duration
    return clip


def transition_clip(
    rig: Rig,
    name: str,
    from_pose: Dict[int | str, Tuple[float, float, float, float]],
    to_pose: Dict[int | str, Tuple[float, float, float, float]],
    seconds: float,
    *,
    loop: bool = False,
    kind: Optional[str] = None,
) -> Clip:
    """A one-shot transition from ``from_pose`` to ``to_pose`` over ``seconds``."""
    clip = Clip(name, loop=loop, duration=seconds, kind=kind)
    joints = sorted({rig.index(j) for j in list(from_pose) + list(to_pose)})
    for index in joints:
        start = quat_normalize(from_pose.get(index, IDENTITY_QUAT))  # type: ignore[arg-type]
        end = quat_normalize(to_pose.get(index, IDENTITY_QUAT))  # type: ignore[arg-type]
        clip.rotations.setdefault(index, []).append((0.0, start))
        clip.rotations[index].append((seconds, end))
    return clip


# ------------------------------------------------------------------- writer


def _quat_close(a: Sequence[float], b: Sequence[float], tolerance: float = 1.0e-4) -> bool:
    # q and -q are the same rotation.
    direct = max(abs(x - y) for x, y in zip(a, b))
    flipped = max(abs(x + y) for x, y in zip(a, b))
    return min(direct, flipped) <= tolerance


def _validate_clip(rig: Rig, clip: Clip) -> None:
    duration = clip.effective_duration()
    for joint, keys in clip.keyed_rotations(rig).items():
        if not keys:
            continue
        if abs(keys[0][0]) > 1.0e-6:
            raise RigError(
                f"clip {clip.name!r} joint {rig.joints[joint].name!r} needs a key at t=0"
            )
        for (t0, _), (t1, _) in zip(keys, keys[1:]):
            if t1 - t0 < 1.0e-6:
                raise RigError(f"clip {clip.name!r} has duplicate or unordered key times")
        if clip.loop and duration > 0.0:
            if abs(keys[-1][0] - duration) > 1.0e-6:
                raise RigError(
                    f"clip {clip.name!r} is a loop: joint {rig.joints[joint].name!r} "
                    f"needs an end key at {duration}"
                )
            if not _quat_close(keys[0][1], keys[-1][1]):
                raise RigError(
                    f"clip {clip.name!r} is a loop: joint {rig.joints[joint].name!r} "
                    "does not return to its first key"
                )
    for joint, keys in clip.keyed_translations(rig).items():
        if not keys:
            continue
        if abs(keys[0][0]) > 1.0e-6:
            raise RigError(
                f"clip {clip.name!r} joint {rig.joints[joint].name!r} needs a key at t=0"
            )
        if clip.loop and duration > 0.0:
            if abs(keys[-1][0] - duration) > 1.0e-6:
                raise RigError(
                    f"clip {clip.name!r} is a loop: joint {rig.joints[joint].name!r} "
                    f"needs an end key at {duration}"
                )
            if max(abs(a - b) for a, b in zip(keys[0][1], keys[-1][1])) > 1.0e-4:
                raise RigError(
                    f"clip {clip.name!r} is a loop: joint {rig.joints[joint].name!r} "
                    "does not return to its first key"
                )


class _Bin:
    def __init__(self) -> None:
        self.data = bytearray()
        self.views: List[dict] = []

    def align(self, alignment: int = 4) -> None:
        while len(self.data) % alignment:
            self.data.append(0)

    def add_view(self, payload: bytes, target: Optional[int]) -> int:
        self.align(4)
        offset = len(self.data)
        self.data.extend(payload)
        view = {"buffer": 0, "byteOffset": offset, "byteLength": len(payload)}
        if target is not None:
            view["target"] = target
        self.views.append(view)
        return len(self.views) - 1


def _f32_bytes(values: Iterable[float]) -> bytes:
    return b"".join(struct.pack("<f", value) for value in values)


def write_model(
    path: Path | str,
    skinned: SkinnedMesh,
    rig: Rig,
    clips: Sequence[Clip],
    texture_png: bytes,
    *,
    name: str,
    generator: str = "tools/entities/rig.py",
) -> dict:
    """Writes a self-contained rigged entity GLB and returns its statistics."""
    rig.finish()
    skinned.ensure()
    mesh = skinned.mesh
    if not mesh.positions:
        raise RigError("cannot write an empty mesh")
    if len(mesh.positions) > 65535:
        raise RigError("entity meshes use 16-bit indices; above 65535 vertices")
    if not texture_png.startswith(b"\x89PNG"):
        raise RigError("entity textures must be PNG")
    if len(rig.joints) > 128:
        raise RigError("the engine ceiling is 128 joints per entity")
    if len(clips) > 64:
        raise RigError("the engine ceiling is 64 clips per entity")
    seen_names = set()
    for clip in clips:
        if clip.name in seen_names:
            raise RigError(f"duplicate clip name {clip.name!r}")
        seen_names.add(clip.name)
        _validate_clip(rig, clip)

    binary = _Bin()
    accessors: List[dict] = []

    def add_accessor(
        payload: bytes,
        *,
        component_type: int,
        type_name: str,
        count: int,
        target: int,
        minimum: Optional[Sequence[float]] = None,
        maximum: Optional[Sequence[float]] = None,
        normalized: bool = False,
    ) -> int:
        view = binary.add_view(payload, target)
        accessor: dict = {
            "bufferView": view,
            "componentType": component_type,
            "count": count,
            "type": type_name,
        }
        if normalized:
            accessor["normalized"] = True
        if minimum is not None:
            accessor["min"] = [float(v) for v in minimum]
        if maximum is not None:
            accessor["max"] = [float(v) for v in maximum]
        accessors.append(accessor)
        return len(accessors) - 1

    # Mesh attributes ----------------------------------------------------
    positions = [component for position in mesh.positions for component in position]
    pos_min = [min(p[i] for p in mesh.positions) for i in range(3)]
    pos_max = [max(p[i] for p in mesh.positions) for i in range(3)]
    position_accessor = add_accessor(
        _f32_bytes(positions),
        component_type=COMPONENT_FLOAT,
        type_name="VEC3",
        count=len(mesh.positions),
        target=TARGET_ARRAY,
        minimum=pos_min,
        maximum=pos_max,
    )
    uv_accessor = add_accessor(
        _f32_bytes([component for uv in mesh.uvs for component in uv]),
        component_type=COMPONENT_FLOAT,
        type_name="VEC2",
        count=len(mesh.uvs),
        target=TARGET_ARRAY,
    )
    color_bytes = bytearray()
    for color in mesh.colors:
        color_bytes.extend(bytes((color[0], color[1], color[2], 255)))
    color_accessor = add_accessor(
        bytes(color_bytes),
        component_type=COMPONENT_UBYTE,
        type_name="VEC4",
        count=len(mesh.colors),
        target=TARGET_ARRAY,
        normalized=True,
    )
    joint_bytes = bytearray()
    for slots in skinned.joints:
        joint_bytes.extend(bytes(min(255, max(0, slot)) for slot in slots))
    joints_accessor = add_accessor(
        bytes(joint_bytes),
        component_type=COMPONENT_UBYTE,
        type_name="VEC4",
        count=len(skinned.joints),
        target=TARGET_ARRAY,
    )
    weights_accessor = add_accessor(
        _f32_bytes([component for weights in skinned.weights for component in weights]),
        component_type=COMPONENT_FLOAT,
        type_name="VEC4",
        count=len(skinned.weights),
        target=TARGET_ARRAY,
    )
    index_accessor = add_accessor(
        b"".join(struct.pack("<H", index) for index in mesh.indices),
        component_type=COMPONENT_USHORT,
        type_name="SCALAR",
        count=len(mesh.indices),
        target=TARGET_ELEMENT,
    )

    # Skin ---------------------------------------------------------------
    inverse_bind = rig.inverse_bind_matrices()
    ibm_accessor = add_accessor(
        _f32_bytes(component for matrix in inverse_bind for component in matrix),
        component_type=COMPONENT_FLOAT,
        type_name="MAT4",
        count=len(inverse_bind),
        target=TARGET_ARRAY,
    )
    image_view = binary.add_view(texture_png, None)

    # Animations ---------------------------------------------------------
    animations: List[dict] = []
    clip_stats: List[dict] = []
    for clip in clips:
        duration = clip.effective_duration()
        samplers: List[dict] = []
        channels: List[dict] = []
        for joint, keys in sorted(clip.keyed_rotations(rig).items()):
            times = [time for time, _ in keys]
            values = [component for _, quat in keys for component in quat]
            input_accessor = add_accessor(
                _f32_bytes(times),
                component_type=COMPONENT_FLOAT,
                type_name="SCALAR",
                count=len(times),
                target=None,
                minimum=[times[0]],
                maximum=[times[-1]],
            )
            output_accessor = add_accessor(
                _f32_bytes(values),
                component_type=COMPONENT_FLOAT,
                type_name="VEC4",
                count=len(times),
                target=None,
            )
            samplers.append(
                {"input": input_accessor, "output": output_accessor, "interpolation": "LINEAR"}
            )
            channels.append(
                {
                    "sampler": len(samplers) - 1,
                    "target": {"node": joint, "path": "rotation"},
                }
            )
        for joint, keys in sorted(clip.keyed_translations(rig).items()):
            times = [time for time, _ in keys]
            values = [component for _, translation in keys for component in translation]
            input_accessor = add_accessor(
                _f32_bytes(times),
                component_type=COMPONENT_FLOAT,
                type_name="SCALAR",
                count=len(times),
                target=None,
                minimum=[times[0]],
                maximum=[times[-1]],
            )
            output_accessor = add_accessor(
                _f32_bytes(values),
                component_type=COMPONENT_FLOAT,
                type_name="VEC3",
                count=len(times),
                target=None,
            )
            samplers.append(
                {"input": input_accessor, "output": output_accessor, "interpolation": "LINEAR"}
            )
            channels.append(
                {
                    "sampler": len(samplers) - 1,
                    "target": {"node": joint, "path": "translation"},
                }
            )
        animations.append({"name": clip.name, "samplers": samplers, "channels": channels})
        clip_stats.append(
            {
                "name": clip.name,
                "duration": duration,
                "loop": clip.loop,
                "channels": len(channels),
                "reference_speed_mps": clip.reference_speed_mps,
                "kind": clip.kind,
            }
        )
    if sum(len(animation["channels"]) for animation in animations) > 4096:
        raise RigError("the engine ceiling is 4096 animation channels per model")

    # Nodes: joints first, then the mesh node as a separate scene root. -----
    joint_count = len(rig.joints)
    mesh_node = joint_count
    nodes: List[dict] = []
    for joint in rig.joints:
        node: dict = {"name": joint.name, "translation": list(joint.translation)}
        if joint.rotation != IDENTITY_QUAT:
            node["rotation"] = list(joint.rotation)
        if joint.scale != (1.0, 1.0, 1.0):
            node["scale"] = list(joint.scale)
        if joint.children:
            node["children"] = list(joint.children)
        nodes.append(node)
    nodes.append({"name": f"{name}_mesh", "mesh": 0, "skin": 0})
    roots = [index for index, joint in enumerate(rig.joints) if joint.parent is None]
    roots.append(mesh_node)

    document = {
        "asset": {
            "version": "2.0",
            "generator": generator,
            "extras": {
                "places_entity_clips": {
                    "version": 2,
                    "generator": generator,
                    "clips": clip_stats,
                }
            },
        },
        "scene": 0,
        "scenes": [{"name": name, "nodes": roots}],
        "nodes": nodes,
        "skins": [
            {
                "name": f"{name}_rig",
                "joints": list(range(joint_count)),
                "skeleton": roots[0],
                "inverseBindMatrices": ibm_accessor,
            }
        ],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {
                            "POSITION": position_accessor,
                            "TEXCOORD_0": uv_accessor,
                            "COLOR_0": color_accessor,
                            "JOINTS_0": joints_accessor,
                            "WEIGHTS_0": weights_accessor,
                        },
                        "indices": index_accessor,
                        "material": 0,
                        "mode": 4,
                    }
                ],
            }
        ],
        "materials": [
            {
                "name": name,
                "pbrMetallicRoughness": {
                    "baseColorTexture": {"index": 0},
                    "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0,
                },
            }
        ],
        "textures": [{"source": 0, "sampler": 0}],
        "images": [{"bufferView": image_view, "mimeType": "image/png"}],
        "samplers": [
            {
                "magFilter": 9729,
                "minFilter": 9987,
                "wrapS": 33071,
                "wrapT": 33071,
            }
        ],
        "accessors": accessors,
        "bufferViews": binary.views,
        "buffers": [{"byteLength": len(binary.data)}],
        "animations": animations,
    }

    json_bytes = json.dumps(document, separators=(",", ":"), sort_keys=True).encode("utf-8")
    while len(json_bytes) % 4:
        json_bytes += b" "
    bin_bytes = bytes(binary.data)
    while len(bin_bytes) % 4:
        bin_bytes += b"\x00"
    total = 12 + 8 + len(json_bytes) + 8 + len(bin_bytes)
    glb = bytearray()
    glb.extend(struct.pack("<III", GLB_MAGIC, 2, total))
    glb.extend(struct.pack("<II", len(json_bytes), CHUNK_JSON))
    glb.extend(json_bytes)
    glb.extend(struct.pack("<II", len(bin_bytes), CHUNK_BIN))
    glb.extend(bin_bytes)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(glb))
    return {
        "path": str(path.relative_to(REPO_ROOT)) if path.is_absolute() else str(path),
        "bytes": total,
        "vertices": len(mesh.positions),
        "triangles": len(mesh.indices) // 3,
        "joints": joint_count,
        "clips": clip_stats,
        "clip_channels": sum(len(animation["channels"]) for animation in animations),
        "accessors": len(accessors),
        "buffer_views": len(binary.views),
    }


# ---------------------------------------------------------------- validation


def _load_glb(path: Path) -> Tuple[dict, bytes]:
    data = path.read_bytes()
    magic, version, _length = struct.unpack_from("<III", data, 0)
    if magic != GLB_MAGIC or version != 2:
        raise RigError(f"{path} is not a GLB 2.0 file")
    offset = 12
    document: Optional[dict] = None
    binary = b""
    while offset < len(data):
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        payload = data[offset : offset + chunk_length]
        offset += chunk_length
        if chunk_type == CHUNK_JSON:
            document = json.loads(payload.decode("utf-8"))
        elif chunk_type == CHUNK_BIN:
            binary = payload
    if document is None:
        raise RigError(f"{path} has no JSON chunk")
    return document, binary


def check_model(path: Path | str) -> dict:
    """Structural checks mirroring the Rust loader's rigged-model rules."""
    path = Path(path)
    document, _binary = _load_glb(path)
    problems: List[str] = []
    nodes = document.get("nodes", [])
    skins = document.get("skins", [])
    animations = document.get("animations", [])
    accessors = document.get("accessors", [])
    if not skins:
        problems.append("no skin")
    if len(skins) > 1:
        problems.append(f"{len(skins)} skins (the engine accepts one)")
    if animations and not skins:
        problems.append("animations without a skin")
    joint_count = len(skins[0]["joints"]) if skins else 0
    if joint_count > 128:
        problems.append(f"{joint_count} joints (the engine ceiling is 128)")
    if len(animations) > 64:
        problems.append(f"{len(animations)} clips (the engine ceiling is 64)")
    channels = sum(len(animation.get("channels", [])) for animation in animations)
    if channels > 4096:
        problems.append(f"{channels} channels (the engine ceiling is 4096)")
    # Node hierarchy: acyclic, joints prior, one mesh node.
    mesh_nodes = [index for index, node in enumerate(nodes) if "mesh" in node]
    if len(mesh_nodes) != 1:
        problems.append(f"{len(mesh_nodes)} mesh nodes (the engine accepts one skinned mesh node)")
    for index, node in enumerate(nodes):
        for child in node.get("children", []):
            if child <= index:
                problems.append(f"node {index} has a non-prior child {child}")
    # Accessor sanity.
    for index, accessor in enumerate(accessors):
        if accessor.get("count", 0) <= 0:
            problems.append(f"accessor {index} has no elements")
        if accessor.get("componentType") == COMPONENT_FLOAT and "min" not in accessor:
            if accessor.get("type") == "SCALAR":
                problems.append(f"accessor {index} (animation input) lacks min/max")
    # JOINTS/WEIGHTS: read back the bytes and check index bounds and sums.
    if skins:
        for mesh in document.get("meshes", []):
            for primitive in mesh.get("primitives", []):
                attributes = primitive.get("attributes", {})
                for required in ("POSITION", "TEXCOORD_0", "JOINTS_0", "WEIGHTS_0"):
                    if required not in attributes:
                        problems.append(f"primitive lacks {required}")
                joints_accessor = accessors[attributes.get("JOINTS_0", 0)]
                weights_accessor = accessors[attributes.get("WEIGHTS_0", 0)]
                if joints_accessor.get("count") != weights_accessor.get("count"):
                    problems.append("JOINTS_0 and WEIGHTS_0 element counts differ")
    clips = []
    for animation in animations:
        name = animation.get("name", "<unnamed>")
        duration = 0.0
        for channel in animation.get("channels", []):
            sampler = animation["samplers"][channel["sampler"]]
            input_accessor = accessors[sampler["input"]]
            duration = max(
                duration,
                float(input_accessor.get("max", [0.0])[0]),
            )
            if sampler.get("interpolation") != "LINEAR":
                problems.append(f"clip {name} uses {sampler.get('interpolation')}, not LINEAR")
        clips.append({"name": name, "duration": duration, "channels": len(animation.get("channels", []))})
    marker = (
        document.get("asset", {}).get("extras", {}).get("places_entity_clips", {})
    )
    if not marker:
        problems.append("asset.extras.places_entity_clips is missing")
    else:
        marker_names = [clip.get("name") for clip in marker.get("clips", [])]
        if marker_names != [clip["name"] for clip in clips]:
            problems.append("the clip marker does not match the animations array")
    stats = {
        "path": str(path),
        "bytes": path.stat().st_size,
        "nodes": len(nodes),
        "joints": joint_count,
        "mesh_nodes": mesh_nodes,
        "clips": clips,
        "problems": problems,
    }
    return stats


# -------------------------------------------------------------------- driver


def _selftest() -> int:
    from tex import Texture

    rig = Rig()
    rig.add("root", None, (0.0, 0.5, 0.0))
    rig.add("tip", 0, (0.0, 0.5, 0.0), tail=(0.0, 0.5, 0.0))
    mesh = Mesh()
    tex = Texture(32, seed=7)
    tex.auto("body")
    grey = (180, 180, 185)
    mesh.box((0.0, 0.25, 0.0), (0.12, 0.5, 0.12), uv=tex.uv("body"), color=grey)
    skinned = SkinnedMesh(mesh)
    auto_weights(skinned, rig, sigma=0.08)
    idle = Clip("idle", loop=True, duration=1.0, kind="idle")
    idle.joint_rotation(rig, "tip", 0.0, quat_rot_x(0.0))
    idle.joint_rotation(rig, "tip", 0.5, quat_rot_x(10.0))
    idle.joint_rotation(rig, "tip", 1.0, quat_rot_x(0.0))
    out = REPO_ROOT / "target" / "entity-selftest" / "selftest.glb"
    stats = write_model(out, skinned, rig, [idle], tex.png_bytes(), name="selftest")
    checked = check_model(out)
    print(json.dumps({"stats": stats, "check": checked}, indent=2))
    if checked["problems"]:
        print("selftest FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Rigged entity GLB writer and checker")
    parser.add_argument("--selftest", action="store_true", help="write and check a tiny rig")
    parser.add_argument("--check", metavar="GLB", help="structurally check one entity GLB")
    args = parser.parse_args(argv)
    if args.selftest:
        return _selftest()
    if args.check:
        print(json.dumps(check_model(args.check), indent=2))
        return 0
    parser.print_help()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
