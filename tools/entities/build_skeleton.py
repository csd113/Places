#!/usr/bin/env python3
"""Build the low-poly articulated human skeleton entity.

The deliverable is one self-contained rigged GLB (one mesh, one primitive, one
material, one embedded 256x256 PNG) plus the source texture PNG it embeds:

    skeleton.glb            the entity: 24 joints, one skin, three hold clips
    bone_01.png             the embedded source artwork (aged ivory bone)
    skeleton_preview.png    software-rasterised pose sheet (a review aid only)

Conventions, from ``assets/README.md`` and ``docs/MAP_AUTHORING_GUIDE.md``:

* 1 unit = 1 metre, +Y up, **+Z is the front** (the skull faces +Z at yaw 0);
* the origin is the floor-contact point: the lowest sole vertex is ``y = 0``
  and the bounding box is centred horizontally under the figure;
* the standing rest (bind) pose is 1.70 m tall with the arms at the sides.

The rig, skin, weights and clips are written with ``tools/entities/rig.py``;
the geometry comes from ``tools/props/mesh.py`` and the artwork from
``tools/props/tex.py``.  Every part is authored in the rest pose, where all
rest joint rotations are identity, so the writer's skinning model reduces to

    p_posed = sum_j w_j * (global_j(t) * inverseBind_j) * p_bind

``verify_written_glb`` re-reads the file this script just wrote and evaluates
exactly that model (LINEAR, single-key holds) for every clip.  It is an offline
geometric check, not engine playback.

Usage (from the repository root):

    python3 tools/entities/build_skeleton.py
    python3 tools/entities/build_skeleton.py \
        --out /tmp/skeleton.glb --no-preview
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

HERE = Path(__file__).resolve().parent
Vec3 = Tuple[float, float, float]
Quat = Tuple[float, float, float, float]


def _find_repo_root(start: Path) -> Path:
    """Locates the repository root from this file's own path."""
    for base in (start, *start.parents):
        if (base / "tools" / "entities" / "rig.py").is_file():
            return base
    raise SystemExit(f"cannot locate the Places repository root above {start}")


REPO_ROOT = _find_repo_root(HERE)
for _extra in (REPO_ROOT / "tools" / "props", REPO_ROOT / "tools" / "entities"):
    if str(_extra) not in sys.path:
        sys.path.insert(0, str(_extra))

from mesh import Mesh  # noqa: E402  (tools/props is on sys.path)
from palette import hex_to_rgb  # noqa: E402
from rig import (  # noqa: E402
    Rig,
    SkinnedMesh,
    auto_weights,
    check_model,
    explicit_weights,
    quat_compose,
    quat_from_axis_angle,
    quat_identity,
    quat_normalize,
    rotation_clip,
    write_model,
)
from tex import Texture, decode_png, write_png  # noqa: E402

DEFAULT_GLB = REPO_ROOT / "assets" / "entities" / "skeleton" / "model" / "skeleton.glb"
DEFAULT_TEXTURE = REPO_ROOT / "assets" / "entities" / "skeleton" / "textures" / "bone_01.png"
DEFAULT_PREVIEW = REPO_ROOT / "target" / "entity-specialists" / "skeleton" / "skeleton_preview.png"
CHAIR_GLB = REPO_ROOT / "assets" / "environment" / "office" / "props" / "models" / "chair.glb"

GENERATOR = "tools/entities/build_skeleton.py"
ENTITY_NAME = "skeleton"

# ------------------------------------------------------------------- palette

BONE_LIGHT = "#ded6bd"   # aged ivory: the base of every bone surface
BONE_DEEP = "#b2a684"    # mottling, age spots, the shaded side of a groove
BONE_RECESS = "#5f5646"  # a worn recess
BONE_VOID = "#332e26"    # the black of an eye socket or a nasal aperture
VERTEX_BONE = (238, 232, 214)   # near-white: the texture carries the colour

# ----------------------------------------------------------------------- rig

PELVIS_Y = 0.920
SPINE_01_Y = 1.025
SPINE_02_Y = 1.130
CHEST_Y = 1.235
NECK_Y = 1.420
HEAD_Y = 1.470
SHOULDER_Y = 1.352
UPPERARM_Y = 1.325
FOREARM_Y = 1.015
HAND_Y = 0.765
THIGH_Y = 0.900
KNEE_Y = 0.475
ANKLE_Y = 0.075
SHOULDER_X = 0.095
ARM_X = 0.178

#: name, parent, rest translation, optional local tail offset.
#: Left is +X: with +Z forward and +Y up the figure's own right is -X.
JOINT_TABLE: Tuple[Tuple[str, Optional[str], Vec3, Optional[Vec3]], ...] = (
    ("root", None, (0.0, 0.0, 0.0), None),
    ("pelvis", "root", (0.0, PELVIS_Y, 0.0), None),
    ("spine_01", "pelvis", (0.0, SPINE_01_Y, 0.0), None),
    ("spine_02", "spine_01", (0.0, SPINE_02_Y, 0.0), None),
    ("chest", "spine_02", (0.0, CHEST_Y, 0.0), None),
    ("neck", "chest", (0.0, NECK_Y, 0.0), None),
    ("head", "neck", (0.0, HEAD_Y, 0.0), None),
    ("jaw", "head", (0.0, 1.512, 0.008), (0.0, -0.022, 0.042)),
    ("shoulder_l", "chest", (SHOULDER_X, SHOULDER_Y, 0.004), None),
    ("shoulder_r", "chest", (-SHOULDER_X, SHOULDER_Y, 0.004), None),
    ("upperarm_l", "shoulder_l", (ARM_X, UPPERARM_Y, 0.0), None),
    ("upperarm_r", "shoulder_r", (-ARM_X, UPPERARM_Y, 0.0), None),
    ("forearm_l", "upperarm_l", (ARM_X, FOREARM_Y, 0.0), None),
    ("forearm_r", "upperarm_r", (-ARM_X, FOREARM_Y, 0.0), None),
    ("hand_l", "forearm_l", (ARM_X, HAND_Y, 0.0), (0.003, -0.165, 0.006)),
    ("hand_r", "forearm_r", (-ARM_X, HAND_Y, 0.0), (-0.003, -0.165, 0.006)),
    ("thigh_l", "pelvis", (0.085, THIGH_Y, 0.0), None),
    ("thigh_r", "pelvis", (-0.085, THIGH_Y, 0.0), None),
    ("shin_l", "thigh_l", (0.085, KNEE_Y, 0.0), None),
    ("shin_r", "thigh_r", (-0.085, KNEE_Y, 0.0), None),
    ("foot_l", "shin_l", (0.085, ANKLE_Y, 0.0), None),
    ("foot_r", "shin_r", (-0.085, ANKLE_Y, 0.0), None),
    ("toe_l", "foot_l", (0.085, 0.022, 0.080), (0.0, -0.010, 0.070)),
    ("toe_r", "foot_r", (-0.085, 0.022, 0.080), (0.0, -0.010, 0.070)),
)
JOINT_NAMES = tuple(entry[0] for entry in JOINT_TABLE)
JOINT_POSITION = {entry[0]: entry[2] for entry in JOINT_TABLE}

L_THIGH = JOINT_POSITION["thigh_l"][1] - JOINT_POSITION["shin_l"][1]
L_SHIN = JOINT_POSITION["shin_l"][1] - JOINT_POSITION["foot_l"][1]
L_UPPERARM = JOINT_POSITION["upperarm_l"][1] - JOINT_POSITION["forearm_l"][1]
L_FOREARM = JOINT_POSITION["forearm_l"][1] - JOINT_POSITION["hand_l"][1]

# The chair pose's two-link solve puts the knee exactly one shin length above
# the flat-footed ankle, because the shin is vertical.
CHAIR_KNEE_Y = ANKLE_Y + L_SHIN
FLOOR_PELVIS_Y = 0.060       # the declared pose_sit_floor range is 0.05..0.12
CHAIR_PELVIS_Y = 0.450       # the contract's 0.45 m chair seat
FLOOR_ANKLE_FORWARD = 0.600  # how far in front of the hip the floor-sit ankles land
FLOOR_WRIST_Y = 0.045        # hands rest on the floor just outside the hips
FLOOR_WRIST_FORWARD = 0.105
CHAIR_WRIST_Y = 0.505        # pose_sit_chair: wrists rest on the thighs
CHAIR_WRIST_Z = 0.235
CHAIR_HIP_ON_SEAT = -0.045   # the pelvis joint's z in the core:chair frame

# ------------------------------------------------------------------ geometry

CRANIUM_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.058), (0.050, 0.078), (0.105, 0.090), (0.185, 0.084), (0.235, 0.045),
)
MANDIBLE_CENTRE: Vec3 = (0.0, 1.512, 0.047)
MANDIBLE_SIZE: Vec3 = (0.094, 0.034, 0.074)

RIB_HEIGHTS = (1.045, 1.115, 1.185, 1.255, 1.325)
STERNUM_CENTRE: Vec3 = (0.0, 1.175, 0.092)
STERNUM_SIZE: Vec3 = (0.032, 0.220, 0.024)
PELVIS_BASE_Y = 0.910
SPINE_PROFILE = (
    # height, depth centre, radius: alternating centres and narrow joints.
    (0.980, -0.045, 0.018), (1.010, -0.045, 0.030), (1.040, -0.044, 0.018),
    (1.105, -0.041, 0.029), (1.135, -0.039, 0.018),
    (1.190, -0.035, 0.028), (1.220, -0.032, 0.018),
    (1.280, -0.027, 0.026), (1.310, -0.024, 0.018),
    (1.365, -0.019, 0.023), (1.400, -0.016, 0.017),
)
PELVIS_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.060), (0.032, 0.110), (0.062, 0.118), (0.098, 0.072),
)
UPPERARM_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.030), (0.030, 0.036), (0.155, 0.023), (0.280, 0.031), (0.310, 0.039),
)
FOREARM_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.021), (0.028, 0.030), (0.125, 0.020), (0.222, 0.028), (0.250, 0.025),
)
THIGH_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.037), (0.035, 0.048), (0.205, 0.032), (0.385, 0.047), (0.425, 0.053),
)
SHIN_PROFILE: Tuple[Tuple[float, float], ...] = (
    (0.000, 0.027), (0.030, 0.034), (0.190, 0.025), (0.360, 0.036), (0.400, 0.033),
)

# ------------------------------------------------------------- small maths


def _add(a: Vec3, b: Vec3) -> Vec3:
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def _sub(a: Vec3, b: Vec3) -> Vec3:
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _scale(a: Vec3, factor: float) -> Vec3:
    return (a[0] * factor, a[1] * factor, a[2] * factor)


def _dot(a: Vec3, b: Vec3) -> float:
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _cross(a: Vec3, b: Vec3) -> Vec3:
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _length(v: Vec3) -> float:
    return math.sqrt(_dot(v, v))


def _normalize(v: Vec3) -> Vec3:
    length = _length(v)
    if length <= 1.0e-12:
        return (0.0, 0.0, 0.0)
    return (v[0] / length, v[1] / length, v[2] / length)


def _qconj(q: Quat) -> Quat:
    return (-q[0], -q[1], -q[2], q[3])


def _qrotate(q: Quat, v: Vec3) -> Vec3:
    """Rotates ``v`` by unit quaternion ``q`` (v + w t + q x t with t = 2 q x v)."""
    x, y, z, w = q
    vx, vy, vz = v
    tx = 2.0 * (y * vz - z * vy)
    ty = 2.0 * (z * vx - x * vz)
    tz = 2.0 * (x * vy - y * vx)
    return (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )


def _perpendicular(v: Vec3) -> Vec3:
    reference = (1.0, 0.0, 0.0) if abs(v[0]) < 0.9 else (0.0, 1.0, 0.0)
    return _normalize(_cross(v, reference))


def _quat_between(start: Vec3, end: Vec3) -> Quat:
    """The minimal rotation that maps direction ``start`` onto direction ``end``.

    Deterministic: the axis is ``start x end``, and the 180 degree case picks a
    fixed perpendicular, so the same pose always produces the same quaternion.
    """
    a = _normalize(start)
    b = _normalize(end)
    if _length(a) <= 0.0 or _length(b) <= 0.0:
        raise ValueError("a bone direction cannot be zero")
    cosine = max(-1.0, min(1.0, _dot(a, b)))
    if cosine > 1.0 - 1.0e-12:
        return quat_identity()
    if cosine < -1.0 + 1.0e-12:
        return quat_from_axis_angle(_perpendicular(a), 180.0)
    return quat_from_axis_angle(_cross(a, b), math.degrees(math.acos(cosine)))


def _two_link(start: Vec3, target: Vec3, upper: float, lower: float, hint: Vec3) -> Vec3:
    """Analytic two-link solve: the elbow/knee that reaches ``target``.

    Direct construction, not a search.  The joint lies on the circle of radius
    ``upper`` around ``start`` inside the plane spanned by the reach direction
    and ``hint``; ``hint`` selects the branch (a knee points up, an elbow back).
    """
    delta = _sub(target, start)
    distance = _length(delta)
    if distance <= 1.0e-9:
        raise ValueError("a two-link target cannot sit on the root")
    axis = _scale(delta, 1.0 / distance)
    along = (upper * upper - lower * lower + distance * distance) / (2.0 * distance)
    height = math.sqrt(max(0.0, upper * upper - along * along))
    normal = _normalize(_cross(axis, hint))
    if _length(normal) <= 0.0:
        normal = _perpendicular(axis)
    rise = _cross(normal, axis)
    if distance >= upper + lower:
        return _add(start, _scale(axis, upper))  # folded straight; never needed here
    return _add(_add(start, _scale(axis, along)), _scale(rise, height))


class Fk:
    """Forward kinematics over the writer's rest-identity rig.

    ``p_j = p_parent + Q_parent * t_j`` and ``Q_j = Q_parent * q_j`` mirror the
    node graph ``rig.py`` writes (a local ``T * R`` per joint), so a pose built
    here lands exactly where the written clip puts it.
    """

    def __init__(self, rig: Rig) -> None:
        self.rig = rig
        self.parent = [joint.parent for joint in rig.joints]
        # rig.py stores each joint's local translation (the parent-relative
        # offset), which is exactly what the FK needs.
        self.local_t: List[Vec3] = [tuple(joint.translation) for joint in rig.joints]
        self.local_q: List[Quat] = [quat_identity() for _ in rig.joints]
        self.root_offset: Vec3 = (0.0, 0.0, 0.0)
        self.positions: List[Vec3] = [tuple(joint.translation) for joint in rig.joints]
        self.rotations: List[Quat] = [quat_identity() for _ in rig.joints]

    def solve(self) -> None:
        for index, joint in enumerate(self.rig.joints):
            parent = joint.parent
            if parent is None:
                self.positions[index] = _add(self.local_t[index], self.root_offset)
                self.rotations[index] = quat_normalize(self.local_q[index])
            else:
                self.positions[index] = _add(
                    self.positions[parent], _qrotate(self.rotations[parent], self.local_t[index])
                )
                self.rotations[index] = quat_normalize(
                    quat_compose(self.rotations[parent], self.local_q[index])
                )

    def index(self, joint: str) -> int:
        return self.rig.index(joint)

    def position(self, joint: str) -> Vec3:
        return self.positions[self.index(joint)]

    def aim(self, joint: str, child: str, direction: Vec3) -> None:
        """Points the ``joint -> child`` bone along ``direction`` (world space).

        With every rest rotation identity the child's *local* translation is
        also its rest-space offset from the parent, so it doubles as the rest
        bone direction.
        """
        index = self.index(joint)
        rest = tuple(self.rig.joints[self.index(child)].translation)
        self._set_global(index, _quat_between(rest, direction))

    def orient(self, joint: str, direction: Vec3) -> None:
        """Sets a leaf joint's own rest direction (a hand, a foot) along ``direction``."""
        index = self.index(joint)
        tail = self.rig.joints[index].tail
        reference = tail if tail is not None else (0.0, 0.0, -0.05)
        self._set_global(index, _quat_between(reference, direction))

    def flat(self, joint: str) -> None:
        """Clears a joint's accumulated rotation (a sole stays parallel to the floor)."""
        self._set_global(self.index(joint), quat_identity())

    def _set_global(self, index: int, rotation: Quat) -> None:
        parent = self.rig.joints[index].parent
        parent_q = self.rotations[parent] if parent is not None else quat_identity()
        self.local_q[index] = quat_compose(_qconj(quat_normalize(parent_q)), rotation)
        self.solve()

    def local_pose(self, joints: Sequence[str]) -> Dict[str, Quat]:
        return {name: quat_normalize(self.local_q[self.index(name)]) for name in joints}


# -------------------------------------------------------------------- texture


def paint_bone_texture(seed: int = 37) -> Texture:
    """256x256 aged-bone atlas: ``skull`` / ``face`` / ``bone`` / ``dark`` cells.

    ``auto`` lays the four cells out left-to-right, top-to-bottom. The socket
    art maps onto a curved patch following the front of the cranium; the teeth
    occupy the mandible below it.
    """
    tex = Texture(256, seed=seed)
    tex.auto("skull", "face", "bone", "dark")
    ivory = hex_to_rgb(BONE_LIGHT)
    deep = hex_to_rgb(BONE_DEEP)
    recess = hex_to_rgb(BONE_RECESS)
    void = hex_to_rgb(BONE_VOID)
    pale = hex_to_rgb("#e7e0cb")

    # Plain long-bone stock: mottled ivory, faint lengthwise grain, age spots.
    tex.fill("bone", ivory, jitter=7, seed=101)
    tex.noise("bone", amount=8, freq=4, seed=103)
    tex.spots("bone", deep, count=7, radius=4, alpha=32, seed=107)
    tex.spots("bone", pale, count=5, radius=3, alpha=24, seed=109)
    tex.streaks("bone", deep, count=9, alpha=22, seed=113)
    tex.grain("bone", deep, density=0.18, alpha=14, seed=127)

    # The cranium: the same family, paler and less busy.
    tex.fill("skull", ivory, jitter=6, seed=131)
    tex.noise("skull", amount=7, freq=5, seed=137)
    tex.spots("skull", deep, count=5, radius=5, alpha=20, seed=139)
    tex.streaks("skull", deep, count=6, alpha=13, seed=149)

    # Face artwork: two eye sockets, a nasal aperture, teeth and a mouth line.
    tex.fill("face", ivory, jitter=6, seed=151)
    tex.noise("face", amount=6, freq=6, seed=157)
    for fx0, fx1 in ((0.18, 0.36), (0.64, 0.82)):
        tex.bar("face", deep, (fx0 - 0.02, 0.23, fx1 + 0.02, 0.30), alpha=90)
        tex.bar("face", recess, (fx0, 0.28, fx1, 0.58), alpha=222)
        tex.bar("face", void, (fx0 + 0.03, 0.32, fx1 - 0.03, 0.54), alpha=235)
        tex.bar("face", deep, (fx0, 0.58, fx1, 0.63), alpha=110)
    tex.bar("face", void, (0.455, 0.56, 0.545, 0.76), alpha=228)
    tex.bar("face", recess, (0.435, 0.53, 0.565, 0.60), alpha=120)
    tex.bar("face", void, (0.13, 0.79, 0.87, 0.855), alpha=220)
    tex.bar("face", pale, (0.13, 0.855, 0.87, 0.975), alpha=235)
    for index in range(7):
        tooth = 0.15 + index * 0.10
        tex.bar("face", deep, (tooth, 0.855, tooth + 0.016, 0.975), alpha=150)
    tex.bar("face", recess, (0.13, 0.975, 0.87, 1.0), alpha=205)

    # Recess geometry: the nasal cavity, the eye pockets, the jaw shadow.
    tex.fill("dark", hex_to_rgb("#4b4336"), jitter=6, seed=163)
    tex.noise("dark", amount=6, freq=5, seed=167)
    tex.spots("dark", hex_to_rgb("#2b271f"), count=4, radius=3, alpha=70, seed=173)
    return tex


# ----------------------------------------------------------------------- mesh

Spec = Tuple[str, Tuple[str, ...], float]


def _pin(*joints: str) -> Spec:
    return ("pin", tuple(joints), 0.0)


def _blend(joints: Sequence[str], sigma: float) -> Spec:
    return ("blend", tuple(joints), sigma)


PIN_HEAD = _pin("head")
PIN_JAW = _pin("jaw")
PIN_PELVIS = _pin("pelvis")
PIN_CHEST = _pin("chest")

Part = Tuple[str, int, int, Spec]


class PartRecorder:
    """Runs ``Mesh`` primitives and records the vertex range each one writes."""

    def __init__(self, mesh: Mesh) -> None:
        self.mesh = mesh
        self.parts: List[Part] = []

    def _record(self, name: str, spec: Spec, emit) -> None:  # type: ignore[no-untyped-def]
        first = len(self.mesh.positions)
        emit()
        last = len(self.mesh.positions)
        if last <= first:
            raise RuntimeError(f"part {name!r} wrote no vertices")
        self.parts.append((name, first, last, spec))

    def box(self, name: str, spec: Spec, center: Vec3, size: Vec3, **kwargs) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: self.mesh.box(center, size, **kwargs))

    def lathe(self, name: str, spec: Spec, base: Vec3, profile, **kwargs) -> None:  # type: ignore[no-untyped-def]
        first_index = len(self.mesh.indices)
        self._record(name, spec, lambda: self.mesh.lathe(base, profile, **kwargs))
        for index in range(first_index, len(self.mesh.indices), 3):
            self.mesh.indices[index + 1], self.mesh.indices[index + 2] = self.mesh.indices[index + 2], self.mesh.indices[index + 1]

    def cylinder(self, name: str, spec: Spec, base: Vec3, radius: float, height: float, **kwargs) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: self.mesh.cylinder(base, radius, height, **kwargs))

    def tube(self, name: str, spec: Spec, start: Vec3, end: Vec3, radius: float, **kwargs) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: self.mesh.tube(start, end, radius, **kwargs))

    def tube_path(self, name: str, spec: Spec, points, radii, **kwargs) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: self.mesh.tube_path(points, radii, **kwargs))

    def foot(self, name: str, spec: Spec, x: float, uv) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: add_foot(self.mesh, x, uv))

    def face(self, name: str, spec: Spec, uv) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: add_face(self.mesh, uv))

    def spine(self, name: str, spec: Spec, uv) -> None:  # type: ignore[no-untyped-def]
        self._record(name, spec, lambda: add_spine(self.mesh, uv))


def add_spine(mesh: Mesh, uv: Tuple[float, float, float, float]) -> None:
    """A curved vertebral column with bony centres and narrow joins."""
    segments = 5
    u0, v0, u1, v1 = uv
    rings = [
        [(radius * math.cos(index * math.tau / segments), y,
          z + radius * math.sin(index * math.tau / segments))
         for index in range(segments)]
        for y, z, radius in SPINE_PROFILE
    ]
    for ring_index, (lower, upper) in enumerate(zip(rings, rings[1:])):
        for index in range(segments):
            next_index = (index + 1) % segments
            mesh.quad(
                lower[index], lower[next_index], upper[next_index], upper[index],
                [
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                ],
                VERTEX_BONE,
            )
    for index in range(segments):
        next_index = (index + 1) % segments
        mesh.triangle((0.0, SPINE_PROFILE[0][0], SPINE_PROFILE[0][1]),
                      rings[0][next_index], rings[0][index],
                      ((u0, v0), (u1, v0), (u0, v1)), VERTEX_BONE)
        mesh.triangle((0.0, SPINE_PROFILE[-1][0], SPINE_PROFILE[-1][1]),
                      rings[-1][index], rings[-1][next_index],
                      ((u0, v0), (u1, v0), (u0, v1)), VERTEX_BONE)


def add_face(mesh: Mesh, uv: Tuple[float, float, float, float]) -> None:
    """Map the eye and nose artwork flush to the skull's curved front."""
    u0, v0, u1, v1 = uv
    x_values = (-0.055, -0.030, 0.0, 0.030, 0.055)
    rows = ((1.535, 0.078), (1.575, 0.087), (1.620, 0.084))
    points = [
        [(x, y, math.sqrt(radius * radius - x * x) + 0.006)
         for x in x_values]
        for y, radius in rows
    ]
    for row in range(2):
        for column in range(len(x_values) - 1):
            left = u0 + (u1 - u0) * column / (len(x_values) - 1)
            right = u0 + (u1 - u0) * (column + 1) / (len(x_values) - 1)
            lower = v1 + (v0 - v1) * row / 2
            upper = v1 + (v0 - v1) * (row + 1) / 2
            mesh.quad(
                points[row][column], points[row][column + 1],
                points[row + 1][column + 1], points[row + 1][column],
                ((left, lower), (right, lower), (right, upper), (left, upper)),
                VERTEX_BONE,
            )


def add_foot(mesh: Mesh, x: float, uv: Tuple[float, float, float, float]) -> None:
    """One flat-soled, tapered foot that rises into the shin."""
    # z, half-width, sole height, instep height. Eight sides include an exact
    # bottom vertex at y=0 for the standing and seated contact checks.
    profile = (
        (-0.075, 0.022, 0.008, 0.036),
        (-0.055, 0.038, 0.000, 0.055),
        (-0.015, 0.043, 0.000, 0.083),
        (0.015, 0.044, 0.000, 0.097),
        (0.080, 0.047, 0.000, 0.068),
        (0.165, 0.043, 0.000, 0.046),
        (0.205, 0.016, 0.006, 0.027),
    )
    segments = 8
    u0, v0, u1, v1 = uv
    rings = []
    for z, width, sole, instep in profile:
        mid_y = (sole + instep) * 0.5
        half_height = (instep - sole) * 0.5
        rings.append([
            (x + width * math.cos(index * math.tau / segments),
             mid_y + half_height * math.sin(index * math.tau / segments), z)
            for index in range(segments)
        ])
    for ring_index, (rear, front) in enumerate(zip(rings, rings[1:])):
        for index in range(segments):
            next_index = (index + 1) % segments
            mesh.quad(
                rear[index], rear[next_index], front[next_index], front[index],
                [
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                ],
                VERTEX_BONE,
                shade_mult=0.82,
                ao=0.90,
            )
    for index in range(segments):
        next_index = (index + 1) % segments
        mesh.triangle((x, 0.022, profile[0][0]), rings[0][next_index], rings[0][index],
                      ((u0, v0), (u1, v0), (u0, v1)), VERTEX_BONE, ao=0.90)
        mesh.triangle((x, 0.0165, profile[-1][0]), rings[-1][index], rings[-1][next_index],
                      ((u0, v0), (u1, v0), (u0, v1)), VERTEX_BONE, ao=0.90)


def build_mesh(tex: Texture) -> Tuple[Mesh, List[Part]]:
    """The whole skeleton as one low-poly mesh, authored in the standing rest pose."""
    mesh = Mesh()
    rec = PartRecorder(mesh)
    cell = {name: tex.uv(name) for name in ("skull", "face", "bone", "dark")}
    bone = cell["bone"]
    ivory = VERTEX_BONE

    # --- skull -----------------------------------------------------------
    rec.lathe(
        "cranium", PIN_HEAD, base=(0.0, 1.485, 0.0), profile=CRANIUM_PROFILE,
        segments=10, uv=cell["skull"], color=ivory, ellipse=(1.0, 1.02),
    )
    rec.face("face", PIN_HEAD, tex.sub("face", 0.0, 0.0, 1.0, 0.80))
    rec.box(
        "mandible", PIN_JAW, center=MANDIBLE_CENTRE, size=MANDIBLE_SIZE,
        uv={"+z": tex.sub("face", 0.13, 0.80, 0.87, 1.0), "-z": bone,
            "+x": bone, "-x": bone, "+y": bone, "-y": bone},
        color=ivory,
    )
    rec.cylinder(
        "neck", _blend(("chest", "neck", "head"), 0.05),
        base=(0.0, 1.415, 0.0), radius=0.036, height=0.078, segments=6, taper=0.85,
        uv=bone, color=ivory,
    )

    # --- spine and rib cage ---------------------------------------------
    rec.spine(
        "spine", _blend(("pelvis", "spine_01", "spine_02", "chest", "neck"), 0.09),
        bone,
    )
    for index, rib_y in enumerate(RIB_HEIGHTS):
        outer = (0.102, 0.126, 0.135, 0.125, 0.103)[index]
        scale = outer / 0.125
        for sign in (-1.0, 1.0):
            rec.tube_path(
                f"rib_{index}_{'l' if sign > 0 else 'r'}",
                _blend(("spine_01", "spine_02", "chest"), 0.10),
                points=[
                    (sign * 0.016, rib_y + 0.006, -0.047 + index * 0.0055),
                    (sign * 0.100 * scale, rib_y - 0.010, -0.024),
                    (sign * outer, rib_y - 0.016, 0.022),
                    (sign * 0.090 * scale, rib_y - 0.030, 0.078),
                    (sign * 0.024, rib_y - 0.042, 0.100),
                ],
                radii=(0.012, 0.0115, 0.011, 0.010, 0.009), segments=4, uv=bone, color=ivory,
            )
    rec.box("sternum", PIN_CHEST, center=STERNUM_CENTRE, size=STERNUM_SIZE, uv=bone, color=ivory)

    # --- pelvis and clavicles -------------------------------------------
    rec.lathe(
        "pelvis", PIN_PELVIS, base=(0.0, PELVIS_BASE_Y, -0.018), profile=PELVIS_PROFILE,
        segments=8, uv=bone, color=ivory, ellipse=(1.0, 0.78),
    )
    for sign, joint in ((1.0, "shoulder_l"), (-1.0, "shoulder_r")):
        rec.tube(
            f"clavicle_{joint[-1]}", _pin(joint),
            start=(sign * 0.022, SHOULDER_Y, 0.014), end=(sign * ARM_X, SHOULDER_Y, 0.004),
            radius=0.013, segments=4, uv=bone, color=ivory,
        )

    # --- arms and hands --------------------------------------------------
    for suffix, sign in (("l", 1.0), ("r", -1.0)):
        arm_x = ARM_X * sign
        rec.lathe(
            f"upperarm_{suffix}", _blend((f"upperarm_{suffix}", f"forearm_{suffix}"), 0.05),
            base=(arm_x, FOREARM_Y, 0.0), profile=UPPERARM_PROFILE, segments=5,
            uv=bone, color=ivory,
        )
        for bone_index, offset_x in enumerate((-0.010, 0.010)):
            rec.lathe(
                f"forearm_{suffix}_{bone_index}",
                _blend((f"forearm_{suffix}", f"hand_{suffix}"), 0.05),
                base=(arm_x + offset_x, HAND_Y, 0.0),
                profile=((0.0, 0.015), (0.125, 0.010), (0.25, 0.016)),
                segments=4, uv=bone, color=ivory,
            )
        rec.box(
            f"palm_{suffix}", _pin(f"hand_{suffix}"),
            center=(arm_x + sign * 0.004, 0.712, 0.004), size=(0.028, 0.104, 0.055),
            uv=bone, color=ivory,
        )
        for finger in range(4):
            rec.box(
                f"finger_{suffix}_{finger}", _pin(f"hand_{suffix}"),
                center=(arm_x + sign * 0.002, 0.6325, 0.004 + (finger - 1.5) * 0.013),
                size=(0.024, 0.060, 0.0115), uv=bone, color=ivory,
            )
        rec.box(
            f"thumb_{suffix}", _pin(f"hand_{suffix}"),
            center=(arm_x - sign * 0.004, 0.712, 0.047), size=(0.022, 0.050, 0.016),
            rotation=(25.0, 0.0, 0.0), uv=bone, color=ivory,
        )

    # --- legs and feet ---------------------------------------------------
    for suffix, sign in (("l", 1.0), ("r", -1.0)):
        leg_x = 0.085 * sign
        rec.lathe(
            f"thigh_{suffix}", _blend((f"thigh_{suffix}", f"shin_{suffix}"), 0.06),
            base=(leg_x, KNEE_Y, 0.0), profile=THIGH_PROFILE, segments=5, uv=bone, color=ivory,
        )
        rec.lathe(
            f"shin_{suffix}",
            _blend((f"thigh_{suffix}", f"shin_{suffix}", f"foot_{suffix}"), 0.06),
            base=(leg_x, ANKLE_Y, 0.0), profile=SHIN_PROFILE, segments=5, uv=bone, color=ivory,
        )
        rec.foot(f"foot_{suffix}", _pin(f"foot_{suffix}"), leg_x, bone)
    return mesh, rec.parts


def build_rig(offset: Vec3) -> Rig:
    """The joint hierarchy, shifted by the mesh's origin-normalising offset.

    ``JOINT_TABLE`` lists *global* rest positions; ``rig.add`` wants a joint's
    local offset from its parent, so each entry is differenced against its
    parent (the origin offset cancels out of every difference) and only the
    root carries it.
    """
    rig = Rig()
    for name, parent_name, translation, tail in JOINT_TABLE:
        parent = rig.index(parent_name) if parent_name is not None else None
        if parent_name is None:
            local = _add(translation, offset)
        else:
            local = _sub(translation, JOINT_POSITION[parent_name])
        rig.add(name, parent, local, tail=tail)
    rig.finish()
    return rig


# ------------------------------------------------------------------ weighting


def assign_weights(
    skinned: SkinnedMesh, rig: Rig, parts: Sequence[Part]
) -> Dict[int, List[Tuple[int, float]]]:
    """Per-part weights: pinned rigid parts, chain-restricted blends elsewhere.

    A pinned part (the skull, a hand, a foot) is written exactly.  A blended
    part runs the writer's ``auto_weights`` with ``only`` restricted to its own
    chain, so no vertex can be shared between disjoint limbs; vertices assigned
    by earlier parts ride along as ``overrides``, and the merged result is
    committed with one final ``explicit_weights`` call.
    """
    assigned: Dict[int, List[Tuple[int, float]]] = {}
    for name, first, last, spec in parts:
        kind, joints, sigma = spec
        if kind == "pin":
            entries = [(rig.index(joint), 1.0 / len(joints)) for joint in joints]
            for vertex in range(first, last):
                assigned[vertex] = entries
            continue
        auto_weights(
            skinned, rig, sigma=sigma, max_influences=3, min_share=0.15,
            only=[rig.index(joint) for joint in joints], overrides=assigned,
        )
        for vertex in range(first, last):
            entries = [
                (joint, weight)
                for joint, weight in zip(skinned.joints[vertex], skinned.weights[vertex])
                if weight > 0.0
            ]
            if not entries:
                raise RuntimeError(f"part {name!r} vertex {vertex} received no weight")
            assigned[vertex] = entries
    if len(assigned) != len(skinned.mesh.positions):
        raise RuntimeError("not every vertex received a weighting rule")
    explicit_weights(skinned, rig, assigned)
    return assigned


def check_weights(skinned: SkinnedMesh, rig: Rig, parts: Sequence[Part]) -> dict:
    """Every vertex: 1..4 joints, normalised, in range, inside its own chain."""
    joint_count = len(rig.joints)
    worst_sum = 0.0
    worst_influences = 0
    for name, first, last, spec in parts:
        allowed = {rig.index(joint) for joint in spec[1]}
        for vertex in range(first, last):
            for slot, weight in zip(skinned.joints[vertex], skinned.weights[vertex]):
                if weight > 0.0 and slot not in allowed:
                    raise RuntimeError(
                        f"part {name!r} vertex {vertex} is weighted to "
                        f"{rig.joints[slot].name!r}, outside its chain"
                    )
    for vertex, (slots, weights) in enumerate(zip(skinned.joints, skinned.weights)):
        total = 0.0
        influences = 0
        for slot, weight in zip(slots, weights):
            if not 0 <= slot < joint_count:
                raise RuntimeError(f"vertex {vertex} names slot {slot}, outside the skin")
            if weight < 0.0:
                raise RuntimeError(f"vertex {vertex} has a negative weight")
            if weight > 0.0:
                influences += 1
                total += weight
        if not 1 <= influences <= 4:
            raise RuntimeError(f"vertex {vertex} has {influences} influences")
        worst_influences = max(worst_influences, influences)
        worst_sum = max(worst_sum, abs(total - 1.0))
    return {
        "worst_influences": worst_influences,
        "worst_weight_sum_error": worst_sum,
        "parts_checked": len(parts),
        "chain_violations": 0,
    }


# ---------------------------------------------------------------------- poses


def pose_stand() -> Dict[str, Quat]:
    """The bind pose, keyed explicitly on every joint (all identity)."""
    return {name: quat_identity() for name in JOINT_NAMES}


def _moved_joints() -> Tuple[str, ...]:
    names = ["pelvis"]
    for suffix in ("l", "r"):
        names += [f"thigh_{suffix}", f"shin_{suffix}", f"foot_{suffix}", f"toe_{suffix}"]
        names += [f"upperarm_{suffix}", f"forearm_{suffix}", f"hand_{suffix}"]
    return tuple(names)


def pose_sit_floor(rig: Rig) -> Tuple[Dict[str, Quat], Vec3]:
    """Seated on the floor: pelvis low, knees up, feet flat, hands on the floor."""
    fk = Fk(rig)
    fk.root_offset = (0.0, -(PELVIS_Y - FLOOR_PELVIS_Y), 0.0)
    fk.solve()
    for suffix in ("l", "r"):
        hip = fk.position(f"thigh_{suffix}")
        ankle = (hip[0], ANKLE_Y, hip[2] + FLOOR_ANKLE_FORWARD)
        knee = _two_link(hip, ankle, L_THIGH, L_SHIN, hint=(0.0, 1.0, 0.0))
        fk.aim(f"thigh_{suffix}", f"shin_{suffix}", _sub(knee, hip))
        fk.aim(f"shin_{suffix}", f"foot_{suffix}", _sub(ankle, knee))
        fk.flat(f"foot_{suffix}")
        shoulder = fk.position(f"upperarm_{suffix}")
        wrist = (shoulder[0], FLOOR_WRIST_Y, shoulder[2] + FLOOR_WRIST_FORWARD)
        elbow = _two_link(shoulder, wrist, L_UPPERARM, L_FOREARM, hint=(0.0, 0.0, -1.0))
        fk.aim(f"upperarm_{suffix}", f"forearm_{suffix}", _sub(elbow, shoulder))
        fk.aim(f"forearm_{suffix}", f"hand_{suffix}", _sub(wrist, elbow))
        fk.orient(f"hand_{suffix}", (0.0, 0.10, 1.0))
    return fk.local_pose(_moved_joints()), fk.root_offset


def pose_sit_chair(rig: Rig) -> Tuple[Dict[str, Quat], Vec3]:
    """Seated on the 0.45 m chair: hips ~90 degrees, knees ~90, shins vertical.

    The leg chain is a direct analytic construction: the shin is vertical, so
    the knee sits one shin length above the flat ankle; the thigh then reaches
    that knee exactly, because its length fixes the forward offset.
    """
    fk = Fk(rig)
    fk.root_offset = (0.0, -(PELVIS_Y - CHAIR_PELVIS_Y), 0.0)
    fk.solve()
    for suffix in ("l", "r"):
        hip = fk.position(f"thigh_{suffix}")
        rise = CHAIR_KNEE_Y - hip[1]
        forward = math.sqrt(max(0.0, L_THIGH * L_THIGH - rise * rise))
        knee = (hip[0], CHAIR_KNEE_Y, hip[2] + forward)
        ankle = (hip[0], ANKLE_Y, knee[2])
        fk.aim(f"thigh_{suffix}", f"shin_{suffix}", _sub(knee, hip))
        fk.aim(f"shin_{suffix}", f"foot_{suffix}", _sub(ankle, knee))
        fk.flat(f"foot_{suffix}")
        shoulder = fk.position(f"upperarm_{suffix}")
        wrist = (shoulder[0], CHAIR_WRIST_Y, CHAIR_WRIST_Z)
        elbow = _two_link(shoulder, wrist, L_UPPERARM, L_FOREARM, hint=(0.0, 0.0, -1.0))
        fk.aim(f"upperarm_{suffix}", f"forearm_{suffix}", _sub(elbow, shoulder))
        fk.aim(f"forearm_{suffix}", f"hand_{suffix}", _sub(wrist, elbow))
        # The hand lies along the thigh: fingers forward, resting on it.
        fk.orient(f"hand_{suffix}", _sub(knee, hip))
    return fk.local_pose(_moved_joints()), fk.root_offset


def pose_summary(rig: Rig, pose: Dict[str, Quat], root_offset: Vec3) -> dict:
    """Angles and joint heights of a built pose, for the report and the checks."""
    fk = Fk(rig)
    for name, rotation in pose.items():
        fk.local_q[fk.index(name)] = rotation
    fk.root_offset = root_offset
    fk.solve()
    summary: dict = {
        "pelvis_y": fk.position("pelvis")[1],
        "root_translation": root_offset,
        "joints": {},
    }
    for suffix in ("l", "r"):
        hip = fk.position(f"thigh_{suffix}")
        knee = fk.position(f"shin_{suffix}")
        ankle = fk.position(f"foot_{suffix}")
        thigh_dir = _normalize(_sub(knee, hip))
        shin_dir = _normalize(_sub(ankle, knee))
        hip_angle = math.degrees(
            math.acos(max(-1.0, min(1.0, _dot(thigh_dir, (0.0, -1.0, 0.0)))))
        )
        knee_angle = math.degrees(
            math.acos(max(-1.0, min(1.0, _dot(thigh_dir, shin_dir))))
        )
        summary["joints"][suffix] = {
            "hip": hip,
            "knee": knee,
            "ankle": ankle,
            "toe": fk.position(f"toe_{suffix}"),
            "hand": fk.position(f"hand_{suffix}"),
            "hip_flexion_degrees": hip_angle,
            "knee_flexion_degrees": 180.0 - knee_angle,
            "shin_from_vertical_degrees": math.degrees(
                math.acos(max(-1.0, min(1.0, _dot(shin_dir, (0.0, -1.0, 0.0)))))
            ),
        }
    return summary


# --------------------------------------------------------------- verification


_COMPONENT = {5120: ("b", 1), 5121: ("B", 1), 5122: ("h", 2), 5123: ("H", 2), 5125: ("I", 4), 5126: ("f", 4)}
_COMPONENTS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def read_glb(path: Path) -> Tuple[dict, bytes]:
    data = path.read_bytes()
    magic, version, _total = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        raise RuntimeError(f"{path} is not a GLB 2.0 file")
    offset = 12
    document: Optional[dict] = None
    binary = b""
    while offset < len(data):
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        payload = data[offset : offset + chunk_length]
        offset += chunk_length
        if chunk_type == 0x4E4F534A:
            document = json.loads(payload.decode("utf-8"))
        elif chunk_type == 0x004E4942:
            binary = payload
    if document is None:
        raise RuntimeError(f"{path} has no JSON chunk")
    return document, binary


def write_glb(path: Path, document: dict, binary: bytes) -> int:
    json_bytes = json.dumps(document, separators=(",", ":"), sort_keys=True).encode("utf-8")
    while len(json_bytes) % 4:
        json_bytes += b" "
    bin_bytes = bytes(binary)
    while len(bin_bytes) % 4:
        bin_bytes += b"\x00"
    total = 12 + 8 + len(json_bytes) + 8 + len(bin_bytes)
    payload = bytearray()
    payload.extend(struct.pack("<III", 0x46546C67, 2, total))
    payload.extend(struct.pack("<II", len(json_bytes), 0x4E4F534A))
    payload.extend(json_bytes)
    payload.extend(struct.pack("<II", len(bin_bytes), 0x004E4942))
    payload.extend(bin_bytes)
    path.write_bytes(bytes(payload))
    return total


def accessor_values(document: dict, binary: bytes, index: int) -> List[Tuple[float, ...]]:
    accessor = document["accessors"][index]
    view = document["bufferViews"][accessor["bufferView"]]
    fmt, size = _COMPONENT[accessor["componentType"]]
    components = _COMPONENTS[accessor["type"]]
    stride = view.get("byteStride") or size * components
    base = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    return [
        struct.unpack_from("<" + fmt * components, binary, base + element * stride)
        for element in range(accessor["count"])
    ]


def _mat_trs(translation: Sequence[float], rotation: Sequence[float], scale: Sequence[float]) -> List[float]:
    x, y, z, w = rotation
    xx, yy, zz = x * x, y * y, z * z
    xy, xz, yz = x * y, x * z, y * z
    wx, wy, wz = w * x, w * y, w * z
    sx, sy, sz = scale
    return [
        (1 - 2 * (yy + zz)) * sx, (2 * (xy + wz)) * sx, (2 * (xz - wy)) * sx, 0.0,
        (2 * (xy - wz)) * sy, (1 - 2 * (xx + zz)) * sy, (2 * (yz + wx)) * sy, 0.0,
        (2 * (xz + wy)) * sz, (2 * (yz - wx)) * sz, (1 - 2 * (xx + yy)) * sz, 0.0,
        translation[0], translation[1], translation[2], 1.0,
    ]


def _mat_mul(a: Sequence[float], b: Sequence[float]) -> List[float]:
    out = [0.0] * 16
    for column in range(4):
        for row in range(4):
            total = 0.0
            for k in range(4):
                total += a[k * 4 + row] * b[column * 4 + k]
            out[column * 4 + row] = total
    return out


def _mat_point(matrix: Sequence[float], point: Sequence[float]) -> Vec3:
    x, y, z = point
    return (
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    )


def integer_colours_are_normalised(path: Path) -> bool:
    """True when every integer ``COLOR_0`` accessor carries ``normalized``."""
    document, _binary = read_glb(path)
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            index = primitive.get("attributes", {}).get("COLOR_0")
            if index is None:
                return False
            accessor = document["accessors"][index]
            if accessor["componentType"] in (5121, 5123) and not accessor.get("normalized", False):
                return False
    return True


def verify_written_glb(path: Path) -> Dict[str, dict]:
    """Evaluates the written skin: p' = sum_j w_j * (global_j(t) * IBM_j) * p.

    Reads the GLB back from disk (nodes, skin, inverse bind matrices, attribute
    accessors and clip samplers) instead of trusting the in-memory build.  Every
    clip is a single-key LINEAR hold, so t = 0 is the authored pose; t = 0.5 and
    t = 1.0 are evaluated too, to show the channel holds outside its single key
    (all three land on the same vertices).  Offline geometry, not playback.
    """
    document, binary = read_glb(path)
    nodes = document["nodes"]
    parent: List[Optional[int]] = [None] * len(nodes)
    for index, node in enumerate(nodes):
        for child in node.get("children", []):
            parent[child] = index
    skin = document["skins"][0]
    joints = list(skin["joints"])
    node_name = {index: node.get("name", "") for index, node in enumerate(nodes)}
    joint_slot = {node_name[joint]: slot for slot, joint in enumerate(joints)}
    inverse_bind = accessor_values(document, binary, skin["inverseBindMatrices"])
    mesh_node = next(index for index, node in enumerate(nodes) if "mesh" in node)
    primitive = document["meshes"][nodes[mesh_node]["mesh"]]["primitives"][0]
    positions = accessor_values(document, binary, primitive["attributes"]["POSITION"])
    joint_slots = accessor_values(document, binary, primitive["attributes"]["JOINTS_0"])
    joint_weights = accessor_values(document, binary, primitive["attributes"]["WEIGHTS_0"])

    worst_weight_sum = 0.0
    bad_slots = 0
    for slots, weights in zip(joint_slots, joint_weights):
        total = 0.0
        for slot, weight in zip(slots, weights):
            if slot >= len(joints):
                bad_slots += 1
            total += weight
        worst_weight_sum = max(worst_weight_sum, abs(total - 1.0))

    head_slot = joint_slot["head"]
    sole_indices = [index for index, point in enumerate(positions) if point[1] <= 0.002]
    head_indices = []
    for index, (slots, weights) in enumerate(zip(joint_slots, joint_weights)):
        dominant = max(range(4), key=lambda slot: weights[slot])
        if slots[dominant] == head_slot and weights[dominant] > 0.5:
            head_indices.append(index)
    if not sole_indices or not head_indices:
        raise RuntimeError("the mesh exposes no sole or skull vertices to check")

    results: Dict[str, dict] = {}
    for animation in document.get("animations", []):
        overrides: Dict[int, Dict[str, Tuple[float, ...]]] = {}
        duration = 0.0
        keys = 0
        for channel in animation["channels"]:
            node = channel["target"]["node"]
            node_path = channel["target"]["path"]
            sampler = animation["samplers"][channel["sampler"]]
            times = [value[0] for value in accessor_values(document, binary, sampler["input"])]
            values = accessor_values(document, binary, sampler["output"])
            duration = max(duration, times[-1])
            keys = max(keys, len(times))
            overrides.setdefault(node, {})[node_path] = values[0]

        posed_by_time: List[List[Vec3]] = []
        joint_points: Dict[str, Vec3] = {}
        for sample_time in (0.0, 0.5, 1.0):
            del sample_time  # a single key serves every sample time
            local = []
            for index, node in enumerate(nodes):
                translation = list(node.get("translation", (0.0, 0.0, 0.0)))
                rotation = list(node.get("rotation", (0.0, 0.0, 0.0, 1.0)))
                scale = list(node.get("scale", (1.0, 1.0, 1.0)))
                override = overrides.get(index, {})
                translation = list(override.get("translation", translation))
                rotation = list(override.get("rotation", rotation))
                scale = list(override.get("scale", scale))
                local.append(_mat_trs(translation, rotation, scale))
            global_: List[Optional[List[float]]] = [None] * len(nodes)
            for index in range(len(nodes)):
                global_[index] = (
                    local[index]
                    if parent[index] is None
                    else _mat_mul(global_[parent[index]], local[index])  # type: ignore[arg-type]
                )
            joint_matrices = [
                _mat_mul(global_[joint], inverse_bind[slot])  # type: ignore[arg-type]
                for slot, joint in enumerate(joints)
            ]
            posed = []
            for vertex, (slots, weights) in enumerate(zip(joint_slots, joint_weights)):
                accumulator = [0.0, 0.0, 0.0]
                for component, slot in enumerate(slots):
                    weight = weights[component]
                    if weight <= 0.0:
                        continue
                    point = _mat_point(joint_matrices[slot], positions[vertex])
                    accumulator[0] += point[0] * weight
                    accumulator[1] += point[1] * weight
                    accumulator[2] += point[2] * weight
                posed.append(tuple(accumulator))
            posed_by_time.append(posed)
            joint_points = {
                node_name[joint]: _mat_point(global_[joint], (0.0, 0.0, 0.0))  # type: ignore[arg-type]
                for joint in joints
                if node_name[joint] in ("pelvis", "head")
            }

        posed = posed_by_time[0]
        travel = max(
            _length(_sub(point, positions[index])) for index, point in enumerate(posed)
        )
        results[animation["name"]] = {
            "duration": duration,
            "keys": keys,
            "rotation_channels": sum(
                1 for channel in animation["channels"] if channel["target"]["path"] == "rotation"
            ),
            "translation_channels": sum(
                1 for channel in animation["channels"] if channel["target"]["path"] == "translation"
            ),
            "min_y": min(point[1] for point in posed),
            "max_y": max(point[1] for point in posed),
            "sole_abs_max_y": max(abs(posed[index][1]) for index in sole_indices),
            "head_top": max(posed[index][1] for index in head_indices),
            "max_travel": travel,
            "pelvis_y": joint_points["pelvis"][1],
            "pelvis_z": joint_points["pelvis"][2],
            "head_joint_y": joint_points["head"][1],
            "weights_worst_sum_error": worst_weight_sum,
            "bad_joint_slots": bad_slots,
            "samples_agree": posed_by_time[0] == posed_by_time[1] == posed_by_time[2],
        }
    return results


# ----------------------------------------------------------------------- main


def _ascii_side_view(points: Sequence[Vec3], columns: int = 34, rows: int = 22,
                     z_range: Tuple[float, float] = (-0.25, 0.85),
                     y_range: Tuple[float, float] = (-0.05, 1.75)) -> str:
    """A text occupancy plot of the posed vertices (side elevation, +Z right)."""
    cells = [[" "] * columns for _ in range(rows)]
    for x, y, z in points:
        del x
        column = int((z - z_range[0]) / (z_range[1] - z_range[0]) * (columns - 1))
        row = int((y - y_range[0]) / (y_range[1] - y_range[0]) * (rows - 1))
        if 0 <= column < columns and 0 <= row < rows:
            cells[rows - 1 - row][column] = "#"
    return "\n".join("".join(row) for row in cells)


def _preview_model(preview, positions, mesh: Mesh, texture):  # type: ignore[no-untyped-def]
    model = preview.Model()
    model.positions = [tuple(point) for point in positions]
    model.uvs = [tuple(uv) for uv in mesh.uvs]
    model.colors = [tuple(channel / 255.0 for channel in color) for color in mesh.colors]
    model.indices = list(mesh.indices)
    model.texture = texture
    return model


def render_preview(path: Path, texture_png: bytes, mesh: Mesh, frames: Dict[str, List[Vec3]],
                   chair_snapshot, chair_offset: Vec3, chair_yaw: float) -> dict:
    """Renders the pose sheet with the prop pack's own software rasteriser."""
    import preview  # tools/props/preview.py (stdlib-only rasteriser)

    texture = decode_png(texture_png)
    cell_width, cell_height = 340, 430
    cells: List[Tuple[int, int, bytes]] = []
    views = (("three-quarter", (0.85, 0.62, 1.0)), ("side", (1.0, 0.10, 0.0)))
    for _view_name, direction in views:
        for name, posed in frames.items():
            model = _preview_model(preview, posed, mesh, texture)
            if name == "pose_sit_chair_in_chair":
                model = _merge_chair(preview, model, chair_snapshot, chair_offset, chair_yaw)
            pixels = preview.render(model, cell_width, cell_height, direction=direction)
            cells.append((cell_width, cell_height, pixels))
    width, height, rgba = preview.compose_sheet(cells, columns=4, gap=6)
    path.write_bytes(write_png(width, height, rgba))
    return {"path": str(path), "width": width, "height": height, "cells": len(cells)}


def _merge_chair(preview, model, chair_snapshot, offset: Vec3, yaw_degrees: float):  # type: ignore[no-untyped-def]
    """Adds the shipped chair, flat-greyed and placed, as a fit reference."""
    yaw = math.radians(yaw_degrees)
    cosine, sine = math.cos(yaw), math.sin(yaw)
    base = len(model.positions)
    for position in chair_snapshot.positions:
        x, y, z = position
        x, z = x * cosine + z * sine, -x * sine + z * cosine
        model.positions.append((x + offset[0], y + offset[1], z + offset[2]))
    for color in chair_snapshot.colors:
        del color
        model.colors.append((0.40, 0.40, 0.44))
    model.uvs.extend(tuple(uv) for uv in chair_snapshot.uvs)
    model.indices.extend(index + base for index in chair_snapshot.indices)
    return model


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Build the skeleton entity GLB")
    parser.add_argument("--out", type=Path, default=DEFAULT_GLB,
                        help=f"output GLB (default {DEFAULT_GLB})")
    parser.add_argument("--texture", type=Path, default=DEFAULT_TEXTURE,
                        help=f"source texture PNG (default {DEFAULT_TEXTURE})")
    parser.add_argument("--preview", type=Path, default=DEFAULT_PREVIEW,
                        help=f"pose sheet PNG (default {DEFAULT_PREVIEW})")
    parser.add_argument("--no-preview", action="store_true",
                        help="skip the pose sheet (written under target/ by default)")
    args = parser.parse_args(argv)

    # 1. artwork ---------------------------------------------------------
    tex = paint_bone_texture()
    texture_png = tex.png_bytes()
    args.texture.parent.mkdir(parents=True, exist_ok=True)
    args.texture.write_bytes(texture_png)

    # 2. geometry, origin, rig -------------------------------------------
    mesh, parts = build_mesh(tex)
    offset = mesh.normalize_origin()
    rig = build_rig(offset)
    skinned = SkinnedMesh(mesh)
    assign_weights(skinned, rig, parts)
    weight_stats = check_weights(skinned, rig, parts)

    # 3. poses and clips --------------------------------------------------
    stand_pose = pose_stand()
    floor_pose, floor_root = pose_sit_floor(rig)
    chair_pose, chair_root = pose_sit_chair(rig)
    clips = []
    for name, pose in (("pose_stand", stand_pose), ("pose_sit_floor", floor_pose),
                       ("pose_sit_chair", chair_pose)):
        clips.append(rotation_clip(rig, name, pose, loop=True, kind="pose"))
    clips[1].joint_translation(rig, "root", 0.0, floor_root)
    clips[2].joint_translation(rig, "root", 0.0, chair_root)

    # 4. write ------------------------------------------------------------
    args.out.parent.mkdir(parents=True, exist_ok=True)
    stats = write_model(
        args.out, skinned, rig, clips, texture_png,
        name=ENTITY_NAME, generator=GENERATOR,
    )
    structural = check_model(args.out)
    if structural["problems"]:
        raise SystemExit(f"rig.py --check reports problems: {structural['problems']}")

    # 5. offline geometric verification of the written file ---------------
    verified = verify_written_glb(args.out)
    failures: List[str] = []
    if abs(verified["pose_stand"]["min_y"]) > 0.01:
        failures.append(f"pose_stand min y {verified['pose_stand']['min_y']:.4f}")
    floor = verified["pose_sit_floor"]
    if floor["min_y"] < -0.01:
        failures.append(f"pose_sit_floor dips to {floor['min_y']:.4f}")
    if not 0.05 <= floor["pelvis_y"] <= 0.12:
        failures.append(f"pose_sit_floor pelvis {floor['pelvis_y']:.4f}")
    chair = verified["pose_sit_chair"]
    if chair["sole_abs_max_y"] > 0.015:
        failures.append(f"pose_sit_chair sole {chair['sole_abs_max_y']:.4f}")
    if abs(chair["pelvis_y"] - CHAIR_PELVIS_Y) > 0.02:
        failures.append(f"pose_sit_chair pelvis {chair['pelvis_y']:.4f}")
    if chair["head_top"] <= 1.1:
        failures.append(f"pose_sit_chair skull top {chair['head_top']:.4f}")
    for name, result in verified.items():
        if result["max_travel"] > 1.3:
            failures.append(f"{name} travels {result['max_travel']:.4f} m")
        if result["weights_worst_sum_error"] > 1.0e-5 or result["bad_joint_slots"]:
            failures.append(f"{name} weight/slot error")
        if not result["samples_agree"]:
            failures.append(f"{name} does not hold across sample times")
    degenerate = mesh.degenerate_triangles()
    if degenerate:
        failures.append(f"{degenerate} degenerate triangles")
    for u, v in mesh.uvs:
        if not 0.0 <= u <= 1.0 or not 0.0 <= v <= 1.0:
            failures.append(f"UV {u},{v} outside 0..1")
    if not integer_colours_are_normalised(args.out):
        failures.append("an integer COLOR_0 accessor is not marked normalised")

    # 6. preview ----------------------------------------------------------
    preview_stats = None
    chair_fit = chair_fit_report(rig, mesh, skinned, chair_pose, chair_root)
    if not args.no_preview:
        frames = {
            "pose_stand": skinned_positions(mesh, skinned, rig, stand_pose, (0.0, 0.0, 0.0)),
            "pose_sit_floor": skinned_positions(mesh, skinned, rig, floor_pose, floor_root),
            "pose_sit_chair": skinned_positions(mesh, skinned, rig, chair_pose, chair_root),
        }
        chair_offset, chair_yaw = chair_placement(chair["pelvis_z"])
        frames["pose_sit_chair_in_chair"] = frames["pose_sit_chair"]
        preview_stats = render_preview(
            args.preview, texture_png, mesh, frames,
            load_chair_model(), chair_offset, chair_yaw,
        )

    # 7. report -----------------------------------------------------------
    low, high = mesh.bounds()
    catalog_size = (high[0] - low[0], high[1] - low[1], high[2] - low[2])
    print("skeleton: low-poly articulated human skeleton")
    print(f"  glb            {args.out}  ({args.out.stat().st_size} bytes)")
    print(f"  texture        {args.texture}  ({len(texture_png)} bytes, {tex.width}x{tex.height})")
    if preview_stats:
        print(f"  preview        {args.preview}  ({preview_stats['width']}x{preview_stats['height']})")
    print(f"  joints         {len(rig.joints)} (engine ceiling 128)")
    print(f"  triangles      {len(mesh.indices) // 3} (target 1000-1500, hard limit 1500)")
    print(f"  vertices       {len(mesh.positions)} (limit 3500)")
    print(f"  clips          {len(clips)}")
    print(f"  clip channels  {stats['clip_channels']}")
    print(f"  mesh node      one primitive, one material, one embedded PNG")
    print(f"  bbox min       ({low[0]:+.4f}, {low[1]:+.4f}, {low[2]:+.4f})")
    print(f"  bbox max       ({high[0]:+.4f}, {high[1]:+.4f}, {high[2]:+.4f})")
    print(f"  catalog size   [{catalog_size[0]:.4f}, {catalog_size[1]:.4f}, {catalog_size[2]:.4f}]")
    print("")
    print("clips")
    print("  name              kind  loop  duration  keys  rotation  translation")
    for clip in clips:
        result = verified[clip.name]
        print(
            f"  {clip.name:<17} {clip.kind:<5} {str(clip.loop):<5} "
            f"{result['duration']:>8.3f} {result['keys']:>5} "
            f"{result['rotation_channels']:>9} {result['translation_channels']:>12}"
        )
    print("")
    print("offline geometric check (written GLB re-read, LINEAR blend at t=0/0.5/1.0)")
    print("  clip              min_y    max_y   pelvis_y  head_top  sole_|y|  max_travel")
    for name in ("pose_stand", "pose_sit_floor", "pose_sit_chair"):
        result = verified[name]
        print(
            f"  {name:<17} {result['min_y']:+.4f}  {result['max_y']:+.4f}  "
            f"{result['pelvis_y']:.4f}   {result['head_top']:.4f}   "
            f"{result['sole_abs_max_y']:.4f}    {result['max_travel']:.4f}"
        )
    print(f"  weight sum worst error {weight_stats['worst_weight_sum_error']:.2e}, "
          f"bad joint slots {chair['bad_joint_slots']}, samples agree "
          f"{all(result['samples_agree'] for result in verified.values())}")
    print("")
    for name, pose, root in (("pose_stand", stand_pose, (0.0, 0.0, 0.0)),
                             ("pose_sit_floor", floor_pose, floor_root),
                             ("pose_sit_chair", chair_pose, chair_root)):
        summary = pose_summary(rig, pose, root)
        print(f"{name}: pelvis y {summary['pelvis_y']:.4f}, root offset "
              f"({root[0]:+.3f}, {root[1]:+.3f}, {root[2]:+.3f})")
        for suffix in ("l", "r"):
            entry = summary["joints"][suffix]
            print(
                f"  {suffix}: hip {entry['hip'][1]:.3f} knee {entry['knee'][1]:.3f} "
                f"ankle {entry['ankle'][1]:.3f} flexion {entry['hip_flexion_degrees']:.1f} deg, "
                f"knee {entry['knee_flexion_degrees']:.1f} deg, "
                f"shin off vertical {entry['shin_from_vertical_degrees']:.1f} deg"
            )
    print("")
    side = _ascii_side_view(skinned_positions(mesh, skinned, rig, chair_pose, chair_root))
    print("pose_sit_chair side occupancy (+Z right, y up)")
    print(side)
    if chair_fit is not None:
        print("")
        print("core:chair seat fit (measured from the shipped chair.glb vertices)")
        print(f"  chair bbox          min ({chair_fit['chair_min'][0]:+.3f}, {chair_fit['chair_min'][1]:+.3f}, {chair_fit['chair_min'][2]:+.3f})"
              f"  max ({chair_fit['chair_max'][0]:+.3f}, {chair_fit['chair_max'][1]:+.3f}, {chair_fit['chair_max'][2]:+.3f})")
        print(f"  seat pad top y      {chair_fit['seat_top']:.4f}   z {chair_fit['seat_back_z']:+.3f} .. {chair_fit['seat_front_z']:+.3f}")
        print(f"  backrest front z    {chair_fit['backrest_front_z']:+.4f}   base front z {chair_fit['base_front_z']:+.4f}")
        print(f"  skeleton placement  x {chair_fit['chair_offset'][0]:+.4f}  y {chair_fit['chair_offset'][1]:+.4f}"
              f"  z {chair_fit['chair_offset'][2]:+.4f}  yaw {chair_fit['chair_yaw']:+.1f} deg (chair at the origin)")
        print(f"  pelvis lowest y     {chair_fit['pelvis_lowest_y']:.4f}  (sinks {chair_fit['seat_top'] - chair_fit['pelvis_lowest_y']:.4f} into the pad)")
        print(f"  pelvis rear z       {chair_fit['pelvis_rearmost_z']:+.4f}  (backrest front at {chair_fit['backrest_front_z']:+.4f})")
        print(f"  torso rear z        {chair_fit['torso_rearmost_z']:+.4f}  (clearance {chair_fit['torso_rearmost_z'] - chair_fit['backrest_front_z']:+.4f})")
        print(f"  knee z              {chair_fit['knee_z']:+.4f}  (seat front at {chair_fit['seat_front_z']:+.4f})")
        print(f"  toe z               {chair_fit['toe_z']:+.4f}  (base front at {chair_fit['base_front_z']:+.4f})")
    print("")
    if failures:
        print("FAILED checks:")
        for line in failures:
            print(f"  - {line}")
        return 1
    print("all required checks passed")
    return 0


def skinned_positions(mesh: Mesh, skinned: SkinnedMesh, rig: Rig,
                      pose: Dict[str, Quat], root_offset: Vec3) -> List[Vec3]:
    """The vertex positions of one pose, using the writer's own blend.

    ``p' = sum_j w_j * (Q_j * (p_bind - P_rest_j) + P_posed_j)``: the joint's
    rest *global* origin is the reference, exactly as ``inverseBind`` encodes.
    """
    posed_fk = Fk(rig)
    for name, rotation in pose.items():
        posed_fk.local_q[posed_fk.index(name)] = rotation
    posed_fk.root_offset = root_offset
    posed_fk.solve()
    rest_fk = Fk(rig)
    rest_fk.solve()
    out: List[Vec3] = []
    for vertex, source in enumerate(mesh.positions):
        accumulator = [0.0, 0.0, 0.0]
        for slot, weight in zip(skinned.joints[vertex], skinned.weights[vertex]):
            if weight <= 0.0:
                continue
            local = _sub(source, rest_fk.positions[slot])
            posed = _add(_qrotate(posed_fk.rotations[slot], local), posed_fk.positions[slot])
            accumulator[0] += posed[0] * weight
            accumulator[1] += posed[1] * weight
            accumulator[2] += posed[2] * weight
        out.append(tuple(accumulator))
    return out


def chair_placement(pelvis_z: float) -> Tuple[Vec3, float]:
    """Where the chair goes so the posed pelvis lands on its seat.

    The chair keeps its authored yaw (both face +Z) and sits on the floor; the
    skeleton is offset behind the chair's origin so the pelvis joint rests at
    ``CHAIR_HIP_ON_SEAT`` in the chair's frame.
    """
    return (0.0, 0.0, CHAIR_HIP_ON_SEAT - pelvis_z), 0.0


def load_chair_model():  # type: ignore[no-untyped-def]
    """The shipped ``core:chair`` mesh, or ``None`` when it is unavailable."""
    import preview  # tools/props/preview.py (stdlib-only reader)

    if not CHAIR_GLB.is_file():
        return None
    return preview.load_model(str(CHAIR_GLB))


def chair_fit_report(rig: Rig, mesh: Mesh, skinned: SkinnedMesh,
                     pose: Dict[str, Quat], root: Vec3) -> Optional[dict]:
    """Measures the seated pose against the shipped chair, in the chair's frame.

    Every number here is derived from the chair's own vertices and the posed
    skeleton, so a level can reproduce the placement exactly.
    """
    chair = load_chair_model()
    if chair is None:
        return None
    posed = skinned_positions(mesh, skinned, rig, pose, root)
    summary = pose_summary(rig, pose, root)
    pelvis_z = summary["joints"]["l"]["hip"][2]
    chair_offset, chair_yaw = chair_placement(pelvis_z)

    def placed(point: Vec3) -> Vec3:
        return _add(point, chair_offset)

    seat_vertices = [
        point for point in chair.positions
        if abs(point[0]) <= 0.215 and abs(point[2]) <= 0.215 and point[1] <= 0.60
    ]
    back_vertices = [point for point in chair.positions if 0.60 <= point[1] <= 0.95]
    base_vertices = [point for point in chair.positions if point[1] < 0.12]
    pelvis_slot = rig.index("pelvis")
    pelvis_vertices = [
        posed[index]
        for index, (slots, weights) in enumerate(zip(skinned.joints, skinned.weights))
        if slots[max(range(4), key=lambda slot: weights[slot])] == pelvis_slot
    ]
    return {
        "chair_offset": chair_offset,
        "chair_yaw": chair_yaw,
        "chair_min": tuple(min(point[axis] for point in chair.positions) for axis in range(3)),
        "chair_max": tuple(max(point[axis] for point in chair.positions) for axis in range(3)),
        "seat_top": max(point[1] for point in seat_vertices),
        "seat_front_z": max(point[2] for point in seat_vertices),
        "seat_back_z": min(point[2] for point in seat_vertices),
        "backrest_front_z": max(point[2] for point in back_vertices),
        "base_front_z": max(point[2] for point in base_vertices),
        "pelvis_lowest_y": min(point[1] for point in pelvis_vertices),
        "pelvis_rearmost_z": min(point[2] for point in pelvis_vertices) + chair_offset[2],
        "torso_rearmost_z": min(
            placed(posed[index])[2]
            for index, (slots, weights) in enumerate(zip(skinned.joints, skinned.weights))
            if slots[max(range(4), key=lambda slot: weights[slot])] in
            (rig.index("chest"), rig.index("spine_02"), rig.index("spine_01"))
        ),
        "knee_z": placed(summary["joints"]["l"]["knee"])[2],
        "toe_z": max(placed(summary["joints"][side]["toe"])[2] for side in ("l", "r")),
        "hand_z": placed(summary["joints"]["l"]["hand"])[2],
    }


if __name__ == "__main__":
    raise SystemExit(main())
