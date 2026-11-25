"""Check the shipped human sheet ghost's facial clearance and skin attachment.

Read the committed GLB, not the retired constructor. The independently fitted
cat has its own accepted geometry and is not subject to this face-color rule.
"""
import math
from pathlib import Path
import unittest

from assets.inspection import glb

GHOST = (Path(__file__).resolve().parents[1]
         / "assets/entities/sheet-ghost/model/sheet-ghost.glb")
CLEARANCE = .006


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



def facial_vertices(colours):
    return {index for index, rgba in enumerate(colours) if sum(rgba[:3]) / 3 < .35}


def validate_face(document, binary, *, positions=None):
    """The painted features and their front cloth must share rigid support."""
    primitives = document["meshes"][0]["primitives"]
    attributes = primitives[0]["attributes"]

    def read(attribute):
        return glb._read_accessor(document, binary, attributes[attribute])

    points = read("POSITION") if positions is None else positions
    joints = read("JOINTS_0")
    weights = read("WEIGHTS_0")
    face = facial_vertices(read("COLOR_0"))
    triangles = []
    for primitive in primitives:
        indices = [int(row[0]) for row in glb._read_accessor(document, binary, primitive["indices"])]
        if len(indices) % 3:
            raise ValueError("sheet ghost primitive does not contain whole triangles")
        triangles.extend(indices[start:start + 3] for start in range(0, len(indices), 3))
    cloth = [tri for tri in triangles if not any(i in face for i in tri)]

    def rigid(index):
        return joints[index][0] == 1 and weights[index] == (1., 0., 0., 0.)

    for index in face:
        if not rigid(index):
            raise ValueError("facial feature no longer follows the rigid body joint")
        x, y, z = points[index]
        surface, support = front_surface(points, cloth, x, y, return_support=True)
        if z - surface < CLEARANCE - 1e-6:
            raise ValueError("facial vertex is recessed or touches its sheet")
        if not all(rigid(joint) for joint in support):
            raise ValueError("facial cloth rim no longer follows the rigid body joint")

    facial_triangles = [tri for tri in triangles if all(i in face for i in tri)]
    for tri, support, overlap in support_pairs(points, facial_triangles, cloth):
        a, b = [points[i] for i in tri], [points[i] for i in support]
        gap = min(triangle_height(a, x, y) - triangle_height(b, x, y) for x, y in overlap)
        if gap < .05 and not all(rigid(index) for index in support):
            raise ValueError("supporting cloth no longer follows the rigid body joint")
        if gap < CLEARANCE - 1e-6:
            raise ValueError("facial triangle intersects its sheet")
    return len(face)


class GhostAttachmentTests(unittest.TestCase):
    def test_overlap_detects_a_cloth_ridge_between_feature_vertices(self):
        face = [(0, 0, .1), (1, 0, .1), (0, 1, .1)]
        cloth = [(.2, .2, .2), (.3, .2, .2), (.2, .3, .2)]
        overlap = projected_overlap(face, cloth)
        self.assertEqual(len(overlap), 3)
        self.assertTrue(all(triangle_height(face, x, y) < triangle_height(cloth, x, y)
                            for x, y in overlap))

    def test_human_face_has_surface_clearance_and_unchanged_rigid_attachment(self):
        document, binary = glb.read_glb_chunks(GHOST.read_bytes())
        self.assertEqual(validate_face(document, binary), 648)
        attributes = document["meshes"][0]["primitives"][0]["attributes"]
        normals = glb._read_accessor(document, binary, attributes["NORMAL"])
        for normal in normals:
            self.assertTrue(all(math.isfinite(v) for v in normal))
            self.assertAlmostEqual(sum(v * v for v in normal), 1., delta=1e-5)

    def test_a_recessed_face_is_rejected_even_with_its_attachment_marker_unchanged(self):
        document, binary = glb.read_glb_chunks(GHOST.read_bytes())
        attributes = document["meshes"][0]["primitives"][0]["attributes"]
        points = glb._read_accessor(document, binary, attributes["POSITION"])
        face = facial_vertices(glb._read_accessor(document, binary, attributes["COLOR_0"]))
        recessed = [(x, y, z - .5) if index in face else (x, y, z)
                    for index, (x, y, z) in enumerate(points)]
        with self.assertRaisesRegex(ValueError, "recessed|touches"):
            validate_face(document, binary, positions=recessed)


if __name__ == "__main__":
    unittest.main()
