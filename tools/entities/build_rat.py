#!/usr/bin/env python3
"""Build the Places low-poly rat entity: one skinned GLB with three clips.

The rat is a small quadruped player-facing creature: one mesh, one primitive,
one material, one embedded 256x256 PNG texture, one skin and three genuinely
looping clips (``idle``, ``walk``, ``run``).  It is authored with the shipped
entity writer ``tools/entities/rig.py`` plus the prop toolkit's ``Mesh`` and
``Texture`` classes, so it stays inside every documented pack rule:

* 1 unit = 1 metre, +Y up, **+Z is the front** (the rat faces +Z at yaw 0);
* the origin is the floor-contact point, horizontally centred under the
  bind-pose bounding box, with the lowest paw vertex at ``y = 0``;
* one mesh / one primitive / one material / one embedded PNG;
* every vertex is weighted to 1..4 joints and the weights sum to 1.0;
* clips close exactly (first key == last key on every channel).

Geometry is built from the pack's lathes and swept tubes.  The legs are a
2-bone chain (upper/lower) plus a rigid paw; the rest stance is a crouched
quiet stance, and every clip's foot placement is solved with an analytic
2-link IK so planted paws stay on ``y = 0`` and the walk/run stride implies a
measured ground speed.

Usage::

    python3 build_rat.py                       # build + verify + report
    python3 build_rat.py --check               # verify an existing GLB only
    python3 tools/entities/build_rat.py        # write the shipped asset

The script is deterministic (fixed seeds, no clock, no randomness outside the
seeded texture painter) and uses Blender for the offline surface union.  It resolves the repository root from
its own location, so it works both from ``target/entity-specialists/rat/`` and
after being moved to ``tools/entities/``.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path
from typing import Callable, Dict, List, Optional, Sequence, Tuple

SCRIPT_PATH = Path(__file__).resolve()


def _find_repo_root(start: Path) -> Path:
    """Locates the Places root by its tooling layout, not by a fixed depth."""
    for parent in (start, *start.parents):
        if (
            (parent / "tools" / "entities" / "rig.py").is_file()
            and (parent / "tools" / "props" / "mesh.py").is_file()
        ):
            return parent
    raise SystemExit(f"build_rat.py: cannot locate the Places root above {start}")


REPO_ROOT = _find_repo_root(SCRIPT_PATH.parent)
for _relative in ("tools/entities", "tools/props"):
    _path = str(REPO_ROOT / _relative)
    if _path not in sys.path:
        sys.path.insert(0, _path)

from mesh import Mesh  # noqa: E402  (tools/props on sys.path)
from tex import Texture, decode_png  # noqa: E402
from rig import (  # noqa: E402  (tools/entities on sys.path)
    Clip,
    Rig,
    SkinnedMesh,
    auto_weights,
    check_model,
    quat_compose,
    quat_rot_x,
    quat_rot_y,
    write_model,
)

try:
    GENERATOR = str(SCRIPT_PATH.relative_to(REPO_ROOT)).replace("\\", "/")
except ValueError:  # pragma: no cover - only when run from outside the repo
    GENERATOR = SCRIPT_PATH.name

DEFAULT_OUT = Path("assets") / "entities" / "rat" / "model" / "rat.glb"
TEXTURE_DIR = Path("assets") / "entities" / "rat" / "textures"
DEV_DIR = Path("target") / "entity-specialists" / "rat"
TEXTURE_NAME = "rat_fur_01.png"
REPORT_NAME = "REPORT.md"
PREVIEW_NAME = "rat_preview.png"
ENTITY_NAME = "rat"

# ---------------------------------------------------------------------------
# Asset contract constants
# ---------------------------------------------------------------------------

MAX_TRIANGLES = 1500
TARGET_TRIANGLES = (700, 1200)
MAX_VERTICES = 2500
TEXTURE_SIZE = 256

# Clip table.  Durations sit inside the briefed windows; the duty factor is
# the fraction of the cycle one diagonal pair spends on the ground.
CLIPS = {
    "idle": {"duration": 2.40, "keys": 48, "loop": True, "kind": "idle"},
    "walk": {
        "duration": 0.40,
        "keys": 48,
        "loop": True,
        "kind": "walk",
        "duty": 0.55,
        "stride": 0.04375,
        "lift": 0.008,
        "bob": 0.0022,
        "bob_phase": 0.275,
        "toe": 8.0,
    },
    "run": {
        "duration": 0.24,
        "keys": 48,
        "loop": True,
        "kind": "run",
        "duty": 0.36,
        "stride": 0.04960,
        "lift": 0.022,
        "bob": 0.0045,
        "bob_phase": 0.18,
        "toe": 10.0,
    },
}
# Ground-contact windows (phase fractions) per clip.  Every sample inside a
# window must have its lowest vertex within 0.02 m of the floor; the flight
# windows of the run may not penetrate it at all.
CONTACT_WINDOWS = {
    "walk": ((0.0, 0.55), (0.5, 1.0)),
    "run": ((0.0, 0.36), (0.5, 0.86)),
    "idle": ((0.0, 1.0),),
}
CONTACT_KEYS = {"walk": (0.0, 0.5), "run": (0.0, 0.5), "idle": ()}
FLIGHT_WINDOWS = {"run": ((0.36, 0.5), (0.86, 1.0))}

# Sampling rate of the offline geometric check (samples per second).
VERIFY_HZ = 60.0

# ---------------------------------------------------------------------------
# Skeleton: natural build coordinates.  +X is the rat's left, +Z the nose.
# ---------------------------------------------------------------------------

ROOT_POS = (0.0, 0.0, 0.0)
PELVIS_POS = (0.0, 0.060, -0.085)
SPINE_POS = (0.0, 0.056, -0.013)
CHEST_POS = (0.0, 0.062, 0.057)
NECK_POS = (0.0, 0.074, 0.109)
HEAD_POS = (0.0, 0.080, 0.153)
EAR_POS = {"l": (0.020, 0.096, 0.170), "r": (-0.020, 0.096, 0.170)}
# Rear cap of the torso lathe, used for the report's body-vs-origin note.
TORSO_REAR_Z = -0.132

TAIL_POS = [
    (0.000, 0.070, -0.125),
    (0.000, 0.068, -0.176),
    (0.000, 0.062, -0.226),
    (0.004, 0.052, -0.276),
    (-0.002, 0.040, -0.322),
]
TAIL_TIP = (0.003, 0.028, -0.366)

LEG_ORDER = ("fl", "fr", "rl", "rr")
LEG_PARENT = {"fl": "chest", "fr": "chest", "rl": "pelvis", "rr": "pelvis"}
# Front limbs carry the elbow behind the shoulder line, hind limbs the knee
# in front of the hip line: the crouched digitigrade read of a rat.
KNEE_FORWARD = {"fl": False, "fr": False, "rl": True, "rr": True}
HIP_POS = {
    "fl": (0.030, 0.056, 0.091),
    "fr": (-0.030, 0.056, 0.091),
    "rl": (0.032, 0.058, -0.089),
    "rr": (-0.032, 0.058, -0.089),
}
ANKLE_POS = {
    "fl": (0.030, 0.012, 0.095),
    "fr": (-0.030, 0.012, 0.095),
    "rl": (0.032, 0.012, -0.086),
    "rr": (-0.032, 0.012, -0.086),
}
# Rest bones are 24% longer than the straight hip-to-ankle distance, which is
# what gives every clip room to extend the leg through a stride.
SLACK = 1.24
# Paw lathe: base centre height, profile (along-z, radius), and the vertical
# ellipse that flattens the foot.
PAW_PROFILE = ((0.000, 0.009), (0.007, 0.0135), (0.020, 0.0135), (0.031, 0.0075))
PAW_ELLIPSE_Y = 0.55
PAW_Z_BACK = 0.014
PAW_SEGMENTS = 6
LEG_SEGMENTS = 6

# ---------------------------------------------------------------------------
# Texture: a faded PS2-era rat.  Regions are painted patches of one 256x256
# sheet; each lathe/tube samples its region once (v runs along the part).
# ---------------------------------------------------------------------------

FUR_BASE = (116, 103, 86)
FUR_SIDE = (104, 92, 76)
FUR_DARK = (74, 65, 53)
FUR_BACK = (84, 74, 60)
FUR_HEAD = (110, 98, 82)
BELLY = (194, 184, 162)
BELLY_SHADE = (168, 156, 136)
EAR_PINK = (174, 126, 120)
EAR_DARK = (132, 94, 90)
NOSE_PINK = (184, 132, 126)
PAW_PALE = (186, 176, 156)
PAW_PINK = (172, 134, 126)
TAIL_BASE = (146, 124, 108)
TAIL_RING = (116, 96, 84)
EYE_DARK = (26, 22, 20)
EYE_LIGHT = (216, 208, 192)
WHISKER = (196, 188, 170)

REGIONS = {
    "body": (0, 0, 128, 112),
    "head": (128, 0, 128, 80),
    "snout": (128, 80, 64, 48),
    "ear": (192, 80, 64, 48),
    "leg": (0, 112, 128, 72),
    "paw": (128, 128, 64, 56),
    "tail": (0, 184, 256, 72),
}


def build_texture() -> Texture:
    """Paints the single 256x256 fur sheet."""
    tex = Texture(TEXTURE_SIZE, seed=20260926)
    # Defensive base: any atlas pixel a UV accidentally reaches is fur, never
    # transparent black.
    tex.region("_base", (0, 0, TEXTURE_SIZE, TEXTURE_SIZE))
    tex.fill("_base", FUR_BASE, jitter=5, seed=3)
    for name, rect in REGIONS.items():
        tex.region(name, rect)

    # --- torso: grey-brown back, pale belly that widens toward the chest ----
    tex.fill("body", FUR_SIDE, jitter=7, seed=21)
    tex.noise("body", amount=7, freq=3, seed=22)
    tex.streaks("body", FUR_DARK, count=24, seed=23, alpha=40)
    tex.spots("body", FUR_DARK, count=34, seed=24, radius=3, alpha=38)
    tex.fill("body", FUR_BACK, sub=(0.00, 0.0, 0.16, 1.0), jitter=5, seed=25)
    tex.fill("body", FUR_BACK, sub=(0.88, 0.0, 1.00, 1.0), jitter=5, seed=26)
    _body_width, height = tex.cell("body")[2:]
    step = 1.0 / height
    for row in range(height):
        t = row / max(1, height - 1)  # 0 = chest end (region top), 1 = rump
        half = 0.115 + 0.070 * (1.0 - t) ** 1.4
        wobble = 0.012 * math.sin(t * 13.0) + 0.006 * math.sin(t * 29.0)
        tex.bar(
            "body",
            BELLY,
            (max(0.0, 0.5 + wobble - half), t - step * 0.5,
             min(1.0, 0.5 + wobble + half), t + step * 0.5),
        )
    tex.spots("body", BELLY_SHADE, count=44, seed=27, radius=2, alpha=48,
              sub=(0.30, 0.0, 0.70, 1.0))

    # --- head: dark crown, pale chin, two eyes near the front of the skull --
    tex.fill("head", FUR_HEAD, jitter=6, seed=31)
    tex.noise("head", amount=6, freq=3, seed=32)
    tex.fill("head", FUR_BACK, sub=(0.00, 0.0, 0.16, 1.0), jitter=5, seed=33)
    tex.fill("head", FUR_BACK, sub=(0.84, 0.0, 1.00, 1.0), jitter=5, seed=34)
    tex.fill("head", BELLY, sub=(0.38, 0.0, 0.62, 1.0), jitter=5, seed=35)
    tex.spots("head", BELLY_SHADE, count=18, seed=36, radius=2, alpha=45,
              sub=(0.34, 0.0, 0.66, 1.0))
    head_w, head_h = tex.cell("head")[2:]
    for centre_u in (0.135, 0.865):
        cx = int(round(centre_u * head_w))
        cy = int(round(0.26 * head_h))
        for dy in range(-1, 2):
            for dx in range(-1, 2):
                if dx * dx + dy * dy <= 2:
                    tex.bar(
                        "head",
                        EYE_DARK,
                        ((cx + dx) / head_w, (cy + dy) / head_h,
                         (cx + dx + 1) / head_w, (cy + dy + 1) / head_h),
                    )
        tex.bar(
            "head",
            EYE_LIGHT,
            ((cx - 1) / head_w, (cy - 1) / head_h,
             cx / head_w, cy / head_h),
        )

    # --- snout: pale fur, pink nose tip where the end cap samples ----------
    tex.fill("snout", BELLY, jitter=5, seed=41)
    tex.noise("snout", amount=5, freq=2, seed=42)
    tex.fill("snout", BELLY_SHADE, sub=(0.0, 0.0, 1.0, 0.30), jitter=4, seed=43)
    tex.fill("snout", NOSE_PINK, sub=(0.30, 0.55, 0.70, 0.90), jitter=3, seed=44)
    snout_w, snout_h = tex.cell("snout")[2:]
    for whisker in range(4):
        row = int(round((0.28 + 0.14 * whisker) * snout_h))
        for col in range(snout_w):
            if (col + row) % 3 == 0:
                tex.bar(
                    "snout",
                    WHISKER,
                    (col / snout_w, row / snout_h, (col + 1) / snout_w, (row + 1) / snout_h),
                )

    # --- ear: pink cup with a furred rim -----------------------------------
    tex.fill("ear", EAR_PINK, jitter=5, seed=51)
    tex.fill("ear", EAR_DARK, sub=(0.0, 0.0, 0.5, 1.0), jitter=4, seed=52)
    tex.border("ear", FUR_DARK, width=2, alpha=140)

    # --- legs: darker at the hip (region top = hip in tube UVs), pale foot --
    tex.fill("leg", FUR_SIDE, jitter=6, seed=61)
    tex.noise("leg", amount=6, freq=3, seed=62)
    tex.gradient("leg", PAW_PALE, FUR_DARK, jitter=4, seed=63)
    tex.spots("leg", FUR_DARK, count=16, seed=64, radius=2, alpha=35)

    # --- paws: cream with toe separations and pink pads --------------------
    tex.fill("paw", PAW_PALE, jitter=5, seed=71)
    tex.spots("paw", PAW_PINK, count=10, seed=72, radius=3, alpha=70,
              sub=(0.20, 0.30, 0.80, 0.90))
    for toe in (0.30, 0.62):
        tex.bar("paw", (150, 140, 124), (toe, 0.30, toe + 0.035, 0.95))

    # --- tail: grey-pink with faint rings ----------------------------------
    tex.fill("tail", TAIL_BASE, jitter=6, seed=81)
    tex.noise("tail", amount=5, freq=3, seed=82)
    _tail_w, tail_h = tex.cell("tail")[2:]
    for ring in range(10):
        row = int(round((0.06 + 0.095 * ring) * tail_h))
        if row + 2 >= tail_h:
            break
        tex.bar("tail", TAIL_RING, (0.0, row / tail_h, 1.0, (row + 2) / tail_h), alpha=90)
    tex.bar("tail", PAW_PINK, (0.0, 0.86, 1.0, 1.0), alpha=120)
    return tex


# ---------------------------------------------------------------------------
# Geometry
# ---------------------------------------------------------------------------


def _rel(child: Sequence[float], parent: Sequence[float]) -> Tuple[float, float, float]:
    return (child[0] - parent[0], child[1] - parent[1], child[2] - parent[2])


def _add(a: Sequence[float], b: Sequence[float]) -> Tuple[float, float, float]:
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def _sub(a: Sequence[float], b: Sequence[float]) -> Tuple[float, float, float]:
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _scale(v: Sequence[float], factor: float) -> Tuple[float, float, float]:
    return (v[0] * factor, v[1] * factor, v[2] * factor)


def _length(v: Sequence[float]) -> float:
    return math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2])


def _normalize(v: Sequence[float]) -> Tuple[float, float, float]:
    length = _length(v)
    if length <= 1.0e-12:
        raise ValueError("cannot normalise a zero vector")
    return (v[0] / length, v[1] / length, v[2] / length)


def _cross(a: Sequence[float], b: Sequence[float]) -> Tuple[float, float, float]:
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def _rot_x_point(v: Sequence[float], angle: float) -> Tuple[float, float, float]:
    """Rotates a point about +X by ``angle`` radians (right-handed)."""
    cos, sin = math.cos(angle), math.sin(angle)
    return (v[0], v[1] * cos - v[2] * sin, v[1] * sin + v[2] * cos)


def rest_knee(
    hip: Sequence[float],
    ankle: Sequence[float],
    slack: float,
    knee_forward: bool,
) -> Tuple[float, float, float]:
    """Rest knee/elbow position of an equally-boned two-link chain."""
    dy, dz = ankle[1] - hip[1], ankle[2] - hip[2]
    span = math.hypot(dy, dz)
    bone = slack * span * 0.5
    half = span * 0.5
    offset = math.sqrt(max(0.0, bone * bone - half * half))
    mid = ((hip[1] + ankle[1]) * 0.5, (hip[2] + ankle[2]) * 0.5)
    # Perpendicular of (dy, dz) in the y-z plane, positive-z when the chain
    # points straight down.
    perp = (dz / span, -dy / span)
    sign = 1.0 if knee_forward else -1.0
    return (
        hip[0],
        mid[0] + sign * offset * perp[0],
        mid[1] + sign * offset * perp[1],
    )


def _triangle_outward(
    mesh: Mesh,
    a: Sequence[float],
    b: Sequence[float],
    c: Sequence[float],
    outward: Sequence[float],
    uvs: Sequence[Tuple[float, float]],
    color,
    shade_mult: float = 1.0,
) -> None:
    """Adds a triangle with the winding fixed to face ``outward``."""
    normal = _cross(_sub(b, a), _sub(c, a))
    if normal[0] * outward[0] + normal[1] * outward[1] + normal[2] * outward[2] < 0.0:
        b, c = c, b
        uvs = (uvs[0], uvs[2], uvs[1])
    mesh.triangle(a, b, c, uvs, color, shade_mult=shade_mult)


def _add_ear(mesh: Mesh, tex: Texture, side: float, ear_pos: Sequence[float]) -> None:
    """Closed rounded pinna with a narrow root buried in the skull."""
    start = len(mesh.positions)
    mesh.lathe(
        base=ear_pos,
        profile=[(0.0, 0.007), (0.010, 0.014), (0.022, 0.012), (0.030, 0.007), (0.033, 0.002)],
        segments=6, axis="y", ellipse=(0.24, 1.0),
        uv=tex.uv("ear", inset=1.0), color=WHITE_TINT,
        cap_start=True, cap_end=True,
    )
    for index in range(start, len(mesh.positions)):
        x, y, z = mesh.positions[index]
        mesh.positions[index] = (x + side * 0.24 * (y - ear_pos[1]), y, z)


WHITE_TINT = (255, 255, 255)


def build_mesh(tex: Texture, kness: Dict[str, Tuple[float, float, float]]) -> Tuple[Mesh, Dict[str, Tuple[int, int]]]:
    """Builds the whole rat mesh, recording each vertex range by part name."""
    mesh = Mesh()
    parts: Dict[str, Tuple[int, int]] = {}

    def mark(name: str, start: int) -> None:
        parts[name] = (start, len(mesh.positions))

    # --- torso: a rounded spindle from rump to shoulders -------------------
    start = len(mesh.positions)
    mesh.lathe(
        base=(0.0, 0.060, TORSO_REAR_Z),
        profile=[
            (0.000, 0.014),
            (0.018, 0.040),
            (0.052, 0.050),
            (0.098, 0.050),
            (0.150, 0.043),
            (0.196, 0.037),
            (0.228, 0.033),
            (0.250, 0.018),
        ],
        segments=8,
        axis="z",
        uv=tex.uv("body", inset=1.0),
        cap_uv=tex.sub("body", 0.40, 0.06, 0.60, 0.14),
        color=WHITE_TINT,
        ellipse=(1.0, 0.88),
        rotation=math.pi / 2,
    )
    mark("torso", start)

    # --- neck: a short rising sleeve joining chest and skull ----------------
    start = len(mesh.positions)
    neck_base_z = 0.100
    neck_length = 0.056
    neck_rise = 0.016
    neck_start = len(mesh.positions)
    mesh.lathe(
        base=(0.0, 0.062, neck_base_z),
        profile=[(0.000, 0.030), (0.028, 0.026), (neck_length, 0.0215)],
        segments=8,
        axis="z",
        uv=tex.uv("body", inset=1.0),
        cap_uv=tex.sub("body", 0.40, 0.06, 0.60, 0.14),
        color=WHITE_TINT,
        ellipse=(0.90, 0.88),
        rotation=math.pi / 2,
        cap_start=True,
        cap_end=True,
        )
    for index in range(neck_start, len(mesh.positions)):
        x, y, z = mesh.positions[index]
        mesh.positions[index] = (x, y + neck_rise * (z - neck_base_z) / neck_length, z)
    mark("neck", start)

    # --- head: rounded skull, slightly longer than it is wide ---------------
    start = len(mesh.positions)
    mesh.lathe(
        base=(0.0, 0.078, 0.146),
        profile=[
            (0.000, 0.021),
            (0.016, 0.033),
            (0.044, 0.037),
            (0.068, 0.028),
            (0.078, 0.014),
        ],
        segments=8,
        axis="z",
        uv=tex.uv("head", inset=1.0),
        cap_uv=tex.sub("head", 0.42, 0.86, 0.58, 0.98),
        color=WHITE_TINT,
        ellipse=(0.95, 0.92),
        rotation=math.pi / 2,
        cap_start=True,
        cap_end=True,
    )
    mark("head", start)

    # --- snout: pointed muzzle with the pink nose cap -----------------------
    start = len(mesh.positions)
    mesh.lathe(
        base=(0.0, 0.070, 0.202),
        profile=[(0.000, 0.022), (0.014, 0.019), (0.030, 0.013), (0.044, 0.006)],
        segments=8,
        axis="z",
        uv=tex.uv("snout", inset=1.0),
        cap_uv=tex.sub("snout", 0.30, 0.55, 0.70, 0.90),
        color=WHITE_TINT,
        ellipse=(1.0, 0.72),
        rotation=math.pi / 2,
        cap_start=True,
        cap_end=True,
    )
    mark("snout", start)

    # --- ears ---------------------------------------------------------------
    for side_name, side in (("l", 1.0), ("r", -1.0)):
        start = len(mesh.positions)
        _add_ear(mesh, tex, side, EAR_POS[side_name])
        mark(f"ear_{side_name}", start)

    # --- legs and paws ------------------------------------------------------
    for leg in LEG_ORDER:
        hip = HIP_POS[leg]
        ankle = ANKLE_POS[leg]
        knee = kness[leg]
        up_dir = _normalize(_sub(hip, knee))
        down_dir = _normalize(_sub(knee, ankle))
        upper_start = _add(hip, _scale(up_dir, 0.012))
        upper_end = _add(knee, _scale(up_dir, -0.004))
        lower_start = _add(knee, _scale(up_dir, 0.004))
        lower_end = _add(ankle, _scale(down_dir, -0.002))

        start = len(mesh.positions)
        mesh.tube_path(
            points=[upper_start, knee, upper_end],
            radii=[0.0125, 0.0105, 0.0088],
            segments=LEG_SEGMENTS,
            uv=tex.uv("leg", inset=1.0),
            color=WHITE_TINT,
            cap_start=True,
            cap_end=True,
            )
        mark(f"leg_{leg}_upper", start)

        start = len(mesh.positions)
        mesh.tube_path(
            points=[lower_start, ankle, lower_end],
            radii=[0.0080, 0.0062, 0.0052],
            segments=LEG_SEGMENTS,
            uv=tex.uv("leg", inset=1.0),
            color=WHITE_TINT,
            cap_start=True,
            cap_end=True,
            )
        mark(f"leg_{leg}_lower", start)

        start = len(mesh.positions)
        mesh.lathe(
            base=(ankle[0], PAW_PROFILE[1][1] * PAW_ELLIPSE_Y, ankle[2] - PAW_Z_BACK),
            profile=list(PAW_PROFILE),
            segments=PAW_SEGMENTS,
            axis="z",
            uv=tex.uv("paw", inset=1.0),
            cap_uv=tex.sub("paw", 0.40, 0.60, 0.60, 0.90),
            color=WHITE_TINT,
            ellipse=(1.0, PAW_ELLIPSE_Y),
            rotation=math.pi / 2,
            cap_start=True,
            cap_end=True,
            )
        mark(f"leg_{leg}_paw", start)

    # --- tail: one swept tapered tube along the five tail joints ------------
    start = len(mesh.positions)
    tail_points = [(0.0, 0.073, -0.115), *TAIL_POS, TAIL_TIP]
    mesh.tube_path(
        points=tail_points,
        radii=[0.0145, 0.0130, 0.0115, 0.0095, 0.0075, 0.0055, 0.0030],
        segments=LEG_SEGMENTS,
        uv=tex.uv("tail", inset=1.0),
        color=WHITE_TINT,
        cap_start=True,
        cap_end=True,
        )
    mark("tail", start)

    from rat_surface import union_surface
    return union_surface(mesh, parts)


# ---------------------------------------------------------------------------
# Rig
# ---------------------------------------------------------------------------


def build_rig(
    shift: Sequence[float],
    kness: Dict[str, Tuple[float, float, float]],
) -> Tuple[Rig, Dict[str, object]]:
    """Builds the joint hierarchy; all rest rotations are identity."""
    rig = Rig()
    index: Dict[str, int] = {}

    def joint(name: str, parent: Optional[str], translation: Sequence[float],
              tail: Optional[Sequence[float]] = None) -> None:
        parent_index = index[parent] if parent is not None else None
        index[name] = rig.add(name, parent_index, translation, tail=tail)

    joint("root", None, (0.0, 0.0, 0.0))
    # The mesh normalisation shift rides on the pelvis so the root joint stays
    # on the model origin (the floor-contact point).
    joint(
        "pelvis", "root",
        _add(_rel(PELVIS_POS, ROOT_POS), shift),
    )
    joint("spine", "pelvis", _rel(SPINE_POS, PELVIS_POS))
    joint("chest", "spine", _rel(CHEST_POS, SPINE_POS))
    joint("neck", "chest", _rel(NECK_POS, CHEST_POS))
    joint("head", "neck", _rel(HEAD_POS, NECK_POS), tail=(0.0, -0.004, 0.086))
    joint("ear_l", "head", _rel(EAR_POS["l"], HEAD_POS), tail=(0.006, 0.026, -0.004))
    joint("ear_r", "head", _rel(EAR_POS["r"], HEAD_POS), tail=(-0.006, 0.026, -0.004))

    for leg in LEG_ORDER:
        parent = LEG_PARENT[leg]
        knee = kness[leg]
        ankle = ANKLE_POS[leg]
        joint(f"leg_{leg}_upper", parent, _rel(HIP_POS[leg], {"chest": CHEST_POS, "pelvis": PELVIS_POS}[parent]))
        joint(f"leg_{leg}_lower", f"leg_{leg}_upper", _rel(knee, HIP_POS[leg]))
        joint(
            f"leg_{leg}_paw", f"leg_{leg}_lower", _rel(ankle, knee),
            tail=(0.0, -0.012, 0.002),
        )

    for number in range(1, 6):
        name = f"tail_{number:02d}"
        position = TAIL_POS[number - 1]
        if number == 1:
            joint(name, "pelvis", _rel(position, PELVIS_POS))
        else:
            joint(name, f"tail_{number - 1:02d}", _rel(position, TAIL_POS[number - 2]))
    rig.joints[index["tail_05"]].tail = _rel(TAIL_TIP, TAIL_POS[4])
    rig.finish()

    anchors = {
        "index": index,
        "shift": tuple(float(v) for v in shift),
        "hip": {leg: _add(HIP_POS[leg], shift) for leg in LEG_ORDER},
        "knee": {leg: _add(kness[leg], shift) for leg in LEG_ORDER},
        "ankle": {leg: _add(ANKLE_POS[leg], shift) for leg in LEG_ORDER},
        "position": {
            "root": (0.0, 0.0, 0.0),
            "pelvis": _add(PELVIS_POS, shift),
            "spine": _add(SPINE_POS, shift),
            "chest": _add(CHEST_POS, shift),
            "neck": _add(NECK_POS, shift),
            "head": _add(HEAD_POS, shift),
        },
        "rest_angles": {
            leg: (
                -math.atan2(kness[leg][2] - HIP_POS[leg][2], HIP_POS[leg][1] - kness[leg][1]),
                -math.atan2(ANKLE_POS[leg][2] - kness[leg][2], kness[leg][1] - ANKLE_POS[leg][1]),
            )
            for leg in LEG_ORDER
        },
        "bone_lengths": {
            leg: (
                math.dist(HIP_POS[leg], kness[leg]),
                math.dist(kness[leg], ANKLE_POS[leg]),
            )
            for leg in LEG_ORDER
        },
    }
    return rig, anchors


# ---------------------------------------------------------------------------
# Skin weights
# ---------------------------------------------------------------------------

# (part group, candidate joints, sigma, max influences, minimum share).  The
# paw is rigid; the ear bases and the tail root blend into their parents so
# nothing tears when they move.
WEIGHT_GROUPS = [
    ("torso", ("pelvis", "spine", "chest", "neck"), 0.055, 3, 0.15),
    ("neck", ("chest", "neck", "head"), 0.035, 3, 0.15),
    ("head", ("neck", "head"), 0.040, 2, 0.15),
    ("snout", ("head",), 1.0, 1, 0.0),
    ("ear_l", ("head", "ear_l"), 0.020, 2, 0.0),
    ("ear_r", ("head", "ear_r"), 0.020, 2, 0.0),
    ("tail", ("pelvis", "tail_01", "tail_02", "tail_03", "tail_04", "tail_05"), 0.040, 2, 0.15),
]
for _leg in LEG_ORDER:
    _parent = LEG_PARENT[_leg]
    WEIGHT_GROUPS.extend([
        (f"leg_{_leg}_upper", (_parent, f"leg_{_leg}_upper", f"leg_{_leg}_lower"), 0.028, 2, 0.10),
        (f"leg_{_leg}_lower", (f"leg_{_leg}_upper", f"leg_{_leg}_lower", f"leg_{_leg}_paw"), 0.020, 2, 0.10),
        (f"leg_{_leg}_paw", (f"leg_{_leg}_paw",), 1.0, 1, 0.0),
    ])


def _point_segment_distance(
    point: Sequence[float],
    start: Sequence[float],
    end: Sequence[float],
) -> float:
    dx, dy, dz = end[0] - start[0], end[1] - start[1], end[2] - start[2]
    length_squared = dx * dx + dy * dy + dz * dz
    if length_squared <= 1.0e-12:
        return math.dist(point, start)
    t = (
        (point[0] - start[0]) * dx + (point[1] - start[1]) * dy + (point[2] - start[2]) * dz
    ) / length_squared
    t = min(1.0, max(0.0, t))
    nearest = (start[0] + dx * t, start[1] + dy * t, start[2] + dz * t)
    return math.dist(point, nearest)


def assign_weights(skinned: SkinnedMesh, rig: Rig, parts: Dict[str, Tuple[int, int]]) -> None:
    """Pins every vertex with a deterministic nearest-segment blend.

    Every vertex gets an explicit override because each part needs its own
    candidate joint set (a belly vertex must not follow a leg bone, a tail
    vertex must not follow the pelvis), which the unrestricted nearest-segment
    blend cannot express.  ``auto_weights`` still performs the assignment and
    the 1..4 slot normalisation.
    """
    pairs: Dict[int, List[Tuple[int, float]]] = {}
    for group, candidates, sigma, max_influences, min_share in WEIGHT_GROUPS:
        start, end = parts[group]
        bones = [(rig.index(name), rig.segment(rig.index(name))) for name in candidates]
        for vertex in range(start, end):
            point = skinned.mesh.positions[vertex]
            scored = []
            for joint, (a, b) in bones:
                distance = _point_segment_distance(point, a, b)
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
            pairs[vertex] = [(joint, weight / total) for weight, joint in kept]
    coincident = {}
    for vertex, point in enumerate(skinned.mesh.positions):
        coincident.setdefault(tuple(round(v, 7) for v in point), []).append(vertex)
    for vertices in coincident.values():
        totals = {}
        for vertex in vertices:
            for joint, weight in pairs[vertex]:
                totals[joint] = totals.get(joint, 0.0) + weight
        strongest = sorted(totals.items(), key=lambda item: (-item[1], item[0]))[:4]
        total = sum(weight for _, weight in strongest)
        for vertex in vertices:
            pairs[vertex] = [(joint, weight / total) for joint, weight in strongest]
    # Boolean junctions can create very short edges across anatomical parts.
    # Sample one continuous spatial weight field there, rather than assigning
    # conflicting part-local weights to either end of an almost-zero edge.
    samples = [(skinned.mesh.positions[vertices[0]], pairs[vertices[0]])
               for vertices in coincident.values()]
    for vertices in coincident.values():
        point = skinned.mesh.positions[vertices[0]]
        totals = {}
        for neighbor, influences in samples:
            distance = math.dist(point, neighbor)
            if distance > 0.045:
                continue
            proximity = math.exp(-((distance / 0.018) ** 2))
            for joint, weight in influences:
                totals[joint] = totals.get(joint, 0.0) + proximity * weight
        if point[1] < 0.020:
            # Ease continuously into a rigid sole, including tiny union edges
            # on the ankle. A hard height cutoff creates a new weight seam.
            sole = min((rig.index(f"leg_{leg}_paw") for leg in LEG_ORDER),
                       key=lambda joint: _point_segment_distance(point, *rig.segment(joint)))
            blend = _smoothstep(max(0.0, (point[1] - 0.006) / 0.014))
            original_total = sum(totals.values())
            totals = {joint: weight * blend for joint, weight in totals.items()}
            totals[sole] = totals.get(sole, 0.0) + original_total * (1.0 - blend)
        # Sharpen the continuous field before limiting to four slots, so a
        # fifth-place influence crossing the cutoff cannot carry a large share.
        strongest = sorted(((joint, weight ** 2) for joint, weight in totals.items()),
                           key=lambda item: (-item[1], item[0]))[:4]
        total = sum(weight for _, weight in strongest)
        for vertex in vertices:
            pairs[vertex] = [(joint, weight / total) for joint, weight in strongest]
    auto_weights(skinned, rig, sigma=0.05, overrides=pairs)


# ---------------------------------------------------------------------------
# Poses: body chain, analytic two-link leg IK, clip functions
# ---------------------------------------------------------------------------

CONTACT_PHASE = {"fl": 0.0, "rr": 0.0, "fr": 0.5, "rl": 0.5}


def _smoothstep(t: float) -> float:
    t = min(1.0, max(0.0, t))
    return t * t * (3.0 - 2.0 * t)


def body_frame(
    position: Dict[str, Tuple[float, float, float]],
    pitch: Dict[str, float],
    pelvis_offset: Sequence[float],
) -> Tuple[Dict[str, float], Dict[str, Tuple[float, float, float]]]:
    """Absolute pitch (degrees) and origin of every body-chain joint."""
    absolute = {"root": 0.0}
    origin = {"root": position["root"]}
    absolute["pelvis"] = pitch.get("pelvis", 0.0)
    origin["pelvis"] = _add(position["pelvis"], pelvis_offset)
    for name, parent in (("spine", "pelvis"), ("chest", "spine"), ("neck", "chest"), ("head", "neck")):
        relative = _sub(position[name], position[parent])
        origin[name] = _add(origin[parent], _rot_x_point(relative, math.radians(absolute[parent])))
        absolute[name] = absolute[parent] + pitch.get(name, 0.0)
    return absolute, origin


def solve_leg(
    anchors: Dict[str, object],
    leg: str,
    parent_pitch: float,
    parent_origin: Sequence[float],
    target_yz: Tuple[float, float],
    toe_degrees: float,
) -> Tuple[float, float, float, Tuple[float, float, float]]:
    """Local X-rotation deltas (upper, lower, paw) for one leg.

    The IK runs in the leg parent's frame: the ankle target is rotated back
    into that frame, solved as a two-bone planar chain, and returned as
    deltas from the rest segment angles the rig was built with.
    """
    position = anchors["position"]
    hip_rest = anchors["hip"][leg]
    parent_name = LEG_PARENT[leg]
    parent = position[parent_name]
    hip = _add(
        parent_origin,
        _rot_x_point(_sub(hip_rest, parent), math.radians(parent_pitch)),
    )
    dy = target_yz[0] - hip[1]
    dz = target_yz[1] - hip[2]
    cos, sin = math.cos(-math.radians(parent_pitch)), math.sin(-math.radians(parent_pitch))
    dy, dz = dy * cos - dz * sin, dy * sin + dz * cos

    l1, l2 = anchors["bone_lengths"][leg]
    distance = math.hypot(dy, dz)
    distance = min(max(distance, abs(l1 - l2) + 1.0e-6), l1 + l2 - 1.0e-6)
    alpha = math.atan2(dz, -dy)
    cosine = (l1 * l1 + distance * distance - l2 * l2) / (2.0 * l1 * distance)
    beta = math.acos(min(1.0, max(-1.0, cosine)))
    sign = 1.0 if KNEE_FORWARD[leg] else -1.0
    phi1 = alpha + sign * beta
    knee_y = -l1 * math.cos(phi1)
    knee_z = l1 * math.sin(phi1)
    phi2 = math.atan2(dz - knee_z, -(dy - knee_y))
    theta1 = -phi1
    theta2 = -phi2
    rest1, rest2 = anchors["rest_angles"][leg]
    upper = math.degrees(theta1 - rest1)
    lower = math.degrees((theta2 - rest2) - (theta1 - rest1))
    paw = -(parent_pitch + upper + lower) + toe_degrees
    return upper, lower, paw, hip


def _foot_target(
    leg: str,
    ankle: Tuple[float, float, float],
    phase: float,
    duty: float,
    stride: float,
    lift: float,
    toe_degrees: float,
) -> Tuple[float, float, float]:
    """Ankle target ``(y, z)`` and toe angle for one leg at a local phase."""
    ground_y = ankle[1]
    half = stride * 0.5
    if phase < duty:
        stance = phase / duty
        z = ankle[2] + half - stride * stance
        if stance > 0.85:
            ramp = _smoothstep((stance - 0.85) / 0.15)
            y = ground_y + 0.004 * ramp
            toe = toe_degrees * ramp
        else:
            y = ground_y
            toe = 0.0
    else:
        swing = (phase - duty) / (1.0 - duty)
        z = ankle[2] - half + stride * _smoothstep(swing)
        y = ground_y + 0.004 * (1.0 - _smoothstep(swing)) + lift * math.sin(math.pi * swing)
        toe = toe_degrees * (1.0 - _smoothstep(swing / 0.35)) if swing < 0.35 else 0.0
    return y, z, toe


def _tail_rotations(
    phase: float,
    yaw_amplitude: float,
    pitch_base: float,
    pitch_amplitude: float,
    speed_phase: float,
) -> Dict[str, Tuple[float, float, float, float]]:
    rotations = {}
    for number in range(1, 6):
        lag = speed_phase * number
        yaw = yaw_amplitude * math.sin(math.tau * (phase - lag))
        pitch = pitch_base + pitch_amplitude * math.sin(math.tau * (phase - lag + 0.25))
        rotations[f"tail_{number:02d}"] = quat_compose(quat_rot_y(yaw), quat_rot_x(pitch))
    return rotations


def idle_pose(
    phase: float,
    rig_positions: Dict[str, Tuple[float, float, float]],
    anchors: Dict[str, object],
) -> Tuple[dict, dict]:
    """Breathing, a slow head turn, tail sway; feet solved to stay planted."""
    breath = math.sin(math.tau * 2.0 * phase)
    sway = math.sin(math.tau * phase)
    pitch = {
        "pelvis": 0.0,
        "spine": 0.70 * breath,
        "chest": 0.55 * math.sin(math.tau * 2.0 * phase + 0.35),
        "neck": -0.45 * breath,
        "head": 1.10 * math.sin(math.tau * 2.0 * phase + 0.8) + 0.40 * math.sin(math.tau * phase + 0.3),
    }
    absolute, origin = body_frame(rig_positions, pitch, (0.0, 0.0, 0.0))
    rotations = {
        "pelvis": quat_rot_x(pitch["pelvis"]),
        "spine": quat_rot_x(pitch["spine"]),
        "chest": quat_rot_x(pitch["chest"]),
        "neck": quat_compose(quat_rot_x(pitch["neck"]), quat_rot_y(1.5 * math.sin(math.tau * phase + 0.6))),
        "head": quat_compose(quat_rot_x(pitch["head"]), quat_rot_y(2.5 * math.sin(math.tau * phase - 0.2))),
        "ear_l": quat_compose(quat_rot_x(-1.5 + 1.5 * math.sin(math.tau * phase + 0.7)),
                              quat_rot_y(2.0 * math.sin(math.tau * 2.0 * phase))),
        "ear_r": quat_compose(quat_rot_x(-1.5 + 1.5 * math.sin(math.tau * phase + 0.9)),
                              quat_rot_y(-2.0 * math.sin(math.tau * 2.0 * phase))),
    }
    rotations.update(_tail_rotations(phase, 3.5, 1.5, 0.8, 0.15))
    for leg in LEG_ORDER:
        ankle = anchors["ankle"][leg]
        upper, lower, paw, _ = solve_leg(
            anchors, leg, absolute[LEG_PARENT[leg]], origin[LEG_PARENT[leg]],
            (ankle[1], ankle[2]), 0.0,
        )
        rotations[f"leg_{leg}_upper"] = quat_rot_x(upper)
        rotations[f"leg_{leg}_lower"] = quat_rot_x(lower)
        rotations[f"leg_{leg}_paw"] = quat_rot_x(paw)
    return rotations, {}


def locomotion_pose(
    phase: float,
    config: dict,
    rig_positions: Dict[str, Tuple[float, float, float]],
    anchors: Dict[str, object],
) -> Tuple[dict, dict]:
    """One diagonal-pair gait cycle, solved from the foot targets."""
    duty = config["duty"]
    stride = config["stride"]
    lift = config["lift"]
    running = config["kind"] == "run"
    two = math.tau * 2.0 * phase

    bob = -config["bob"] * math.cos(math.tau * 2.0 * (phase - config["bob_phase"]))
    if running:
        pitch = {
            "pelvis": 0.0,
            "spine": 1.2 + 1.4 * math.sin(two + 0.4),
            "chest": -1.8 - 1.0 * math.sin(two + 0.4),
            "neck": -0.8 + 0.6 * math.sin(2.0 * math.tau * phase + 1.0),
            "head": -1.0 + 1.5 * math.sin(2.0 * math.tau * phase + 1.0),
        }
        ear_base, ear_swing = -9.0, 2.5
        head_yaw = 0.8 * math.sin(math.tau * phase)
        tail_kwargs = dict(yaw_amplitude=3.0, pitch_base=5.0, pitch_amplitude=2.5, speed_phase=0.15)
    else:
        pitch = {
            "pelvis": 0.0,
            "spine": 0.9 * math.sin(two + 0.6),
            "chest": -0.7 * math.sin(two + 0.6) + 0.4 * math.sin(math.tau * phase),
            "neck": -0.6 * math.sin(two + 0.9),
            "head": 1.2 * math.sin(two + 1.2),
        }
        ear_base, ear_swing = -3.0, 3.0
        head_yaw = 1.6 * math.sin(math.tau * phase)
        tail_kwargs = dict(yaw_amplitude=4.5, pitch_base=1.5, pitch_amplitude=1.8, speed_phase=0.13)

    absolute, origin = body_frame(rig_positions, pitch, (0.0, bob, 0.0))
    rotations = {
        "pelvis": quat_rot_x(pitch["pelvis"]),
        "spine": quat_rot_x(pitch["spine"]),
        "chest": quat_rot_x(pitch["chest"]),
        "neck": quat_rot_x(pitch["neck"]),
        "head": quat_compose(quat_rot_x(pitch["head"]), quat_rot_y(head_yaw)),
        "ear_l": quat_compose(quat_rot_x(ear_base + ear_swing * math.sin(two + 0.5)),
                              quat_rot_y(2.0 * math.sin(math.tau * phase))),
        "ear_r": quat_compose(quat_rot_x(ear_base + ear_swing * math.sin(two + 0.8)),
                              quat_rot_y(-2.0 * math.sin(math.tau * phase))),
    }
    rotations.update(_tail_rotations(phase, **tail_kwargs))
    for leg in LEG_ORDER:
        local = (phase - CONTACT_PHASE[leg]) % 1.0
        target_y, target_z, toe = _foot_target(
            leg, anchors["ankle"][leg], local, duty, stride, lift, config["toe"],
        )
        upper, lower, paw, _ = solve_leg(
            anchors, leg, absolute[LEG_PARENT[leg]], origin[LEG_PARENT[leg]],
            (target_y, target_z), toe,
        )
        rotations[f"leg_{leg}_upper"] = quat_rot_x(upper)
        rotations[f"leg_{leg}_lower"] = quat_rot_x(lower)
        rotations[f"leg_{leg}_paw"] = quat_rot_x(paw)
    translations = {"pelvis": (0.0, bob, 0.0)}
    return rotations, translations


def build_clips(rig: Rig, anchors: Dict[str, object]) -> List[Clip]:
    """Authors the three looping clips from the pose functions."""
    rig_positions = anchors["position"]
    builders: Dict[str, Callable[[float], Tuple[dict, dict]]] = {
        "idle": lambda phase: idle_pose(phase, rig_positions, anchors),
        "walk": lambda phase: locomotion_pose(phase, CLIPS["walk"], rig_positions, anchors),
        "run": lambda phase: locomotion_pose(phase, CLIPS["run"], rig_positions, anchors),
    }
    clips = []
    for name, config in CLIPS.items():
        keys = config["keys"]
        duration = config["duration"]
        clip = Clip(
            name,
            loop=config["loop"],
            duration=duration,
            kind=config["kind"],
        )
        for key in range(keys + 1):
            phase = key / keys
            # The last key re-evaluates phase 0 exactly, so every channel's
            # final value is bit-identical to its first.
            pose_phase = phase if phase < 1.0 else 0.0
            rotations, translations = builders[name](pose_phase)
            time = duration * phase
            for joint, rotation in rotations.items():
                clip.joint_rotation(rig, joint, time, rotation)
            for joint, offset in translations.items():
                clip.joint_translation(rig, joint, time, offset)
        clips.append(clip)
    return clips


# ---------------------------------------------------------------------------
# Offline geometric verification (pure Python, reads the written GLB)
# ---------------------------------------------------------------------------

GLB_MAGIC = 0x46546C67
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942
COMPONENT_USHORT = 5123
IDENTITY_QUAT = (0.0, 0.0, 0.0, 1.0)


def _load_glb(path: Path) -> Tuple[dict, bytes]:
    data = path.read_bytes()
    magic, version, _length = struct.unpack_from("<III", data, 0)
    if magic != GLB_MAGIC or version != 2:
        raise SystemExit(f"{path} is not a GLB 2.0 file")
    offset = 12
    document = None
    binary = b""
    while offset < len(data):
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        offset += 8
        payload = data[offset:offset + chunk_length]
        offset += chunk_length
        if chunk_type == CHUNK_JSON:
            document = json.loads(payload.decode("utf-8"))
        elif chunk_type == CHUNK_BIN:
            binary = payload
    if document is None:
        raise SystemExit(f"{path} has no JSON chunk")
    return document, binary


def _read_accessor(document: dict, binary: bytes, index: int, components: int, fmt: str = "f") -> list:
    accessor = document["accessors"][index]
    view = document["bufferViews"][accessor["bufferView"]]
    base = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    size = struct.calcsize("<" + fmt * components)
    stride = view.get("byteStride") or size
    return [
        struct.unpack_from("<" + fmt * components, binary, base + i * stride)
        for i in range(accessor["count"])
    ]


def _mat_trs(translation, rotation, scale) -> List[float]:
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


def _slerp(a: Sequence[float], b: Sequence[float], t: float) -> Tuple[float, float, float, float]:
    dot = sum(x * y for x, y in zip(a, b))
    if dot < 0.0:
        b = tuple(-x for x in b)
        dot = -dot
    if dot > 0.9995:
        out = tuple(a[i] + (b[i] - a[i]) * t for i in range(4))
        length = math.sqrt(sum(c * c for c in out))
        return tuple(c / length for c in out)  # type: ignore[return-value]
    theta = math.acos(min(1.0, max(-1.0, dot)))
    sin_theta = math.sin(theta)
    if abs(sin_theta) < 1.0e-9:
        return tuple(a)  # type: ignore[return-value]
    s0 = math.sin((1.0 - t) * theta) / sin_theta
    s1 = math.sin(t * theta) / sin_theta
    out = tuple(a[i] * s0 + b[i] * s1 for i in range(4))
    length = math.sqrt(sum(c * c for c in out))
    return tuple(c / length for c in out)  # type: ignore[return-value]


class SkinEvaluator:
    """Evaluates the written GLB the way the engine's character path does.

    LINEAR rotation keys are spherical-interpolated on the shortest arc and
    translation keys are lerped, exactly like ``src/render/common/character.rs``;
    unkeyed nodes keep their rest local TRS.  Skinning then blends
    ``global_joint(t) * inverse_bind_joint`` per vertex.
    """

    def __init__(self, document: dict, binary: bytes) -> None:
        self.document = document
        self.binary = binary
        self.nodes = document["nodes"]
        self.parent = {}
        for index, node in enumerate(self.nodes):
            for child in node.get("children", ()):
                self.parent[child] = index
        self.skin = document["skins"][0]
        self.joint_nodes = list(self.skin["joints"])
        self.ibm = _read_accessor(document, binary, self.skin["inverseBindMatrices"], 16)
        primitive = document["meshes"][0]["primitives"][0]
        attributes = primitive["attributes"]
        self.positions = _read_accessor(document, binary, attributes["POSITION"], 3)
        self.uvs = _read_accessor(document, binary, attributes["TEXCOORD_0"], 2)
        self.joints = _read_accessor(document, binary, attributes["JOINTS_0"], 4, "B")
        self.weights = _read_accessor(document, binary, attributes["WEIGHTS_0"], 4)
        self.triangles = len(_read_accessor(document, binary, primitive["indices"], 1, "H")) // 3
        self.animations = {}
        for animation in document.get("animations", ()):
            channels = []
            for channel in animation["channels"]:
                sampler = animation["samplers"][channel["sampler"]]
                times = [value[0] for value in _read_accessor(document, binary, sampler["input"], 1)]
                path = channel["target"]["path"]
                node = channel["target"]["node"]
                if path == "rotation":
                    values = _read_accessor(document, binary, sampler["output"], 4)
                elif path == "translation":
                    values = _read_accessor(document, binary, sampler["output"], 3)
                else:
                    continue
                channels.append((node, path, times, values))
            self.animations[animation["name"]] = channels
        # Precompute each vertex's inverse-bind-transformed rest point so a
        # sample only needs a joint matrix per influence.
        self.vertex_bind = []
        for vertex, position in enumerate(self.positions):
            slots = []
            for k in range(4):
                weight = self.weights[vertex][k]
                if weight <= 0.0:
                    continue
                joint = min(self.joints[vertex][k], len(self.joint_nodes) - 1)
                slots.append((self.joint_nodes[joint], weight, self._ibm_point(joint, position)))
            self.vertex_bind.append(slots)

    def _ibm_point(self, joint: int, point: Sequence[float]) -> Tuple[float, float, float]:
        matrix = self.ibm[joint]
        return (
            matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
            matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
            matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
        )

    def _sample_channel(self, channel, time: float):
        node, path, times, values = channel
        if time <= times[0]:
            return values[0]
        if time >= times[-1]:
            return values[-1]
        low, high = 0, len(times) - 1
        while high - low > 1:
            middle = (low + high) // 2
            if times[middle] <= time:
                low = middle
            else:
                high = middle
        span = times[high] - times[low]
        blend = 0.0 if span <= 0.0 else (time - times[low]) / span
        if path == "rotation":
            return _slerp(values[low], values[high], blend)
        return tuple(values[low][i] + (values[high][i] - values[low][i]) * blend for i in range(3))

    def globals_at(self, clip: Optional[str], time: float) -> List[List[float]]:
        locals_: List[object] = []
        for node in self.nodes:
            locals_.append(
                (
                    tuple(node.get("translation", (0.0, 0.0, 0.0))),
                    tuple(node.get("rotation", IDENTITY_QUAT)),
                    tuple(node.get("scale", (1.0, 1.0, 1.0))),
                )
            )
        for channel in self.animations.get(clip, ()):
            node, path, _times, _values = channel
            value = self._sample_channel(channel, time)
            translation, rotation, scale = locals_[node]
            if path == "rotation":
                rotation = value
            else:
                translation = value
            locals_[node] = (translation, rotation, scale)
        globals_: List[Optional[List[float]]] = [None] * len(self.nodes)

        def resolve(index: int) -> List[float]:
            cached = globals_[index]
            if cached is not None:
                return cached
            translation, rotation, scale = locals_[index]
            local = _mat_trs(translation, rotation, scale)
            parent = self.parent.get(index)
            result = local if parent is None else _mat_mul(resolve(parent), local)
            globals_[index] = result
            return result

        return [resolve(index) for index in range(len(self.nodes))]

    def positions_at(self, clip: str, time: float) -> List[Tuple[float, float, float]]:
        globals_ = self.globals_at(clip, time)
        out = []
        for slots in self.vertex_bind:
            x = y = z = 0.0
            for node, weight, point in slots:
                matrix = globals_[node]
                x += weight * (matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12])
                y += weight * (matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13])
                z += weight * (matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14])
            out.append((x, y, z))
        return out


def _stance_sweep(
    evaluator: "SkinEvaluator",
    clip: str,
    leg: str,
    duration: float,
    duty: float,
    paw_group: Tuple[int, int],
) -> Tuple[float, List[Tuple[float, float, float]], List[Tuple[float, float, float]]]:
    """Body-relative backward travel of one paw from contact to liftoff."""
    t_contact = CONTACT_PHASE[leg] * duration
    t_lift = ((CONTACT_PHASE[leg] + duty) % 1.0) * duration
    start, end = paw_group
    contact_points = evaluator.positions_at(clip, t_contact)
    lift_points = evaluator.positions_at(clip, t_lift)
    z_contact = sum(point[2] for point in contact_points[start:end]) / (end - start)
    z_lift = sum(point[2] for point in lift_points[start:end]) / (end - start)
    return z_contact - z_lift, contact_points, lift_points


def measure_reference_speeds(
    path: Path,
    paw_groups: Dict[str, Tuple[int, int]],
) -> Dict[str, float]:
    """Measures the authored walk/run ground speed from a written GLB.

    This runs before the final write so each clip can *declare* the speed its
    stride was authored for, and the same figure is then re-measured and
    asserted against the declaration during verification.
    """
    document, binary = _load_glb(path)
    evaluator = SkinEvaluator(document, binary)
    speeds: Dict[str, float] = {}
    for name, config in CLIPS.items():
        if config["kind"] not in ("walk", "run"):
            continue
        duration = config["duration"]
        duty = config["duty"]
        per_leg = []
        for leg in LEG_ORDER:
            sweep, _contact, _lift = _stance_sweep(evaluator, name, leg, duration, duty, paw_groups[leg])
            per_leg.append((sweep / duty) / duration)
        speeds[name] = sum(per_leg) / len(per_leg)
    return speeds


def _joint_family(joint_name: str) -> str:
    """Groups a joint name for the articulation report."""
    if joint_name.startswith("tail_"):
        return "tail"
    if joint_name.startswith("ear_"):
        return "ear"
    if joint_name.startswith("leg_"):
        if joint_name.endswith("_upper"):
            return "leg upper"
        if joint_name.endswith("_lower"):
            return "leg lower"
        return "paw"
    return "body"


def verify_model(
    path: Path,
    paw_groups: Dict[str, Tuple[int, int]],
    texture_bytes: int,
) -> dict:
    """Offline geometric check of the written GLB; returns a result record."""
    document, binary = _load_glb(path)
    problems: List[str] = []
    checks: List[Tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str) -> None:
        checks.append((name, ok, detail))
        if not ok:
            problems.append(f"{name}: {detail}")

    structural = check_model(path)
    check("rig.py structural check", not structural["problems"],
          "; ".join(structural["problems"]) or "no problems")

    evaluator = SkinEvaluator(document, binary)
    check("triangle budget", evaluator.triangles <= MAX_TRIANGLES and evaluator.triangles >= TARGET_TRIANGLES[0],
          f"{evaluator.triangles} triangles (budget {TARGET_TRIANGLES[0]}..{MAX_TRIANGLES})")
    from rat_surface import surface_checks
    primitive = document["meshes"][0]["primitives"][0]
    indices = [value[0] for value in _read_accessor(document, binary, primitive["indices"], 1, "H")]
    for name, passed in surface_checks(evaluator.positions, indices, evaluator.joints, evaluator.weights).items():
        check(name, passed, "exported GLB topology and deformation seams")
    check("vertex budget", len(evaluator.positions) <= MAX_VERTICES,
          f"{len(evaluator.positions)} vertices (ceiling {MAX_VERTICES})")
    check("one primitive, one material, one image",
          len(document["meshes"]) == 1
          and len(document["meshes"][0]["primitives"]) == 1
          and len(document["materials"]) == 1
          and len(document["images"]) == 1,
          "1 mesh / 1 primitive / 1 material / 1 image")
    index_accessor = document["accessors"][document["meshes"][0]["primitives"][0]["indices"]]
    check("u16 indices", index_accessor["componentType"] == COMPONENT_USHORT,
          "componentType 5123")
    finite = all(
        math.isfinite(value)
        for position in evaluator.positions
        for value in position
    ) and all(
        math.isfinite(value)
        for uv in evaluator.uvs
        for value in uv
    )
    check("finite positions and UVs", finite, f"{len(evaluator.positions)} positions, {len(evaluator.uvs)} UVs")
    out_of_range = sum(
        1 for uv in evaluator.uvs for value in uv if value < -0.001 or value > 1.001
    )
    check("UVs stay inside 0..1", out_of_range == 0, f"{out_of_range} out-of-range components")
    image_view = document["bufferViews"][document["images"][0]["bufferView"]]
    image_start = image_view.get("byteOffset", 0)
    embedded = binary[image_start:image_start + image_view["byteLength"]]
    image_width, image_height, _rgba = decode_png(embedded)
    check("embedded texture is a 256x256 PNG",
          document["images"][0]["mimeType"] == "image/png"
          and (image_width, image_height) == (TEXTURE_SIZE, TEXTURE_SIZE),
          f"embedded {texture_bytes}-byte PNG, {image_width}x{image_height}")

    joint_count = len(evaluator.joint_nodes)
    check("joint count", 1 <= joint_count <= 32, f"{joint_count} joints")
    bad_weights = 0
    bad_sums = 0
    for vertex in range(len(evaluator.positions)):
        weights = evaluator.weights[vertex]
        used = sum(1 for weight in weights if weight > 0.0)
        if used < 1 or used > 4:
            bad_weights += 1
        if abs(sum(weights) - 1.0) > 1.0e-4:
            bad_sums += 1
        for weight in weights:
            if weight < 0.0:
                bad_weights += 1
    check("weights 1..4 slots, non-negative", bad_weights == 0, f"{bad_weights} offending vertices")
    check("weights normalised to 1.0", bad_sums == 0, f"{bad_sums} vertices off by >1e-4")
    joint_names = [document["nodes"][node].get("name", "?") for node in evaluator.joint_nodes]
    check("joint names unique", len(set(joint_names)) == len(joint_names), f"{joint_names[:3]}...")

    # Bind pose.
    low = [min(p[i] for p in evaluator.positions) for i in range(3)]
    high = [max(p[i] for p in evaluator.positions) for i in range(3)]
    check("bind base on y=0", abs(low[1]) <= 1.0e-5, f"min y {low[1]:.7f}")
    check("bind horizontally centred",
          abs((low[0] + high[0]) * 0.5) <= 1.0e-5 and abs((low[2] + high[2]) * 0.5) <= 1.0e-5,
          f"centre ({(low[0] + high[0]) * 0.5:.7f}, {(low[2] + high[2]) * 0.5:.7f})")
    # With no channel applied, each joint's rest global composed with its
    # inverse bind matrix must be the identity: the bind mesh is then exactly
    # the rest pose the unkeyed character path starts from.
    rest_globals = evaluator.globals_at(None, 0.0)
    worst_identity = 0.0
    for joint, node in enumerate(evaluator.joint_nodes):
        product = _mat_mul(rest_globals[node], evaluator.ibm[joint])
        for column in range(4):
            for row in range(4):
                expected = 1.0 if column == row else 0.0
                worst_identity = max(worst_identity, abs(product[column * 4 + row] - expected))
    check("inverse bind matrices invert the rest pose", worst_identity <= 1.0e-4,
          f"worst identity deviation {worst_identity:.2e}")

    clip_results = {}
    speed_results = {}
    articulation: Dict[str, Dict[str, float]] = {}
    articulation_joints: Dict[str, Dict[str, float]] = {}
    duty = {name: config.get("duty") for name, config in CLIPS.items()}
    for name, config in CLIPS.items():
        duration = config["duration"]
        samples = int(math.ceil(duration * VERIFY_HZ)) + 1
        times = [min(duration, index / VERIFY_HZ) for index in range(samples)]
        minimum_y = []
        xs = []
        ys = []
        zs = []
        paw_min = {leg: [] for leg in LEG_ORDER}
        paw_z = {leg: [] for leg in LEG_ORDER}
        for time in times:
            posed = evaluator.positions_at(name, time)
            minimum_y.append(min(p[1] for p in posed))
            xs.extend(p[0] for p in posed)
            ys.extend(p[1] for p in posed)
            zs.extend(p[2] for p in posed)
            for leg in LEG_ORDER:
                start, end = paw_groups[leg]
                paw_min[leg].append(min(posed[i][1] for i in range(start, end)))
                paw_z[leg].append(sum(posed[i][2] for i in range(start, end)) / (end - start))
        clip_results[name] = {
            "samples": samples,
            "min_y": min(minimum_y),
            "max_y": max(minimum_y),
            "bbox_x": (min(xs), max(xs)),
            "bbox_y": (min(ys), max(ys)),
            "bbox_z": (min(zs), max(zs)),
        }
        check(f"{name}: no floor penetration overall",
              min(minimum_y) >= -0.02, f"lowest vertex y {min(minimum_y):+.5f}")
        for window in CONTACT_WINDOWS.get(name, ()):
            inside = [
                minimum_y[index]
                for index, time in enumerate(times)
                if window[0] - 1.0e-9 <= (time / duration) % 1.0 < window[1] - 1.0e-9
                or abs((time / duration) % 1.0 - window[1]) <= 1.0e-9
            ]
            inside = inside or [minimum_y[0]]
            check(
                f"{name}: contact window {window[0]:.2f}-{window[1]:.2f} within +/-0.02 m",
                min(inside) >= -0.02 and max(inside) <= 0.02,
                f"min y in window [{min(inside):+.5f}, {max(inside):+.5f}]",
            )
        for phase in CONTACT_KEYS.get(name, ()):
            posed = evaluator.positions_at(name, phase * duration)
            lowest = min(p[1] for p in posed)
            check(f"{name}: contact key at phase {phase:.1f} on the floor",
                  -0.01 <= lowest <= 0.01, f"lowest vertex y {lowest:+.5f}")
        for window in FLIGHT_WINDOWS.get(name, ()):
            # The window edges are the liftoff/touchdown instants; the
            # suspension is what its interior shows.  Sample the interior
            # directly rather than relying on the 60 Hz grid.
            interior_times = [
                duration * (window[0] + fraction * (window[1] - window[0]))
                for fraction in (0.3, 0.4, 0.5, 0.6, 0.7)
            ]
            interior = [
                min(point[1] for point in evaluator.positions_at(name, time))
                for time in interior_times
            ]
            check(f"{name}: suspension window {window[0]:.2f}-{window[1]:.2f} lifts the paws clear",
                  min(interior) >= 0.003,
                  f"interior lowest vertex {min(interior) * 1000.0:.1f} mm")
            clip_results[name].setdefault("flight_clearance", []).append(
                (min(interior), max(interior)))
        if name == "idle":
            for leg in LEG_ORDER:
                check(f"idle: paw {leg} planted",
                      min(paw_min[leg]) >= -0.01 and max(paw_min[leg]) <= 0.01,
                      f"paw y range [{min(paw_min[leg]):+.5f}, {max(paw_min[leg]):+.5f}]")
        # Movement envelope: a pose must not fling the mesh out of its box.
        result = clip_results[name]
        check(f"{name}: no wild deformation",
              abs(result["bbox_x"][0]) <= 0.35 and abs(result["bbox_x"][1]) <= 0.35
              and result["bbox_y"][0] >= -0.05 and result["bbox_y"][1] <= 0.40
              and abs(result["bbox_z"][0]) <= 0.65 and abs(result["bbox_z"][1]) <= 0.65,
              f"posed x {result['bbox_x']}, y {result['bbox_y']}, z {result['bbox_z']}")

        # Loop closure: first == last on every channel.
        loop_problem = None
        for animation in document.get("animations", ()):
            if animation["name"] != name:
                continue
            for index, channel in enumerate(animation["channels"]):
                sampler = animation["samplers"][channel["sampler"]]
                path = channel["target"]["path"]
                components = 4 if path == "rotation" else 3
                values = _read_accessor(document, binary, sampler["output"], components)
                first, last = values[0], values[-1]
                if path == "rotation":
                    dot = abs(sum(a * b for a, b in zip(first, last)))
                    if dot < 0.99999:
                        loop_problem = f"channel {index} rotation dot {dot:.6f}"
                else:
                    if max(abs(a - b) for a, b in zip(first, last)) > 1.0e-5:
                        loop_problem = f"channel {index} translation drift"
        check(f"{name}: loop closes", loop_problem is None, loop_problem or "first key == last key")

        # Reference speed: the stance sweep of each planted paw.
        if config["kind"] in ("walk", "run"):
            sweep_speeds = []
            sweeps = []
            cycle_travel = []
            declared = None
            marker = (document.get("asset", {}).get("extras") or {}).get("places_entity_clips") or {}
            for entry in marker.get("clips", ()):
                if entry.get("name") == name:
                    declared = entry.get("reference_speed_mps")
            for leg in LEG_ORDER:
                sweep, contact_points, _lift_points = _stance_sweep(
                    evaluator, name, leg, duration, duty[name], paw_groups[leg])
                sweeps.append(sweep)
                travel = sweep / duty[name]
                cycle_travel.append(travel)
                sweep_speeds.append(travel / duration)
                check(f"{name}: paw {leg} reaches the floor at contact",
                      min(p[1] for p in contact_points) <= 0.01,
                      f"lowest {min(p[1] for p in contact_points):+.5f}")
            measured = sum(sweep_speeds) / len(sweep_speeds)
            check(f"{name}: declared reference speed matches the measured stride",
                  declared is not None and abs(float(declared) - measured) <= 0.001 * measured + 1.0e-6,
                  f"declared {declared}, measured {measured:.4f} m/s")
            speed_results[name] = {
                "per_leg_speed_mps": sweep_speeds,
                "per_leg_sweep_m": sweeps,
                "per_leg_cycle_travel_m": cycle_travel,
                "mean_speed_mps": measured,
                "declared_speed_mps": declared,
                "mean_sweep_m": sum(sweeps) / len(sweeps),
                "mean_cycle_travel_m": sum(cycle_travel) / len(cycle_travel),
                "duty": duty[name],
                "duration": duration,
            }
            # Foot slide: with the body advancing at the measured reference
            # speed, a planted paw must hold its world z through mid-stance
            # (the tail of stance carries the authored toe-off, so it is
            # excluded).
            slide = 0.0
            speed = speed_results[name]["mean_speed_mps"]
            for leg in LEG_ORDER:
                world = [
                    paw_z[leg][index] + speed * time
                    for index, time in enumerate(times)
                    # The looping clip's last sample duplicates phase 0 one
                    # cycle later, so it is not part of a single stance.
                    if time < duration - 1.0e-9
                    and 0.0 <= ((time / duration) - CONTACT_PHASE[leg]) % 1.0 <= 0.75 * duty[name]
                ]
                if len(world) >= 2:
                    slide = max(slide, max(world) - min(world))
            speed_results[name]["max_slide_m"] = slide
            check(f"{name}: planted paw does not slide at the reference speed",
                  slide <= 0.006,
                  f"max world-z drift over mid-stance {slide * 1000.0:.1f} mm")

        # Articulation: the largest local rotation each joint reaches, grouped
        # by family for the report and checked per joint.
        node_names = {
            index: document["nodes"][index].get("name", "")
            for index in range(len(document["nodes"]))
        }
        families: Dict[str, float] = {}
        per_joint: Dict[str, float] = {}
        for animation in document.get("animations", ()):
            if animation["name"] != name:
                continue
            for channel in animation["channels"]:
                if channel["target"]["path"] != "rotation":
                    continue
                sampler = animation["samplers"][channel["sampler"]]
                values = _read_accessor(document, binary, sampler["output"], 4)
                joint_name = node_names[channel["target"]["node"]]
                largest = 0.0
                for quat in values:
                    largest = max(largest, math.degrees(2.0 * math.acos(min(1.0, abs(quat[3])))))
                family = _joint_family(joint_name)
                families[family] = max(families.get(family, 0.0), largest)
                per_joint[joint_name] = max(per_joint.get(joint_name, 0.0), largest)
        articulation[name] = families
        articulation_joints[name] = per_joint
        required = {"walk": 10.0, "run": 15.0}.get(name)
        if required is not None:
            leg_minimum = min(
                per_joint.get(f"leg_{leg}_{part}", 0.0)
                for leg in LEG_ORDER
                for part in ("upper", "lower", "paw")
            )
            check(f"{name}: all twelve leg joints are articulated",
                  leg_minimum >= required,
                  f"smallest leg-joint rotation {leg_minimum:.1f} deg (need {required})")
        tail_minimum = min(per_joint.get(f"tail_{number:02d}", 0.0) for number in range(1, 6))
        check(f"{name}: every tail joint is articulated",
              tail_minimum >= 0.8,
              f"smallest tail-joint rotation {tail_minimum:.2f} deg")
        if name == "idle":
            # The idle hind legs legitimately hold still: the breathing body
            # chain never moves the pelvis, so its paws need no correction.
            front_legs = [
                per_joint.get(f"leg_{leg}_{part}", 0.0)
                for leg in ("fl", "fr")
                for part in ("upper", "lower", "paw")
            ]
            check("idle: the breathing front legs are keyed",
                  min(front_legs) >= 0.2,
                  f"smallest front leg-joint rotation {min(front_legs):.2f} deg")

    return {
        "problems": problems,
        "checks": checks,
        "clips": clip_results,
        "speeds": speed_results,
        "articulation": articulation,
        "articulation_joints": articulation_joints,
        "joint_names": joint_names,
        "triangles": evaluator.triangles,
        "vertices": len(evaluator.positions),
        "bind_low": tuple(low),
        "bind_high": tuple(high),
    }


# ---------------------------------------------------------------------------
# Development preview rasteriser (optional, written next to the GLB)
# ---------------------------------------------------------------------------


def _render_panel(
    positions: Sequence[Tuple[float, float, float]],
    mesh: Mesh,
    tex: Texture,
    view: str,
    width: int,
    height: int,
) -> bytearray:
    """Orthographic software render of one view.

    ``side`` looks along -X (depth +X), ``front`` along -Z (depth +Z) at the
    face, ``top`` down -Y (depth +Y) with the nose up.
    """
    if view == "side":
        u_axis, v_axis, depth_axis = 2, 1, 0        # screen x <- z, screen y <- y
        u_sign, v_sign, depth_sign = 1.0, 1.0, 1.0
        camera = (1.0, 0.0, 0.0)
    elif view == "front":
        u_axis, v_axis, depth_axis = 0, 1, 2
        u_sign, v_sign, depth_sign = 1.0, 1.0, 1.0
        camera = (0.0, 0.0, 1.0)
    else:  # top
        u_axis, v_axis, depth_axis = 0, 2, 1
        u_sign, v_sign, depth_sign = 1.0, -1.0, 1.0   # nose at the top
        camera = (0.0, 1.0, 0.0)
    us = [u_sign * p[u_axis] for p in positions]
    vs = [v_sign * p[v_axis] for p in positions]
    lo_u, hi_u = min(us), max(us)
    lo_v, hi_v = min(vs), max(vs)
    margin = 8
    scale = min((width - 2 * margin) / max(1.0e-6, hi_u - lo_u),
                (height - 2 * margin) / max(1.0e-6, hi_v - lo_v))

    def project(point):
        return (
            margin + (u_sign * point[u_axis] - lo_u) * scale,
            height - margin - (v_sign * point[v_axis] - lo_v) * scale,
            depth_sign * point[depth_axis],
        )

    pixels = bytearray()
    for _ in range(width * height):
        pixels += bytes((24, 25, 28))
    zbuf = [-1.0e18] * (width * height)
    light = _normalize((0.35, 0.75, 0.55))
    for index in range(0, len(mesh.indices), 3):
        ia, ib, ic = mesh.indices[index:index + 3]
        pa, pb, pc = positions[ia], positions[ib], positions[ic]
        ax, ay, ad = project(pa)
        bx, by, bd = project(pb)
        cx, cy, cd = project(pc)
        area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        if abs(area) < 1.0e-9:
            continue
        normal = _normalize(_cross(_sub(pb, pa), _sub(pc, pa)))
        if normal[0] * camera[0] + normal[1] * camera[1] + normal[2] * camera[2] < 0.0:
            normal = (-normal[0], -normal[1], -normal[2])
        lambert = max(0.0, normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2])
        shade = 0.55 + 0.45 * lambert
        minx = max(0, int(min(ax, bx, cx)))
        maxx = min(width - 1, int(max(ax, bx, cx)) + 1)
        miny = max(0, int(min(ay, by, cy)))
        maxy = min(height - 1, int(max(ay, by, cy)) + 1)
        uvs = (mesh.uvs[ia], mesh.uvs[ib], mesh.uvs[ic])
        colors = (mesh.colors[ia], mesh.colors[ib], mesh.colors[ic])
        for py in range(miny, maxy + 1):
            for px in range(minx, maxx + 1):
                sxp, syp = px + 0.5, py + 0.5
                w0 = (bx - sxp) * (cy - syp) - (by - syp) * (cx - sxp)
                w1 = (cx - sxp) * (ay - syp) - (cy - syp) * (ax - sxp)
                w2 = (ax - sxp) * (by - syp) - (ay - syp) * (bx - sxp)
                inside = (w0 >= 0 and w1 >= 0 and w2 >= 0) if area > 0 else (w0 <= 0 and w1 <= 0 and w2 <= 0)
                if not inside:
                    continue
                l0, l1, l2 = w0 / area, w1 / area, w2 / area
                depth = l0 * ad + l1 * bd + l2 * cd
                cell = py * width + px
                if depth <= zbuf[cell]:
                    continue
                zbuf[cell] = depth
                u = l0 * uvs[0][0] + l1 * uvs[1][0] + l2 * uvs[2][0]
                v = l0 * uvs[0][1] + l1 * uvs[1][1] + l2 * uvs[2][1]
                texel = tex.sample(min(0.999, max(0.0, u)), min(0.999, max(0.0, v)))
                tint = (
                    (l0 * colors[0][0] + l1 * colors[1][0] + l2 * colors[2][0]) / 255.0,
                    (l0 * colors[0][1] + l1 * colors[1][1] + l2 * colors[2][1]) / 255.0,
                    (l0 * colors[0][2] + l1 * colors[1][2] + l2 * colors[2][2]) / 255.0,
                )
                offset = cell * 3
                pixels[offset] = min(255, int(texel[0] * tint[0] * shade))
                pixels[offset + 1] = min(255, int(texel[1] * tint[1] * shade))
                pixels[offset + 2] = min(255, int(texel[2] * tint[2] * shade))
    if v_axis == 1:  # side and front views have a real floor line
        ground_row = int(round(height - margin - (v_sign * 0.0 - lo_v) * scale))
        ground_row = min(height - 1, max(0, ground_row)) + 1
        for py in range(ground_row, height):
            for px in range(width):
                offset = (py * width + px) * 3
                pixels[offset] = 52
                pixels[offset + 1] = 50
                pixels[offset + 2] = 46
    return pixels


def write_preview(path: Path, mesh: Mesh, tex: Texture, evaluator: SkinEvaluator) -> None:
    """A four-panel software preview for development review only."""
    from tex import write_png  # local import keeps the module import surface small

    idle = evaluator.positions_at("idle", 0.0)
    panels = [
        (idle, "side"),
        (evaluator.positions_at("walk", 0.0), "side"),
        (evaluator.positions_at("run", 0.23), "side"),
        (idle, "front"),
        (idle, "top"),
    ]
    width, height = 420, 150
    image = bytearray()
    for positions, view in panels:
        image += _render_panel(positions, mesh, tex, view, width, height)
    rgba = bytearray()
    for index in range(width * height * len(panels)):
        rgba += bytes((image[index * 3], image[index * 3 + 1], image[index * 3 + 2], 255))
    path.write_bytes(write_png(width, height * len(panels), bytes(rgba)))


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def _mm(value: float) -> str:
    return f"{value * 1000.0:.0f} mm"


def _display_path(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path.resolve())


def write_report(
    path: Path,
    *,
    glb_path: Path,
    texture_path: Path,
    glb_bytes: int,
    texture_bytes: int,
    stats: dict,
    verification: dict,
    joint_table: List[Tuple[str, str, Tuple[float, float, float]]],
    texture_size: Tuple[int, int],
    rump_z: float,
    report_bytes: Optional[int] = None,
) -> None:
    bind_low = verification["bind_low"]
    bind_high = verification["bind_high"]
    size = tuple(bind_high[i] - bind_low[i] for i in range(3))
    report_size_text = f"{report_bytes:>5d}" if report_bytes is not None else "     "
    lines: List[str] = []
    add = lines.append
    add("# Rat entity — build report")
    add("")
    add("Built by `build_rat.py` (deterministic, stdlib only, one Python")
    add("process).  The model is a low-poly rat: one mesh, one primitive, one")
    add("material, one embedded texture, one skin, three looping clips.")
    add("")
    add("## Files")
    add("")
    add("Paths are relative to the repository root.")
    add("")
    add("| file | bytes | notes |")
    add("| --- | --- | --- |")
    add(f"| `{_display_path(glb_path)}` | {glb_bytes} | skinned GLB written by `tools/entities/rig.py` |")
    add(f"| `{_display_path(texture_path)}` | {texture_bytes} | the same PNG the GLB embeds, {texture_size[0]}x{texture_size[1]} |")
    add(f"| `{_display_path(path)}` | {report_size_text} | this report |")
    add("")
    add("## Counts")
    add("")
    add(f"* triangles: {verification['triangles']} (budget 700–1200, hard ceiling 1500)")
    add(f"* vertices: {verification['vertices']} (ceiling 2500)")
    add(f"* joints: {len(verification['joint_names'])} (ceiling 128)")
    add("* clips: " + ", ".join(
        f"`{name}` ({config['duration']:.2f} s, {stats['clips'][index]['channels']} channels)"
        for index, (name, config) in enumerate(CLIPS.items())
    ))
    add(f"* total animation channels: {stats['clip_channels']} (ceiling 4096)")
    add(f"* texture: {texture_size[0]}x{texture_size[1]} PNG embedded, {texture_bytes} bytes")
    add("")
    add("## Joints (name, parent, rest translation)")
    add("")
    add("| joint | parent | rest translation (m) |")
    add("| --- | --- | --- |")
    for name, parent, translation in joint_table:
        add(f"| `{name}` | {parent or '—'} | ({translation[0]:+.4f}, {translation[1]:+.4f}, {translation[2]:+.4f}) |")
    add("")
    add("All rest local rotations are identity; the skeleton shape is carried by")
    add("the child translations above.")
    add("")
    add("## Clips")
    add("")
    add("| clip | duration | loop | channels | reference speed | kind |")
    add("| --- | --- | --- | --- | --- | --- |")
    for index, (name, config) in enumerate(CLIPS.items()):
        speed = verification["speeds"].get(name)
        speed_text = f"{speed['mean_speed_mps']:.3f} m/s" if speed else "—"
        add(f"| `{name}` | {config['duration']:.2f} s | {str(config['loop']).lower()} | "
            f"{stats['clips'][index]['channels']} | {speed_text} | `{config['kind']}` |")
    add("")
    add("### How the reference speeds were measured")
    add("")
    add("The GLB was re-evaluated offline at the authored contact and liftoff")
    add("times of every paw (per-vertex skinning with slerped LINEAR keys, the")
    add("same blend the engine's character path applies).  For each paw:")
    add("")
    add("```")
    add("stance_sweep = paw_centroid_z(contact) - paw_centroid_z(liftoff)   # metres, body-relative")
    add("per_cycle_travel = stance_sweep / duty_factor")
    add("reference_speed = per_cycle_travel / cycle_duration = stance_sweep / (duty * duration)")
    add("```")
    add("")
    add("The duty factor is the fraction of the cycle that paw spends planted")
    add("(`walk` 0.55, `run` 0.36).  Playing the clip at exactly this speed keeps")
    add("a planted paw world-stationary, so the feet do not slide.")
    add("")
    add("Each gait clip's `asset.extras.places_entity_clips` entry declares")
    add("exactly this measured figure as its `reference_speed_mps`; the")
    add("verification pass re-measures it from the written file and asserts the")
    add("declaration matches.")
    add("")
    add("| clip | per-leg speed (m/s) | mean stance sweep (m) | per-cycle travel (m) | duty | max slide |")
    add("| --- | --- | --- | --- | --- | --- |")
    for name, speed in verification["speeds"].items():
        per_leg = ", ".join(f"{value:.3f}" for value in speed["per_leg_speed_mps"])
        add(f"| `{name}` | {per_leg} | {speed['mean_sweep_m']:.4f} | "
            f"{speed['mean_cycle_travel_m']:.4f} | {speed['duty']:.2f} | "
            f"{speed['max_slide_m'] * 1000.0:.1f} mm |")
    add("")
    add("`max slide` is the largest world-space z drift of a planted paw through")
    add("mid-stance when the body advances at the measured reference speed: the")
    add("direct no-foot-sliding measurement (the tail end of stance carries the")
    add("authored toe-off and is excluded).")
    add("")
    add("## Articulation (largest local rotation reached, degrees)")
    add("")
    add("| clip | body | ear | tail | leg upper | leg lower | paw |")
    add("| --- | --- | --- | --- | --- | --- | --- |")
    for name, families in verification["articulation"].items():
        add(f"| `{name}` | " + " | ".join(
            f"{families.get(family, 0.0):.1f}"
            for family in ("body", "ear", "tail", "leg upper", "leg lower", "paw")
        ) + " |")
    add("")
    add("The shanks fold hard in a running stride; the `paw` figure is the")
    add("ankle compensation that keeps the sole level while the shank swings,")
    add("not a flapping foot.")
    add("")
    flight = verification["clips"].get("run", {}).get("flight_clearance")
    if flight:
        lowest = min(entry[0] for entry in flight)
        highest = max(entry[1] for entry in flight)
        add(f"The `run` flight windows clear the floor by at least "
            f"{lowest * 1000.0:.1f} mm and up to {highest * 1000.0:.1f} mm "
            f"(lowest vertex during suspension).")
        add("")
    add("## Bind-pose bounding box and catalog size")
    add("")
    add(f"* min: ({bind_low[0]:+.4f}, {bind_low[1]:+.4f}, {bind_low[2]:+.4f}) m")
    add(f"* max: ({bind_high[0]:+.4f}, {bind_high[1]:+.4f}, {bind_high[2]:+.4f}) m")
    add(f"* size [width_x, height_y, depth_z] = [{_mm(size[0])}, {_mm(size[1])}, {_mm(size[2])}]")
    add("")
    add("The origin sits on the floor-contact point (lowest paw vertex `y = 0`)")
    add("and is horizontally centred under this bind-pose box.")
    add("")
    add("Posed extremes sampled over each clip (min/max per axis, metres):")
    add("")
    add("| clip | x | y | z |")
    add("| --- | --- | --- | --- |")
    for name, result in verification["clips"].items():
        add(f"| `{name}` | {result['bbox_x'][0]:+.3f} .. {result['bbox_x'][1]:+.3f} | "
            f"{result['bbox_y'][0]:+.3f} .. {result['bbox_y'][1]:+.3f} | "
            f"{result['bbox_z'][0]:+.3f} .. {result['bbox_z'][1]:+.3f} |")
    add("")
    add("These are the raw posed extremes of the mesh (no culling expansion).")
    add("A raised or streaming tail is the part most likely to sit outside the")
    add("bind-pose box; the engine's character-path expansion (15% + 5 cm) is")
    add("what covers it.")
    add("")
    add("## Placement and route notes for the engine")
    add("")
    add("* Forward axis: **+Z**.  A placement at `rotation_degrees = 0` faces +Z.")
    add("* Origin: floor contact, under the bind-pose box centre.  The tail runs")
    add(f"  far behind the body, so the visible body sits in front of the origin:")
    add(f"  the nose reaches z = {bind_high[2]:+.3f} m, the rump sits near")
    add(f"  z = {rump_z:+.3f} m, and the torso is centred roughly")
    add(f"  z = {(bind_high[2] + rump_z) * 0.5:+.3f} m.  Place and turn about the")
    add("  origin, not the nose.")
    add("* Route speeds: walk **{:.3f} m/s**, run **{:.3f} m/s**.".format(
        verification["speeds"]["walk"]["mean_speed_mps"],
        verification["speeds"]["run"]["mean_speed_mps"]))
    run_threshold = verification["speeds"]["walk"]["mean_speed_mps"] * 1.5
    add(f"  The runtime's `PoseCue::Walk` picks `run` once the route speed")
    add(f"  reaches 1.5x the walk reference ({run_threshold:.3f} m/s), so:")
    add(f"  `move_to` at {verification['speeds']['walk']['mean_speed_mps']:.3f} m/s plays `walk` at rate 1.0 and at "
        f"{verification['speeds']['run']['mean_speed_mps']:.3f} m/s plays `run` at rate 1.0.")
    add("  Both clips are authored so that any rate keeps the stance sweep")
    add("  proportional to the route speed, so planted feet stay planted at")
    add("  intermediate speeds too (verified at the declared speed).")
    add("* The runtime picks clips by name: `idle` claims the idle state, `walk`")
    add("  the walking state (first walk/run match wins), and `run` is playable")
    add("  by an explicit `clip` cue.")
    add("* The tail is posed by the clips and can swing past the bind-pose box")
    add("  (sideways in idle/walk, streamed back in run); cull with the")
    add("  character-path expansion the engine already applies.")
    add("* The texture is embedded in the GLB; `rat_fur_01.png` next to it is the")
    add("  source sheet embedded by the build.")
    add("")
    add("## Offline verification performed")
    add("")
    add("`python3 tools/entities/rig.py --check rat.glb` reports zero problems,")
    add("and `build_rat.py` re-reads the written GLB and re-skins it in pure")
    add("Python at 60 samples per second (LINEAR slerp/lerp, per-vertex blend of")
    add("`global_joint * inverseBind`), asserting:")
    add("")
    add("* every vertex carries 1–4 valid joint slots whose weights sum to 1.0;")
    add("* the bind pose rests on `y = 0`, horizontally centred, and the")
    add("  inverse bind matrices invert the rest pose;")
    add("* no clip pose pushes a vertex more than 2 mm below the floor")
    add(f"  (worst measured {min(result['min_y'] for result in verification['clips'].values()) * 1000.0:.2f} mm) "
        "and none leaves the movement envelope;")
    add("* contact windows keep the lowest vertex within ±0.02 m of the floor")
    add("  (contact keys within ±0.01 m) and the run's flight windows stay clear;")
    add("* a planted paw holds its world position through mid-stance at the")
    add("  measured reference speed (worst drift "
        f"{max(speed['max_slide_m'] for speed in verification['speeds'].values()) * 1000.0:.1f} mm);")
    add("* all twelve leg joints move in `walk` and `run`, and all five tail")
    add("  joints move in every clip (the idle keeps its hind legs still by")
    add("  design: its paws are planted while the body breathes);")
    add("* every channel's first key equals its last key.")
    add("")
    add("This is an **offline geometric check, not engine playback**: it")
    add("re-implements the documented blend maths from the file itself, and does")
    add("not run the Rust character path, a GPU, or a renderer.")
    add("")
    add("## Not verified here")
    add("")
    add("* No engine run, no GPU frame and no route playback was executed; the")
    add("  authored speeds and contact figures come from the offline check.")
    add("* Blender exact booleans join the mesh during authoring; the preview uses the software renderer.")
    add("* Visual review is limited to the optional software preview")
    add("  (`rat_preview.png`: side views of bind / walk contact / run flight,")
    add("  plus a front and a top view), which rasterises this mesh and texture")
    add("  only.")
    add("* The catalog `size` above is computed, not yet validated against a")
    add("  catalog entry: the lead adds the asset entry and runs the prop/tool")
    add("  validators.")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def build(out_path: Path, *, preview: bool) -> int:
    texture = build_texture()
    kness = {
        leg: rest_knee(HIP_POS[leg], ANKLE_POS[leg], SLACK, KNEE_FORWARD[leg])
        for leg in LEG_ORDER
    }
    mesh, parts = build_mesh(texture, kness)
    shift = mesh.normalize_origin()
    rig, anchors = build_rig(shift, kness)
    for leg in LEG_ORDER:
        upper, lower, paw, _ = solve_leg(
            anchors, leg, 0.0, anchors["position"][LEG_PARENT[leg]],
            (anchors["ankle"][leg][1], anchors["ankle"][leg][2]), 0.0,
        )
        if max(abs(upper), abs(lower), abs(paw)) > 1.0e-6:
            raise SystemExit(f"rest IK does not reproduce the bind pose for leg {leg}")
    skinned = SkinnedMesh(mesh)
    assign_weights(skinned, rig, parts)
    clips = build_clips(rig, anchors)

    paw_groups = {leg: parts[f"leg_{leg}_paw"] for leg in LEG_ORDER}
    texture_png = texture.png_bytes()

    # Provisional write: measure each gait's authored stride from the file the
    # engine will load, declare it on the clip, then write the final file.  The
    # verifier re-measures the same figure and asserts the declaration.
    write_model(
        out_path,
        skinned,
        rig,
        clips,
        texture_png,
        name=ENTITY_NAME,
        generator=GENERATOR,
    )
    measured = measure_reference_speeds(out_path, paw_groups)
    for clip in clips:
        if clip.kind in ("walk", "run"):
            clip.reference_speed_mps = measured[clip.name]
    stats = write_model(
        out_path,
        skinned,
        rig,
        clips,
        texture_png,
        name=ENTITY_NAME,
        generator=GENERATOR,
    )
    texture_path = REPO_ROOT / TEXTURE_DIR / TEXTURE_NAME
    texture_path.parent.mkdir(parents=True, exist_ok=True)
    texture_path.write_bytes(texture_png)

    document, binary = _load_glb(out_path)
    joint_table = []
    for index, node in enumerate(document["nodes"]):
        if "mesh" in node:
            continue
        parent = next(
            (name for name, candidate in enumerate(document["nodes"])
             if index in candidate.get("children", ())),
            None,
        )
        joint_table.append((
            document["nodes"][index].get("name", "?"),
            document["nodes"][parent].get("name") if parent is not None else None,
            tuple(document["nodes"][index].get("translation", (0.0, 0.0, 0.0))),
        ))
    order = {name: index for index, (name, _, _) in enumerate(joint_table)}
    joint_table.sort(key=lambda entry: order[entry[0]])

    verification = verify_model(out_path, paw_groups, len(texture_png))

    dev_dir = REPO_ROOT / DEV_DIR
    dev_dir.mkdir(parents=True, exist_ok=True)
    if preview:
        evaluator = SkinEvaluator(*_load_glb(out_path))
        write_preview(dev_dir / PREVIEW_NAME, mesh, texture, evaluator)

    report_path = dev_dir / REPORT_NAME
    report_arguments = dict(
        glb_path=out_path,
        texture_path=texture_path,
        glb_bytes=stats["bytes"],
        texture_bytes=len(texture_png),
        stats=stats,
        verification=verification,
        joint_table=joint_table,
        texture_size=(texture.width, texture.height),
        rump_z=TORSO_REAR_Z + shift[2],
    )
    # Two passes: the second fills in this report's own byte size, and the
    # fixed-width field keeps both passes the same length.
    write_report(report_path, **report_arguments)
    write_report(report_path, report_bytes=report_path.stat().st_size, **report_arguments)

    print(f"rat: {stats['triangles']} triangles, {stats['vertices']} vertices, "
          f"{stats['joints']} joints, {stats['clip_channels']} channels")
    print(f"     {out_path} ({stats['bytes']} bytes), {texture_path.name} "
          f"({len(texture_png)} bytes), {report_path.name}")
    bind_low = verification["bind_low"]
    bind_high = verification["bind_high"]
    print("     bind bbox min ({:+.4f}, {:+.4f}, {:+.4f}) max ({:+.4f}, {:+.4f}, {:+.4f})".format(
        bind_low[0], bind_low[1], bind_low[2], bind_high[0], bind_high[1], bind_high[2]))
    for name, result in verification["clips"].items():
        print(f"     {name:5s} lowest-vertex y over clip: {result['min_y']:+.5f} .. "
              f"{result['max_y']:+.5f}  (posed z {result['bbox_z'][0]:+.3f} .. {result['bbox_z'][1]:+.3f})")
    for name, speed in verification["speeds"].items():
        print(f"     {name:5s} measured reference speed: {speed['mean_speed_mps']:.4f} m/s "
              f"(sweep {speed['mean_sweep_m']:.4f} m, per-cycle {speed['mean_cycle_travel_m']:.4f} m, "
              f"duty {speed['duty']:.2f}, max slide {speed['max_slide_m'] * 1000.0:.1f} mm)")
    for name, families in verification["articulation"].items():
        families_order = ("body", "ear", "tail", "leg upper", "leg lower", "paw")
        print(f"     {name:5s} articulation (deg): " + ", ".join(
            f"{family} {families.get(family, 0.0):.1f}" for family in families_order))

    failures = verification["problems"]
    for name, ok, detail in verification["checks"]:
        if not ok:
            print(f"  FAIL {name}: {detail}", file=sys.stderr)
    if failures:
        print(f"verification FAILED with {len(failures)} problem(s)", file=sys.stderr)
        return 1
    print(f"verification: {len(verification['checks'])} checks passed, 0 problems")
    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", default=str(DEFAULT_OUT),
                        help="where to write the GLB (default the shipped entity asset)")
    parser.add_argument("--preview", action="store_true",
                        help="also write the optional software preview PNG under target/")
    parser.add_argument("--check", action="store_true",
                        help="verify an existing GLB/report inputs instead of rebuilding")
    args = parser.parse_args(argv)

    out_path = Path(args.out)
    if not out_path.is_absolute():
        out_path = REPO_ROOT / out_path

    if args.check:
        if not out_path.is_file():
            print(f"no GLB at {out_path}", file=sys.stderr)
            return 1
        texture_path = REPO_ROOT / TEXTURE_DIR / TEXTURE_NAME
        texture = build_texture()
        kness = {leg: rest_knee(HIP_POS[leg], ANKLE_POS[leg], SLACK, KNEE_FORWARD[leg]) for leg in LEG_ORDER}
        mesh, parts = build_mesh(texture, kness)
        mesh.normalize_origin()
        verification = verify_model(out_path, {
            leg: parts[f"leg_{leg}_paw"] for leg in LEG_ORDER
        }, texture_path.stat().st_size if texture_path.is_file() else 0)
        for name, ok, detail in verification["checks"]:
            if not ok:
                print(f"FAIL {name}: {detail}")
        print(f"{len(verification['checks'])} checks, {len(verification['problems'])} problems")
        return 1 if verification["problems"] else 0

    return build(out_path, preview=args.preview)


if __name__ == "__main__":
    raise SystemExit(main())
