"""The outdoor ground kit's models: standing grass tufts.

Each entry is a ``{"outdoor:<id>": build_function}`` pair following the pack
contract in ``tools/props/README.md`` and the commented exemplar
``parts/utility.py``: metres, Y-up, origin on the floor-contact point, ``+Z``
facing the player, one embedded texture per prop, and the catalogue's ``size``
matched within the mesh validator's tolerance.

Grass tufts are alpha-cutout models: their texture keeps its painted alpha
(``p.set_texture(size, alpha=True)``) and their material declares
``alpha_mode="mask"`` with the glTF default 0.5 cutoff, so the game draws
them through the alpha-tested cutout pass. They are non-solid and the level
scatter tool marks them ``"occludes": false`` so the light bake never grinds
a solid shadow box out of thousands of blade cards.

Construction, shared by both tufts:

* one 256x256 RGBA atlas with two painted blade groups; the RGB behind the
  transparent margins is a dark green veil, never black, so bilinear filtering
  and mipmaps do not fringe the blades;
* each card is a vertical strip of three stacked quads (six triangles), bent a
  few centimetres along its height.  The tallest card of every tuft reaches the
  catalogue height exactly, and at least one card is yawed 0 degrees and one
  90 degrees, so the tuft's footprint is exactly ``size[0] x size[2]`` after a
  final x/z fit;
* per-card yaw, width, height, bend and mirrored/alternate blade group keep the
  4-7 cards of a tuft from reading as coplanar planes;
* cards stand on y = 0, the union is horizontally centred, and ``+Z`` is the
  nominal face direction (the level's rotation yaw aims the tuft).

Budgets: 30 triangles (5 cards) for the small tuft and 42 (7 cards) for the
large one, both far below the 500-triangle target; one 256x256 embedded atlas.
"""

from __future__ import annotations

import math

from mesh import PropBuilder
from tex import Texture

# --------------------------------------------------------------- atlas contract
#
# One square alpha atlas.  Two blade groups sit in the left and right halves,
# each with a transparent margin on every side; cards reference one group and
# may mirror it, so four visual variants come from one sheet.

ATLAS_SIZE = 256
BLADE_REGIONS = {
    "blade_a": (8, 2, 112, 250),
    "blade_b": (136, 2, 112, 250),
}
BLADE_COUNT = 7
# The colour behind alpha 0.  Masking never shows it directly, but a dark-green
# veil keeps mipmap bleed from darkening the blade edges.
VEIL_RGB = (34, 44, 28)
BLADE_BASE = (46.0, 60.0, 35.0)   # darker, rooted base
BLADE_TIP = (94.0, 114.0, 70.0)   # muted green tip
BLADE_DRY = (102.0, 100.0, 62.0)  # one blade in six reads a touch dry

# Vertex tints per strip level: a baked contact gradient on top of the painted
# one.  The top level keeps the painted colour almost untouched.
CARD_TINTS = ((196, 205, 188), (230, 236, 224), (248, 250, 244))

# --------------------------------------------------------------- tuft layouts
#
# (yaw_degrees, half_width, height, bend, region, mirrored).  Yaw 0 and 90
# always carry the two widest cards, so the footprint extremes are well
# defined; the final fit then makes them exact.

GRASS_PATCH_SMALL_CARDS = (
    (0.0, 0.275, 0.300, 0.035, "blade_a", False),
    (90.0, 0.275, 0.270, -0.030, "blade_b", True),
    (38.0, 0.215, 0.285, 0.028, "blade_b", False),
    (142.0, 0.220, 0.245, -0.024, "blade_a", True),
    (308.0, 0.225, 0.265, 0.030, "blade_b", False),
)

GRASS_PATCH_LARGE_CARDS = (
    (0.0, 0.475, 0.620, 0.070, "blade_a", False),
    (90.0, 0.475, 0.560, -0.060, "blade_b", True),
    (36.0, 0.370, 0.585, 0.055, "blade_b", False),
    (118.0, 0.380, 0.520, -0.050, "blade_a", True),
    (208.0, 0.365, 0.548, -0.060, "blade_a", False),
    (252.0, 0.375, 0.500, 0.048, "blade_b", True),
    (300.0, 0.385, 0.535, -0.052, "blade_b", False),
)


# ------------------------------------------------------------------- painting


def _set_pixel(tex: Texture, x: int, y: int, rgb, alpha: int = 255) -> None:
    """Direct RGBA plot: the shared painter forces alpha 255, and a cutout
    atlas needs real transparency, so this module writes its pixels itself."""
    if x < 0 or y < 0 or x >= tex.width or y >= tex.height:
        return
    index = (y * tex.width + x) * 4
    tex.pixels[index] = max(0, min(255, int(round(rgb[0]))))
    tex.pixels[index + 1] = max(0, min(255, int(round(rgb[1]))))
    tex.pixels[index + 2] = max(0, min(255, int(round(rgb[2]))))
    tex.pixels[index + 3] = alpha


def _paint_blade_group(tex: Texture, rect, blade_count: int) -> None:
    """Paints overlapping tapered blades into one atlas region.

    Every region keeps transparent margins on all four sides.  Blades are
    rooted just above the region's bottom edge and the first blade's tip
    reaches the region's top row, so the tallest geometry card is cut by a
    blade tip rather than by a flat canvas edge.
    """
    x0, y0, width, height = rect
    rng = tex.rng
    root_y = y0 + height - 3
    top_y = y0
    for index in range(blade_count):
        half_base = rng.uniform(0.024, 0.042) * width
        centre = x0 + width * (index + 0.5) / blade_count + rng.uniform(-0.05, 0.05) * width
        lean = rng.uniform(-0.22, 0.22) * width
        wobble = rng.uniform(-0.10, 0.10) * width
        if index == 0:
            tip_y = top_y
        else:
            tip_y = top_y + rng.randint(0, int(0.38 * height))
        tip_rgb = BLADE_DRY if rng.chance(0.18) else BLADE_TIP
        base_tone = rng.uniform(0.86, 1.08)
        tip_tone = rng.uniform(0.88, 1.12)
        for y in range(root_y, tip_y - 1, -1):
            t = (root_y - y) / float(max(1, root_y - tip_y))
            curve = lean * t * t + wobble * math.sin(math.pi * t)
            half = max(0.55, half_base * (1.0 - t) ** 0.9)
            centre_x = centre + curve
            centre_x = min(x0 + width - half - 0.5, max(x0 + half + 0.5, centre_x))
            tone = base_tone * (1.0 - t) + tip_tone * t
            rgb = (
                (BLADE_BASE[0] * (1.0 - t) + tip_rgb[0] * t) * tone,
                (BLADE_BASE[1] * (1.0 - t) + tip_rgb[1] * t) * tone,
                (BLADE_BASE[2] * (1.0 - t) + tip_rgb[2] * t) * tone,
            )
            left = int(math.floor(centre_x - half + 0.5))
            right = int(math.floor(centre_x + half + 0.5))
            for px in range(left, right + 1):
                _set_pixel(tex, px, y, rgb, 255)


def _blade_atlas(p: PropBuilder) -> Texture:
    """Paints the cutout atlas and registers both blade regions."""
    tex = p.set_texture(ATLAS_SIZE, alpha=True)
    # RGB veil behind the zero alpha: keeps filtered blade edges green.
    for index in range(0, len(tex.pixels), 4):
        tex.pixels[index] = VEIL_RGB[0]
        tex.pixels[index + 1] = VEIL_RGB[1]
        tex.pixels[index + 2] = VEIL_RGB[2]
    for name, rect in BLADE_REGIONS.items():
        tex.region(name, rect)
        _paint_blade_group(tex, rect, BLADE_COUNT)
    return tex


# ------------------------------------------------------------------- geometry


def _blade_uv(tex: Texture, name: str, mirrored: bool):
    u0, v0, u1, v1 = tex.uv(name)
    if mirrored:
        return (u1, v0, u0, v1)
    return (u0, v0, u1, v1)


def _add_card(mesh, card, tex: Texture) -> None:
    """One alpha card: three stacked, bent quads facing the card's yaw."""
    yaw, half_width, height, bend, region, mirrored = card
    cos_yaw = math.cos(math.radians(yaw))
    sin_yaw = math.sin(math.radians(yaw))
    u0, v0, u1, v1 = _blade_uv(tex, region, mirrored)
    span = v1 - v0
    levels = len(CARD_TINTS)
    for level in range(levels):
        t0 = level / levels
        t1 = (level + 1) / levels
        low = height * t0
        high = height * t1
        lean_low = bend * t0 * t0
        lean_high = bend * t1 * t1
        # Local corners bottom-left, bottom-right, top-right, top-left, then
        # yawed about Y with the pack's +Z-forward convention.
        local = (
            (lean_low - half_width, low),
            (lean_low + half_width, low),
            (lean_high + half_width, high),
            (lean_high - half_width, high),
        )
        points = [(x * cos_yaw, y, -x * sin_yaw) for (x, y) in local]
        uv_rect = (
            u0,
            v0 + span * (levels - 1 - level) / levels,
            u1,
            v0 + span * (levels - level) / levels,
        )
        mesh.quad(*points, uv=uv_rect, color=CARD_TINTS[level])


def _fit_footprint(mesh, target_width: float, target_depth: float) -> None:
    """Scales x/z about the bbox centre so the footprint is exact.

    Card bends push a few centimetres past the nominal half-width; the fit
    removes that drift without touching heights or UVs.
    """
    low, high = mesh.bounds()
    span_x = high[0] - low[0]
    span_z = high[2] - low[2]
    centre_x = (low[0] + high[0]) * 0.5
    centre_z = (low[2] + high[2]) * 0.5
    scale_x = target_width / span_x if span_x > 1e-6 else 1.0
    scale_z = target_depth / span_z if span_z > 1e-6 else 1.0
    mesh.positions = [
        (round((x - centre_x) * scale_x, 6), y, round((z - centre_z) * scale_z, 6))
        for (x, y, z) in mesh.positions
    ]


def _build_tuft(p: PropBuilder, cards) -> None:
    tex = _blade_atlas(p)
    slot = p.material("grass_blades", alpha_mode="mask")
    p.begin_material(slot)
    for card in cards:
        _add_card(p.mesh, card, tex)
    _fit_footprint(p.mesh, p.size[0], p.size[2])
    p.mesh.normalize_origin()
    p.add_note(
        f"{len(cards)} crossed alpha-cutout cards, {len(cards) * 3 * 2} triangles; "
        "blades painted to the card top; one 256px atlas, mask cutoff 0.5"
    )


def build_grass_patch_small(p: PropBuilder) -> None:
    """Low-density tuft: five cards, 30 triangles, catalogue size 0.55 x 0.30 x 0.55."""
    _build_tuft(p, GRASS_PATCH_SMALL_CARDS)


def build_grass_patch_large(p: PropBuilder) -> None:
    """Dense tuft: seven cards, 42 triangles, catalogue size 0.95 x 0.62 x 0.95."""
    _build_tuft(p, GRASS_PATCH_LARGE_CARDS)


PROPS = {
    "outdoor:grass_patch_small": build_grass_patch_small,
    "outdoor:grass_patch_large": build_grass_patch_large,
}
