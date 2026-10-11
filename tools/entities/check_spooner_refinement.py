#!/usr/bin/env python3
"""Small explicit preservation/mesh gate for the Spooner-man body export.

This reads two GLBs, never compiles a map and never starts Blender or the game.
It protects the confirmed failure modes: accidentally modified head/animation,
lost skin influences, material-seam holes, inward normals and unused black UVs.
"""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/props"))
from refine_spooner_man import HEAD_Z, Model, OUTPUT, SOURCE, primitives  # noqa: E402
from tex import decode_png  # noqa: E402


def require(condition, message):
    if not condition:
        raise ValueError(message)


def head_faces(model):
    faces = []
    for material, attributes, triangles in primitives(model):
        for triangle in triangles:
            if min(attributes["POSITION"][triangle, 2]) < HEAD_Z:
                continue
            faces.append(bytes([material]) + b"".join(
                np.asarray(attributes[name][triangle], dtype="<f4").tobytes()
                for name in sorted(attributes)))
    return faces


def images(model):
    result = []
    for image in model.json["images"]:
        view = model.json["bufferViews"][image["bufferView"]]
        start = view.get("byteOffset", 0)
        result.append(model.bin[start:start + view["byteLength"]])
    return result


def animation_signature(model):
    result = []
    for animation in model.json["animations"]:
        channels = []
        for channel in animation["channels"]:
            sampler = animation["samplers"][channel["sampler"]]
            arrays = []
            for key in ("input", "output"):
                accessor = model.json["accessors"][sampler[key]]
                count = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[accessor["type"]]
                arrays.append(np.asarray(model.read_accessor(sampler[key], count), dtype="<f4").tobytes())
            channels.append((channel["target"], sampler.get("interpolation"), arrays))
        result.append((animation["name"], channels))
    return result


def check(path):
    original, current = Model(str(SOURCE)), Model(str(path))
    for field in ("nodes", "scenes", "scene", "materials", "textures", "samplers"):
        require(original.json.get(field) == current.json.get(field), f"changed {field}")
    original_skin, current_skin = original.skin.copy(), current.skin.copy()
    original_bind = original_skin.pop("inverseBindMatrices")
    current_bind = current_skin.pop("inverseBindMatrices")
    require(original_skin == current_skin, "changed rig/joint order")
    require(original.read_accessor(original_bind, 16) == current.read_accessor(current_bind, 16),
            "changed inverse bind matrices")
    baseline_head, final_head = head_faces(original), head_faces(current)
    require(baseline_head == final_head, "changed original head attributes/materials/winding")
    require(len(final_head) == 454, "unexpected original head boundary")
    require(images(original) == images(current), "changed embedded PNG bytes")
    require(animation_signature(original) == animation_signature(current), "changed animation keys")
    clip_marker = "places_entity_clips"
    require(original.json["asset"]["extras"][clip_marker]["clips"] ==
            current.json["asset"]["extras"][clip_marker]["clips"], "changed clip playback metadata")
    edges = collections.Counter()
    seam_weights = {}
    max_seam_weight_difference = 0.0
    count, vertices, positions = 0, 0, []
    dark_samples = 0
    width, height, rgba = decode_png(images(current)[0])
    for material, attributes, triangles in primitives(current):
        for values in attributes.values():
            require(np.isfinite(values).all(), "nonfinite vertex attribute")
        p, n = attributes["POSITION"], attributes["NORMAL"]
        uv, joints, weights = (attributes[k] for k in ("TEXCOORD_0", "JOINTS_0", "WEIGHTS_0"))
        require(np.all((uv >= 0) & (uv <= 1)), "invalid UV")
        require(np.max(abs(np.linalg.norm(n, axis=1) - 1)) < 1e-5, "nonunit normal")
        require(np.all((joints >= 0) & (joints < 26)), "invalid skin index")
        require(np.all(weights >= 0) and np.max(abs(weights.sum(axis=1) - 1)) < 1e-5,
                "invalid/unnormalized skin weights")
        for point, slots, values in zip(p, joints, weights):
            key = tuple(round(float(x), 7) for x in point)
            influences = collections.defaultdict(float)
            for slot, weight in zip(slots, values):
                influences[int(slot)] += float(weight)
            if key in seam_weights:
                previous = seam_weights[key]
                difference = max(abs(influences[k] - previous[k])
                                 for k in set(influences) | set(previous))
                max_seam_weight_difference = max(max_seam_weight_difference, difference)
                require(difference < 1e-5, "material seam separates during animation")
            else:
                seam_weights[key] = influences
        cross = np.cross(p[triangles[:, 1]] - p[triangles[:, 0]],
                         p[triangles[:, 2]] - p[triangles[:, 0]])
        require(np.min(np.linalg.norm(cross, axis=1)) > 1e-10, "zero-area face")
        require(np.min(np.sum(cross * n[triangles].mean(axis=1), axis=1)) >= -1e-10,
                "inward lighting normal")
        for triangle in triangles:
            for i, j in zip(triangle, np.roll(triangle, -1)):
                edge = tuple(sorted(tuple(round(float(x), 6) for x in p[k]) for k in (i, j)))
                edges[edge] += 1
            centre = p[triangle].mean(axis=0)
            if material == 0 and -.17 < centre[2] < .21:
                # UV centroid of every torso face must now reach real dark fur.
                u, v = uv[triangle].mean(axis=0)
                index = (min(height - 1, int(v * height)) * width +
                         min(width - 1, int(u * width))) * 4
                pixel = rgba[index:index + 3]
                require(0 < max(pixel) < 90, "torso face reaches blank black/white artwork")
                dark_samples += 1
        count += len(triangles)
        vertices += len(p)
        positions.extend(p)
    require(set(edges.values()) == {2}, "open/nonmanifold material or anatomy seam")
    p = np.array(positions)
    return {"triangles": count, "vertices": vertices, "head_triangles_unchanged": len(final_head),
            "head_attribute_sha256": hashlib.sha256(b"".join(final_head)).hexdigest(),
            "texture_sha256": [hashlib.sha256(image).hexdigest() for image in images(current)],
            "joint_count": 26, "clips_keys_and_metadata_unchanged": len(current.json["animations"]),
            "closed_edges": len(edges), "dark_fur_faces_checked": dark_samples,
            "max_seam_weight_difference": max_seam_weight_difference,
            "bind_min": p.min(axis=0).tolist(), "bind_max": p.max(axis=0).tolist(),
            "glb_bytes": len(current.raw), "glb_sha256": hashlib.sha256(current.raw).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--glb", type=Path, default=OUTPUT)
    args = parser.parse_args()
    print(json.dumps(check(args.glb), indent=2))


if __name__ == "__main__":
    main()
