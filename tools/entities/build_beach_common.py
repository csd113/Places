"""Small deterministic authoring helpers shared by the three Beach animal rigs.

This is an offline tool. Normal exports load committed native PNG artwork;
``--author-textures`` deliberately paints the master and its 256 px derivative.
No application code generates or repairs texture imagery at runtime.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "tools" / "props"))

from mesh import Mesh  # noqa: E402
from rig import Clip, Rig, SkinnedMesh, check_model, write_model  # noqa: E402
from tex import decode_png, write_png  # noqa: E402
from validate_entities import Model, frame_times  # noqa: E402

NATIVE_SIZE = 256
MASTER_SIZE = 1024
WHITE = (255, 255, 255)


def subtract(a, b):
    return tuple(x - y for x, y in zip(a, b))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def unit(v):
    length = math.sqrt(dot(v, v))
    if length <= 1e-12:
        raise ValueError("zero length direction")
    return tuple(x / length for x in v)


class Animal:
    """Closed, faceted pieces with one rigid influence per piece vertex."""

    def __init__(self, palette):
        self.mesh = Mesh()
        self.rig = Rig()
        self.palette = palette
        self.slots = []
        self.origins = {}
        self.part_triangles = {}

    def joint(self, name, position, parent=None):
        parent_id = self.rig.index(parent) if parent is not None else None
        local = subtract(position, self.origins[parent]) if parent else position
        index = self.rig.add(name, parent_id, local)
        self.origins[name] = position
        return index

    def uv(self, region):
        index = list(self.palette).index(region)
        # Eight swatches, four columns. Four native texels separate UVs from
        # region edges, so every mip retains the intended colour family.
        x, y = (index % 4) * 64, (index // 4) * 128
        return ((x + 4) / 256, (y + 4) / 256,
                (x + 60) / 256, (y + 124) / 256)

    def triangle(self, points, region, joint, outward):
        a, b, c = points
        normal = cross(subtract(b, a), subtract(c, a))
        if dot(normal, normal) <= 1e-16:
            raise ValueError(f"degenerate {joint}/{region} triangle")
        if dot(normal, outward) < 0:
            b, c = c, b
        u0, v0, u1, v1 = self.uv(region)
        self.mesh.triangle(a, b, c, ((u0, v1), (u1, v1), ((u0 + u1) * .5, v0)),
                           WHITE, shade_mult=1.0)
        self.slots.extend([self.rig.index(joint)] * 3)
        self.part_triangles[joint] = self.part_triangles.get(joint, 0) + 1

    def polyhedron(self, vertices, faces, region, joint):
        """Convex closed solid; each fan triangle faces away from its centre."""
        centre = tuple(sum(p[i] for p in vertices) / len(vertices) for i in range(3))
        for face in faces:
            for i in range(1, len(face) - 1):
                points = (vertices[face[0]], vertices[face[i]], vertices[face[i + 1]])
                middle = tuple(sum(p[k] for p in points) / 3 for k in range(3))
                self.triangle(points, region, joint, subtract(middle, centre))

    def ellipsoid(self, centre, radius, region, joint, segments=8, rings=3):
        """Faceted closed ellipsoid with single pole vertices, no zero caps."""
        cx, cy, cz = centre
        rx, ry, rz = radius
        vertices = [(cx, cy - ry, cz)]
        for row in range(1, rings + 1):
            latitude = -math.pi / 2 + math.pi * row / (rings + 1)
            for column in range(segments):
                angle = math.tau * column / segments + math.pi / segments
                vertices.append((cx + rx * math.cos(latitude) * math.cos(angle),
                                 cy + ry * math.sin(latitude),
                                 cz + rz * math.cos(latitude) * math.sin(angle)))
        vertices.append((cx, cy + ry, cz))
        top = len(vertices) - 1
        faces = []
        for i in range(segments):
            nxt = (i + 1) % segments
            faces.append((0, 1 + i, 1 + nxt))
            faces.append((top, 1 + (rings - 1) * segments + i,
                          1 + (rings - 1) * segments + nxt))
            for row in range(rings - 1):
                faces.append((1 + row * segments + i, 1 + row * segments + nxt,
                              1 + (row + 1) * segments + nxt, 1 + (row + 1) * segments + i))
        self.polyhedron(vertices, faces, region, joint)

    def segment(self, start, end, radius, region, joint, sides=5, end_radius=None):
        """Closed tapered tube with a stable cross section perpendicular to it."""
        axis = unit(subtract(end, start))
        helper = (0, 1, 0) if abs(axis[1]) < .9 else (0, 0, 1)
        across = unit(cross(axis, helper))
        up = cross(axis, across)
        vertices = []
        for p, r in ((start, radius), (end, end_radius if end_radius is not None else radius)):
            for i in range(sides):
                angle = math.tau * i / sides
                vertices.append(tuple(p[k] + r * (across[k] * math.cos(angle)
                                                + up[k] * math.sin(angle)) for k in range(3)))
        faces = [tuple(range(sides)), tuple(range(sides, sides * 2))]
        faces.extend((i, (i + 1) % sides, (i + 1) % sides + sides, i + sides)
                     for i in range(sides))
        self.polyhedron(vertices, faces, region, joint)

    def prism(self, polygon, axis, thickness, region, joint):
        """Closed thin piece; projected ear clipping also handles concave fins.

        Folded wings may have a nonplanar centre sheet. Caps follow one
        consistent winding against the extrusion axis, rather than an
        unreliable global centroid test on each nonplanar cap triangle.
        """
        normal = unit(axis)
        helper = (0, 1, 0) if abs(normal[1]) < .9 else (0, 0, 1)
        across = unit(cross(normal, helper))
        up = cross(normal, across)
        projected = [(dot(p, across), dot(p, up)) for p in polygon]
        area = sum(projected[i][0] * projected[(i + 1) % len(polygon)][1]
                   - projected[(i + 1) % len(polygon)][0] * projected[i][1]
                   for i in range(len(polygon)))
        if abs(area) < 1e-12:
            raise ValueError("prism outline has no projected area")
        if area < 0:
            polygon = list(reversed(polygon))
            projected = list(reversed(projected))
        order = list(range(len(polygon)))
        triangles = []

        def turn(a, b, c):
            return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])

        while len(order) > 3:
            for i, b in enumerate(order):
                a, c = order[i - 1], order[(i + 1) % len(order)]
                if turn(projected[a], projected[b], projected[c]) <= 1e-12:
                    continue
                if any(turn(projected[a], projected[b], projected[p]) >= -1e-12
                       and turn(projected[b], projected[c], projected[p]) >= -1e-12
                       and turn(projected[c], projected[a], projected[p]) >= -1e-12
                       for p in order if p not in (a, b, c)):
                    continue
                triangles.append((a, b, c))
                del order[i]
                break
            else:
                raise ValueError("prism outline cannot be triangulated; inspect self intersections")
        triangles.append(tuple(order))
        vertices = [tuple(p[k] + sign * normal[k] * thickness / 2 for k in range(3))
                    for sign in (-1, 1) for p in polygon]
        n = len(polygon)
        for a, b, c in triangles:
            self.triangle((vertices[a], vertices[c], vertices[b]), region, joint,
                          tuple(-v for v in normal))
            self.triangle((vertices[a + n], vertices[b + n], vertices[c + n]), region, joint, normal)
        for i in range(n):
            nxt = (i + 1) % n
            outward = cross(subtract(polygon[nxt], polygon[i]), normal)
            self.triangle((vertices[i], vertices[nxt], vertices[nxt + n]), region, joint, outward)
            self.triangle((vertices[i], vertices[nxt + n], vertices[i + n]), region, joint, outward)

    def centre_at_floor(self):
        low, high = self.mesh.bounds()
        offset = ((low[0] + high[0]) / 2, low[1], (low[2] + high[2]) / 2)
        self.mesh.positions = [subtract(p, offset) for p in self.mesh.positions]
        for joint in self.rig.joints:
            if joint.parent is None:
                joint.translation = subtract(joint.translation, offset)
        self.origins = {name: subtract(p, offset) for name, p in self.origins.items()}

    def skinned(self):
        return SkinnedMesh(self.mesh, [[j, 0, 0, 0] for j in self.slots],
                           [[1.0, 0.0, 0.0, 0.0] for _ in self.slots])


def author_texture(palette):
    """Broad restrained painted facets, never noise or baked illumination."""
    rgba = bytearray(MASTER_SIZE * MASTER_SIZE * 4)
    colours = list(palette.values())
    if len(colours) != 8:
        raise ValueError("animal atlases carry exactly eight fitted regions")
    for y in range(MASTER_SIZE):
        row = y // 512
        local_y = y % 512
        for x in range(MASTER_SIZE):
            column = x // 256
            local_x = x % 256
            colour = colours[row * 4 + column]
            # One diagonal painted facet per cell, gently distinct from its
            # base. These are albedo variation; geometry owns actual shading.
            factor = .975 if local_y > 290 + local_x * .55 else 1.0
            rgb = tuple(round(c * factor) for c in colour)
            index = (y * MASTER_SIZE + x) * 4
            rgba[index:index + 4] = bytes((*rgb, 255))
    return write_png(MASTER_SIZE, MASTER_SIZE, bytes(rgba)), downsample(bytes(rgba))


def downsample(master_rgba):
    native = bytearray(NATIVE_SIZE * NATIVE_SIZE * 4)
    for y in range(NATIVE_SIZE):
        for x in range(NATIVE_SIZE):
            samples = [(y * 4 + dy) * MASTER_SIZE * 4 + (x * 4 + dx) * 4
                       for dy in range(4) for dx in range(4)]
            pixel = [round(sum(master_rgba[i + c] for i in samples) / 16) for c in range(4)]
            index = (y * NATIVE_SIZE + x) * 4
            native[index:index + 4] = bytes(pixel)
    return write_png(NATIVE_SIZE, NATIVE_SIZE, bytes(native))


def validate(path, animal):
    """Read actual GLB bytes, sample full clips and prove runtime envelopes."""
    structure = check_model(path)
    if structure["problems"]:
        raise ValueError(structure["problems"])
    model = Model(path)
    for joints, weights in zip(model.vertex_joints, model.vertex_weights):
        if any(not math.isfinite(w) or w < 0 for w in weights) or abs(sum(weights) - 1) > 1e-6:
            raise ValueError("invalid skin weights")
        if any(not 0 <= int(j) < len(model.joints) for j in joints):
            raise ValueError("skin influence names a joint outside the rig")
    low = [min(p[i] for p in model.bind_skin) for i in range(3)]
    high = [max(p[i] for p in model.bind_skin) for i in range(3)]
    centre = [(lo + hi) / 2 for lo, hi in zip(low, high)]
    half = [(hi - lo) / 2 for lo, hi in zip(low, high)]
    envelope_low = [c - h * 1.15 - .05 for c, h in zip(centre, half)]
    envelope_high = [c + h * 1.15 + .05 for c, h in zip(centre, half)]
    if max(abs(low[0] + high[0]), abs(low[2] + high[2])) > 1e-5 or abs(low[1]) > 1e-5:
        raise ValueError("origin must be the horizontally centred floor contact")
    clip_stats = []
    for clip in model.clips:
        times = frame_times(clip, 120.0, None)
        posed_low = [math.inf] * 3
        posed_high = [-math.inf] * 3
        max_stretch = 0
        for time in times:
            posed = model._skin(model._pose_globals(clip, time))
            for axis in range(3):
                posed_low[axis] = min(posed_low[axis], min(p[axis] for p in posed))
                posed_high[axis] = max(posed_high[axis], max(p[axis] for p in posed))
            for original, length in zip(model.bind_edges, model._edge_lengths(posed)):
                if original > 1e-9:
                    max_stretch = max(max_stretch, length / original)
        if any(lo < bound - 1e-5 for lo, bound in zip(posed_low, envelope_low)) or any(
                hi > bound + 1e-5 for hi, bound in zip(posed_high, envelope_high)):
            raise ValueError(f'{clip["name"]} escapes runtime culling/lighting envelope: '
                             f'{posed_low}..{posed_high} vs {envelope_low}..{envelope_high}')
        if clip["name"] in ("idle", "walk") and posed_low[1] < -.002:
            raise ValueError(f'{clip["name"]} penetrates the support floor')
        animation = next(a for a in model.animations if a["name"] == clip["name"])
        for channel in animation["channels"]:
            sampler = animation["samplers"][channel["sampler"]]
            values = model._read_accessor(sampler["output"],
                                          "VEC4" if channel["target"]["path"] == "rotation" else "VEC3")
            if clip["loop"]:
                direct = max(abs(a - b) for a, b in zip(values[0], values[-1]))
                if channel["target"]["path"] == "rotation":
                    direct = min(direct, max(abs(a + b) for a, b in zip(values[0], values[-1])))
                if direct > 1e-6:
                    raise ValueError(f'{clip["name"]} loop is discontinuous')
        clip_stats.append({"name": clip["name"], "duration": clip["duration"],
                           "frames_120hz": len(times), "posed_min": posed_low,
                           "posed_max": posed_high, "max_edge_stretch": max_stretch,
                           "reference_speed_mps": clip["reference_speed_mps"]})
        if clip["name"] == "walk" and "leg_l_01" in animal.origins:
            # The planted lowest toe vertex is exported at both stance ends.
            # Measure its mean backwards travel from the GLB, independently
            # of the builder's rest-tip stride formula.
            slot = next(i for i, node in enumerate(model.nodes) if node.get("name") == "leg_l_01")
            foot = [i for i, (joints, p) in enumerate(zip(model.vertex_joints, model.bind_skin))
                    if int(joints[0]) == slot and p[1] <= low[1] + 1e-6]
            if not foot:
                raise ValueError("walk has no planted toe vertices")
            start = model._skin(model._pose_globals(clip, 0))
            end_time = clip["duration"] * .60
            end = model._skin(model._pose_globals(clip, end_time))
            speed = sum(start[i][2] - end[i][2] for i in foot) / len(foot) / end_time
            if abs(speed - clip["reference_speed_mps"]) > 1e-5:
                raise ValueError("walk clip reference speed differs from exported stance sweep")
            clip_stats[-1]["measured_stance_speed_mps"] = speed
    # Soup vertices are intentional: every triangle owns a flat geometric
    # normal. Check winding at piece construction, degenerates again on export.
    oriented_edges = {}
    for index in range(0, len(model.indices), 3):
        a, b, c = [model.positions[i] for i in model.indices[index:index + 3]]
        normal = cross(subtract(b, a), subtract(c, a))
        if dot(normal, normal) <= 1e-16:
            raise ValueError("exported triangle is degenerate")
        for p, q in ((a, b), (b, c), (c, a)):
            first, second = tuple(round(v, 6) for v in p), tuple(round(v, 6) for v in q)
            key = tuple(sorted((first, second)))
            count, orientation = oriented_edges.get(key, (0, 0))
            oriented_edges[key] = (count + 1, orientation + (1 if first < second else -1))
    if any(count != 2 for count, _ in oriented_edges.values()):
        raise ValueError("exported geometry has an open or nonmanifold edge")
    if any(orientation != 0 for _, orientation in oriented_edges.values()):
        raise ValueError("exported closed shell has inconsistent shared-edge winding")
    for mesh in model.document["meshes"]:
        for primitive in mesh["primitives"]:
            uvs = model._read_accessor(primitive["attributes"]["TEXCOORD_0"], "VEC2")
            if any(not math.isfinite(v) or not 0 <= v <= 1 for uv in uvs for v in uv):
                raise ValueError("invalid fitted UV")
    return {"triangles": len(model.indices) // 3, "vertices": len(model.positions),
            "joints": len(model.joints), "bind_min": low, "bind_max": high,
            "size": [hi - lo for lo, hi in zip(low, high)],
            "envelope_min": envelope_low, "envelope_max": envelope_high,
            "closed_edges": len(oriented_edges), "shared_edge_winding": "consistent",
            "part_triangles": animal.part_triangles, "clips": clip_stats}


def main(name, palette, build, clips, argv=None):
    parser = argparse.ArgumentParser(description=f"Deterministic Beach {name} asset author/export")
    parser.add_argument("--check", action="store_true", help="check export determinism without writes")
    parser.add_argument("--author-textures", action="store_true", help="explicitly repaint source + native PNG")
    args = parser.parse_args(argv)
    directory = REPO_ROOT / "assets" / "entities" / f"beach_{name}"
    texture_path = directory / "textures" / "surface_01.png"
    master_path = directory / "textures" / "surface_master.png"
    path = directory / "model" / f"beach_{name}.glb"
    if args.author_textures and args.check:
        raise ValueError("--author-textures mutates artwork and cannot be combined with --check")
    if args.author_textures:
        master, native = author_texture(palette)
        texture_path.parent.mkdir(parents=True, exist_ok=True)
        master_path.write_bytes(master)
        texture_path.write_bytes(native)
    if not texture_path.is_file() or not master_path.is_file():
        raise ValueError("committed source/native PNGs missing; intentionally use --author-textures first")
    master_width, master_height, master_rgba = decode_png(master_path.read_bytes())
    native_width, native_height, native_rgba = decode_png(texture_path.read_bytes())
    if (master_width, master_height) != (MASTER_SIZE, MASTER_SIZE) or (
            native_width, native_height) != (NATIVE_SIZE, NATIVE_SIZE):
        raise ValueError("1024 master and 256 native square atlas contract violated")
    if any(alpha != 255 for alpha in master_rgba[3::4]) or any(
            alpha != 255 for alpha in native_rgba[3::4]):
        raise ValueError("animal artwork must be opaque")
    if downsample(master_rgba) != texture_path.read_bytes():
        raise ValueError("native atlas is stale against committed master")
    if args.check:
        master, native = author_texture(palette)
        if master_path.read_bytes() != master or texture_path.read_bytes() != native:
            raise ValueError("committed source/native PNGs differ from their deterministic authoring source")
    animal = build()
    animal.centre_at_floor()
    animations = clips(animal)
    generator = f"tools/entities/build_beach_{name}.py"
    with tempfile.TemporaryDirectory(prefix=f"places-beach-{name}-") as temp:
        fresh_path = Path(temp) / path.name
        # rig.write_model's report accepts paths outside its repository only
        # when passed as relative paths; the export itself is still in /tmp.
        write_model(Path(os.path.relpath(fresh_path, Path.cwd())), animal.skinned(), animal.rig, animations,
                    texture_path.read_bytes(), name=f"beach_{name}", generator=generator)
        stats = validate(fresh_path, animal)
        exported = fresh_path.read_bytes()
        if args.check:
            if not path.is_file() or path.read_bytes() != exported:
                raise ValueError("GLB differs from deterministic builder and committed PNG")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            if not path.is_file() or path.read_bytes() != exported:
                path.write_bytes(exported)
    print(json.dumps({"asset": str(path.relative_to(REPO_ROOT)),
                      "sha256": hashlib.sha256(exported).hexdigest(),
                      "bytes": len(exported), "deterministic_check": args.check, **stats}, indent=2))
    return 0
