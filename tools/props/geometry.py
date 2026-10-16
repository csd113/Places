"""Conservative winding repair and seam-aware topology audit for static props.

Position welding is used only to discover adjacency; UV/color seam vertices are
never merged. Non-manifold edges and zero-volume sheets are not treated as solids.
"""
from collections import defaultdict, deque
from dataclasses import dataclass, replace
import math
import heapq


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def _dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def inspect(positions, indices, *, repair=False):
    """Orient edge-connected shells consistently, then outward by signed volume.

    Open curved shells use their centred signed volume; planar sheets keep their
    majority orientation. Contradictory/non-orientable components are left alone.
    The caller owns intentional openings: this function never fills holes.
    """
    keys = [tuple(round(v, 6) for v in p) for p in positions]
    triangles = [indices[i:i+3] for i in range(0, len(indices), 3)]
    edges = defaultdict(list)
    degenerate = 0
    for face, tri in enumerate(triangles):
        a, b, c = [positions[i] for i in tri]
        normal = _cross(_sub(b, a), _sub(c, a))
        degenerate += _dot(normal, normal) < 1e-20
        for i in range(3):
            a, b = keys[tri[i]], keys[tri[(i+1)%3]]
            edges[tuple(sorted((a, b)))].append((face, a < b))
    graph = defaultdict(list)
    conflicts = 0
    for uses in edges.values():
        if len(uses) == 2:
            (a, da), (b, db) = uses
            graph[a].append((b, da == db))
            graph[b].append((a, da == db))
            conflicts += da == db
    visited = set()
    flipped = []
    shells = []
    for start in range(len(triangles)):
        if start in visited:
            continue
        signs = {start: False}
        queue = deque([start])
        contradiction = False
        while queue:
            a = queue.popleft()
            for b, toggle in graph[a]:
                value = signs[a] ^ toggle
                if b in signs:
                    contradiction |= signs[b] != value
                else:
                    signs[b] = value
                    queue.append(b)
        visited.update(signs)
        points = {tuple(positions[v]) for face in signs for v in triangles[face]}
        center = tuple(sum(p[i] for p in points)/len(points) for i in range(3))
        volume = 0.0
        for face, flip in signs.items():
            a, b, c = [_sub(positions[v], center) for v in triangles[face]]
            volume += _dot(a, _cross(b, c)) * (-1 if flip else 1) / 6
        extent = max(max(p[i] for p in points)-min(p[i] for p in points) for i in range(3))
        nonplanar = abs(volume) > max(1e-14, extent**3 * 1e-10)
        invert = volume < 0 if nonplanar else sum(signs.values()) > len(signs)/2
        selected = [] if contradiction else [face for face, value in signs.items() if value ^ invert]
        flipped.extend(selected)
        shells.append({'triangles':len(signs), 'volume':volume, 'flips':len(selected), 'contradiction':contradiction})
    if repair:
        for face in flipped:
            offset = face*3
            indices[offset+1], indices[offset+2] = indices[offset+2], indices[offset+1]
    return {'triangles':len(triangles), 'degenerate':degenerate,
            'boundary_edges':sum(len(v)==1 for v in edges.values()),
            'nonmanifold_edges':sum(len(v)>2 for v in edges.values()),
            'inconsistent_edges':conflicts, 'flipped_triangles':len(flipped),
            'components':len(shells), 'contradictory_components':sum(s['contradiction'] for s in shells)}


def boundary_loops(positions, indices):
    """Return simple boundary cycles; ambiguous junctions are deliberately skipped."""
    keys = [tuple(round(v, 6) for v in p) for p in positions]
    edges = defaultdict(list)
    for offset in range(0, len(indices), 3):
        tri = indices[offset:offset+3]
        for a, b in zip(tri, tri[1:]+tri[:1]):
            edges[tuple(sorted((keys[a], keys[b])))].append((a, b, offset))
    boundary = [uses[0] for uses in edges.values() if len(uses)==1]
    neighbors = defaultdict(list)
    for i, (a, b, _) in enumerate(boundary):
        neighbors[keys[a]].append(i); neighbors[keys[b]].append(i)
    used = set(); loops = []
    for start in range(len(boundary)):
        if start in used:
            continue
        a, b, _ = boundary[start]
        loop = [a]; current = start; point = keys[a]; cycle = set()
        while current not in cycle:
            cycle.add(current); used.add(current)
            a, b, _ = boundary[current]
            nxt = b if keys[a]==point else a
            point = keys[nxt]
            if point == keys[loop[0]]:
                loops.append((loop, [boundary[e][2] for e in cycle])); break
            loop.append(nxt)
            if len(neighbors[point]) != 2:
                break
            current = next(e for e in neighbors[point] if e != current)
    return loops


@dataclass(frozen=True)
class SurfaceTriangle:
    """An offline triangle with an address in its source, never a draw index.

    ``records`` retain every original vertex attribute, not merely position.
    Only exact, equally oriented, opaque triangles with identical records in
    the same primitive can be proposed for deletion. Other layers are review.
    """
    reference: str
    points: tuple
    material: str = ""
    alpha_mode: str = "OPAQUE"
    records: tuple = ()
    owner: str = ""
    instance: str = ""


def glb_triangles(raw, *, source="model"):
    """Read the active GLB scene using the toolkit's node/accessor transforms.

    Bind-pose animated and skinned assets are refused: deleting a coincident
    rest-pose face could remove a surface that separates during animation.
    """
    import struct
    import glb

    checked = glb.read_glb(raw)
    document = checked.json
    total = struct.unpack_from("<I", raw, 8)[0]
    if total != len(raw):
        raise ValueError(f"{source}: bytes outside the declared GLB container")
    if (document.get("skins") or document.get("animations")
            or any("weights" in node for node in document.get("nodes", []))
            or any("weights" in mesh or any("targets" in primitive for primitive in mesh.get("primitives", []))
                   for mesh in document.get("meshes", []))):
        raise ValueError(f"{source}: animated/skinned/morph geometry requires pose review")
    blob = b""
    offset = 12
    while offset + 8 <= total:
        length, kind = struct.unpack_from("<II", raw, offset)
        if offset + 8 + length > total:
            raise ValueError(f"{source}: chunk exceeds the declared GLB container")
        if kind == glb.CHUNK_BIN:
            blob = raw[offset + 8:offset + 8 + length]
        offset += 8 + length
    if offset != total:
        raise ValueError(f"{source}: incomplete chunk in the declared GLB container")
    nodes = document.get("nodes", [])
    placements = []
    if nodes:
        world, roots = glb._node_rest_matrices(document)
        visited = set()
        def walk(index):
            if index in visited:
                return
            visited.add(index)
            node = nodes[index]
            if "mesh" in node:
                placements.append((index, node["mesh"], world[index]))
            for child in node.get("children", []):
                walk(child)
        for root in roots:
            walk(root)
    else:
        placements = [(None, i, glb._IDENTITY_MATRIX)
                      for i in range(len(document["meshes"]))]
    result = []
    for node, mesh, matrix in placements:
        for slot, primitive in enumerate(document["meshes"][mesh]["primitives"]):
            attributes = {name: glb._read_accessor(document, blob, accessor)
                          for name, accessor in primitive["attributes"].items()}
            positions = attributes["POSITION"]
            indices = [int(row[0]) for row in glb._read_accessor(document, blob, primitive["indices"])] if "indices" in primitive else list(range(len(positions)))
            material_index = primitive.get("material")
            material = document.get("materials", [])[material_index] if material_index is not None else {}
            material_id = f"{source}/materials/{material_index}"
            owner = f"{source}/nodes/{node}/meshes/{mesh}/primitives/{slot}"
            for start in range(0, len(indices), 3):
                corners = indices[start:start + 3]
                if len(corners) != 3:
                    raise ValueError(f"{owner}: incomplete triangle")
                points = tuple(glb._transform_point(matrix, positions[i]) for i in corners)
                records = tuple(tuple((name, tuple(values[i])) for name, values in sorted(attributes.items())) for i in corners)
                result.append(SurfaceTriangle(f"{owner}/triangles/{start // 3}", points,
                                              material_id, material.get("alphaMode", "OPAQUE"), records, owner))
    return result


def transformed_triangles(triangles, matrix, *, owner):
    """Apply an explicit column-major world matrix; keep source face addresses.

    The caller supplies the engine-resolved placement, including floor height
    and span deformation. This deliberately does not guess those from JSON.
    """
    import glb
    if len(matrix) != 16 or not all(math.isfinite(v) for v in matrix):
        raise ValueError("a finite 4x4 placement matrix is required")
    return [replace(triangle, points=tuple(glb._transform_point(matrix, p) for p in triangle.points),
                    reference=f"{owner}/{triangle.reference}", owner=f"{owner}/{triangle.owner}", instance=owner)
            for triangle in triangles]


def _projected_overlap(first, second, axis):
    """Triangle intersection area in a local frame, including oblique planes."""
    axes = [i for i in range(3) if i != axis]
    origin = first[0]
    def project(points):
        return [tuple(p[a] - origin[a] for a in axes) for p in points]
    def cross(a, b):
        return a[0] * b[1] - a[1] * b[0]
    def area(points):
        return sum(cross(a, b) for a, b in zip(points, points[1:] + points[:1])) / 2
    subject, clip = project(first), project(second)
    if area(subject) < 0:
        subject.reverse()
    if area(clip) < 0:
        clip.reverse()
    for start, end in zip(clip, clip[1:] + clip[:1]):
        edge = _sub(end, start)
        output = []
        for a, b in zip(subject, subject[1:] + subject[:1]):
            da, db = cross(edge, _sub(a, start)), cross(edge, _sub(b, start))
            if da >= 0:
                output.append(a)
            if (da >= 0) != (db >= 0):
                t = da / (da - db)
                output.append(tuple(x + t * (y - x) for x, y in zip(a, b)))
        subject = output
        if len(subject) < 3:
            return 0.0
    return abs(area(subject))


def _cyclic_record(records):
    return min(records[i:] + records[:i] for i in range(3)) if records else ()


def clip_attribute_triangle(records, *, axis, minimum):
    """Clip a triangle to one authored plane, interpolating all float attributes.

    Return an oriented triangle fan. Existing retained corner records are
    returned unchanged; intersections preserve the original affine UV mapping.
    This is geometry clipping, with no movement or depth bias.
    """
    if axis not in (0, 1, 2) or not math.isfinite(minimum) or len(records) != 3:
        raise ValueError("a triangle and finite axis plane are required")
    result = []
    for first, second in zip(records, records[1:] + records[:1]):
        a, b = dict(first), dict(second)
        da, db = a["POSITION"][axis] - minimum, b["POSITION"][axis] - minimum
        if da >= 0:
            result.append(first)
        if (da >= 0) != (db >= 0):
            t = da / (da - db)
            vertex = {name: tuple(x + t * (y - x) for x, y in zip(a[name], b[name])) for name in a}
            # Use the exact declared plane, avoiding a second rounded seam.
            position = list(vertex["POSITION"])
            position[axis] = minimum
            vertex["POSITION"] = tuple(position)
            result.append(tuple(sorted(vertex.items())))
    return [tuple((result[0], result[i], result[i + 1])) for i in range(1, len(result) - 1)]


def overlap_report(triangles, *, near_m=0.003, min_area_m2=0.0001,
                   max_candidates=2_000_000, max_findings=2000, cross_instances=False):
    """Deterministic bounded offline coplanar/near-coplanar triangle audit.

    A conservative spatial interval sweep rejects disjoint faces before polygon
    clipping without quantizing plane normals or distances. Budget exhaustion is an
    explicit incomplete result; it must never be interpreted as a clean audit.
    No render/depth settings are involved and this function performs no writes.
    """
    if not math.isfinite(near_m) or near_m <= 0 or not math.isfinite(min_area_m2) or min_area_m2 <= 0:
        raise ValueError("positive finite distance and area thresholds required")
    if max_candidates < 1 or max_findings < 1:
        raise ValueError("positive candidate and finding limits required")
    # Projection drops the dominant normal axis. Its coordinate separation
    # may be distance / |normal[axis]|, bounded by sqrt(3) times plane distance.
    bounds_margin = math.sqrt(3) * near_m
    triangles = list(triangles)
    prepared = []
    for triangle in triangles:
        if len(triangle.points) != 3 or not all(math.isfinite(v) for p in triangle.points for v in p):
            raise ValueError(f"{triangle.reference}: nonfinite/incomplete triangle")
        normal = _cross(_sub(triangle.points[1], triangle.points[0]), _sub(triangle.points[2], triangle.points[0]))
        length = math.sqrt(_dot(normal, normal))
        if length < 1e-12:
            continue
        normal = tuple(v / length for v in normal)
        axis = max(range(3), key=lambda i: abs(normal[i]))
        bounds = tuple((min(p[i] for p in triangle.points), max(p[i] for p in triangle.points)) for i in range(3))
        prepared.append((triangle, normal, bounds, axis))
    # Every pair whose X spans overlap is visited exactly once. Unlike plane
    # bins, this remains conservative at dominant-axis ties and when unrelated
    # distant geometry changes a scene's extent or numerical origin.
    prepared.sort(key=lambda row: (row[2][0][0], row[0].reference))
    active = {}
    expiry = []
    findings, candidates = [], 0
    totals = defaultdict(int)
    removable = set()
    for ordinal, current in enumerate(prepared):
        triangle, normal, bounds, axis = current
        while expiry and expiry[0][0] < bounds[0][0] - bounds_margin:
            _, old_ordinal, instance = heapq.heappop(expiry)
            del active[instance][old_ordinal]
            if not active[instance]:
                del active[instance]
        previous_rows = (row for instance, rows in active.items()
                         if not cross_instances or instance != triangle.instance for row in rows.values())
        for previous in previous_rows:
            other, other_normal, other_bounds, _ = previous
            candidates += 1
            if candidates > max_candidates:
                return {"complete": False, "reason": "candidate budget exhausted", "candidates": candidates - 1,
                        "totals": dict(sorted(totals.items())), "findings": findings, "repair_plan": []}
            if any(bounds[i][0] > other_bounds[i][1] + bounds_margin or other_bounds[i][0] > bounds[i][1] + bounds_margin for i in range(3)):
                continue
            if abs(_dot(normal, other_normal)) < 1 - 1e-9:
                continue
            separation = max(abs(_dot(normal, _sub(p, triangle.points[0]))) for p in other.points)
            if separation > near_m:
                continue
            # Use one pair frame, with a stable first-axis tie break. Near
            # parallel faces straddling a dominant-axis tie must not select
            # different projections merely because visitation order changed.
            alignment = 1 if _dot(normal, other_normal) >= 0 else -1
            shared = tuple(a + alignment * b for a, b in zip(normal, other_normal))
            largest = max(abs(value) for value in shared)
            projection_axis = next(i for i, value in enumerate(shared) if abs(value) >= largest - 1e-12)
            area = _projected_overlap(triangle.points, other.points, projection_axis) / abs(normal[projection_axis])
            if area <= min_area_m2:
                continue
            facing = _dot(normal, other_normal)
            kind = "near-coplanar" if separation > 1e-6 else "coplanar-overlap"
            if facing < 0:
                kind = "opposite-facing"
            same_records = bool(triangle.records and other.records and _cyclic_record(triangle.records) == _cyclic_record(other.records))
            same_points = _cyclic_record(triangle.points) == _cyclic_record(other.points)
            safe = (same_points and same_records and facing > 0 and triangle.owner == other.owner
                    and triangle.material == other.material and triangle.alpha_mode == other.alpha_mode == "OPAQUE")
            if safe:
                kind = "duplicate-triangle"
                # Keep the lexicographically first stable address. A third
                # copy must never cause the keeper itself to be removed.
                keep, remove = sorted((triangle.reference, other.reference))
                removable.add(remove)
            totals[kind] += 1
            if len(findings) < max_findings:
                first, second = sorted((triangle.reference, other.reference))
                findings.append({"kind": kind, "first": first, "second": second,
                                 "materials": [triangle.material, other.material],
                                 "alpha_modes": [triangle.alpha_mode, other.alpha_mode],
                                 "separation_m": separation, "area_m2": area,
                                 "position": [sum(p[i] for p in triangle.points) / 3 for i in range(3)],
                                 "auto_repairable": safe})
        active.setdefault(triangle.instance, {})[ordinal] = current
        heapq.heappush(expiry, (bounds[0][1], ordinal, triangle.instance))
    return {"complete": True, "candidates": candidates, "totals": dict(sorted(totals.items())),
            "findings": sorted(findings, key=lambda row: (row["first"], row["second"])),
            "truncated_findings": sum(totals.values()) > len(findings),
            "repair_plan": [{"action": "remove-triangle", "reference": reference} for reference in sorted(removable)]}
