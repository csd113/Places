"""The Home tabletop set: cutlery, a plate, a bowl and a small potted plant.

The pack contract is the same as every other part module (see
``parts/utility.py`` and ``tools/props/README.md``): the catalogue ``size`` is
authoritative, the origin is the base-contact centre with ``y = 0`` at the
contact plane, ``+Z`` is the piece's working end, one embedded texture per
model and colours from :mod:`palette`.

The six pieces are one quiet, tidy domestic family -- polished steel flatware
with dark riveted handles, pale porcelain china and one small glazed plant --
so they read as a single table setting at 480x272:

* **Flatware (+Z is the working end).**  The knife, fork and spoon lie flat
  with their undersides on the table (``y = 0``); the handle runs to ``-Z``.
  A place setting rotates each piece so ``+Z`` points at the plate.
* **Plate and bowl lathes.**  Each is one conceptual surface of revolution
  split into four bands so the well/rim/edge/underside (plate) and
  outside/rim/inside/foot (bowl) each sample their own 32 px atlas cell.
  The bowl's rim top is the catalogue height; the plate's outer edge is its
  catalogue width.
* **Small potted plant.**  Deliberately *not* the 1.0 m ``core:plant``: a
  9.5 cm glazed pot, soil, a short stem and seven closed folded leaf shells in
  two tiers, just over half the triangles of the floor plant.

Source art lives in commit beside each GLB
(``assets/environments/home/props/<name>.png``) and is embedded by
``build.py`` at build time, exactly like the refreshed domestic pack.
"""

from __future__ import annotations

import math
from pathlib import Path

import palette
from mesh import PropBuilder
from parts.refreshed import load_atlas_from, solid_cylinder
from tex import Texture

def _normalize(vector: tuple[float, float, float]) -> tuple[float, float, float]:
    length = math.sqrt(vector[0] ** 2 + vector[1] ** 2 + vector[2] ** 2)
    if length < 1e-9:
        return (0.0, 0.0, 1.0)
    return (vector[0] / length, vector[1] / length, vector[2] / length)

def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])

def _lerp(a, b, t: float):
    return tuple(x + (y - x) * t for x, y in zip(a, b))

def _winding(points) -> tuple[float, float, float]:
    """Newell normal of a polygon, used to keep a leaf's bright side up."""
    normal = [0.0, 0.0, 0.0]
    for index, point in enumerate(points):
        other = points[(index + 1) % len(points)]
        normal[0] += (point[1] - other[1]) * (point[2] + other[2])
        normal[1] += (point[2] - other[2]) * (point[0] + other[0])
        normal[2] += (point[0] - other[0]) * (point[1] + other[1])
    return (normal[0], normal[1], normal[2])

def _blade(mesh, points, uvs, reference, up, shade: float = 1.0) -> None:
    """Emit a leaf's upper skin; _leaf adds a separate lower skin and rim."""
    order = list(points)
    uv_order = list(uvs)
    if sum(a * b for a, b in zip(_winding(order), reference)) < 0.0:
        order.reverse()
        uv_order.reverse()
    if len(order) == 4:
        mesh.quad(*order, uv=uv_order, color=up, shade_mult=shade)
    else:
        mesh.triangle(*order, uvs=uv_order, color=up, shade_mult=shade)

def _leaf(mesh, base, tip, bend, width: float, uv, up, down, shade: float = 1.0) -> None:
    """One folded leaf with a thin closed shell instead of coplanar backfaces."""
    start = len(mesh.indices)
    azimuth = math.atan2(tip[2] - base[2], tip[0] - base[0])
    side = (-math.sin(azimuth), 0.0, math.cos(azimuth))
    mid = tuple((a + b) * 0.5 + c for a, b, c in zip(base, tip, bend))
    direction = _normalize(tuple(m - b for m, b in zip(mid, base)))
    normal = _normalize(_cross(direction, side))

    # Spine points: the blade bows out of the base-tip plane so it is not a
    # flat card, and the tip stays exactly where the caller asked for it.
    spine = [
        base,
        tuple(p + n * (width * 0.05) for p, n in zip(_lerp(base, mid, 0.62), normal)),
        tuple(p + n * (width * 0.12) for p, n in zip(mid, normal)),
        tip,
    ]
    # Widest just past the base and tapering to a short blunt tip: a broad
    # paddle reads as a paper dart from across the room, a lance does not.
    widths = (width * 0.40, width * 0.85, width * 0.62, width * 0.14)
    left = [(p[0] - side[0] * w, p[1], p[2] - side[2] * w) for p, w in zip(spine, widths)]
    right = [(p[0] + side[0] * w, p[1], p[2] + side[2] * w) for p, w in zip(spine, widths)]

    u0, v0, u1, v1 = uv
    rows = (v1, v0 + 0.62 * (v1 - v0), v0 + 0.26 * (v1 - v0), v0)
    left_tip_u = u0 + (u1 - u0) * 0.40
    right_tip_u = u0 + (u1 - u0) * 0.60
    _blade(mesh, [left[0], right[0], right[1], left[1]],
           [(u0, rows[0]), (u1, rows[0]), (u1, rows[1]), (u0, rows[1])],
           normal, up, shade)
    _blade(mesh, [left[1], right[1], right[2], left[2]],
           [(u0, rows[1]), (u1, rows[1]), (u1, rows[2]), (u0, rows[2])],
           normal, up, shade)
    _blade(mesh, [left[2], right[2], right[3], left[3]],
           [(u0, rows[2]), (u1, rows[2]), (right_tip_u, rows[3]), (left_tip_u, rows[3])],
           normal, up, shade)

    # Preserve the upper silhouette; offset the back by 0.8 mm and join only
    # boundary edges. Shared fold edges stay internal to each continuous skin.
    edges = {}
    top_indices = list(mesh.indices[start:])
    def underside(point):
        return tuple(point[i] - normal[i] * 0.0008 for i in range(3))
    for offset in range(0, len(top_indices), 3):
        ids = top_indices[offset:offset + 3]
        mesh.triangle(*(underside(mesh.positions[i]) for i in reversed(ids)),
                      uvs=[mesh.uvs[i] for i in reversed(ids)], color=down,
                      shade_mult=shade * 0.85)
        for a, b in zip(ids, ids[1:] + ids[:1]):
            key = tuple(sorted((mesh.positions[a], mesh.positions[b])))
            if key in edges:
                del edges[key]
            else:
                edges[key] = (a, b)
    for a, b in edges.values():
        pa, pb = mesh.positions[a], mesh.positions[b]
        mesh.quad(pb, pa, underside(pa), underside(pb),
                  uv=[mesh.uvs[b], mesh.uvs[a], mesh.uvs[a], mesh.uvs[b]],
                  color=down, shade_mult=shade * 0.9)


# Current refined geometry counts; build.py enforces the pack-wide budgets.
TARGETS = {
    "home:knife": 98,
    "home:fork": 308,
    "home:spoon": 232,
    "home:plate": 256,
    "home:bowl": 256,
    "home:plant_table": 326,
}

MODEL_DIR = Path(__file__).resolve().parents[3] / "assets/environments/home/props"

# ------------------------------------------------------------------ palette

POT_GLAZE = palette.mix(
    palette.hex_to_rgb(palette.PLASTIC_WHITE),
    palette.hex_to_rgb(palette.INSTITUTIONAL_GREEN),
    0.30,
)
SOIL = palette.mix(palette.hex_to_rgb(palette.WOOD_DARK), palette.hex_to_rgb(palette.GRIME), 0.55)
LEAF_TINT = (255, 255, 255)
LEAF_UNDER = (225, 235, 220)


def _tint(color: tuple[int, int, int], lift: float = 0.55) -> tuple[int, int, int]:
    """Vertex colour for a face whose texture is painted in ``color``.

    The shader multiplies texture and vertex colour, so the tint is lifted
    towards the pack's light neutral instead of repeating the albedo.
    """
    return palette.mix(color, palette.hex_to_rgb(palette.PLASTIC_WHITE), lift)


def _atlas(p: PropBuilder, name: str, regions: tuple) -> Texture:
    return load_atlas_from(p, MODEL_DIR / f"{name}.png", regions)


# ------------------------------------------------------------------- plant


def build_plant_table(p: PropBuilder) -> None:
    """Small potted table plant: glazed pot, soil, stem and seven leaf shells.

    Half the height of the floor-standing ``core:plant`` and with seven broad
    leaves instead of fourteen, but the same closed two-sided folded shells,
    so the crown still reads as separate blades rather than one blob.
    """
    tex = _atlas(p, "plant_table", ("pot", "soil", "leaf", "stem"))
    pot_uv = tex.uv("pot")
    soil_uv = tex.uv("soil")
    stem_uv = tex.uv("stem")
    leaf_uv = tex.uv("leaf", inset=2)

    pot_tint = _tint(POT_GLAZE, 0.42)
    soil_tint = _tint(SOIL, 0.35)
    stem_tint = _tint(palette.hex_to_rgb(palette.STEM_GREEN), 0.45)

    # Pot: a ten-sided tapered cylinder, widest at the rim (the footprint).
    solid_cylinder(p, (0.0, 0.0, 0.0), 0.050, 0.095, segments=10, taper=1.30,
                   bottom=True, side_uv=pot_uv, cap_uv=pot_uv, color=pot_tint)
    solid_cylinder(p, (0.0, 0.081, 0.0), 0.065, 0.014, segments=10,
                   side_uv=pot_uv, cap_uv=soil_uv, color=(255, 255, 255))
    # Soil: a closed cylinder mounded 3 mm proud of the pot's rim ring.
    p.cylinder((0.0, 0.090, 0.0), 0.060, 0.008, segments=10, side_uv=soil_uv,
               cap_uv=soil_uv, color=soil_tint)
    # Stem: a five-sided taper up to 0.25 m; the leaves take the rest.
    solid_cylinder(p, (0.0, 0.090, 0.0), 0.012, 0.160, segments=5, taper=0.75,
                   side_uv=stem_uv, cap_uv=stem_uv, color=stem_tint)

    # (azimuth, base height, tip radius, tip height, blade width, shade)
    # Four broad upper leaves on the axes own the 0.13 m footprint; three
    # shorter lower leaves fill the diagonals without crowding the crown.
    leaves = (
        (0.0, 0.185, 0.065, 0.262, 0.024, 1.03),
        (90.0, 0.195, 0.065, 0.280, 0.023, 0.98),
        (180.0, 0.185, 0.065, 0.262, 0.024, 1.01),
        (270.0, 0.190, 0.065, 0.270, 0.023, 0.96),
        (45.0, 0.128, 0.058, 0.205, 0.022, 1.00),
        (165.0, 0.120, 0.058, 0.198, 0.022, 0.95),
        (285.0, 0.124, 0.058, 0.202, 0.021, 1.04),
    )
    for azimuth, base_y, tip_radius, tip_y, width, shade in leaves:
        angle = math.radians(azimuth)
        direction = (math.cos(angle), 0.0, math.sin(angle))
        base = (direction[0] * 0.012, base_y, direction[2] * 0.012)
        tip = (direction[0] * tip_radius, tip_y, direction[2] * tip_radius)
        bend = (direction[0] * 0.018, 0.016, direction[2] * 0.018)
        _leaf(p.mesh, base, tip, bend, width, leaf_uv, LEAF_TINT, LEAF_UNDER, shade)
    p.add_note("seven closed folded leaf shells in two tiers; no alpha cut-outs")


# File-backed domestic remakes; other builders remain unchanged.
from parts import domestic_remade

PROPS = {
    "home:knife": domestic_remade.knife,
    "home:fork": domestic_remade.fork,
    "home:spoon": domestic_remade.spoon,
    "home:plate": domestic_remade.plate,
    "home:bowl": domestic_remade.bowl,
    "home:plant_table": build_plant_table,
}
