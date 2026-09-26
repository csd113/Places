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
(``assets/environment/home/props/models/<name>.png``) and is embedded by
``build.py`` at build time, exactly like the refreshed domestic pack.
"""

from __future__ import annotations

import math
from pathlib import Path

import palette
from mesh import FACE_KEYS, PropBuilder
from parts.decor import _leaf
from parts.refreshed import load_atlas_from, solid_cylinder, padded_box, orient_outward
from tex import Texture

# Current refined geometry counts; build.py enforces the pack-wide budgets.
TARGETS = {
    "home:knife": 98,
    "home:fork": 308,
    "home:spoon": 232,
    "home:plate": 256,
    "home:bowl": 256,
    "home:plant_table": 326,
}

MODEL_DIR = Path(__file__).resolve().parents[3] / "assets/environment/home/props/models"

# ------------------------------------------------------------------ palette

STEEL = palette.hex_to_rgb("#cfcfcc")
STEEL_DARK = palette.hex_to_rgb("#8f908d")
NICKEL = palette.hex_to_rgb("#c9cbca")
PORCELAIN = palette.hex_to_rgb("#e8e6df")
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


def _box(p: PropBuilder, center, size, uv, color, hidden=("-y",), colors=None) -> None:
    """Bevel exposed closed parts; omit only explicitly hidden assembly faces."""
    faces = dict(uv) if isinstance(uv, dict) else {key: uv for key in FACE_KEYS}
    for key in hidden:
        faces[key] = None
    if hidden == () and not isinstance(uv, dict):
        padded_box(p, center, size, uv, bevel=min(size) * 0.20)
    else:
        start = len(p.mesh.indices)
        p.box(center, size, uv=faces, color=color, colors=colors)
        orient_outward(p.mesh, start, center)


# ------------------------------------------------------------------- knife


def build_knife(p: PropBuilder) -> None:
    """Table knife: a riveted walnut handle, a bolster and a flat steel blade.

    The blade is a ten-triangle tapered prism: its base end is buried in the
    bolster, so only the two faces, the two edges and the blunt tip are
    emitted.  The working (blade) end is ``+Z``.
    """
    tex = _atlas(p, "knife", ("blade", "handle", "bolster", "spare"))
    handle_uv = tex.uv("handle")
    bolster_uv = tex.uv("bolster")
    blade_uv = tex.uv("blade")

    steel = _tint(STEEL, 0.50)
    nickel = _tint(NICKEL, 0.45)
    walnut = _tint(palette.hex_to_rgb("#3f352b"), 0.45)

    # Handle: 0.105 m of dark walnut, its full 16 mm height owning the
    # catalogue height (the blade is a thin 2.5 mm slab on the table).  The
    # tail reaches -0.1075 m so the 0.215 m bounds centre on the origin.
    _box(p, (0.0, 0.008, -0.055), (0.013, 0.016, 0.105), handle_uv, walnut, hidden=())
    # Bolster: 14 mm of turned nickel between handle and blade.
    _box(p, (0.0, 0.0065, 0.0045), (0.020, 0.013, 0.014), bolster_uv, nickel, hidden=())

    # Blade: plan trapezoid from the bolster to a blunt tip at +Z.  The base
    # sits inside the bolster so no cap is needed at that end.
    y_bottom, y_top = 0.0, 0.0025
    base_z, tip_z = 0.009, 0.1075
    base_half, tip_half = 0.011, 0.005
    corners = (
        (-base_half, base_z),
        (base_half, base_z),
        (tip_half, tip_z),
        (-tip_half, tip_z),
    )
    top = [(x, y_top, z) for x, z in corners]
    bottom = [(x, y_bottom, z) for x, z in corners]
    u0, v0, u1, v1 = blade_uv
    top_uv = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]
    p.mesh.quad(*reversed(top), uv=list(reversed(top_uv)), color=steel, shade_mult=1.0, ao=1.0)
    p.mesh.quad(*bottom, uv=top_uv, color=steel, shade_mult=0.62, ao=1.0)
    for a, b in ((0, 1), (1, 2), (2, 3), (3, 0)):
        if a == 0 and b == 1:
            continue  # the base edge is buried in the bolster
        p.mesh.quad(bottom[a], bottom[b], top[b], top[a], uv=[top_uv[a], top_uv[b], top_uv[b], top_uv[a]],
                    color=steel, shade_mult=0.86, ao=1.0)
    p.add_note("flat blade prism; riveted walnut handle; bolster hides the blade root")


# -------------------------------------------------------------------- fork


def build_fork(p: PropBuilder) -> None:
    """Four-tine table fork: riveted handle, neck, head plate and four tines.

    Low bevels soften the handles and tines without subdivisions. The tine
    gaps and narrow neck preserve the fork silhouette at gameplay distance.
    """
    tex = _atlas(p, "fork", ("head", "tine", "handle", "neck"))
    handle_uv = tex.uv("handle")
    neck_uv = tex.uv("neck")
    head_uv = tex.uv("head")
    tine_uv = tex.uv("tine")

    steel = _tint(STEEL, 0.50)
    nickel = _tint(NICKEL, 0.45)
    walnut = _tint(palette.hex_to_rgb("#3f352b"), 0.45)

    _box(p, (0.0, 0.006, -0.0455), (0.013, 0.012, 0.105), handle_uv, walnut, hidden=())
    _box(p, (0.0, 0.005, 0.021), (0.015, 0.010, 0.028), neck_uv, nickel, hidden=())
    _box(p, (0.0, 0.003, 0.0505), (0.026, 0.006, 0.031), head_uv, steel, hidden=())
    for x in (-0.00975, -0.00325, 0.00325, 0.00975):
        _box(p, (x, 0.002, 0.082), (0.005, 0.004, 0.032), tine_uv, steel, hidden=())
    p.add_note("four separate tines on a head plate; the tine gaps carry the read")


# ------------------------------------------------------------------- spoon


def build_spoon(p: PropBuilder) -> None:
    """Table spoon: riveted handle, neck and a shallow twelve-sided oval bowl.

    The bowl follows the underside, thin rim and concave interior as a closed
    shell, leaving the mouth open.
    """
    tex = _atlas(p, "spoon", ("bowl", "handle", "neck", "spare"))
    handle_uv = tex.uv("handle")
    neck_uv = tex.uv("neck")
    bowl_uv = tex.uv("bowl")

    steel = _tint(STEEL, 0.50)
    nickel = _tint(NICKEL, 0.45)
    walnut = _tint(palette.hex_to_rgb("#3f352b"), 0.45)

    _box(p, (0.0, 0.006, -0.040), (0.013, 0.012, 0.105), handle_uv, walnut, hidden=())
    # The neck runs under the bowl's flare so the two never float apart.
    _box(p, (0.0, 0.004, 0.0415), (0.014, 0.008, 0.058), neck_uv, nickel, hidden=())
    # Follow the underside, thin rim and concave interior: no disc over the mouth.
    profile = ((0.000, 0.004), (0.004, 0.012), (0.018, 0.018),
               (0.020, 0.017), (0.008, 0.011), (0.004, 0.004))
    start = len(p.mesh.indices)
    p.lathe((0.0, 0.0, 0.0705), profile, segments=12, ellipse=(1.0, 1.22), uv=bowl_uv, cap_uv=bowl_uv,
            color=steel, cap_start=True, cap_end=True)
    for offset in range(start, len(p.mesh.indices), 3):
        p.mesh.indices[offset + 1], p.mesh.indices[offset + 2] = p.mesh.indices[offset + 2], p.mesh.indices[offset + 1]
    p.add_note("closed thin-walled oval bowl with a genuinely concave interior")


# ------------------------------------------------------------------- plate


def build_plate(p: PropBuilder) -> None:
    """Porcelain dinner plate: one sixteen-sided surface split into four bands.

    The bands are the underside/foot, the outer edge, the rim lip and the
    well; each samples its own atlas cell, and the caps close the underside
    centre and the well floor so the piece has no holes.  The outer edge owns
    the 0.22 m catalogue width, the rim top the 0.022 m height.
    """
    tex = _atlas(p, "plate", ("face", "rim", "edge", "foot"))
    porcelain = _tint(PORCELAIN, 0.60)

    bands = (
        ("foot", ((0.0030, 0.040), (0.0000, 0.058), (0.0050, 0.064)), True, False),
        ("edge", ((0.0050, 0.064), (0.0100, 0.100), (0.0160, 0.110)), False, False),
        ("rim", ((0.0160, 0.110), (0.0220, 0.102)), False, False),
        ("face", ((0.0220, 0.102), (0.0110, 0.076), (0.0080, 0.034)), False, True),
    )
    for region, profile, cap_start, cap_end in bands:
        uv = tex.uv(region)
        start = len(p.mesh.indices)
        p.lathe((0.0, 0.0, 0.0), profile, segments=16, uv=uv, cap_uv=uv, color=porcelain,
                cap_start=cap_start, cap_end=cap_end)
        for offset in range(start, len(p.mesh.indices), 3):
            p.mesh.indices[offset + 1], p.mesh.indices[offset + 2] = p.mesh.indices[offset + 2], p.mesh.indices[offset + 1]
    p.add_note("four lathe bands (foot/edge/rim/well); outer edge owns the 0.22 m width")


# -------------------------------------------------------------------- bowl


def build_bowl(p: PropBuilder) -> None:
    """Porcelain cereal bowl: one sixteen-sided surface split into four bands.

    Outside, rim, inside and foot each sample their own cell; the base disc
    and the inner floor cap close the solid.  The rim top is the catalogue
    height and the outside wall owns the catalogue width.
    """
    tex = _atlas(p, "bowl", ("outside", "inside", "foot", "rim"))
    porcelain = _tint(PORCELAIN, 0.60)

    bands = (
        ("foot", ((0.000, 0.030), (0.008, 0.034)), True, False),
        ("outside", ((0.008, 0.034), (0.026, 0.050), (0.048, 0.061), (0.065, 0.075)), False, False),
        ("rim", ((0.065, 0.075), (0.060, 0.070)), False, False),
        ("inside", ((0.060, 0.070), (0.042, 0.055), (0.017, 0.030)), False, True),
    )
    for region, profile, cap_start, cap_end in bands:
        uv = tex.uv(region)
        start = len(p.mesh.indices)
        p.lathe((0.0, 0.0, 0.0), profile, segments=16, uv=uv, cap_uv=uv, color=porcelain,
                cap_start=cap_start, cap_end=cap_end)
        for offset in range(start, len(p.mesh.indices), 3):
            p.mesh.indices[offset + 1], p.mesh.indices[offset + 2] = p.mesh.indices[offset + 2], p.mesh.indices[offset + 1]
    p.add_note("four lathe bands (foot/outside/rim/inside); rim top owns the height")


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


PROPS = {
    "home:knife": build_knife,
    "home:fork": build_fork,
    "home:spoon": build_spoon,
    "home:plate": build_plate,
    "home:bowl": build_bowl,
    "home:plant_table": build_plant_table,
}
