"""Read-only checks of GLB vertex attributes and coincident surfaces."""
from collections import defaultdict
import math

from .animation import finite_rows


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def _dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def non_finite_attributes(model):
    """Find invalid authored vertex data before geometric measurements."""
    errors = []
    for name, rows in (("positions", model.positions), ("UVs", model.uvs), ("colors", model.colors)):
        if not finite_rows(rows):
            errors.append(f"non-finite {name}")
    return errors


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
        normal = _cross(_sub(b, a), _sub(c, a))
        length = math.sqrt(_dot(normal, normal))
        normals.append(tuple(v/length for v in normal) if length else (0, 0, 0))
        for a, b in zip(triangle, triangle[1:]+triangle[:1]):
            edges[tuple(sorted((keys[a], keys[b])))].append(face)
    quads = defaultdict(list)
    for faces in edges.values():
        if len(faces) != 2:
            continue
        a, b = faces
        points = tuple(sorted({keys[i] for face in faces for i in triangles[face]}))
        if len(points) != 4 or _dot(normals[a], normals[b]) < .999999:
            continue
        # A coplanar pair's shared edge must be an interior diagonal, not a
        # boundary between two adjacent or folded panels.
        shared_keys = set(keys[i] for i in triangles[a]) & set(keys[i] for i in triangles[b])
        if len(shared_keys) != 2:
            continue
        first, second = sorted(shared_keys)
        side = [_dot(_cross(_sub(second, first), _sub(p, first)), normals[a])
                for p in points if p not in shared_keys]
        if side[0]*side[1] >= 0:
            continue
        quads[points].append({"triangles": sorted(faces), "normal": normals[a]})
    return [{"points": points, "surfaces": surfaces,
             "opposite_facing": any(_dot(a["normal"], b["normal"]) < -.999999
                                     for a in surfaces for b in surfaces)}
            for points, surfaces in sorted(quads.items())
            if any(set(a["triangles"]).isdisjoint(b["triangles"])
                   for a in surfaces for b in surfaces)]
