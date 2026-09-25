"""Opt-in closed primitives and file-backed atlases for refreshed props.

Shared legacy mesh generators are unchanged; only callers opt into these fixes.
"""
from pathlib import Path
from itertools import product

from mesh import FACE_KEYS, FACE_SHADE, _rotate
from tex import decode_png


def load_atlas(p, name, regions):
    source = Path(__file__).resolve().parents[3] / "assets/core/props/models" / (name + ".png")
    width, height, pixels = decode_png(source.read_bytes())
    # 256x256 is the normal native prop-atlas size; 32/64/128 remain legal for
    # lighter props. The runtime decoder accepts up to 1024 and downscales to
    # the active quality level, but shipping art above the native size only
    # wastes GLB bytes because High and Medium sample prop sheets at 256.
    if (width, height) not in ((32, 32), (64, 64), (128, 128), (256, 256)) or any(
        a != 255 for a in pixels[3::4]
    ):
        raise ValueError(
            f"{source.name} must be an opaque square 32/64/128/256 px atlas "
            f"(found {width}x{height})"
        )
    tex = p.set_texture(width)
    tex.pixels[:] = pixels
    if regions:
        tex.auto(*regions)
    return tex


def orient_outward(mesh, start, center):
    """Orient a newly emitted convex component against its interior point."""
    for offset in range(start, len(mesh.indices), 3):
        a, b, c = [mesh.positions[i] for i in mesh.indices[offset:offset + 3]]
        ab = [b[i] - a[i] for i in range(3)]
        ac = [c[i] - a[i] for i in range(3)]
        normal = (ab[1] * ac[2] - ab[2] * ac[1],
                  ab[2] * ac[0] - ab[0] * ac[2],
                  ab[0] * ac[1] - ab[1] * ac[0])
        if sum(normal[i] * (a[i] - center[i]) for i in range(3)) < 0:
            mesh.indices[offset + 1], mesh.indices[offset + 2] = (
                mesh.indices[offset + 2], mesh.indices[offset + 1])


def solid_box(p, center, size, **kwargs):
    uv = kwargs.get("uv")
    if isinstance(uv, dict):
        fallback = next(value for value in uv.values() if value is not None)
        kwargs["uv"] = {key: uv.get(key) or fallback for key in FACE_KEYS}
    start = len(p.mesh.indices)
    p.box(center, size, **kwargs)
    orient_outward(p.mesh, start, center)
    # Match the atlas's horizontal direction to the width of horizontal faces.
    for offset in range(start, len(p.mesh.indices), 3):
        ids = p.mesh.indices[offset:offset + 3]
        for sign, face in ((1, "+y"), (-1, "-y")):
            if all(abs(p.mesh.positions[i][1] - center[1] - sign * size[1] * 0.5) < 1e-7 for i in ids):
                rect = kwargs["uv"][face] if isinstance(kwargs["uv"], dict) else kwargs["uv"]
                u0, v0, u1, v1 = rect
                for i in ids:
                    x, _, z = p.mesh.positions[i]
                    p.mesh.uvs[i] = (u0 + ((x - center[0]) / size[0] + 0.5) * (u1 - u0),
                                    v0 + ((z - center[2]) / size[2] + 0.5) * (v1 - v0))
    if kwargs.get("proxy", True):
        proxy_color(p, kwargs["uv"], kwargs.get("color", (255, 255, 255)))


def solid_cylinder(p, base, radius, height, **kwargs):
    kwargs["bottom"] = True
    start = len(p.mesh.indices)
    p.cylinder(base, radius, height, **kwargs)
    axis = "xyz".index(kwargs.get("axis", "y"))
    center = list(base)
    center[axis] += height * 0.5
    orient_outward(p.mesh, start, center)
    if kwargs.get("proxy", True):
        proxy_color(p, kwargs.get("uv") or kwargs["side_uv"], kwargs.get("color", (255, 255, 255)))


def proxy_color(p, uv, color):
    """Colour the most recent untextured editor proxy from its atlas region."""
    if isinstance(uv, dict):
        uv = uv.get("+z") or next(value for value in uv.values() if value is not None)
    u0, v0, u1, v1 = uv
    samples = [p.tex.pixels[(y * p.tex.width + x) * 4:(y * p.tex.width + x) * 4 + 3]
               for y in range(int(v0 * p.tex.height), int(v1 * p.tex.height))
               for x in range(int(u0 * p.tex.width), int(u1 * p.tex.width))]
    rgb = tuple(round(sum(pixel[c] for pixel in samples) / len(samples) * color[c] / 255) for c in range(3))
    p.mesh.parts[-1]["color"] = "#%02x%02x%02x" % rgb


def padded_box(p, center, size, uv, bevel=0.025, rotation=(0.0, 0.0, 0.0)):
    """Closed 44-triangle cushion with bevelled edges, replacing stacked boxes."""
    half = [dimension * 0.5 for dimension in size]
    radius = min(bevel, min(half) * 0.8)
    vertices = {}
    for signs in product((-1, 1), repeat=3):
        for axis in range(3):
            vertices[signs, axis] = tuple(signs[i] * (half[i] if i == axis else half[i] - radius)
                                          for i in range(3))

    def face(keys):
        points = [vertices[key] for key in keys]
        a, b, c = points[:3]
        ab, ac = [b[i] - a[i] for i in range(3)], [c[i] - a[i] for i in range(3)]
        normal = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2],
                  ab[0] * ac[1] - ab[1] * ac[0]]
        if sum(normal[i] * a[i] for i in range(3)) < 0:
            points.reverse()
            normal = [-value for value in normal]
        axis = max(range(3), key=lambda i: abs(normal[i]))
        u_axis, v_axis = ((2, 1), (0, 2), (0, 1))[axis]
        u0, v0, u1, v1 = uv
        uvs = [(u0 + (point[u_axis] / size[u_axis] + 0.5) * (u1 - u0),
                v1 - (point[v_axis] / size[v_axis] + 0.5) * (v1 - v0)) for point in points]
        world_normal = _rotate(tuple(normal), rotation)
        shade = sum(abs(value) * FACE_SHADE[("+" if value >= 0 else "-") + "xyz"[i]]
                    for i, value in enumerate(world_normal)) / sum(abs(value) for value in world_normal)
        world = [tuple(value + center[i] for i, value in enumerate(_rotate(point, rotation)))
                 for point in points]
        if len(world) == 4:
            p.mesh.quad(*world, uv=uvs, color=(255, 255, 255), shade_mult=shade)
        else:
            p.mesh.triangle(*world, uvs=uvs, color=(255, 255, 255), shade_mult=shade)

    # Six face panels, twelve edge strips, then eight corner triangles.
    for axis in range(3):
        others = [i for i in range(3) if i != axis]
        for sign in (-1, 1):
            keys = []
            for a, b in ((-1, -1), (-1, 1), (1, 1), (1, -1)):
                signs = [sign] * 3
                signs[others[0]], signs[others[1]] = a, b
                keys.append((tuple(signs), axis))
            face(keys)
    for a, b in ((0, 1), (0, 2), (1, 2)):
        c = 3 - a - b
        for sa, sb in product((-1, 1), repeat=2):
            signs = [0, 0, 0]
            signs[a], signs[b], signs[c] = sa, sb, -1
            low = tuple(signs)
            signs[c] = 1
            high = tuple(signs)
            face([(low, a), (high, a), (high, b), (low, b)])
    for signs in product((-1, 1), repeat=3):
        face([(signs, axis) for axis in range(3)])
    p.mesh.parts.append({"shape": "box", "center": list(center), "size": list(size),
                         "rotation": list(rotation), "color": "#ffffff"})
    proxy_color(p, uv, (255, 255, 255))
