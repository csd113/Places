"""Project the existing ghosts' recessed facial meshes onto their cloth.

The painted features follow their original rigid body joint. The human's
supporting face cloth follows that joint too; the cat retains its original
body/hem blend. Validate animated clearance in addition to the bind pose.
"""

import math
import struct


def projected_overlap(a, b):
    """Vertices of two triangles' convex intersection in the XY plane."""
    polygon = [tuple(p[:2]) for p in a]
    cross = lambda p, q, r: (q[0]-p[0])*(r[1]-p[1])-(q[1]-p[1])*(r[0]-p[0])
    winding = 1 if cross(*b) > 0 else -1
    if abs(cross(*b)) < 1e-12 or abs(cross(*a)) < 1e-12:
        return []
    for start, end in zip(b, b[1:] + b[:1]):
        clipped = []
        for previous, current in zip(polygon[-1:] + polygon[:-1], polygon):
            dp = winding * cross(start, end, previous)
            dc = winding * cross(start, end, current)
            if (dp >= 0) != (dc >= 0):
                t = dp / (dp-dc)
                clipped.append(tuple(previous[k]+t*(current[k]-previous[k]) for k in range(2)))
            if dc >= 0:
                clipped.append(current)
        polygon = clipped
        if not polygon:
            break
    return polygon


def triangle_height(triangle, x, y):
    a, b, c = triangle
    determinant = (b[1]-c[1])*(a[0]-c[0])+(c[0]-b[0])*(a[1]-c[1])
    u = ((b[1]-c[1])*(x-c[0])+(c[0]-b[0])*(y-c[1]))/determinant
    v = ((c[1]-a[1])*(x-c[0])+(a[0]-c[0])*(y-c[1]))/determinant
    return u*a[2]+v*b[2]+(1-u-v)*c[2]


def support_pairs(points, faces, cloth):
    """Exact overlap vertices: a linear height difference has extrema here."""
    supports = []
    for support in cloth:
        b = [points[i] for i in support]
        if sum(p[2] for p in b) <= 0:
            continue
        bounds = tuple((min(p[k] for p in b), max(p[k] for p in b)) for k in (0, 1))
        supports.append((support, b, bounds))
    for face in faces:
        a = [points[i] for i in face]
        bounds_a = tuple((min(p[k] for p in a), max(p[k] for p in a)) for k in (0, 1))
        for support, b, bounds_b in supports:
            if any(bounds_a[k][1] < bounds_b[k][0]-1e-9 or
                   bounds_b[k][1] < bounds_a[k][0]-1e-9 for k in (0, 1)):
                continue
            overlap = projected_overlap(a, b)
            if overlap:
                yield face, support, overlap


def front_surface(points, triangles, x, y, return_support=False):
    """Front-facing cloth support, bridging the original facial apertures.

    Inside a facial aperture, the nearest front triangle's edge continues
    the cloth rather than selecting the back-facing surface behind the hole.
    """
    hits = []
    nearest = None
    for ia, ib, ic in triangles:
        a, b, c = points[ia], points[ib], points[ic]
        # These centred +Z-front models also contain inward-wound back
        # triangles; a winding test alone cannot distinguish that backing.
        if a[2] + b[2] + c[2] <= 0.:
            continue
        determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
        if abs(determinant) <= 1e-12:
            continue
        u = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / determinant
        v = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / determinant
        height = u * a[2] + v * b[2] + (1 - u - v) * c[2]
        if min(u, v, 1 - u - v) >= -1e-7:
            hits.append((height, (ia, ib, ic)))
        else:
            for start_index, end_index in ((ia, ib), (ib, ic), (ic, ia)):
                start, end = points[start_index], points[end_index]
                dx, dy = end[0]-start[0], end[1]-start[1]
                length = dx*dx + dy*dy
                t = max(0., min(1., ((x-start[0])*dx+(y-start[1])*dy)/length)) if length else 0.
                distance = (x-start[0]-t*dx)**2 + (y-start[1]-t*dy)**2
                if nearest is None or distance < nearest[0] - 1e-12:
                    nearest = (distance, start[2] + t*(end[2]-start[2]), (start_index, end_index))
    if not hits:
        if nearest is None or nearest[0] > .25**2:
            raise ValueError(f"facial vertex has no nearby front cloth support at {x}, {y}")
        return (nearest[1], nearest[2]) if return_support else nearest[1]
    height, support = max(hits, key=lambda hit: hit[0])
    return (height, support) if return_support else height


def write_values(document, binary, index, values):
    entry = document["accessors"][index]
    if entry["componentType"] != 5126:
        raise ValueError("ghost geometric attributes must be float32")
    view = document["bufferViews"][entry["bufferView"]]
    offset = view.get("byteOffset", 0) + entry.get("byteOffset", 0)
    components = len(values[0])
    stride = view.get("byteStride", components * 4)
    for i, value in enumerate(values):
        struct.pack_into("<" + "f" * components, binary, offset + i * stride, *value)
    if "min" in entry:
        entry["min"] = [min(value[i] for value in values) for i in range(components)]
    if "max" in entry:
        entry["max"] = [max(value[i] for value in values) for i in range(components)]


def attach(document, binary, name, accessor, indices_of, vertex_colours):
    marker = "places_attached_face"
    extras = document["asset"].setdefault("extras", {})
    if extras.get(marker) == 2:
        return document, binary
    if extras.get(marker):
        raise ValueError("restore the original ghost GLB before updating its facial attachment")
    primitives = document["meshes"][0]["primitives"]
    attributes = primitives[0]["attributes"]
    points, _, _ = accessor(document, binary, attributes["POSITION"])
    colours = vertex_colours(document, binary, primitives[0])
    face = {i for i, c in enumerate(colours) if sum(c[:3]) / 3 < .35}
    indices = [i for primitive in primitives for i in indices_of(document, binary, primitive)]
    triangles = [indices[i:i + 3] for i in range(0, len(indices), 3)]
    cloth = [tri for tri in triangles if not any(i in face for i in tri)]
    # Keep the original raised feature relief above the newly conformed back.
    back = min(points[i][2] for i in face)
    clearance = .006 if name == "sheet-ghost" else .003
    changed = list(points)
    front = max(front_surface(points, cloth, points[i][0], points[i][1]) for i in face)
    for i in sorted(face):
        x, y, z = points[i]
        # The cat has a planar front with intentional gaps near its eyes.
        # Rays through those gaps see its back; preserve the rigid feature
        # mesh on the front plane instead of folding it through those gaps.
        surface = front if name == "sheet-ghost-cat" else front_surface(points, cloth, x, y)
        changed[i] = (x, y, surface + clearance + z - back)
    # The source is triangle soup. Weld duplicate feature corners logically,
    # then lift only intersecting triangles (including their shared corners).
    # Vertex projection alone misses cloth ridges inside a facial triangle.
    groups = {}
    for i in sorted(face):
        groups.setdefault(tuple(round(v, 7) for v in points[i]), []).append(i)
    group_for = {i: group for group in groups.values() for i in group}
    facial_triangles = [tri for tri in triangles if all(i in face for i in tri)]
    pairs = list(support_pairs(points, facial_triangles, cloth))
    for tri, support, overlap in pairs:
        a, b = [changed[i] for i in tri], [points[i] for i in support]
        deficit = max(clearance-triangle_height(a, x, y)+triangle_height(b, x, y)
                      for x, y in overlap)
        if deficit > 0:
            for i in {j for corner in tri for j in group_for[corner]}:
                x, y, z = changed[i]
                changed[i] = (x, y, z+deficit+2e-6)
    normals, _, _ = accessor(document, binary, attributes["NORMAL"])
    normals = list(normals)
    accumulated = {i: [0., 0., 0.] for i in face}
    for tri in triangles:
        if not all(i in face for i in tri):
            continue
        a, b, c = (changed[i] for i in tri)
        ab = [b[j] - a[j] for j in range(3)]
        ac = [c[j] - a[j] for j in range(3)]
        normal = (ab[1]*ac[2]-ab[2]*ac[1], ab[2]*ac[0]-ab[0]*ac[2], ab[0]*ac[1]-ab[1]*ac[0])
        for i in tri:
            for axis in range(3):
                accumulated[i][axis] += normal[axis]
    for i, normal in accumulated.items():
        length = math.sqrt(sum(v*v for v in normal))
        if length > 1e-12:
            normals[i] = tuple(v / length for v in normal)
    buffer = bytearray(binary)
    write_values(document, buffer, attributes["POSITION"], changed)
    write_values(document, buffer, attributes["NORMAL"], normals)
    extras[marker] = 2
    return document, bytes(buffer)


def validate(document, binary, name, accessor, indices_of, vertex_colours):
    primitives = document["meshes"][0]["primitives"]
    attributes = primitives[0]["attributes"]
    points, _, _ = accessor(document, binary, attributes["POSITION"])
    colours = vertex_colours(document, binary, primitives[0])
    joints, _, _ = accessor(document, binary, attributes["JOINTS_0"])
    weights, _, _ = accessor(document, binary, attributes["WEIGHTS_0"])
    face = {i for i, c in enumerate(colours) if sum(c[:3]) / 3 < .35}
    indices = [i for primitive in primitives for i in indices_of(document, binary, primitive)]
    triangles = [indices[i:i + 3] for i in range(0, len(indices), 3)]
    cloth = [tri for tri in triangles if not any(i in face for i in tri)]
    clearance = .006 if name == "sheet-ghost" else .003
    for i in face:
        if joints[i][0] != 1 or weights[i] != (1., 0., 0., 0.):
            raise ValueError("facial feature no longer follows the rigid body joint")
        x, y, z = points[i]
        surface, support = front_surface(points, cloth, x, y, return_support=True)
        if z - surface < clearance - 1e-6:
            raise ValueError("facial vertex is recessed or touches its sheet")
        if name == "sheet-ghost" and any(joints[j][0] != 1 or weights[j] != (1., 0., 0., 0.) for j in support):
            raise ValueError("facial cloth rim no longer follows the rigid body joint")
    facial_triangles = [tri for tri in triangles if all(i in face for i in tri)]
    for tri, support, overlap in support_pairs(points, facial_triangles, cloth):
        a, b = [points[i] for i in tri], [points[i] for i in support]
        gap = min(triangle_height(a, x, y)-triangle_height(b, x, y) for x, y in overlap)
        if name == "sheet-ghost" and gap < .05 and any(joints[i][0] != 1 or weights[i] != (1., 0., 0., 0.) for i in support):
            raise ValueError("supporting cloth no longer follows the rigid body joint")
        for x, y in overlap:
            if triangle_height(a, x, y)-triangle_height(b, x, y) < clearance-1e-6:
                raise ValueError("facial triangle intersects its sheet")
    return len(face)
