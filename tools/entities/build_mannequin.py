#!/usr/bin/env python3
"""Deterministic low-poly grey concrete human mannequin (rigged entity GLB).

The asset is an artist's lay figure: an egg head, neck, chest/waist/pelvis, two
arms with elbows and mitt hands, two legs with knees and flat feet, painted with
one real concrete texture. It is written with the repository's rigged-GLB
writer (``tools/entities/rig.py``), so the GLB carries one skin, 23 named
joints and three single-pose HOLD clips:

    pose_stand          the bind pose, all identity rotations
    pose_arms_up        both arms raised overhead, slight elbow bend
    pose_arms_forward   both arms extended horizontally forward

Conventions match ``assets/README.md``: 1 unit = 1 metre, +Y up, +Z front, the
origin is the floor-contact point horizontally centred under the bounding box,
and the texture is one embedded PNG painted with ``tools/props/tex.py``.

The build is pure stdlib, deterministic and single-process. After writing it

  * structurally checks the GLB with ``rig.check_model`` (the same rules the
    Rust loader enforces), and
  * evaluates the skinned mesh per clip from the *written file* with the
    writer's own blend maths (``p_posed = sum_j w_j * global_j(t) * IBM_j *
    p_bind``) to prove the bind pose is grounded, that the arm poses actually
    reach, and that no vertex is flung across the room by a broken pivot.

This is an offline geometric check, not engine playback.

    python3 tools/entities/build_mannequin.py
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path
from typing import Callable, Dict, List, Optional, Sequence, Tuple

DEFAULT_OUT = "assets/entities/mannequin/model/mannequin.glb"
TEXTURE_DIR = "assets/entities/mannequin/textures"
TEXTURE_NAME = "concrete_grey_01.png"

# --------------------------------------------------------------------- paths


def _find_repo_root(start: Path) -> Path:
    """Walks up from ``start`` until the repository root (tools/entities) shows."""
    for candidate in (start, *start.parents):
        if (candidate / "tools" / "entities" / "rig.py").is_file():
            return candidate
    raise SystemExit(f"cannot locate the Places repository root above {start}")


REPO_ROOT = _find_repo_root(Path(__file__).resolve().parent)
for _extra in (REPO_ROOT / "tools" / "entities", REPO_ROOT / "tools" / "props"):
    if str(_extra) not in sys.path:
        sys.path.insert(0, str(_extra))

import palette  # noqa: E402  (tools/props on sys.path)
from mesh import Mesh  # noqa: E402
from tex import Texture  # noqa: E402
from rig import (  # noqa: E402
    IDENTITY_QUAT,
    Rig,
    SkinnedMesh,
    _mat_mul,
    _mat_trs,
    check_model,
    quat_rot_x,
    quat_rot_z,
    rotation_clip,
    transform_point,
    write_model,
)

# ---------------------------------------------------------------- proportions

# 1.72 m figure, 1:7.0 head-to-body (0.245 m head), 0.42 m shoulder span.
HIP_Y = 0.950
KNEE_Y = 0.520
ANKLE_Y = 0.085
# The shoulder joint is the arm chain's root (a clavicle); the upper arm sits
# on the deltoid centre and carries every arm vertex, so a raised arm rotates
# rigidly about a pivot inside its own shoulder cap instead of shearing.
SHOULDER_X = 0.075
SHOULDER_Y = 1.415
UPPERARM_X = 0.153
UPPERARM_Y = 1.392
ELBOW_X = 0.153
ELBOW_Y = 1.045
WRIST_Y = 0.810
HAND_TIP_Y = 0.660
HIP_X = 0.085

# Vertex colour multiplying the concrete sheet: near-neutral so the painted
# grey stays the albedo, with a little headroom for the baked face shading.
TINT = (250, 250, 249)

# Texture space: one repeat of the sheet spans this many metres of body, so
# every part is mapped into a proportional sub-rectangle and the concrete grain
# keeps a roughly constant scale over the whole figure.
TEXTURE_METRES = 0.60

# Body segment names, used for both geometry spans and weight candidate sets.
JOINT_NAMES = (
    "root",
    "pelvis",
    "spine_01",
    "spine_02",
    "chest",
    "neck",
    "head",
    "shoulder_l",
    "shoulder_r",
    "upperarm_l",
    "upperarm_r",
    "forearm_l",
    "forearm_r",
    "hand_l",
    "hand_r",
    "thigh_l",
    "thigh_r",
    "shin_l",
    "shin_r",
    "foot_l",
    "foot_r",
    "toe_l",
    "toe_r",
)


def joint_table() -> List[Tuple[str, Optional[str], Tuple[float, float, float]]]:
    """Joint name, parent name and rest translation, parents before children."""
    rows: List[Tuple[str, Optional[str], Tuple[float, float, float]]] = [
        ("root", None, (0.0, 0.0, 0.0)),
        ("pelvis", "root", (0.0, HIP_Y, 0.0)),
        ("spine_01", "pelvis", (0.0, 0.045, 0.0)),
        ("spine_02", "spine_01", (0.0, 0.075, 0.0)),
        ("chest", "spine_02", (0.0, 0.100, 0.0)),
        ("neck", "chest", (0.0, 0.270, 0.0)),
        ("head", "neck", (0.0, 0.060, 0.0)),
    ]
    for side, sign in (("l", 1.0), ("r", -1.0)):
        rows += [
            (f"shoulder_{side}", "chest", (sign * SHOULDER_X, SHOULDER_Y - 1.170, 0.0)),
            (
                f"upperarm_{side}",
                f"shoulder_{side}",
                (sign * (UPPERARM_X - SHOULDER_X), UPPERARM_Y - SHOULDER_Y, 0.0),
            ),
            (f"forearm_{side}", f"upperarm_{side}", (sign * (ELBOW_X - UPPERARM_X), ELBOW_Y - UPPERARM_Y, 0.0)),
            (f"hand_{side}", f"forearm_{side}", (0.0, WRIST_Y - ELBOW_Y, 0.0)),
            (f"thigh_{side}", "pelvis", (sign * HIP_X, 0.0, 0.0)),
            (f"shin_{side}", f"thigh_{side}", (0.0, KNEE_Y - HIP_Y, 0.006)),
            (f"foot_{side}", f"shin_{side}", (0.0, ANKLE_Y - KNEE_Y, -0.016)),
            (f"toe_{side}", f"foot_{side}", (0.0, -0.062, 0.108)),
        ]
    return rows


def joint_tails() -> Dict[str, Tuple[float, float, float]]:
    """Local tail points for the leaf joints (head, hands, toes)."""
    tails: Dict[str, Tuple[float, float, float]] = {"head": (0.0, 0.200, 0.0)}
    for side in ("l", "r"):
        tails[f"hand_{side}"] = (0.0, HAND_TIP_Y - WRIST_Y, 0.0)
        tails[f"toe_{side}"] = (0.0, 0.0, 0.085)
    return tails


# --------------------------------------------------------------------- mesh


def _smoothstep(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)


def add_foot(mesh: Mesh, x: float, uv: Tuple[float, float, float, float]) -> None:
    """Build one rounded, flat-soled foot from heel through toe."""
    # Each ring is (front position, half-width, sole height, instep height).
    # The high middle rings meet the shin; the forefoot lowers without the
    # separate, square toe box used by the original mesh.
    profile = (
        (-0.075, 0.022, 0.008, 0.036),
        (-0.057, 0.037, 0.000, 0.053),
        (-0.020, 0.042, 0.000, 0.080),
        (0.012, 0.044, 0.000, 0.097),
        (0.050, 0.046, 0.000, 0.080),
        (0.102, 0.047, 0.000, 0.059),
        (0.163, 0.044, 0.000, 0.047),
        (0.192, 0.036, 0.000, 0.039),
        (0.210, 0.015, 0.006, 0.027),
    )
    segments = 8
    u0, v0, u1, v1 = uv
    rings = []
    for z, width, sole, instep in profile:
        centre_y = (sole + instep) * 0.5
        radius_y = (instep - sole) * 0.5
        rings.append([
            (x + width * math.cos(index * math.tau / segments),
             centre_y + radius_y * math.sin(index * math.tau / segments), z)
            for index in range(segments)
        ])

    for ring_index, (rear, front) in enumerate(zip(rings, rings[1:])):
        for index in range(segments):
            next_index = (index + 1) % segments
            # Match Mesh.lathe's outward winding and keep the texture continuous.
            mesh.quad(
                rear[index], rear[next_index], front[next_index], front[index],
                [
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * ring_index / (len(rings) - 1)),
                    (u0 + (u1 - u0) * (index + 1) / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                    (u0 + (u1 - u0) * index / segments, v1 + (v0 - v1) * (ring_index + 1) / (len(rings) - 1)),
                ],
                TINT,
                shade_mult=0.82,
                ao=0.90,
            )
    for index in range(segments):
        next_index = (index + 1) % segments
        mesh.triangle((x, 0.022, profile[0][0]), rings[0][next_index], rings[0][index],
                      ((u0, v0), (u1, v0), (u0, v1)), TINT, ao=0.90)
        mesh.triangle((x, 0.0165, profile[-1][0]), rings[-1][index], rings[-1][next_index],
                      ((u0, v0), (u1, v0), (u0, v1)), TINT, ao=0.90)


def build_body(mesh: Mesh, tex: Texture) -> List[Tuple[str, int, int]]:
    """Adds every body part and returns (name, first vertex, end vertex) spans."""
    spans: List[Tuple[str, int, int]] = []

    def uv(length: float, u_span: float = 1.0) -> Tuple[float, float, float, float]:
        return tex.sub("body", 0.0, 0.0, u_span, min(1.0, max(0.05, length / TEXTURE_METRES)))

    def part(name: str, fn: Callable[..., None], *args, **kwargs) -> None:
        start = len(mesh.positions)
        first_index = len(mesh.indices)
        fn(*args, **kwargs)
        if fn in (mesh.lathe, mesh.cylinder):
            # The legacy Y-axis primitives wind inward; opt in locally so
            # concrete casts have outward normals in other glTF consumers too.
            for index in range(first_index, len(mesh.indices), 3):
                mesh.indices[index + 1], mesh.indices[index + 2] = mesh.indices[index + 2], mesh.indices[index + 1]
        spans.append((name, start, len(mesh.positions)))

    # Torso: pelvis -> hips -> waist -> ribs -> chest -> shoulder shelf, one
    # lathe with an elliptical section (0.72 of the width deep, so the side
    # view still reads as a torso).
    part(
        "torso",
        mesh.lathe,
        (0.0, 0.900, 0.0),
        [
            (0.000, 0.105),
            (0.045, 0.125),
            (0.130, 0.110),
            (0.215, 0.105),
            (0.310, 0.125),
            (0.420, 0.145),
            (0.500, 0.143),
            (0.530, 0.100),
        ],
        segments=8,
        axis="y",
        uv=uv(0.53),
        color=TINT,
        ellipse=(1.0, 0.72),
        )

    # Head: egg, chin to crown, slightly narrower than it is deep.
    part(
        "head",
        mesh.lathe,
        (0.0, 1.475, 0.0),
        [
            (0.000, 0.050),
            (0.028, 0.070),
            (0.085, 0.088),
            (0.145, 0.094),
            (0.195, 0.086),
            (0.230, 0.062),
            (0.245, 0.028),
        ],
        segments=8,
        axis="y",
        uv=uv(0.245),
        color=TINT,
        ellipse=(0.95, 0.92),
        )

    # Neck: short tapered drum tucked into both the chest and the skull.
    part(
        "neck",
        mesh.cylinder,
        (0.0, 1.400, 0.004),
        0.046,
        0.115,
        segments=6,
        axis="y",
        uv=uv(0.115),
        color=TINT,
        taper=0.92,
        )

    for side, sign in (("l", 1.0), ("r", -1.0)):
        x = sign * UPPERARM_X

        # Shoulder cap + upper arm, one straight lathe from the elbow up over
        # the deltoid. The widest ring (r = 0.057, so the 0.42 m shoulder span
        # is all cap) sits exactly on the upper-arm pivot, which is what lets
        # a raised arm swing without shearing its own shoulder.
        part(
            f"upperarm_{side}",
            mesh.lathe,
            (x, 1.030, 0.0),
            [
                (0.000, 0.036),
                (0.080, 0.042),
                (0.180, 0.045),
                (0.268, 0.048),
                (0.328, 0.053),
                (0.362, 0.057),
                (0.390, 0.030),
            ],
            segments=8,
            axis="y",
            uv=uv(0.390),
            color=TINT,
            )

        # Forearm: elbow to wrist, gentle taper.
        part(
            f"forearm_{side}",
            mesh.lathe,
            (x, 0.800, 0.0),
            [(0.000, 0.028), (0.075, 0.032), (0.165, 0.037), (0.245, 0.038)],
            segments=8,
            axis="y",
            uv=uv(0.245),
            color=TINT,
            )

        # Mitt hand: flattened front-to-back, knuckle bulge, rounded tip.
        part(
            f"hand_{side}",
            mesh.lathe,
            (x, HAND_TIP_Y, 0.0),
            [(0.000, 0.014), (0.018, 0.028), (0.055, 0.034), (0.100, 0.032), (0.150, 0.026)],
            segments=6,
            axis="y",
            uv=uv(0.150),
            color=TINT,
            ellipse=(1.0, 0.62),
            )

        # Thigh: knee to hip ball.
        part(
            f"thigh_{side}",
            mesh.lathe,
            (sign * HIP_X, 0.505, 0.005),
            [(0.000, 0.050), (0.110, 0.060), (0.230, 0.070), (0.340, 0.077), (0.415, 0.079), (0.440, 0.070)],
            segments=8,
            axis="y",
            uv=uv(0.44),
            color=TINT,
            )

        # Shin: ankle to knee, calf swell below the knee.
        part(
            f"shin_{side}",
            mesh.lathe,
            (sign * HIP_X, 0.075, 0.0),
            [(0.000, 0.030), (0.105, 0.040), (0.235, 0.052), (0.360, 0.048), (0.450, 0.044)],
            segments=8,
            axis="y",
            uv=uv(0.45),
            color=TINT,
            )

        # Foot: one tapered volume with a flat sole and raised instep.
        part(
            f"foot_{side}",
            add_foot,
            mesh,
            sign * HIP_X,
            uv(0.285, 0.45),
        )

    return spans


# ------------------------------------------------------------------ texture


def paint_concrete(tex: Texture) -> None:
    """Load the authored concrete PNG; rebuilding never repaints the artwork."""
    from tex import decode_png
    width, height, pixels = decode_png((REPO_ROOT / TEXTURE_DIR / TEXTURE_NAME).read_bytes())
    if (width, height) != (256, 256) or any(a != 255 for a in pixels[3::4]):
        raise ValueError("mannequin concrete must be an opaque 256x256 PNG")
    tex.auto("body")
    tex.pixels[:] = pixels


# ------------------------------------------------------------------ weights


def _point_segment_distance(
    point: Sequence[float], start: Sequence[float], end: Sequence[float]
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
    return math.sqrt((px - (ax + dx * t)) ** 2 + (py - (ay + dy * t)) ** 2 + (pz - (az + dz * t)) ** 2)


# Candidate joints per part. Restricting the candidates is what keeps an
# exponential nearest-bone blend from reaching across the body: an arm vertex
# can never pick up a leg bone or the far arm.
CANDIDATES: Dict[str, Tuple[str, ...]] = {
    "torso": ("pelvis", "spine_01", "spine_02", "chest", "neck"),
    "head": ("neck", "head"),
    "neck": ("chest", "neck", "head"),
    # The shoulder (clavicle) joint carries no vertices: the whole arm chain
    # belongs to the upper arm, whose pivot is inside the deltoid cap.
    "upperarm_l": ("upperarm_l", "forearm_l"),
    "upperarm_r": ("upperarm_r", "forearm_r"),
    "forearm_l": ("upperarm_l", "forearm_l", "hand_l"),
    "forearm_r": ("upperarm_r", "forearm_r", "hand_r"),
    "hand_l": ("forearm_l", "hand_l"),
    "hand_r": ("forearm_r", "hand_r"),
    "thigh_l": ("pelvis", "thigh_l", "shin_l"),
    "thigh_r": ("pelvis", "thigh_r", "shin_r"),
    "shin_l": ("thigh_l", "shin_l", "foot_l"),
    "shin_r": ("thigh_r", "shin_r", "foot_r"),
    "foot_l": ("shin_l", "foot_l", "toe_l"),
    "foot_r": ("shin_r", "foot_r", "toe_r"),
    "toe_l": ("foot_l", "toe_l"),
    "toe_r": ("foot_r", "toe_r"),
}


def _foot_blend(side: str, ankle_z: float) -> Callable[[Sequence[float]], Dict[str, float]]:
    """Ankle/toe blend for the tapered foot.

    A heel vertex is exactly as far from the shin bone as from the foot bone,
    so distance alone splits every foot vertex 50/50 across the ankle. The foot
    is instead weighted along the foot: the sole belongs to ``foot``, the front
    third hands over to ``toe``, and only the top of the ankle follows ``shin``.
    """

    def weights(position: Sequence[float]) -> Dict[str, float]:
        local_z = position[2] - ankle_z
        toe = _smoothstep((local_z - 0.055) / 0.085)
        up = _smoothstep((position[1] - 0.026) / 0.042)
        w_toe = toe * (1.0 - 0.30 * up)
        w_shin = 0.45 * up * (1.0 - toe)
        return {
            f"foot_{side}": max(0.0, 1.0 - w_toe - w_shin),
            f"toe_{side}": w_toe,
            f"shin_{side}": w_shin,
        }

    return weights


def assign_weights(
    skinned: SkinnedMesh,
    rig: Rig,
    spans: Sequence[Tuple[str, int, int]],
    custom: Dict[str, Callable[[Sequence[float]], Dict[str, float]]],
    *,
    sigma: float = 0.07,
    max_influences: int = 3,
    min_share: float = 0.12,
) -> None:
    """Deterministic nearest-segment blend, restricted per body part.

    This is ``rig.auto_weights``' maths (exp(-(d/sigma)^2) over the closest
    segments, small shares dropped and the rest renormalised to 1) with a
    per-part candidate list instead of one global one, and with
    :meth:`rig.Rig.segment` endpoints.
    """
    count = len(skinned.mesh.positions)
    joints = [[0, 0, 0, 0] for _ in range(count)]
    weights = [[0.0, 0.0, 0.0, 0.0] for _ in range(count)]
    covered = [False] * count
    for name, start, end in spans:
        if name in custom:
            for vertex in range(start, end):
                entries = [(rig.index(joint), weight) for joint, weight in custom[name](skinned.mesh.positions[vertex]).items() if weight > 1.0e-6]
                entries.sort(key=lambda entry: (-entry[1], entry[0]))
                entries = entries[:4]
                total = sum(weight for _, weight in entries)
                if total <= 0.0:
                    raise SystemExit(f"part {name!r} produced an empty weight set")
                for slot, (joint, weight) in enumerate(entries):
                    joints[vertex][slot] = joint
                    weights[vertex][slot] = weight / total
                covered[vertex] = True
            continue
        candidates = [rig.index(joint) for joint in CANDIDATES[name]]
        segments = {joint: rig.segment(joint) for joint in candidates}
        for vertex in range(start, end):
            position = skinned.mesh.positions[vertex]
            scored = sorted(
                (
                    (math.exp(-((_point_segment_distance(position, *segments[joint]) / sigma) ** 2)), joint)
                    for joint in candidates
                ),
                key=lambda entry: (-entry[0], entry[1]),
            )[:max_influences]
            total = sum(score for score, _ in scored)
            if total <= 1.0e-9:
                scored = [(1.0, scored[0][1])]
                total = 1.0
            kept = [(score / total, joint) for score, joint in scored if score / total >= min_share] or [
                (1.0, scored[0][1])
            ]
            total = sum(weight for weight, _ in kept)
            for slot, (weight, joint) in enumerate(kept[:4]):
                joints[vertex][slot] = joint
                weights[vertex][slot] = weight / total
            covered[vertex] = True
    if not all(covered):
        raise SystemExit("some vertices were never weighted by a body part")
    skinned.joints = joints
    skinned.weights = weights


# -------------------------------------------------------------------- poses


def pose_stand() -> Dict[str, Tuple[float, float, float, float]]:
    return {name: IDENTITY_QUAT for name in JOINT_NAMES}


def pose_arms_up() -> Dict[str, Tuple[float, float, float, float]]:
    """Both arms swung up over the head: 155 degrees total, 15 of elbow bend.

    The clavicle leads (12) and the upper arm carries the raise (143); the
    upper arm's pivot sits on the deltoid cap, so its geometry turns rigidly.
    """
    pose = pose_stand()
    for side, sign in (("l", 1.0), ("r", -1.0)):
        pose[f"shoulder_{side}"] = quat_rot_z(12.0 * sign)
        pose[f"upperarm_{side}"] = quat_rot_z(143.0 * sign)
        pose[f"forearm_{side}"] = quat_rot_z(15.0 * sign)
    return pose


def pose_arms_forward() -> Dict[str, Tuple[float, float, float, float]]:
    """Both arms straight forward and horizontal; the forearm drops 8 degrees."""
    pose = pose_stand()
    for side in ("l", "r"):
        pose[f"shoulder_{side}"] = quat_rot_x(-8.0)
        pose[f"upperarm_{side}"] = quat_rot_x(-82.0)
        pose[f"forearm_{side}"] = quat_rot_x(-8.0)
    return pose


def build_clips(rig: Rig):
    return [
        rotation_clip(rig, "pose_stand", pose_stand(), loop=True, kind="pose"),
        rotation_clip(rig, "pose_arms_up", pose_arms_up(), loop=True, kind="pose"),
        rotation_clip(rig, "pose_arms_forward", pose_arms_forward(), loop=True, kind="pose"),
    ]


# ------------------------------------------------------------------ writing


def build(out_path: Path) -> dict:
    texture = Texture(256, seed=41)
    paint_concrete(texture)
    texture_path = REPO_ROOT / TEXTURE_DIR / TEXTURE_NAME
    texture_path.parent.mkdir(parents=True, exist_ok=True)
    texture_path.write_bytes(texture.png_bytes())

    mesh = Mesh()
    spans = build_body(mesh, texture)

    # Contract: origin on the floor under the bounding-box centre. Build in
    # natural coordinates (ankle at z = 0) and shift the whole figure -- mesh
    # and rig -- so the box is centred; the sole already sits on y = 0.
    low, high = mesh.bounds()
    shift = (-(low[0] + high[0]) * 0.5, -low[1], -(low[2] + high[2]) * 0.5)
    mesh.translate(shift)

    rig = Rig()
    indices: Dict[str, int] = {}
    tails = joint_tails()
    for name, parent, translation in joint_table():
        if name == "pelvis":
            translation = (translation[0], translation[1], shift[2])
        indices[name] = rig.add(
            name,
            indices[parent] if parent is not None else None,
            translation,
            tail=tails.get(name),
        )
    rig.finish()

    skinned = SkinnedMesh(mesh)
    ankle_z = shift[2] - 0.010
    custom = {
        "foot_l": _foot_blend("l", ankle_z),
        "foot_r": _foot_blend("r", ankle_z),
    }
    assign_weights(skinned, rig, spans, custom)

    clips = build_clips(rig)
    stats = write_model(out_path, skinned, rig, clips, texture.png_bytes(), name="mannequin")
    stats["texture_png"] = str(texture_path)
    stats["spans"] = len(spans)
    stats["bounds"] = {
        "min": [min(p[axis] for p in mesh.positions) for axis in range(3)],
        "max": [max(p[axis] for p in mesh.positions) for axis in range(3)],
    }
    stats["shift"] = list(shift)
    return stats


# ------------------------------------------------------- offline pose check

_COMPONENT = {5120: ("b", 1), 5121: ("B", 1), 5122: ("h", 2), 5123: ("H", 2), 5125: ("I", 4), 5126: ("f", 4)}
_TYPE_COUNT = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def load_glb(path: Path) -> Tuple[dict, bytes]:
    data = path.read_bytes()
    magic, version, _length = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        raise SystemExit(f"{path} is not a GLB 2.0 file")
    offset = 12
    document: Optional[dict] = None
    binary = b""
    while offset + 8 <= len(data):
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        payload = data[offset : offset + chunk_length]
        offset += chunk_length
        if chunk_type == 0x4E4F534A:
            document = json.loads(payload.decode("utf-8"))
        elif chunk_type == 0x004E4942:
            binary = payload
    if document is None:
        raise SystemExit(f"{path} has no JSON chunk")
    return document, binary


def read_accessor(document: dict, binary: bytes, index: int) -> List[tuple]:
    accessor = document["accessors"][index]
    view = document["bufferViews"][accessor["bufferView"]]
    fmt, size = _COMPONENT[accessor["componentType"]]
    count = _TYPE_COUNT[accessor["type"]]
    stride = accessor.get("byteStride") or view.get("byteStride") or size * count
    base = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    return [
        struct.unpack_from("<" + fmt * count, binary, base + element * stride)
        for element in range(accessor["count"])
    ]


def _slerp(a: Sequence[float], b: Sequence[float], t: float) -> List[float]:
    dot = sum(x * y for x, y in zip(a, b))
    if dot < 0.0:
        b = [-value for value in b]
        dot = -dot
    if dot > 0.9995:
        blended = [x + (y - x) * t for x, y in zip(a, b)]
        length = math.sqrt(sum(value * value for value in blended)) or 1.0
        return [value / length for value in blended]
    theta = math.acos(max(-1.0, min(1.0, dot)))
    sin_theta = math.sin(theta)
    wa = math.sin((1.0 - t) * theta) / sin_theta
    wb = math.sin(t * theta) / sin_theta
    return [x * wa + y * wb for x, y in zip(a, b)]


def _sample(times: Sequence[float], values: Sequence[tuple], time: float, rotation: bool) -> tuple:
    if time <= times[0] + 1.0e-9:
        return tuple(values[0])
    if time >= times[-1] - 1.0e-9:
        return tuple(values[-1])
    for index in range(len(times) - 1):
        if times[index] <= time <= times[index + 1]:
            span = times[index + 1] - times[index]
            t = 0.0 if span <= 1.0e-9 else (time - times[index]) / span
            if rotation:
                return tuple(_slerp(values[index], values[index + 1], t))
            return tuple(a + (b - a) * t for a, b in zip(values[index], values[index + 1]))
    return tuple(values[-1])


def evaluate_clip(document: dict, binary: bytes, clip_name: str, time: float) -> List[Tuple[float, float, float]]:
    """Skinned positions for one clip at ``time``, from the written file.

    ``p_posed = sum_j w_j * (global_j(t) * inverseBind_j) * p_bind`` with the
    local transform of an unkeyed joint left at its rest TRS, exactly the maths
    the runtime's LINEAR pose evaluator implies.
    """
    nodes = document["nodes"]
    parents: List[Optional[int]] = [None] * len(nodes)
    for index, node in enumerate(nodes):
        for child in node.get("children", []):
            parents[child] = index
    locals_: List[Tuple[tuple, tuple, tuple]] = [
        (
            tuple(node.get("translation", (0.0, 0.0, 0.0))),
            tuple(node.get("rotation", (0.0, 0.0, 0.0, 1.0))),
            tuple(node.get("scale", (1.0, 1.0, 1.0))),
        )
        for node in nodes
    ]
    animation = next(
        (entry for entry in document.get("animations", []) if entry.get("name") == clip_name), None
    )
    if animation is None:
        raise SystemExit(f"clip {clip_name!r} is missing from the written GLB")
    for channel in animation["channels"]:
        sampler = animation["samplers"][channel["sampler"]]
        times = [value[0] for value in read_accessor(document, binary, sampler["input"])]
        values = read_accessor(document, binary, sampler["output"])
        node = channel["target"]["node"]
        path = channel["target"]["path"]
        sampled = _sample(times, values, time, path == "rotation")
        translation, rotation, scale = locals_[node]
        if path == "rotation":
            locals_[node] = (translation, sampled, scale)
        elif path == "translation":
            locals_[node] = (sampled, rotation, scale)

    globals_: List[Optional[List[float]]] = [None] * len(nodes)
    for index in range(len(nodes)):
        translation, rotation, scale = locals_[index]
        local = _mat_trs(translation, rotation, scale)
        parent = parents[index]
        if parent is None:
            globals_[index] = local
        else:
            if globals_[parent] is None:
                raise SystemExit("node hierarchy is not parent-before-child")
            globals_[index] = _mat_mul(globals_[parent], local)

    primitive = document["meshes"][0]["primitives"][0]
    attributes = primitive["attributes"]
    positions = read_accessor(document, binary, attributes["POSITION"])
    joints = read_accessor(document, binary, attributes["JOINTS_0"])
    weights = read_accessor(document, binary, attributes["WEIGHTS_0"])
    ibms = [tuple(row) for row in read_accessor(document, binary, document["skins"][0]["inverseBindMatrices"])]
    skin_matrices = [
        _mat_mul(globals_[index], ibms[index]) for index in range(len(ibms))
    ]
    posed: List[Tuple[float, float, float]] = []
    for vertex, position in enumerate(positions):
        accumulator = [0.0, 0.0, 0.0]
        for slot in range(4):
            weight = weights[vertex][slot]
            if weight <= 0.0:
                continue
            point = transform_point(skin_matrices[joints[vertex][slot]], position)
            for axis in range(3):
                accumulator[axis] += weight * point[axis]
        posed.append((accumulator[0], accumulator[1], accumulator[2]))
    return posed


def verify(path: Path) -> dict:
    """Offline geometric check of the written GLB (not engine playback)."""
    document, binary = load_glb(path)
    problems: List[str] = []
    primitive = document["meshes"][0]["primitives"][0]["attributes"]
    joints = read_accessor(document, binary, primitive["JOINTS_0"])
    weights = read_accessor(document, binary, primitive["WEIGHTS_0"])
    positions = read_accessor(document, binary, primitive["POSITION"])
    joint_count = len(document["skins"][0]["joints"])
    for vertex, (slots, shares) in enumerate(zip(joints, weights)):
        total = sum(shares)
        if abs(total - 1.0) > 1.0e-4:
            problems.append(f"vertex {vertex} weights sum to {total:.6f}, not 1.0")
        used = 0
        for slot, share in zip(slots, shares):
            if slot >= joint_count:
                problems.append(f"vertex {vertex} names joint {slot}, outside 0..{joint_count - 1}")
            if share > 0.0:
                used += 1
        if not 1 <= used <= 4:
            problems.append(f"vertex {vertex} uses {used} joint influences, outside 1..4")

    def weighted_to(*names: str, minimum: float = 0.5) -> List[int]:
        wanted = set()
        for name in names:
            for index, node in enumerate(document["nodes"]):
                if node.get("name") == name:
                    wanted.add(index)
        chosen = []
        for vertex, (slots, shares) in enumerate(zip(joints, weights)):
            if sum(share for slot, share in zip(slots, shares) if slot in wanted) >= minimum:
                chosen.append(vertex)
        return chosen

    bounds = {
        "min": [min(point[axis] for point in positions) for axis in range(3)],
        "max": [max(point[axis] for point in positions) for axis in range(3)],
    }
    crown = max(positions[vertex][1] for vertex in weighted_to("head"))
    chest = max(positions[vertex][2] for vertex in weighted_to("chest", "spine_02"))
    foot_vertices = weighted_to("foot_l", "foot_r", "toe_l", "toe_r")
    hand_vertices = weighted_to("hand_l", "hand_r")
    indices = [
        value[0] for value in read_accessor(document, binary, document["meshes"][0]["primitives"][0]["indices"])
    ]

    def edge_stretch(posed: Sequence[Sequence[float]]) -> float:
        """Largest triangle-edge length change between bind and posed mesh."""
        worst = 0.0
        for start in range(0, len(indices) - 2, 3):
            for first, second in ((0, 1), (1, 2), (2, 0)):
                a, b = indices[start + first], indices[start + second]
                bind_length = math.dist(positions[a], positions[b])
                if bind_length <= 1.0e-6:
                    continue
                ratio = math.dist(posed[a], posed[b]) / bind_length
                worst = max(worst, ratio, 1.0 / ratio)
        return worst

    report: Dict[str, dict] = {}
    for clip in ("pose_stand", "pose_arms_up", "pose_arms_forward"):
        posed = evaluate_clip(document, binary, clip, 0.0)
        displacement = max(
            math.dist(posed[vertex], positions[vertex]) for vertex in range(len(positions))
        )
        sample = {
            "max_displacement_m": displacement,
            "max_edge_stretch": edge_stretch(posed),
            "min_y_m": min(point[1] for point in posed),
            "foot_min_y_m": min(posed[vertex][1] for vertex in foot_vertices),
            "hand_tip_y_m": max(posed[vertex][1] for vertex in hand_vertices),
            "hand_tip_z_m": max(posed[vertex][2] for vertex in hand_vertices),
        }
        report[clip] = sample
        # A hand tip legitimately travels 2*r*sin(theta/2) = 1.45 m when a
        # 0.745 m arm swings 155 degrees overhead, so the absolute cap is set
        # just above that; a broken pivot or weight flings vertices much
        # further. The edge-stretch figure below is the sharper detector.
        if displacement > 1.6:
            problems.append(f"clip {clip}: a vertex moves {displacement:.3f} m from its bind position")
        if sample["max_edge_stretch"] > 2.0:
            problems.append(f"clip {clip}: an edge stretches {sample['max_edge_stretch']:.2f}x")
    if abs(report["pose_stand"]["min_y_m"]) > 0.01:
        problems.append(f"pose_stand: the lowest vertex is at y={report['pose_stand']['min_y_m']:.4f}")
    if report["pose_stand"]["foot_min_y_m"] < -0.01:
        problems.append("pose_stand: a foot vertex sinks below the floor")
    if report["pose_arms_up"]["hand_tip_y_m"] < 1.95:
        problems.append("pose_arms_up: the hands do not reach above 1.95 m")
    if report["pose_arms_up"]["hand_tip_y_m"] < crown + 0.20:
        problems.append("pose_arms_up: the hands do not clear the head crown by 0.20 m")
    if report["pose_arms_forward"]["hand_tip_z_m"] < 0.55:
        problems.append("pose_arms_forward: the hands do not reach 0.55 m forward")
    if report["pose_arms_forward"]["hand_tip_z_m"] < chest + 0.40:
        problems.append("pose_arms_forward: the hands do not clear the chest by 0.40 m")
    influences = [sum(1 for share in shares if share > 0.0) for _slots, shares in zip(joints, weights)]
    return {
        "bounds": bounds,
        "bind_crown_y_m": crown,
        "bind_chest_front_z_m": chest,
        "clip_samples": report,
        "foot_vertices": len(foot_vertices),
        "hand_vertices": len(hand_vertices),
        "influence_histogram": {count: influences.count(count) for count in (1, 2, 3, 4)},
        "problems": problems,
    }


# ------------------------------------------------------------------- report


def print_stats(stats: dict, checked: dict, verification: dict) -> None:
    low = verification["bounds"]["min"]
    high = verification["bounds"]["max"]
    size = [high[axis] - low[axis] for axis in range(3)]
    print("mannequin build stats")
    print(f"  path                 {stats['path']}")
    print(f"  bytes                {stats['bytes']}")
    print(f"  vertices             {stats['vertices']}")
    print(f"  triangles            {stats['triangles']}")
    print(f"  joints               {stats['joints']}")
    print(f"  clips                {len(stats['clips'])}")
    print(f"  clip channels        {stats['clip_channels']}")
    print(f"  accessors            {stats['accessors']}")
    print(f"  buffer views         {stats['buffer_views']}")
    print(f"  bind bbox min        [{low[0]:.4f}, {low[1]:.4f}, {low[2]:.4f}]")
    print(f"  bind bbox max        [{high[0]:.4f}, {high[1]:.4f}, {high[2]:.4f}]")
    print(f"  catalog size m       [{size[0]:.3f}, {size[1]:.3f}, {size[2]:.3f}]")
    print(f"  crown / chest front  {verification['bind_crown_y_m']:.3f} m / {verification['bind_chest_front_z_m']:.3f} m")
    print(f"  joint influences     {verification['influence_histogram']} (vertices)")
    print("  clip                  duration   channels")
    for clip in stats["clips"]:
        print(f"    {clip['name']:<20} {clip['duration']:.3f}      {clip['channels']}")
    print("  offline pose samples (from the written bytes)")
    for name, sample in verification["clip_samples"].items():
        print(
            f"    {name:<20} max_move {sample['max_displacement_m']:.4f} m  "
            f"stretch {sample['max_edge_stretch']:.3f}x  min_y {sample['min_y_m']:+.4f} m  "
            f"foot_min_y {sample['foot_min_y_m']:+.4f} m  hand_tip_y {sample['hand_tip_y_m']:.3f} m  "
            f"hand_tip_z {sample['hand_tip_z_m']:+.3f} m"
        )
    print(f"  structural check     {'OK' if not checked['problems'] else checked['problems']}")
    print(f"  geometry check       {'OK' if not verification['problems'] else verification['problems']}")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Build the grey concrete mannequin entity")
    parser.add_argument("--out", default=DEFAULT_OUT, help="output GLB path (relative paths resolve in the repository root)")
    parser.add_argument("--json", action="store_true", help="also print the stats as JSON")
    args = parser.parse_args(argv)

    out_path = Path(args.out)
    if not out_path.is_absolute():
        out_path = (REPO_ROOT / out_path).resolve()

    stats = build(out_path)
    checked = check_model(out_path)
    verification = verify(out_path)
    print_stats(stats, checked, verification)
    if args.json:
        print(json.dumps({"stats": stats, "check": checked, "verify": verification}, indent=2, sort_keys=True))
    if checked["problems"] or verification["problems"]:
        print("MANNEQUIN BUILD FAILED", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
