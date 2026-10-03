#!/usr/bin/env python3
"""Read-only inventory and integrity audit of every repository model and PNG.

Topology, UV area and similarity findings are review candidates, not automatic
repair instructions: assembled shells, swatch UVs and family atlases are legal.
Uses the existing GLB/PNG readers and the standard library only.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import struct
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/props"))
sys.path.insert(0, str(ROOT / "tools/entities"))
sys.path.insert(0, str(ROOT / "tools"))
import glb
import geometry
import rig
from build_rat import _slerp
import validate as asset_validation
from tex import decode_png
from execution import atomic_write, ordered_map, worker_count


def digest(data):
    return hashlib.sha256(data).hexdigest()


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


def image_record(data):
    width, height, pixels = decode_png(data)
    alpha = pixels[3::4]
    return {"width": width, "height": height, "bytes": len(data),
            "decoded_bytes": width * height * 4, "sha256": digest(data),
            "pixel_sha256": digest(struct.pack("<II", width, height)+pixels), "alpha_range": [min(alpha), max(alpha)]}


def finite_rows(rows):
    return all(math.isfinite(v) for row in rows for v in row)


def coincident_quads(model):
    """Find doubled planar quads even when their triangle diagonals differ.

    These remain review candidates: foliage cards may intentionally carry
    both sides. Position keys never replace the model's UV/color seam vertices.
    """
    keys = [tuple(round(v, 6) for v in point) for point in model.positions]
    triangles = [model.indices[o:o+3] for o in range(0, len(model.indices), 3)]
    normals = []
    edges = defaultdict(list)
    for face, triangle in enumerate(triangles):
        a, b, c = [model.positions[i] for i in triangle]
        normal = geometry._cross(geometry._sub(b, a), geometry._sub(c, a))
        length = math.sqrt(geometry._dot(normal, normal))
        normals.append(tuple(v/length for v in normal) if length else (0, 0, 0))
        for a, b in zip(triangle, triangle[1:]+triangle[:1]):
            edges[tuple(sorted((keys[a], keys[b])))].append(face)
    quads = defaultdict(list)
    for faces in edges.values():
        if len(faces) != 2:
            continue
        a, b = faces
        points = tuple(sorted({keys[i] for face in faces for i in triangles[face]}))
        if len(points) != 4 or geometry._dot(normals[a], normals[b]) < .999999:
            continue
        # A coplanar pair's shared edge must be an interior diagonal, not a
        # boundary between two adjacent or folded panels.
        shared_keys = set(keys[i] for i in triangles[a]) & set(keys[i] for i in triangles[b])
        if len(shared_keys) != 2:
            continue
        first, second = sorted(shared_keys)
        side = [geometry._dot(geometry._cross(geometry._sub(second, first), geometry._sub(p, first)), normals[a])
                for p in points if p not in shared_keys]
        if side[0]*side[1] >= 0:
            continue
        quads[points].append({"triangles": sorted(faces), "normal": normals[a]})
    return [{"points": points, "surfaces": surfaces,
             "opposite_facing": any(geometry._dot(a["normal"], b["normal"]) < -.999999
                                     for a in surfaces for b in surfaces)}
            for points, surfaces in sorted(quads.items())
            if any(set(a["triangles"]).isdisjoint(b["triangles"])
                   for a in surfaces for b in surfaces)]


def read_accessor(document, binary, index):
    """The prop reader handles vectors; skin inverse binds also need MAT4."""
    accessor = document["accessors"][index]
    if accessor["type"] != "MAT4":
        return glb._read_accessor(document, binary, index)
    if accessor["componentType"] != 5126 or "sparse" in accessor:
        raise ValueError("inverse bind matrices must be dense float32 MAT4")
    view = document["bufferViews"][accessor["bufferView"]]
    offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride", 64)
    count = accessor["count"]
    if stride < 64 or offset + max(0, count-1)*stride + 64 > len(binary):
        raise ValueError("inverse bind matrix data is truncated")
    return [struct.unpack_from("<16f", binary, offset+i*stride) for i in range(count)]


def animation_record(document, binary, animation):
    errors = []
    channels = []
    for channel in animation.get("channels", []):
        sampler = animation["samplers"][channel["sampler"]]
        target = channel["target"]
        times = [v[0] for v in glb._read_accessor(document, binary, sampler["input"])]
        values = glb._read_accessor(document, binary, sampler["output"])
        interpolation = sampler.get("interpolation", "LINEAR")
        if not times or not finite_rows(values) or not all(math.isfinite(t) for t in times):
            errors.append("empty or non-finite animation channel")
        if any(b <= a for a, b in zip(times, times[1:])) or (times and times[0] < 0):
            errors.append("animation times must be non-negative and strictly increasing")
        if len(times) != len(values) or interpolation not in ("LINEAR", "STEP"):
            errors.append("unsupported interpolation or channel length mismatch")
        if target.get("path") not in ("rotation", "translation", "scale"):
            errors.append("unsupported animation target")
        if not 0 <= target.get("node", -1) < len(document.get("nodes", [])):
            errors.append("invalid animation target node")
        if target.get("path") == "rotation" and any(abs(sum(v*v for v in row)-1) > 0.002 for row in values):
            errors.append("animation quaternion is not unit length")
        delta = None
        if values:
            delta = max(abs(a-b) for a, b in zip(values[0], values[-1]))
            if target.get("path") == "rotation":
                delta = min(delta, max(abs(a+b) for a, b in zip(values[0], values[-1])))
        channels.append({"node": target.get("node"), "path": target.get("path"),
                         "keys": len(times), "duration": max(times, default=0),
                         "interpolation": interpolation, "endpoint_delta": delta})
    return {"name": animation.get("name", ""), "channels": channels,
            "duration": max((c["duration"] for c in channels), default=0), "errors": errors}


def inspect_rigid_frames(document, binary, animation, duration):
    """Exercise rigid clips at 60 Hz, including their exact final frame."""
    tracks = []
    for channel in animation["channels"]:
        sampler = animation["samplers"][channel["sampler"]]
        tracks.append((channel["target"], sampler.get("interpolation", "LINEAR"),
                       [row[0] for row in glb._read_accessor(document, binary, sampler["input"])],
                       glb._read_accessor(document, binary, sampler["output"])))
    count = max(1, math.ceil(duration*60))
    for frame in range(count+1):
        time_ = duration*frame/count
        nodes = [dict(node) for node in document["nodes"]]
        for target, interpolation, times, values in tracks:
            right = next((i for i, value in enumerate(times) if value >= time_), len(times)-1)
            left = max(0, right-1)
            t = 0 if right == left else (time_-times[left])/(times[right]-times[left])
            if interpolation == "STEP" and time_ < times[right]:
                value = values[left]
            elif target["path"] == "rotation":
                value = _slerp(values[left], values[right], t)
            else:
                value = tuple(a+(b-a)*t for a, b in zip(values[left], values[right]))
            nodes[target["node"]][target["path"]] = value
        posed = {**document, "nodes": nodes}
        matrices, _ = glb._node_rest_matrices(posed)
        for matrix in matrices:
            if not all(math.isfinite(v) for v in matrix):
                raise ValueError("non-finite posed rigid transform")
        for index, node in enumerate(nodes):
            if "mesh" not in node:
                continue
            for primitive in document["meshes"][node["mesh"]]["primitives"]:
                positions = glb._read_accessor(document, binary, primitive["attributes"]["POSITION"])
                if not finite_rows(glb._transform_point(matrices[index], p) for p in positions):
                    raise ValueError("non-finite rigid animated vertex")
    return count+1


def inspect_model(path):
    data = path.read_bytes()
    model = glb.read_glb(data)
    document, binary = rig._load_glb(path)
    errors = []
    reviews = []
    for name, rows in (("positions", model.positions), ("UVs", model.uvs), ("colors", model.colors)):
        if not finite_rows(rows):
            errors.append(f"non-finite {name}")
    if any(v < -0.01 or v > 1.01 for row in model.uvs for v in row):
        errors.append("UV outside supported fitted range")
    if errors:
        return {"format": "GLB 2.0", "bytes": len(data), "sha256": digest(data), "errors": errors}
    low, high = model.bounds()
    topology = geometry.inspect(model.positions, model.indices)
    if topology["degenerate"]:
        errors.append(f'{topology["degenerate"]} zero-area triangles')
    if topology["inconsistent_edges"]:
        reviews.append("mixed winding along shared edges")
    faces = Counter(tuple(sorted(tuple(round(v, 6) for v in model.positions[i]) for i in model.indices[o:o+3]))
                    for o in range(0, len(model.indices), 3))
    duplicate_faces = sum(count-1 for count in faces.values() if count > 1)
    if duplicate_faces:
        reviews.append("coincident triangle surfaces; check intentional doubled sheets")
    doubled_quads = coincident_quads(model)
    if doubled_quads:
        reviews.append("coincident planar quads; check intentional doubled sheets")
    uv_zero = 0
    for offset in range(0, len(model.indices), 3):
        a, b, c = [model.uvs[i] for i in model.indices[offset:offset+3]]
        uv_zero += abs((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])) < 1e-12
    used_materials = set()
    used_accessors = set()
    normals = []
    unused_vertices = 0
    bad_weights = 0
    used_joints = set()
    for mesh in document["meshes"]:
        for primitive in mesh["primitives"]:
            attributes = primitive["attributes"]
            used_accessors.update(attributes.values())
            positions = glb._read_accessor(document, binary, attributes["POSITION"])
            indices = [v[0] for v in glb._read_accessor(document, binary, primitive["indices"])] if "indices" in primitive else range(len(positions))
            if "indices" in primitive:
                used_accessors.add(primitive["indices"])
            unused_vertices += len(positions) - len(set(indices))
            if "material" in primitive:
                used_materials.add(primitive["material"])
            if "NORMAL" in attributes:
                rows = glb._read_accessor(document, binary, attributes["NORMAL"])
                if not finite_rows(rows) or any(abs(sum(v*v for v in row)-1) > 0.02 for row in rows):
                    errors.append("invalid normal vectors")
                normals.extend(rows)
            if "TANGENT" in attributes:
                rows = glb._read_accessor(document, binary, attributes["TANGENT"])
                if not finite_rows(rows) or any(abs(sum(v*v for v in row[:3])-1) > 0.02 or abs(abs(row[3])-1) > 0.001 for row in rows):
                    errors.append("invalid tangent vectors")
            if "WEIGHTS_0" in attributes:
                weights = glb._read_accessor(document, binary, attributes["WEIGHTS_0"])
                joints = glb._read_accessor(document, binary, attributes["JOINTS_0"])
                joint_count = len(document["skins"][0]["joints"])
                for ws, js in zip(weights, joints):
                    bad_weights += not all(math.isfinite(w) and w >= 0 for w in ws) or abs(sum(ws)-1) > 0.001
                    for weight, joint in zip(ws, js):
                        if weight > 0:
                            used_joints.add(joint)
                            if not 0 <= joint < joint_count:
                                errors.append("invalid skin joint index")
    if bad_weights:
        errors.append(f"{bad_weights} unnormalized/non-finite skin weights")
    materials = document.get("materials", [])
    used_images = set()
    for material_index in used_materials:
        if not 0 <= material_index < len(materials):
            errors.append("invalid material index")
            continue
        material = materials[material_index]
        references = [material.get("pbrMetallicRoughness", {}).get("baseColorTexture"), material.get("emissiveTexture")]
        for reference in references:
            if reference is not None:
                used_images.add(document["textures"][reference["index"]]["source"])
    images = []
    for index, image in enumerate(document.get("images", [])):
        if image.get("mimeType") != "image/png" or "uri" in image:
            errors.append("image is not an embedded PNG")
            continue
        view = document["bufferViews"][image["bufferView"]]
        start = view.get("byteOffset", 0)
        record = image_record(binary[start:start+view["byteLength"]])
        record.update(index=index, name=image.get("name", ""), used=index in used_images)
        if max(record["width"], record["height"]) > 256:
            errors.append("embedded image exceeds native prop budget")
        images.append(record)
    animations = [animation_record(document, binary, a) for a in document.get("animations", [])]
    for animation in document.get("animations", []):
        for sampler in animation.get("samplers", []):
            used_accessors.update((sampler["input"], sampler["output"]))
    for skin in document.get("skins", []):
        if "inverseBindMatrices" in skin:
            used_accessors.add(skin["inverseBindMatrices"])
            if not finite_rows(read_accessor(document, binary, skin["inverseBindMatrices"])):
                errors.append("non-finite inverse bind matrix")
    for animation in animations:
        errors.extend(animation["errors"])
    if not document.get("skins"):
        for original, record in zip(document.get("animations", []), animations):
            if not record["errors"]:
                record["posed_frames"] = inspect_rigid_frames(document, binary, original, record["duration"])
    transforms = [{"node": index, **{k: node[k] for k in ("translation", "rotation", "scale", "matrix") if k in node}}
                  for index, node in enumerate(document.get("nodes", [])) if any(k in node for k in ("translation", "rotation", "scale", "matrix"))]
    for transform in transforms:
        if not all(math.isfinite(v) for k, row in transform.items() if k != "node" for v in row):
            errors.append("non-finite node transform")
    return {"format": "GLB 2.0", "bytes": len(data), "sha256": digest(data),
            "vertices": len(model.positions), "triangles": model.triangle_count,
            "bounds": [low, high], "dimensions": [b-a for a, b in zip(low, high)],
            "topology": topology, "duplicate_faces": duplicate_faces,
            "coincident_quads": doubled_quads, "zero_uv_area_faces": uv_zero,
            "unused_primitive_vertices": unused_vertices, "authored_normal_vertices": len(normals),
            "geometry_sha256": digest(json.dumps(sorted(faces), separators=(",", ":")).encode()),
            "materials": materials, "unused_materials": sorted(set(range(len(materials)))-used_materials),
            "images": images, "transforms": transforms,
            "skins": [{"joints": len(s["joints"]), "unused_weighted_joint_slots": sorted(set(range(len(s["joints"]))) - used_joints)} for s in document.get("skins", [])],
            "animations": animations, "clip_metadata": document.get("asset", {}).get("extras", {}).get("places_entity_clips"),
            "unused_accessors": sorted(set(range(len(document.get("accessors", []))))-used_accessors),
            "errors": sorted(set(errors)), "review": reviews}


def inspect_png(path):
    try:
        return image_record(path.read_bytes())
    except (ValueError, KeyError) as error:
        return {"errors": [str(error)]}


def model_job(path):
    try:
        return inspect_model(path)
    except (ValueError, KeyError, IndexError, TypeError, struct.error) as error:
        return {"errors": [str(error)]}


def inventory(root=ROOT, workers=None):
    catalog = json.loads((root / "assets/catalog.json").read_text())
    entries = {e["id"]: e for e in catalog["assets"]}
    refs = defaultdict(set)
    code_refs = defaultdict(set)
    for folder, suffix in (("src", ".rs"), ("tools", ".py")):
        for path in sorted((root / folder).rglob("*"+suffix)):
            literals = set(re.findall(r'''["']([A-Za-z0-9:_.-]+)["']''', path.read_text()))
            for asset_id in literals & entries.keys():
                code_refs[asset_id].add(path.relative_to(root).as_posix())
    maps = []
    for folder in ("assets/levels", "levels", "tests/fixtures/levels"):
        for path in sorted((root / folder).rglob("*.json")):
            level = json.loads(path.read_text())
            ids = sorted((set(strings(level)) | {asset_id for asset_id, _ in asset_validation.level_ids(level)}) & entries.keys())
            name = path.relative_to(root).as_posix()
            maps.append({"file": name, "assets": ids})
            for asset_id in ids:
                refs[asset_id].add(name)
    # Expand material dependency use to their textures and automatic trim.
    changed = True
    while changed:
        changed = False
        for asset_id, entry in entries.items():
            for key in ("texture", "normal_texture", "emissive_mask", "baseboard"):
                target = entry.get(key)
                if target in entries:
                    before = len(refs[target])+len(code_refs[target])
                    refs[target].update(refs[asset_id])
                    code_refs[target].update(code_refs[asset_id])
                    changed |= before != len(refs[target])+len(code_refs[target])
    source_by_hash = defaultdict(list)
    source_by_pixels = defaultdict(list)
    textures = []
    textures_by_path = {}
    by_path = {"assets/"+e["model"]: e for e in entries.values() if "model" in e}
    png_paths = sorted((root / "assets").rglob("*.png"))
    for path, record in zip(png_paths, ordered_map(inspect_png, png_paths, workers)):
        name = path.relative_to(root).as_posix()
        entry = by_path.get(name, {})
        if "sha256" in record:
            source_by_hash[record["sha256"]].append(name)
            source_by_pixels[record["pixel_sha256"]].append(name)
        record.update(file=name, id=entry.get("id"), category=entry.get("asset_type", "model source PNG"),
                      maps=sorted(refs[entry.get("id")]), code_references=sorted(code_refs[entry.get("id")]))
        textures.append(record)
        textures_by_path[name] = record
    models = []
    model_paths = sorted((root / "assets").rglob("*.glb"))
    for path, record in zip(model_paths, ordered_map(model_job, model_paths, workers)):
        name = path.relative_to(root).as_posix()
        entry = by_path.get(name, {})
        if "images" in record:
            for image in record["images"]:
                image["source_pngs"] = source_by_hash[image["sha256"]]
                image["equivalent_pixel_sources"] = source_by_pixels[image["pixel_sha256"]]
                if not image["equivalent_pixel_sources"]:
                    record["errors"].append(f'embedded image {image["index"]} has no equivalent source PNG under assets/')
            record["catalog_size"] = entry.get("size")
            if entry.get("size"):
                record["size_matches_catalog"] = all(abs(a-b) <= max(0.02, b*0.06) for a, b in zip(record["dimensions"], entry["size"]))
        record.update(file=name, id=entry.get("id"), category=entry.get("category", "uncatalogued"),
                      maps=sorted(refs[entry.get("id")]), code_references=sorted(code_refs[entry.get("id")]))
        models.append(record)
        for image in record.get("images", []):
            for source in image["equivalent_pixel_sources"]:
                texture = textures_by_path[source]
                texture.setdefault("embedded_in", []).append(name)
                texture["maps"] = sorted(set(texture["maps"]) | set(record["maps"]))
                texture["code_references"] = sorted(set(texture["code_references"]) | set(record["code_references"]))
    material_records = [{**e, "maps": sorted(refs[e["id"]]), "code_references": sorted(code_refs[e["id"]])}
                        for e in entries.values() if e["asset_type"] == "material"]
    def duplicates(records, key):
        groups = defaultdict(list)
        for record in records:
            if key in record:
                groups[record[key]].append(record["file"])
        return [paths for paths in groups.values() if len(paths) > 1]
    other_files = [{"file": path.relative_to(root).as_posix(), "format": path.suffix,
                    "bytes": path.stat().st_size} for path in sorted((root / "assets").rglob("*"))
                   if path.is_file() and path.suffix not in (".glb", ".png")]
    icon = root / "icon.png"
    non_runtime = [{"file": "icon.png", **inspect_png(icon)}] if icon.is_file() else []
    return {"models": models, "textures": textures, "materials": material_records, "maps": maps,
            "other_asset_files": other_files, "non_runtime_images": non_runtime,
            "duplicate_model_files": duplicates(models, "sha256"),
            "same_model_geometry": duplicates(models, "geometry_sha256"),
            "duplicate_png_files": duplicates(textures, "sha256"),
            "same_png_pixels": duplicates(textures, "pixel_sha256"),
            "summary": {"models": len(models), "standalone_pngs": len(textures),
                        "embedded_pngs": sum(len(m.get("images", [])) for m in models),
                        "materials": len(material_records), "maps": len(maps),
                        "animated_models": sum(bool(m.get("animations")) for m in models),
                        "clips": sum(len(m.get("animations", [])) for m in models),
                        "errors": sum(len(r.get("errors", [])) for r in models+textures)}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/asset-audit/inventory.json")
    parser.add_argument("--workers", type=int, help="CPU allocation (bounded to 12)")
    args = parser.parse_args()
    try:
        worker_count(args.workers)
    except ValueError as error:
        parser.error(str(error))
    start = time.monotonic()
    report = inventory(workers=args.workers)
    atomic_write(args.out, (json.dumps(report, indent=2, sort_keys=True, allow_nan=False)+"\n").encode())
    print(json.dumps(report["summary"], sort_keys=True))
    print(f"Inventory: {args.out}; {time.monotonic()-start:.2f} s")
    for record in report["models"]+report["textures"]:
        for error in record.get("errors", []):
            print(f'FAIL {record["file"]}: {error}')
    return 1 if report["summary"]["errors"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
