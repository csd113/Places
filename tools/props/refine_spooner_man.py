#!/usr/bin/env python3
"""Refine the manual cat's cross sections, preserving its head and animation.

The immutable input is an actual pre-refinement export, not a reconstructed
cat. Only coarse circumferential edges are split; existing longitudinal rings
and anatomical connections are retained. New ring vertices are projected onto
authored elliptical sections, rather than leaving subdivided flat panels.
Images, materials, nodes, inverse binds and every animation key are copied.
This is offline geometry authoring; it never creates texture artwork.
"""

from __future__ import annotations

import argparse
import collections
import copy
import hashlib
import math
from pathlib import Path

import numpy as np

from animate_spooner_man import Model, glb_document_bytes
from glb import _read_accessor

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tools/props/sources/spooner_man_original.glb"
OUTPUT = ROOT / "assets/entities/spooner-man/model/spooner-man.glb"
SOURCE_SHA256 = "3ac24ca7efea8a04ce70e285d0219e372826bdd36503d2541237213cb1084981"
HEAD_Z = 0.21899

# z, half width, belly, back: the original authored rings, now given a
# continuous feline rib cage/waist/haunch profile. The final head ring is fixed.
SECTIONS = (
    (-.17424, .033, .171, .242),
    (-.14848, .059, .143, .260),
    (-.10248, .075, .137, .275),
    (-.04912, .064, .131, .267),
    (.00792, .064, .129, .267),
    (.06680, .071, .128, .278),
    (.12568, .068, .127, .279),
    (.16800, .052, .148, .270),
    (.19600, .037, .187, .268),
    (.21000, .030, .195, .271),
    (.21900, .0273, .22135, .2753),
)


def position_key(point):
    return tuple(round(float(x), 7) for x in point)


def interpolate(rows, value):
    return np.array([np.interp(value, [r[0] for r in rows], [r[k] for r in rows])
                     for k in range(1, len(rows[0]))])


def convex_hull(points):
    points = sorted(set(tuple(map(float, p)) for p in points))

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for point in points:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], point) <= 0:
            lower.pop()
        lower.append(point)
    for point in reversed(points):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], point) <= 0:
            upper.pop()
        upper.append(point)
    return np.asarray(lower[:-1] + upper[:-1])


def boundary_radius(hull, direction):
    intersections = []
    for a, b in zip(hull, np.roll(hull, -1, axis=0)):
        edge = b - a
        determinant = direction[0] * edge[1] - direction[1] * edge[0]
        if abs(determinant) < 1e-12:
            continue
        distance = (a[0] * edge[1] - a[1] * edge[0]) / determinant
        t = (a[0] * direction[1] - a[1] * direction[0]) / determinant
        if distance > 0 and -1e-6 <= t <= 1 + 1e-6:
            intersections.append(distance)
    return min(intersections) if intersections else 1.0


def primitives(model):
    result = []
    for primitive in model.json["meshes"][0]["primitives"]:
        attributes = {name: np.asarray(_read_accessor(model.json, model.bin, index))
                      for name, index in primitive["attributes"].items()}
        indices = np.asarray(_read_accessor(model.json, model.bin, primitive["indices"]),
                             dtype=int).reshape(-1, 3)
        result.append((primitive["material"], attributes, indices))
    return result


class Refinement:
    def __init__(self, model, stage):
        self.model = model
        self.stage = stage
        self.inputs = primitives(model)
        self.torso_keys = set()
        self.old_sections = []
        self.section_hulls = []
        self.leg_sections = {}
        self.leg_hulls = {}
        self.weight_loss = 0.0
        self.split_edges = 0
        for material, a, triangles in self.inputs[:2]:
            for triangle in triangles:
                if min(a["POSITION"][triangle, 2]) < HEAD_Z:
                    for p in a["POSITION"][triangle]:
                        if p[2] >= -.174241:
                            self.torso_keys.add(position_key(p))
        for z, *_ in SECTIONS:
            ring = [p for _, a, _ in self.inputs[:2] for p in a["POSITION"]
                    if abs(p[2] - z) < 1e-5]
            self.old_sections.append((z, max(abs(p[0]) for p in ring),
                                      min(p[1] for p in ring), max(p[1] for p in ring)))
            # A convex hull distinguishes the original outer ring from its
            # interior cut/junction vertices. Preserve that radial fraction.
            hull = convex_hull([(p[0], p[1]) for p in ring])
            _, width, low, high = self.old_sections[-1]
            self.section_hulls.append(np.array([
                (x / width, (y - (low + high) / 2) / ((high - low) / 2))
                for x, y in hull
            ]))
        a = self.inputs[2][1]
        for front in (False, True):
            points = [p for p in a["POSITION"] if p[0] > 0 and (p[2] > 0) == front]
            rows = []
            hulls = []
            for y in sorted(set(round(p[1], 5) for p in points if p[1] <= .119)):
                ring = [p for p in points if abs(p[1] - y) < 1e-5]
                xmin, xmax = min(p[0] for p in ring), max(p[0] for p in ring)
                zmin, zmax = min(p[2] for p in ring), max(p[2] for p in ring)
                rows.append((y, (xmin + xmax) / 2, (zmin + zmax) / 2,
                             (xmax - xmin) / 2, (zmax - zmin) / 2))
                _, cx, cz, rx, rz = rows[-1]
                hull = convex_hull([(p[0], p[2]) for p in ring])
                hulls.append(np.array([((x - cx) / rx, (z - cz) / rz) for x, z in hull]))
            self.leg_sections[front] = rows
            self.leg_hulls[front] = hulls
        # Original tail rings follow the rig's model-space polyline.
        self.tail_curve = np.asarray([model.rest_global(model.index_of["pelvis"])[1]] +
                                    [model.rest_global(model.index_of[f"tail_0{i}"])[1]
                                     for i in range(1, 9)])

    def torso(self, p):
        x, y, z = p
        if self.stage >= 3 and abs(z + .17424) < 1e-5:
            # The cut rear cap contains the tail's original insertion boundary.
            # Keep that planar polygon affine and its attachment unchanged.
            return np.asarray(p)
        oldw, low, high = interpolate(self.old_sections, z)
        width, belly, back = interpolate(SECTIONS, z)
        centre, radius = (low + high) / 2, (high - low) / 2
        angle = math.atan2((y - centre) / radius, x / oldw)
        direction = np.array([math.cos(angle), math.sin(angle)])
        boundary = [boundary_radius(hull, direction) for hull in self.section_hulls]
        fraction = math.hypot(x / oldw, (y - centre) / radius) / np.interp(
            z, [r[0] for r in self.old_sections], boundary)
        # Reprojecting coarse polygon chords creates deliberate round sections.
        return np.array([width * math.cos(angle) * fraction,
                         (belly + back) / 2 + (back - belly) / 2 * math.sin(angle) * fraction, z])

    def transform(self, p, material):
        if p[2] >= HEAD_Z:
            return np.array(p)
        if material != 2 and p[2] >= -.174241:
            return self.torso(p)
        if material == 2:
            # Shoulder/hip vertices shared with the torso receive the very same
            # transform. No independent overlapping limb shells are introduced.
            if position_key(p) in self.torso_keys:
                return self.torso(p)
            rows = self.leg_sections[p[2] > 0]
            cx, cz, rx, rz = interpolate(rows, p[1])
            side = 1 if p[0] > 0 else -1
            angle = math.atan2((p[2] - cz) / rz, (abs(p[0]) - cx) / rx)
            fraction = 1.0
            if self.stage >= 3:
                direction = np.array([math.cos(angle), math.sin(angle)])
                boundary = [boundary_radius(hull, direction) for hull in self.leg_hulls[p[2] > 0]]
                fraction = math.hypot((abs(p[0]) - cx) / rx, (p[2] - cz) / rz) / np.interp(
                    p[1], [r[0] for r in rows], boundary)
            # Preserve the paw's exact floor and footprint; round its perimeter.
            target = np.array([side * (cx + rx * math.cos(angle) * fraction), p[1],
                               cz + rz * math.sin(angle) * fraction])
            if p[1] > rows[-1][0]:
                # Upper attachment is already cut into the body: taper the
                # section projection away as it reaches the existing junction.
                blend = min(1.0, (p[1] - rows[-1][0]) / .045)
                if self.stage >= 3:
                    target = np.asarray(p).copy()
                else:
                    target = target * (1 - blend) + np.asarray(p) * blend
                body_delta = self.torso(p) - np.asarray(p)
                target += body_delta * blend
            return target
        if self.stage >= 2:
            # Round the original tail rings in their tangent plane. The medial
            # curve and the eight tail joints stay in the same authored pose.
            curve = self.tail_curve[1:]
            best = None
            for a, b in zip(curve, curve[1:]):
                tangent = b - a
                t = np.clip(np.dot(p - a, tangent) / np.dot(tangent, tangent), 0, 1)
                centre = a + t * tangent
                distance = np.linalg.norm(p - centre)
                if best is None or distance < best[0]:
                    best = (distance, centre, tangent / np.linalg.norm(tangent))
            distance, centre, tangent = best
            offset = np.asarray(p) - centre
            # Avoid lengthening the tip or inflating the root's cut junction.
            if -.280 < p[2] < -.195 and p[1] < .35:
                parallel = np.dot(offset, tangent) * tangent
                radial = offset - parallel
                # Gentle 5% fullness makes the balancing tail less wire-like.
                return centre + parallel + radial * 1.05
        return np.asarray(p)

    def split(self, a, b, material):
        if min(a[2], b[2]) >= HEAD_Z or max(a[2], b[2]) >= .21898:
            return False
        distance = np.linalg.norm(a - b)
        if distance < .006:
            return False
        if material == 2:
            return abs(a[1] - b[1]) < 1e-5 and distance < .065
        if min(a[2], b[2]) >= -.174241:
            return abs(a[2] - b[2]) < 1e-5
        if self.stage >= 2:
            # Select circumferential tail edges by projection on its rig curve.
            midpoint = (a + b) / 2
            curve = self.tail_curve[1:]
            segment = min(zip(curve, curve[1:]), key=lambda pair:
                          np.linalg.norm(midpoint - (pair[0] + pair[1]) / 2))
            tangent = segment[1] - segment[0]
            return abs(np.dot(a - b, tangent) / np.linalg.norm(tangent)) < .0001
        return False

    def midpoint(self, a, b):
        values = {name: (a[name] + b[name]) / 2 for name in a if name != "JOINTS_0"}
        totals = collections.defaultdict(float)
        for v in (a, b):
            for joint, weight in zip(v["JOINTS_0"], v["WEIGHTS_0"]):
                totals[int(joint)] += float(weight) / 2
        ranked = sorted(totals.items(), key=lambda x: (-x[1], x[0]))
        self.weight_loss = max(self.weight_loss, sum(x[1] for x in ranked[4:]))
        ranked = ranked[:4]
        total = sum(x[1] for x in ranked)
        values["JOINTS_0"] = np.array([x[0] for x in ranked] + [0] * (4 - len(ranked)))
        values["WEIGHTS_0"] = np.array([x[1] / total for x in ranked] + [0] * (4 - len(ranked)))
        return values

    def build(self):
        output = []
        # Decide splits by geometric edge, across UV/material seams. The shared
        # anatomical surface stays conforming even when glTF duplicates corners.
        split_keys = set()
        blocked_keys = set()
        for material, a, triangles in self.inputs:
            for tri in triangles:
                for i, j in zip(tri, np.roll(tri, -1)):
                    p, q = a["POSITION"][i], a["POSITION"][j]
                    key = tuple(sorted((position_key(p), position_key(q))))
                    influences = {int(joint) for k in (i, j)
                                  for joint, weight in zip(a["JOINTS_0"][k], a["WEIGHTS_0"][k])
                                  if weight > 0}
                    if len(influences) > 4:
                        blocked_keys.add(key)
                    if self.split(p, q, material):
                        split_keys.add(key)
        # The runtime accepts four influences. Do not split an edge that would
        # require dropping any of the original skin's nonzero influences.
        split_keys -= blocked_keys
        if self.stage >= 3:
            # Material seams duplicate the same source position. Newly split
            # attachment edges must enter the shared torso transform too.
            for a, b in split_keys:
                if a in self.torso_keys and b in self.torso_keys:
                    self.torso_keys.add(position_key((np.array(a) + np.array(b)) / 2))
        self.split_edges = len(split_keys)
        for material, a, triangles in self.inputs:
            faces = []
            for triangle in triangles:
                corners = [{name: values[i].copy() for name, values in a.items()} for i in triangle]
                head = min(c["POSITION"][2] for c in corners) >= HEAD_Z
                polygon = []
                for i in range(3):
                    v, w = corners[i], corners[(i + 1) % 3]
                    polygon.append(v)
                    key = tuple(sorted((position_key(v["POSITION"]), position_key(w["POSITION"]))))
                    if key in split_keys:
                        polygon.append(self.midpoint(v, w))
                # At most one ring edge per ordinary longitudinal triangle.
                # Rare attachment triangles with multiple selected edges use
                # ear clipping to avoid collinear zero-area triangles.
                if len(polygon) == 3:
                    new_faces = [polygon]
                elif len(polygon) == 4:
                    # Find the inserted vertex by identity, rather than array equality.
                    mid = next(i for i in range(4) if all(polygon[i] is not c for c in corners))
                    opposite = (mid + 2) % 4
                    new_faces = [[polygon[mid], polygon[(mid + 1) % 4], polygon[opposite]],
                                 [polygon[mid], polygon[opposite], polygon[(mid + 3) % 4]]]
                else:
                    # Ear clipping in the original triangle's plane uses only
                    # existing corners and edge midpoints, keeping their skin.
                    normal = np.cross(corners[1]["POSITION"] - corners[0]["POSITION"],
                                      corners[2]["POSITION"] - corners[0]["POSITION"])
                    axis = int(np.argmax(abs(normal)))
                    remaining = polygon[:]
                    new_faces = []
                    while len(remaining) > 3:
                        for i in range(len(remaining)):
                            ear = [remaining[(i - 1) % len(remaining)], remaining[i],
                                   remaining[(i + 1) % len(remaining)]]
                            aa, bb, cc = [np.delete(v["POSITION"], axis) for v in ear]
                            cross = (bb[0] - aa[0]) * (cc[1] - aa[1]) - (bb[1] - aa[1]) * (cc[0] - aa[0])
                            if abs(cross) > 1e-12:
                                new_faces.append(ear)
                                remaining.pop(i)
                                break
                        else:
                            raise ValueError("collinear attachment polygon")
                    new_faces.append(remaining)
                for face in new_faces:
                    faces.append((head, [dict(v, _source_position=v["POSITION"], POSITION=(v["POSITION"] if head else
                                 self.transform(v["POSITION"], material))) for v in face]))
            output.append((material, faces))
        # UV correction is separate from geometric projection. The head never
        # passes through this path, so every one of its original attributes stays.
        for material, faces in output:
            for head, face in faces:
                if head:
                    continue
                centre = sum(v["POSITION"] for v in face) / 3
                dark_tail = material == 0 and self.stage >= 2 and centre[2] < -.174241 and centre[1] < .345
                dark_neck = material == 1 and self.stage >= 2 and centre[2] > .209 and centre[1] > .255
                for v in face:
                    p = v["POSITION"]
                    if material == 0 and p[2] >= -.174241:
                        if self.stage >= 3:
                            width, belly, back = interpolate(SECTIONS, p[2])
                            angle = math.atan2(p[0] / width,
                                               (p[1] - (belly + back) / 2) / ((back - belly) / 2))
                            v["TEXCOORD_0"] = np.array([.52 + .115 * ((p[2] + .175) / .394),
                                                       .025 + .33 * ((angle + math.pi) / math.tau)])
                        else:
                            v["TEXCOORD_0"] = np.array([.855 + .115 * ((p[2] + .175) / .394),
                                                       .025 + .13 * ((p[0] + .083) / .166)])
                    elif dark_tail:
                        # The source has three root triangles at UV (.22,.46),
                        # the WHITE tip swatch. The root is dark fur, while all
                        # actual tip triangles retain their original mapping.
                        # Use the SAME dark atlas column as the torso. Even
                        # dark endpoint UVs in two separate regions can cross
                        # the spare white swatch inside a joining triangle.
                        v["TEXCOORD_0"] = np.array([.54 + .04 * ((p[2] + .297) / .123),
                                                   .06 + .09 * ((p[0] + .02) / .04)])
                    elif dark_neck:
                        # Source upper-neck faces cross from u=.005 to .495:
                        # their interiors hit the white bib despite dark corners.
                        # Keep that back-of-neck strip in existing dark fur.
                        v["TEXCOORD_0"] = np.array([.88 + .04 * ((p[0] + .03) / .06), .09])
        if self.stage >= 3:
            # The manual Boolean junction includes very small triangles. Limit
            # projection locally if one folds, applying the same correction to
            # every material copy. Do not change winding to conceal a fold.
            for _ in range(20):
                corrections = {}
                for _, faces in output:
                    for head, face in faces:
                        if head:
                            continue
                        a, b, c = [v["POSITION"] for v in face]
                        cross = np.cross(b - a, c - a)
                        reference = sum(v["NORMAL"] for v in face) / 3
                        if np.dot(cross, reference) < -1e-12:
                            for v in face:
                                corrections[position_key(v["POSITION"])] = (
                                    v["POSITION"] + v["_source_position"]) / 2
                if not corrections:
                    break
                for _, faces in output:
                    for head, face in faces:
                        if not head:
                            for v in face:
                                v["POSITION"] = corrections.get(position_key(v["POSITION"]), v["POSITION"])
            else:
                raise ValueError("anatomical junction projection folded")
        normals = collections.defaultdict(lambda: np.zeros(3))
        for _, faces in output:
            for head, face in faces:
                if head:
                    continue
                a, b, c = [v["POSITION"] for v in face]
                normal = np.cross(b - a, c - a)
                for v in face:
                    normals[position_key(v["POSITION"])] += normal
        for _, faces in output:
            for head, face in faces:
                if not head:
                    for v in face:
                        n = normals[position_key(v["POSITION"])]
                        length = np.linalg.norm(n)
                        if length < 1e-10:
                            raise ValueError("degenerate authored normal")
                        v["NORMAL"] = n / length
                    if self.stage >= 3:
                        points = np.array([v["POSITION"] for v in face])
                        # Flat sole and rear-cap creases should not share the
                        # side's averaged normals. This also preserves their
                        # correct outward lighting at very close native views.
                        sole = np.max(abs(points[:, 1])) < 1e-6
                        rear = np.max(abs(points[:, 2] + .17424)) < 1e-5
                        if sole or rear:
                            n = np.cross(points[1] - points[0], points[2] - points[0])
                            n = n / np.linalg.norm(n)
                            for v in face:
                                v["NORMAL"] = n
                        else:
                            # Keep a deliberate flat face at strongly concave
                            # cut junctions where averaged normals point inward.
                            n = np.cross(points[1] - points[0], points[2] - points[0])
                            if np.dot(n, sum(v["NORMAL"] for v in face)) <= 0:
                                for v in face:
                                    v["NORMAL"] = n / np.linalg.norm(n)
                for v in face:
                    v.pop("_source_position")
        return output


def export(model, geometry, stage):
    doc = copy.deepcopy(model.json)
    doc["bufferViews"], doc["accessors"] = [], []
    payload = bytearray()
    original_accessors = {}

    def view(data, target=None):
        payload.extend(b"\0" * (-len(payload) % 4))
        result = {"buffer": 0, "byteOffset": len(payload), "byteLength": len(data)}
        if target is not None:
            result["target"] = target
        payload.extend(data)
        doc["bufferViews"].append(result)
        return len(doc["bufferViews"]) - 1

    def accessor(values, kind, component=5126, target=None):
        dtype = {5126: "<f4", 5123: "<u2", 5121: "u1"}[component]
        array = np.asarray(values, dtype=dtype)
        entry = {"bufferView": view(array.tobytes(), target), "componentType": component,
                 "count": len(array), "type": kind}
        if kind == "VEC3" or kind == "SCALAR":
            entry["min"] = np.atleast_1d(array.min(axis=0)).tolist()
            entry["max"] = np.atleast_1d(array.max(axis=0)).tolist()
        doc["accessors"].append(entry)
        return len(doc["accessors"]) - 1

    def original(index):
        if index not in original_accessors:
            a = model.json["accessors"][index]
            values = (model.read_accessor(index, 16) if a["type"] == "MAT4"
                      else _read_accessor(model.json, model.bin, index))
            original_accessors[index] = accessor(values,
                                                 a["type"], a["componentType"])
        return original_accessors[index]

    for skin in doc["skins"]:
        skin["inverseBindMatrices"] = original(skin["inverseBindMatrices"])
    for image in doc["images"]:
        source_view = model.json["bufferViews"][image["bufferView"]]
        start = source_view.get("byteOffset", 0)
        image["bufferView"] = view(model.bin[start:start + source_view["byteLength"]])
    mesh_primitives = []
    for material, faces in geometry:
        vertices, indices, seen = [], [], {}
        for _, face in faces:
            for v in face:
                signature = b"".join(np.asarray(v[name], dtype="<f4").tobytes()
                                     for name in sorted(v))
                if signature not in seen:
                    seen[signature] = len(vertices)
                    vertices.append(v)
                indices.append(seen[signature])
        attributes = {}
        for name in vertices[0]:
            values = [v[name] for v in vertices]
            attributes[name] = accessor(values, {2: "VEC2", 3: "VEC3", 4: "VEC4"}[len(values[0])],
                                         5121 if name == "JOINTS_0" else 5126, 34962)
        mesh_primitives.append({"attributes": attributes, "indices": accessor(indices, "SCALAR", 5123, 34963),
                                "material": material, "mode": 4})
    doc["meshes"][0]["primitives"] = mesh_primitives
    marker = doc["asset"]["extras"]["places_entity_clips"]
    marker.update(base_bin_bytes=len(payload), base_buffer_views=len(doc["bufferViews"]),
                  base_accessors=len(doc["accessors"]))
    for animation in doc["animations"]:
        for sampler in animation["samplers"]:
            sampler["input"], sampler["output"] = original(sampler["input"]), original(sampler["output"])
    doc["buffers"] = [{"byteLength": len(payload)}]
    doc["asset"]["extras"]["places_body_refinement"] = {
        "source_sha256": SOURCE_SHA256, "generator": "tools/props/refine_spooner_man.py", "stage": stage}
    return glb_document_bytes(doc, bytes(payload))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=OUTPUT)
    parser.add_argument("--stage", type=int, choices=(1, 2, 3), default=3)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise SystemExit("immutable manual source changed")
    model = Model(str(SOURCE))
    refinement = Refinement(model, args.stage)
    geometry = refinement.build()
    result = export(model, geometry, args.stage)
    if args.check:
        if args.out.read_bytes() != result:
            raise SystemExit("canonical GLB differs from deterministic refinement")
    else:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_bytes(result)
    print(f"{sum(len(faces) for _, faces in geometry)} triangles; {refinement.split_edges} ring edges refined; "
          f"maximum discarded midpoint influence {refinement.weight_loss:.6f}; {len(result)} bytes")


if __name__ == "__main__":
    main()
