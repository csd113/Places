#!/usr/bin/env python3
"""Build the tiny Office cockroach from closed low-poly solids and a real PNG.

The committed 32-square white sheet is a resized derivative of the shared
file-backed white sheet. This builder only reads it; all visual detail comes
from geometry and vertex colours. No texture imagery is painted or generated.

Six three-joint legs use an alternating tripod gait with two-link IK and flat
tarsi. Animation is local only: idle, walk and run never translate the root.
The +Z-facing model is horizontally centred and its bind feet rest on y = 0.

    python3 tools/props/build_cockroach.py
    python3 tools/props/build_cockroach.py --check

Both commands perform a small offline check of the written GLB. They do not
build maps, run the player, start Blender or invoke the Rust test suite.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "tools" / "entities"))
sys.path.insert(0, str(REPO_ROOT / "tools" / "props"))

from build_beach_common import Animal, cross, dot, subtract, unit  # noqa: E402
from glb import read_glb  # noqa: E402
from rig import Clip, check_model, quat_compose, quat_normalize, quat_rot_y, write_model  # noqa: E402
from resize_embedded_textures import read_glb as read_container, write_glb as write_container  # noqa: E402
from tex import decode_png  # noqa: E402
from validate_entities import Model  # noqa: E402

ASSET_DIR = REPO_ROOT / "assets" / "entities" / "cockroach"
MODEL_PATH = ASSET_DIR / "model" / "cockroach.glb"
TEXTURE_PATH = ASSET_DIR / "textures" / "white_01.png"
GENERATOR = "tools/props/build_cockroach.py"
PALETTE = {
    "shell": (70, 36, 18),
    "wing": (87, 46, 22),
    "shield": (79, 39, 17),
    "belly": (48, 27, 17),
    "seam": (38, 22, 15),
    "leg": (71, 39, 21),
    "antenna": (47, 28, 19),
    "eye": (19, 15, 12),
}
IDENTITY = (0.0, 0.0, 0.0, 1.0)
DUTY = 0.60
CLIPS = {
    "idle": {"duration": 1.80, "stride": 0.0, "lift": 0.0, "keys": 16},
    "walk": {"duration": 0.44, "stride": 0.012, "lift": 0.0020, "keys": 24},
    "run": {"duration": 0.22, "stride": 0.016, "lift": 0.0030, "keys": 24},
}
SHELL_ROUGHNESS = 0.48


def shell_color_factor():
    """Keep the chosen dark-brown display palette in glTF's linear channel.

    COLOR_0 is linear, but the palette was selected as ordinary sRGB bytes.
    A material factor anchors its shell to that intended colour while keeping
    all existing per-part contrast and the file-backed white sheet unchanged.
    """
    srgb = [channel / 255 for channel in PALETTE["shell"]]
    linear = [value / 12.92 if value <= .04045 else ((value + .055) / 1.055) ** 2.4
              for value in srgb]
    return [target / source for target, source in zip(linear, srgb)] + [1.0]


def configure_material(path):
    """Apply this asset's pigment and restrained cuticle sheen after export."""
    document, binary = read_container(path)
    pbr = document["materials"][0]["pbrMetallicRoughness"]
    pbr["baseColorFactor"] = shell_color_factor()
    pbr["roughnessFactor"] = SHELL_ROUGHNESS
    write_container(path, document, binary)


def add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def scale(v, factor):
    return tuple(x * factor for x in v)


def inverse(quaternion):
    x, y, z, w = quaternion
    return (-x, -y, -z, w)


def align_vectors(source, target):
    """Shortest rotation between two nonzero vectors, including opposition."""
    first, second = unit(source), unit(target)
    cosine = max(-1.0, min(1.0, dot(first, second)))
    if cosine < -0.999999:
        helper = (1.0, 0.0, 0.0) if abs(first[0]) < .9 else (0.0, 1.0, 0.0)
        axis = unit(cross(first, helper))
        return (*axis, 0.0)
    axis = cross(first, second)
    return quat_normalize((*axis, 1.0 + cosine))


class Cockroach(Animal):
    """Reuse the shipped closed-solid helpers with file-backed plain colour."""

    def __init__(self):
        super().__init__(PALETTE)
        self.pieces = []
        self.legs = {}

    def uv(self, _region):
        # There is no authored atlas: every part samples the white sheet centre.
        return (.5, .5, .5, .5)

    def triangle(self, points, region, joint, outward):
        super().triangle(points, region, joint, outward)
        self.mesh.colors[-3:] = [self.palette[region]] * 3

    def ellipsoid(self, *args, **kwargs):
        start = len(self.mesh.indices)
        super().ellipsoid(*args, **kwargs)
        self.pieces.append((start, len(self.mesh.indices)))

    def segment(self, *args, **kwargs):
        start = len(self.mesh.indices)
        super().segment(*args, **kwargs)
        self.pieces.append((start, len(self.mesh.indices)))


def build_geometry():
    animal = Cockroach()
    animal.joint("root", (0, 0, 0))
    animal.joint("body", (0, .0115, -.004), "root")
    animal.joint("head", (0, .010, .037), "body")
    # Flattened oval abdomen and two distinct closed wing covers. Their dark
    # centre seam is geometry, and stays readable even at very small scale.
    animal.ellipsoid((0, .0105, -.007), (.0185, .0065, .0345),
                     "shell", "body", segments=10, rings=2)
    animal.ellipsoid((0, .008, -.006), (.0155, .0035, .032),
                     "belly", "body", segments=8, rings=1)
    for sign in (-1, 1):
        animal.ellipsoid((sign * .0085, .014, -.009), (.0090, .0040, .0285),
                         "wing", "body", segments=8, rings=2)
    animal.segment((0, .0178, -.034), (0, .0178, .015), .00065,
                   "seam", "body", sides=4, end_radius=.00065)
    animal.ellipsoid((0, .0125, .023), (.0165, .0065, .0140),
                     "shield", "body", segments=8, rings=2)
    animal.ellipsoid((0, .010, .039), (.0088, .0048, .0078),
                     "shell", "head", segments=6, rings=1)
    for sign, side in ((1, "l"), (-1, "r")):
        animal.ellipsoid((sign * .0069, .011, .043), (.0018, .0020, .0020),
                         "eye", "head", segments=4, rings=1)
        # Thin two-joint antennae point forward, then fan outward. Their bend
        # is visibly independent from the leg scurry and never moves the root.
        base = (sign * .0052, .0115, .044)
        bend = (sign * .014, .009, .064)
        tip = (sign * .025, .0045, .087)
        animal.joint(f"antenna_{side}", base, "head")
        animal.joint(f"antenna_tip_{side}", bend, f"antenna_{side}")
        animal.segment(base, bend, .00055, "antenna", f"antenna_{side}",
                       sides=3, end_radius=.00036)
        animal.segment(bend, tip, .00036, "antenna", f"antenna_tip_{side}",
                       sides=3, end_radius=.00012)
        # A pair of short rear cerci distinguish the rounded abdomen tail.
        animal.segment((sign * .006, .008, -.036), (sign * .009, .0055, -.045),
                       .0007, "antenna", "body", sides=3, end_radius=.0002)
        for index, z in enumerate((.018, .000, -.019)):
            spread = (.010, -.001, -.012)[index]
            hip = (sign * .0125, .0078, z)
            knee = (sign * .0270, .0145, z + spread)
            ankle = (sign * .0405, .0010, z + spread * 1.5)
            toe = (sign * .0435, .0008, ankle[2] + (.004, -.002, -.005)[index])
            name = f"leg_{side}_{index + 1:02d}"
            animal.joint(name, hip, "root")
            animal.joint(f"{name}_shin", knee, name)
            animal.joint(f"{name}_foot", ankle, f"{name}_shin")
            animal.segment(hip, knee, .00145, "leg", name, sides=4, end_radius=.00115)
            animal.segment(knee, ankle, .00115, "leg", f"{name}_shin",
                           sides=4, end_radius=.00065)
            animal.segment(ankle, toe, .00065, "leg", f"{name}_foot",
                           sides=3, end_radius=.00025)
            # One tiny closed tibial spur per leg supplies the spiny read.
            spur = add(knee, (sign * .007, -.0030, -.0050))
            animal.segment(add(knee, (sign * .003, -.001, -.001)), spur, .00055,
                           "leg", f"{name}_shin", sides=3, end_radius=.00012)
            animal.legs[name] = {"hip": hip, "knee": knee, "ankle": ankle,
                                 "tripod": (index + (side == "r")) % 2}
    animal.centre_at_floor()
    # Geometry, skeleton and analytic targets share the same centred frame.
    for name, leg in animal.legs.items():
        leg["hip"] = animal.origins[name]
        leg["knee"] = animal.origins[f"{name}_shin"]
        leg["ankle"] = animal.origins[f"{name}_foot"]
    return animal


def solve_leg(leg, target):
    """Two-link IK in the rest knee's bend plane, with a level foot socket."""
    hip, rest_knee, rest_ankle = leg["hip"], leg["knee"], leg["ankle"]
    upper, lower = subtract(rest_knee, hip), subtract(rest_ankle, rest_knee)
    first, second = math.sqrt(dot(upper, upper)), math.sqrt(dot(lower, lower))
    axis = subtract(target, hip)
    distance = math.sqrt(dot(axis, axis))
    if not abs(first - second) + 1e-8 < distance < first + second - 1e-8:
        raise ValueError("leg target leaves its reachable two-link envelope")
    axis = unit(axis)
    bend = subtract(upper, scale(axis, dot(upper, axis)))
    bend = unit(bend)
    along = (first * first - second * second + distance * distance) / (2 * distance)
    height = math.sqrt(max(0.0, first * first - along * along))
    knee = add(hip, add(scale(axis, along), scale(bend, height)))
    upper_q = align_vectors(upper, subtract(knee, hip))
    lower_q = align_vectors(lower, subtract(target, knee))
    return upper_q, quat_compose(inverse(upper_q), lower_q), inverse(lower_q)


def build_clips(animal):
    clips = []
    for name, definition in CLIPS.items():
        duration, stride = definition["duration"], definition["stride"]
        speed = stride / (DUTY * duration) if stride else None
        clip = Clip(name, duration=duration, loop=True, kind=name, reference_speed_mps=speed)
        count = definition["keys"]
        for index in range(count + 1):
            # Force exact endpoint closure, including the tripod half phase.
            phase = index / count if index < count else 0.0
            time = duration * index / count
            wave = math.sin(math.tau * phase)
            clip.joint_translation(animal.rig, "body", time,
                                   (0, .00018 * (1 + math.cos(2 * math.tau * phase)), 0))
            clip.joint_rotation(animal.rig, "head", time, quat_rot_y(1.5 * wave))
            for sign, side in ((1, "l"), (-1, "r")):
                clip.joint_rotation(animal.rig, f"antenna_{side}", time,
                                    quat_rot_y(sign * (3.0 * wave)))
                clip.joint_rotation(animal.rig, f"antenna_tip_{side}", time,
                                    quat_rot_y(sign * (4.0 * math.sin(math.tau * phase * 2))))
            for leg_name, leg in animal.legs.items():
                target = leg["ankle"]
                if stride:
                    local = (phase + .5 * leg["tripod"]) % 1.0
                    if local < DUTY:
                        sweep = .5 - local / DUTY
                        lift = 0.0
                    else:
                        swing = (local - DUTY) / (1 - DUTY)
                        sweep = -.5 * math.cos(math.pi * swing)
                        lift = definition["lift"] * math.sin(math.pi * swing)
                    target = add(target, (0, lift, stride * sweep))
                rotations = solve_leg(leg, target) if stride else (IDENTITY,) * 3
                for suffix, rotation in zip(("", "_shin", "_foot"), rotations):
                    clip.joint_rotation(animal.rig, f"{leg_name}{suffix}", time, rotation)
        clips.append(clip)
    return clips


def check_topology(animal):
    """Every authored piece must be a closed outward-wound two-manifold."""
    for piece, (start, end) in enumerate(animal.pieces):
        edge_counts = Counter()
        edge_directions = Counter()
        volume = 0.0
        for index in range(start, end, 3):
            points = [animal.mesh.positions[i] for i in animal.mesh.indices[index:index + 3]]
            if dot(cross(subtract(points[1], points[0]), subtract(points[2], points[0])),
                   cross(subtract(points[1], points[0]), subtract(points[2], points[0]))) <= 1e-18:
                raise ValueError(f"piece {piece} has a degenerate face")
            volume += dot(points[0], cross(points[1], points[2])) / 6
            vertices = [tuple(round(v, 9) for v in point) for point in points]
            for first, second in zip(vertices, vertices[1:] + vertices[:1]):
                edge = tuple(sorted((first, second)))
                edge_counts[edge] += 1
                edge_directions[edge] += 1 if first < second else -1
        if any(count != 2 for count in edge_counts.values()) or any(edge_directions.values()):
            raise ValueError(f"piece {piece} has an open or inconsistently wound edge")
        if volume <= 0:
            raise ValueError(f"piece {piece} has inward winding")


def verify(path, animal):
    """Read emitted bytes with both toolkit inspectors and sample local poses."""
    structural = check_model(path)
    if structural["problems"]:
        raise ValueError(structural["problems"])
    imported = read_glb(path.read_bytes())
    model = Model(path)
    if len(model.positions) != animal.mesh.vertex_count:
        raise ValueError("written model lost vertices")
    if any(math.dist(actual, expected) > 1e-7
           for actual, expected in zip(model.positions, animal.mesh.positions)):
        raise ValueError("written geometry differs from the deterministic authored model")
    if any(abs(sum(weights) - 1.0) > 1e-6 for weights in model.vertex_weights):
        raise ValueError("written weights are not normalized")
    if any(not 0 <= slot < len(model.joints) for slots in model.vertex_joints for slot in slots):
        raise ValueError("written joint index is out of bounds")
    check_topology(animal)
    primitive = model.document["meshes"][0]["primitives"][0]
    colors = model._read_accessor(primitive["attributes"]["COLOR_0"], "VEC4")
    expected_colors = [tuple(channel / 255 for channel in color) + (1.0,)
                       for color in animal.mesh.colors]
    if len(colors) != len(expected_colors) or any(
           any(abs(actual - expected) > 1e-7 for actual, expected in zip(color, wanted))
           for color, wanted in zip(colors, expected_colors)):
        raise ValueError("written model lost its authored per-part pigment")
    material = model.document["materials"][primitive["material"]]
    pbr = material["pbrMetallicRoughness"]
    if (pbr.get("baseColorTexture") != {"index": 0}
            or pbr.get("baseColorFactor") != shell_color_factor()
            or pbr.get("roughnessFactor") != SHELL_ROUGHNESS
            or pbr.get("metallicFactor") != 0.0
            or material.get("alphaMode", "OPAQUE") != "OPAQUE"
            or material.get("emissiveFactor", [0.0] * 3) != [0.0] * 3):
        raise ValueError("written model lost its dark-brown opaque non-emissive shell response")
    uvs = model._read_accessor(primitive["attributes"]["TEXCOORD_0"], "VEC2")
    if any(not math.isfinite(v) or not 0 <= v <= 1 for uv in uvs for v in uv):
        raise ValueError("written UVs leave the non-tiling model contract")
    view = model.document["bufferViews"][model.document["images"][0]["bufferView"]]
    png = model.binary[view.get("byteOffset", 0):view.get("byteOffset", 0) + view["byteLength"]]
    if png != TEXTURE_PATH.read_bytes():
        raise ValueError("embedded PNG differs from committed standalone source")
    width, height, pixels = decode_png(png)
    if (width, height) != (32, 32) or any(pixels[i] != 255 for i in range(3, len(pixels), 4)):
        raise ValueError("native sheet violates its opaque 32-square contract")
    if any(not 252 <= pixels[i] <= 255 for i in range(len(pixels)) if i % 4 != 3):
        raise ValueError("native sheet is no longer a flat shared-white derivative")
    samples = {}
    for clip in model.clips:
        lowest, highest, movement, stretch = float("inf"), -float("inf"), 0.0, 1.0
        first = model._skin(model._pose_globals(clip, 0.0))
        # Read the final authored key without the evaluator wrapping to t=0.
        endpoint = {**clip, "loop": False}
        last = model._skin(model._pose_globals(endpoint, clip["duration"]))
        closure = max(math.dist(a, b) for a, b in zip(first, last))
        for index in range(33):
            posed = model._skin(model._pose_globals(clip, clip["duration"] * index / 32))
            lowest = min(lowest, min(p[1] for p in posed))
            highest = max(highest, max(p[1] for p in posed))
            movement = max(movement, max(math.dist(a, b) for a, b in zip(posed, model.bind_skin)))
            lengths = model._edge_lengths(posed)
            stretch = max(stretch, max(max(a / b, b / a) for a, b in zip(lengths, model.bind_edges)))
        if lowest < -.0001 or lowest > .001 or stretch > 1.001 or closure > 1e-6:
            raise ValueError(f"clip {clip['name']} fails floor, rigid geometry or loop closure check")
        samples[clip["name"]] = {"duration_seconds": clip["duration"],
                                 "reference_speed_mps": clip["reference_speed_mps"],
                                 "min_y_m": lowest, "max_y_m": highest,
                                 "max_displacement_m": movement, "max_edge_stretch": stretch,
                                 "loop_error_m": closure}
    bounds = animal.mesh.bounds()
    size = [bounds[1][axis] - bounds[0][axis] for axis in range(3)]
    if animal.mesh.triangle_count > 800 or not .10 <= size[2] <= .14:
        raise ValueError("tiny model exceeds its triangle or physical length budget")
    return {"model": str(path.relative_to(REPO_ROOT)), "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "vertices": len(imported.positions), "triangles": len(imported.indices) // 3,
            "joints": len(model.joints), "closed_pieces": len(animal.pieces),
            "size_m": size, "bounds_m": {"min": bounds[0], "max": bounds[1]},
            "texture_size": [width, height], "texture_decoded_bytes": len(pixels),
            "texture_sha256": hashlib.sha256(png).hexdigest(),
            "base_color_factor_linear": pbr["baseColorFactor"],
            "roughness": pbr["roughnessFactor"], "metallic": pbr["metallicFactor"],
            "clips": samples}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="inspect existing asset without writing")
    args = parser.parse_args()
    animal = build_geometry()
    if not args.check:
        write_model(MODEL_PATH, animal.skinned(), animal.rig, build_clips(animal),
                    TEXTURE_PATH.read_bytes(), name="cockroach", generator=GENERATOR)
        configure_material(MODEL_PATH)
    report = verify(MODEL_PATH, animal)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
