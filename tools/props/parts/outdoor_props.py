"""The outdoor kit's props: tree, lamps, fence post and facade parts.

Owned by the outdoor props writer (writer B). Each entry is a
``{"outdoor:<id>": build_function}`` pair following the pack contract in
``tools/props/README.md`` and the commented exemplar ``parts/utility.py``:
metres, Y-up, origin on the placement point the catalogue documents, ``+Z``
facing the player, one embedded texture per prop, and the catalogue's
``size`` matched within the mesh validator's tolerance.

Design notes:

* the tree's leaf canopy is an alpha-cutout (``alpha_mode="mask"``) material;
  the trunk and branches stay opaque so the silhouette reads at night. The
  canopy's own bounding box is fitted to the catalogue footprint (the same
  trick ``Mesh.normalize_origin`` uses for the origin), so the model measures
  exactly 4.6 x 6.4 x 4.6 m while the cards stay slightly irregular;
* every lamp has an emissive pane as its own material group (the level owns
  the real light) and documents its mounting origin and the exact suggested
  point light. Housings open towards ``+Z``: the pane's outward hemisphere
  stays clear of opaque geometry and the level light sits in front of the
  glass, never inside a metal box;
* the facade parts are modular panels and roof pieces sized in metres so a
  level can butt them against real walls, doors and gable ceilings. The
  doorway panel cuts a real 1.10 x 2.15 m hole through its 0.40 m depth.

The prop textures are painted at build time into each model's embedded PNG
(the prop-asset exception in ``docs/ASSET_SPECIFICATION.md`` section 2): one
128/256 px atlas per prop, UVs strictly inside 0..1.
"""

from __future__ import annotations

import math

import palette
from mesh import PropBuilder
from parts.refreshed import orient_outward, outward_lathe, solid_box, solid_cylinder
from tex import Rng, Texture

# Triangle aims (the pack budget in ``build.py`` is the enforced one).
TARGETS = {
    "outdoor:tree_01": 398,
    "outdoor:lamp_stand": 120,
    "outdoor:lamp_fence": 100,
    "outdoor:lamp_wall": 159,
    "outdoor:fence_post": 28,
    "outdoor:collision_peg": 12,
    "outdoor:house_wall_solid": 48,
    "outdoor:house_wall_window": 168,
    "outdoor:house_wall_doorway": 84,
    "outdoor:house_roof_slope": 36,
    "outdoor:house_roof_ridge": 8,
    "outdoor:house_corner_trim": 24,
    "outdoor:concrete_step": 20,
}

# --------------------------------------------------------------------- colour
#
# The outdoor set is one muted, weathered family: a dark brown-grey lamp
# housing, warm amber glass, green-grey siding with faded cream trim, dull
# slate shingles and pale cast concrete.

TREE_BARK = palette.hex_to_rgb("#5b4a38")
TREE_BRANCH = palette.hex_to_rgb("#514334")
LAMP_METAL = palette.hex_to_rgb("#3f3c36")
LAMP_PANE = palette.hex_to_rgb("#e8c795")
POST_WOOD = palette.hex_to_rgb("#6a5a45")
SIDING = palette.hex_to_rgb("#6d7565")
PLINTH = palette.hex_to_rgb("#66625a")
TRIM = palette.hex_to_rgb("#c3bca6")
GLASS = palette.hex_to_rgb("#2b3138")
SHINGLE = palette.hex_to_rgb("#575146")
CONCRETE = palette.hex_to_rgb("#aaa496")
PEG_GREY = palette.hex_to_rgb("#3d3e42")

# The colour behind alpha 0 on the leaf atlas: a muted leaf green rather than
# black, so mipmaps and bilinear filtering darken the card edges less and the
# flat software preview still reads as foliage.
LEAF_VEIL = (56, 72, 46)


def _tint(color: tuple[int, int, int], lift: float = 0.6) -> tuple[int, int, int]:
    """Vertex colour for a face whose texture is painted in ``color``.

    The shader is ``texture * vertex colour * face shade``; tinting with the
    texture's own colour would multiply it into mud, so the tint is lifted
    towards the pack's light neutral (the same trick as ``parts/furniture``).
    """
    return palette.mix(color, palette.hex_to_rgb(palette.PLASTIC_WHITE), lift)


def _set_pixel(tex: Texture, x: int, y: int, rgb, alpha: int = 255) -> None:
    """Direct RGBA plot: the shared painter forces alpha 255, and a cutout
    atlas needs real transparency, so the leaf region writes its own pixels."""
    if x < 0 or y < 0 or x >= tex.width or y >= tex.height:
        return
    index = (y * tex.width + x) * 4
    tex.pixels[index] = max(0, min(255, int(round(rgb[0]))))
    tex.pixels[index + 1] = max(0, min(255, int(round(rgb[1]))))
    tex.pixels[index + 2] = max(0, min(255, int(round(rgb[2]))))
    tex.pixels[index + 3] = alpha


# --------------------------------------------------------------------- paint


def _paint_bark(tex: Texture, region: str, base, seed: int) -> None:
    """Rough vertical bark: dark fissures, a few lit ridges, grime."""
    dark = palette.shade(base, 0.55)
    light = palette.shade(base, 1.32)
    tex.fill(region, base, jitter=10, seed=seed)
    tex.noise(region, amount=7, freq=3, seed=seed + 1)
    tex.streaks(region, dark, count=11, seed=seed + 2, alpha=95)
    tex.streaks(region, light, count=6, seed=seed + 3, alpha=45)
    tex.grain(region, dark, seed=seed + 4, density=0.35, alpha=42)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 5, radius=2, alpha=30)
    tex.border(region, dark, width=2, alpha=60)


def _paint_leaf_atlas(tex: Texture, region: str, seed: int) -> None:
    """Clustered leaf blobs with transparent gaps on a dark green veil.

    The canvas starts fully transparent; the veil RGB is written with alpha 0
    and the blobs with alpha 255, so the glTF MASK cutoff (0.5) keeps the
    clusters and discards the gaps.
    """
    x0, y0, width, height = tex.cell(region)
    for py in range(y0, y0 + height):
        for px in range(x0, x0 + width):
            _set_pixel(tex, px, py, LEAF_VEIL, 0)
    rng = Rng(seed)
    greens = (
        palette.hex_to_rgb(palette.FOLIAGE_DARK),
        palette.hex_to_rgb(palette.FOLIAGE_GREEN),
        palette.hex_to_rgb(palette.FOLIAGE_LIGHT),
        (112, 132, 86),
    )
    clusters = 8
    for _ in range(clusters):
        cx = x0 + width * rng.uniform(0.18, 0.82)
        cy = y0 + height * rng.uniform(0.18, 0.82)
        for _ in range(rng.randint(24, 34)):
            spread = rng.uniform(0.0, 0.36) * width
            bx = cx + rng.uniform(-spread, spread)
            by = cy + rng.uniform(-spread, spread)
            radius = rng.randint(5, 11)
            tone = rng.uniform(0.84, 1.16)
            _blob(tex, bx, by, radius, palette.shade(rng.pick(greens), tone))
        # A few transparent leaf gaps inside the cluster: foliage, not a disc.
        for _ in range(rng.randint(2, 4)):
            spread = rng.uniform(0.0, 0.22) * width
            bx = cx + rng.uniform(-spread, spread)
            by = cy + rng.uniform(-spread, spread)
            _blob(tex, bx, by, rng.randint(1, 2), LEAF_VEIL, alpha=0)
    # Loose leaves between the clusters keep the card edges ragged.
    for _ in range(20):
        bx = x0 + width * rng.uniform(0.04, 0.96)
        by = y0 + height * rng.uniform(0.04, 0.96)
        tone = rng.uniform(0.82, 1.12)
        _blob(tex, bx, by, rng.randint(2, 5), palette.shade(rng.pick(greens), tone))


def _blob(tex: Texture, centre_x: float, centre_y: float, radius: int, rgb, alpha: int = 255) -> None:
    """One soft-edged pixel blob (a leaf cluster stamp)."""
    r = max(1, int(radius))
    for y in range(int(centre_y) - r, int(centre_y) + r + 1):
        for x in range(int(centre_x) - r, int(centre_x) + r + 1):
            dx = (x - centre_x) / (r + 0.35)
            dy = (y - centre_y) / (r + 0.35)
            if dx * dx + dy * dy <= 1.0:
                _set_pixel(tex, x, y, rgb, alpha)


def _paint_metal_housing(tex: Texture, region: str, base, seed: int) -> None:
    """Dark weathered painted metal: brushed grain, rust freckles, grime."""
    dark = palette.shade(base, 0.60)
    light = palette.shade(base, 1.30)
    tex.fill(region, base, jitter=7, seed=seed)
    tex.noise(region, amount=4, freq=4, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.35, alpha=45)
    tex.grain(region, light, seed=seed + 3, density=0.25, alpha=26)
    tex.streaks(region, dark, count=4, seed=seed + 4, alpha=28)
    tex.spots(region, palette.hex_to_rgb(palette.RUST), count=4, seed=seed + 5, radius=2, alpha=34)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 6, radius=2, alpha=30)
    tex.border(region, dark, width=1, alpha=60)


def _paint_warm_pane(tex: Texture, region: str, seed: int) -> None:
    """The family's warm glass: a soft amber gradient with a hot centre.

    The emissive term is ``emissiveFactor * base texture``, so this canvas is
    the emission mask too: painted bright and even, with only a faint darker
    frame ring where the housing shadows the glass.
    """
    tex.gradient(region, palette.shade(LAMP_PANE, 1.16), palette.shade(LAMP_PANE, 0.82), jitter=3, seed=seed)
    tex.spots(region, palette.shade(LAMP_PANE, 1.32), count=6, seed=seed + 1, radius=3, alpha=26)
    tex.spots(region, palette.shade(LAMP_PANE, 0.74), count=4, seed=seed + 2, radius=2, alpha=20)
    tex.border(region, palette.shade(LAMP_PANE, 0.55), width=2, alpha=110)


def _paint_wood(tex: Texture, region: str, base, seed: int) -> None:
    """Weathered fence timber: vertical grain, dusty highlight, a little grime."""
    dark = palette.shade(base, 0.60)
    light = palette.shade(base, 1.24)
    tex.fill(region, base, jitter=9, seed=seed)
    tex.noise(region, amount=5, freq=3, seed=seed + 1)
    tex.streaks(region, dark, count=14, seed=seed + 2, alpha=85)
    tex.streaks(region, light, count=7, seed=seed + 3, alpha=40)
    tex.grain(region, dark, seed=seed + 4, density=0.40, alpha=45)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 5, radius=2, alpha=26)
    tex.border(region, dark, width=2, alpha=70)


def _paint_siding(tex: Texture, region: str, base, seed: int, height_metres: float = 2.66) -> None:
    """Horizontal clapboard over a painted plinth band.

    The region's V axis runs top (0) to bottom (1) across the panel's full
    height, so the plinth is painted as the last ``0.35 m`` of the region.
    """
    dark = palette.shade(base, 0.70)
    light = palette.shade(base, 1.16)
    tex.fill(region, base, jitter=6, seed=seed)
    strips = 13
    for index in range(strips):
        v0 = index / strips
        v1 = (index + 1) / strips
        tex.band(region, palette.shade(base, 1.0 - 0.05 * (index % 2)), v0, v1)
        tex.band(region, dark, v0, v0 + 0.009, alpha=150)
    tex.noise(region, amount=4, freq=3, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.30, alpha=28)
    tex.grain(region, light, seed=seed + 3, density=0.22, alpha=20)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 4, radius=3, alpha=20)
    band_top = 1.0 - 0.35 / height_metres
    tex.band(region, PLINTH, band_top, 1.0)
    tex.band(region, palette.shade(PLINTH, 0.68), band_top, band_top + 0.014, alpha=170)
    tex.spots(region, palette.shade(PLINTH, 0.84), count=5, seed=seed + 5, radius=3, alpha=28)
    tex.border(region, dark, width=1, alpha=50)


def _paint_trim(tex: Texture, region: str, base, seed: int) -> None:
    """Painted trim board: flat, a whisper of brush grain, soft edges."""
    dark = palette.shade(base, 0.74)
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=3, freq=4, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.20, alpha=22)
    tex.grain(region, palette.shade(base, 1.09), seed=seed + 3, density=0.16, alpha=16)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=2, seed=seed + 4, radius=2, alpha=16)
    tex.border(region, dark, width=1, alpha=40)


def _paint_shingles(tex: Texture, region: str, base, seed: int) -> None:
    """Overlapping slate tabs: horizontal courses with staggered seams."""
    dark = palette.shade(base, 0.62)
    light = palette.shade(base, 1.22)
    tex.fill(region, base, jitter=8, seed=seed)
    tex.noise(region, amount=6, freq=3, seed=seed + 1)
    rows = 8
    for row in range(rows):
        v0 = row / rows
        v1 = (row + 1) / rows
        tex.band(region, palette.shade(base, 0.92 + 0.05 * (row % 2)), v0 + 0.014, v1, alpha=210)
        tex.band(region, dark, v0, v0 + 0.020, alpha=200)
        tex.bar(region, light, (0.0, v0 + 0.014, 1.0, v0 + 0.030), alpha=36)
        for column in range(7):
            u = (column + 0.5 * (row % 2)) / 7.0
            tex.bar(region, dark, (u, v0 + 0.014, u + 0.012, v1), alpha=120)
    tex.streaks(region, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 2, alpha=26)
    tex.spots(region, palette.shade(base, 0.70), count=5, seed=seed + 3, radius=2, alpha=26)
    tex.border(region, dark, width=1, alpha=70)


def _paint_glass(tex: Texture, region: str, seed: int) -> None:
    """Opaque dark blue-grey glazing with one faint sheen; never transparent."""
    tex.gradient(region, palette.shade(GLASS, 1.30), palette.shade(GLASS, 0.78), jitter=3, seed=seed)
    tex.bar(region, palette.shade(GLASS, 1.70), (0.12, 0.06, 0.32, 0.94), alpha=20)
    tex.bar(region, palette.shade(GLASS, 0.55), (0.64, 0.0, 0.80, 1.0), alpha=24)
    tex.border(region, palette.shade(GLASS, 0.50), width=2, alpha=120)


def _paint_concrete(tex: Texture, region: str, seed: int) -> None:
    """Pale cast concrete: fine aggregate speckle and a couple of stains."""
    tex.fill(region, CONCRETE, jitter=6, seed=seed)
    tex.noise(region, amount=5, freq=5, seed=seed + 1)
    tex.grain(region, palette.shade(CONCRETE, 0.80), seed=seed + 2, density=0.25, alpha=26)
    tex.grain(region, palette.shade(CONCRETE, 1.12), seed=seed + 3, density=0.20, alpha=20)
    tex.spots(region, palette.shade(CONCRETE, 0.86), count=6, seed=seed + 4, radius=3, alpha=26)
    tex.spots(region, palette.shade(CONCRETE, 1.10), count=5, seed=seed + 5, radius=2, alpha=20)
    tex.streaks(region, palette.shade(CONCRETE, 0.80), count=3, seed=seed + 6, alpha=24)
    tex.border(region, palette.shade(CONCRETE, 0.72), width=1, alpha=60)


def _paint_dark_grey(tex: Texture, region: str, base, seed: int) -> None:
    """Flat utility dark grey: a whisper of grain, one grime freckle.

    The collision peg's sheet is never really looked at: the level buries the
    cube it paints, so the texture stays as cheap as the geometry.
    """
    dark = palette.shade(base, 0.72)
    tex.fill(region, base, jitter=3, seed=seed)
    tex.grain(region, dark, seed=seed + 1, density=0.28, alpha=30)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=2, seed=seed + 2, radius=2, alpha=22)
    tex.border(region, dark, width=1, alpha=50)


# ---------------------------------------------------------------------- tree
#
# Layout: a tapered six-segment trunk (0.26 m base radius, so 0.52 m across at
# the floor) carries five branches into a broad leaf canopy. The canopy is
# twenty-two alpha-cutout cards on a rough sphere; the cards are then fitted to
# the catalogue footprint, so the model is exactly 4.6 x 6.4 x 4.6 m with its
# base at y = 0 and its horizontal centre at the origin. The trunk's visible
# width fits the level collider the catalogue documents: place the prop solid
# with a size of roughly [0.8, 6.4, 0.8] so only the trunk blocks the player.

CANOPY_CENTRE = (0.0, 4.05, 0.0)
CANOPY_RADIUS = 1.55

# (elevation_degrees, count, yaw_offset_degrees, width, height)
CANOPY_RINGS = (
    (70.0, 4, 45.0, 1.50, 1.35),
    (52.0, 5, 18.0, 1.90, 1.45),
    (2.0, 6, 48.0, 2.00, 1.55),
    (-46.0, 5, 0.0, 1.80, 1.35),
)
CANOPY_CAPS = ((90.0, 1.40, 1.40), (-90.0, 1.70, 1.70))

# (azimuth_degrees, base_height, base_offset, base_radius, reach, rise, tip_radius)
# The base point sits just inside the trunk (the ring stays buried), and the
# branch tapers from ~12 cm across at the trunk to a ~5 cm tip in the canopy.
TREE_BRANCHES = (
    (25.0, 1.62, 0.05, 0.060, 1.45, 2.15, 0.026),
    (95.0, 1.96, 0.05, 0.056, 1.50, 1.95, 0.024),
    (166.0, 1.74, 0.05, 0.058, 1.35, 2.28, 0.025),
    (238.0, 2.05, 0.05, 0.052, 1.42, 1.86, 0.022),
    (310.0, 1.82, 0.05, 0.058, 1.52, 2.06, 0.025),
)


def _normalize(vector):
    length = math.sqrt(sum(value * value for value in vector))
    if length < 1e-9:
        return (0.0, 0.0, 1.0)
    return tuple(value / length for value in vector)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _leaf_card(p: PropBuilder, centre, normal, width: float, height: float, uv,
               color, shade_mult: float, cols: int = 2, rows: int = 2, bow: float = 0.06) -> None:
    """One alpha-cutout leaf card: a bowed grid facing ``normal``.

    Local axes are built so ``cross(right, up) == normal``; quads are emitted
    bottom-left, bottom-right, top-right, top-left with u along ``right`` and
    v decreasing upward, matching the pack's texture orientation.
    """
    n = _normalize(normal)
    reference = (1.0, 0.0, 0.0) if abs(n[1]) > 0.94 else (0.0, 1.0, 0.0)
    right = _normalize(_cross(reference, n))
    up = _cross(n, right)
    u0, v0, u1, v1 = uv
    for column in range(cols):
        for row in range(rows):
            fu0, fu1 = column / cols, (column + 1) / cols
            fv0, fv1 = row / rows, (row + 1) / rows
            corners = []
            uvs = []
            for fu, fv in ((fu0, fv0), (fu1, fv0), (fu1, fv1), (fu0, fv1)):
                bulge = bow * math.sin(math.pi * fu) * math.sin(math.pi * fv)
                point = tuple(
                    centre[axis] + right[axis] * (fu - 0.5) * width + up[axis] * (fv - 0.5) * height
                    + n[axis] * bulge
                    for axis in range(3)
                )
                corners.append(point)
                uvs.append((u0 + fu * (u1 - u0), v1 - fv * (v1 - v0)))
            p.mesh.quad(*corners, uv=uvs, color=color, shade_mult=shade_mult)


def _fit_box(mesh, vertex_start: int, target) -> None:
    """Affine-fits every vertex from ``vertex_start`` to an exact box.

    ``target`` is ``((x0, x1), (y0, y1), (z0, z1))``. The canopy is built
    roughly and then measured onto the catalogue footprint exactly, the same
    way :meth:`Mesh.normalize_origin` measures the placement origin.
    """
    positions = mesh.positions[vertex_start:]
    low = [min(point[axis] for point in positions) for axis in range(3)]
    high = [max(point[axis] for point in positions) for axis in range(3)]
    scale = [
        (target[axis][1] - target[axis][0]) / max(1e-6, high[axis] - low[axis])
        for axis in range(3)
    ]
    mesh.positions[vertex_start:] = [
        tuple(
            round(target[axis][0] + (point[axis] - low[axis]) * scale[axis], 6)
            for axis in range(3)
        )
        for point in positions
    ]


def _leaf_canopy(p: PropBuilder, uv) -> None:
    """Twenty-two cutout cards on a rough sphere, fitted to the canopy box."""
    rng = Rng(0x7EEE)
    vertex_start = len(p.mesh.positions)
    for elevation, count, yaw_offset, width, height in CANOPY_RINGS:
        phi = math.radians(elevation)
        for index in range(count):
            yaw = math.radians(yaw_offset + 360.0 * index / count)
            radial = (math.cos(phi) * math.cos(yaw), math.sin(phi), math.cos(phi) * math.sin(yaw))
            centre = tuple(CANOPY_CENTRE[axis] + CANOPY_RADIUS * radial[axis] for axis in range(3))
            tint = 0.90 + 0.05 * rng.randint(0, 3)
            _leaf_card(p, centre, radial, width, height, uv, (247, 250, 243), tint)
    for elevation, width, height in CANOPY_CAPS:
        centre = (CANOPY_CENTRE[0], CANOPY_CENTRE[1] + CANOPY_RADIUS * math.sin(math.radians(elevation)),
                  CANOPY_CENTRE[2])
        normal = (0.0, math.sin(math.radians(elevation)), 0.0)
        _leaf_card(p, centre, normal, width, height, uv, (238, 244, 236), 0.88 if elevation < 0 else 1.0)
    _fit_box(p.mesh, vertex_start, ((-2.3, 2.3), (1.85, 6.4), (-2.3, 2.3)))


def build_tree(p: PropBuilder) -> None:
    """Stylized leafy tree: tapered trunk, five branches, cutout canopy."""
    tex = p.set_texture(256, seed=701, alpha=True)
    tex.auto("trunk", "branch", "leaf")
    trunk_uv = tex.uv("trunk", inset=2)
    branch_uv = tex.uv("branch", inset=2)
    leaf_uv = tex.uv("leaf", inset=2)
    _paint_bark(tex, "trunk", TREE_BARK, seed=703)
    _paint_bark(tex, "branch", TREE_BRANCH, seed=709)
    _paint_leaf_atlas(tex, "leaf", seed=717)

    trunk_slot = p.material("tree_trunk")
    branch_slot = p.material("tree_bark_branch")
    leaf_slot = p.material("tree_leaves", alpha_mode="mask")

    p.begin_material(trunk_slot)
    profile = (
        (0.00, 0.260),
        (0.12, 0.225),
        (0.72, 0.195),
        (1.30, 0.165),
        (1.85, 0.135),
        (2.45, 0.105),
    )
    start = len(p.mesh.indices)
    p.mesh.lathe((0.0, 0.0, 0.0), profile, segments=6, axis="y", uv=trunk_uv,
                 color=_tint(TREE_BARK, 0.55))
    orient_outward(p.mesh, start, (0.0, 1.2, 0.0))

    p.begin_material(branch_slot)
    for azimuth, base_y, base_offset, base_radius, reach, rise, tip_radius in TREE_BRANCHES:
        angle = math.radians(azimuth)
        direction = (math.cos(angle), 0.0, math.sin(angle))
        base = (direction[0] * base_offset, base_y, direction[2] * base_offset)
        tip = (direction[0] * reach, base_y + rise, direction[2] * reach)
        mid = (
            base[0] + (tip[0] - base[0]) * 0.55,
            base[1] + (tip[1] - base[1]) * 0.55 + 0.10,
            base[2] + (tip[2] - base[2]) * 0.55,
        )
        p.tube_path([base, mid, tip], radii=[base_radius, base_radius * 0.68, tip_radius],
                    segments=5, uv=branch_uv, color=_tint(TREE_BRANCH, 0.55),
                    cap_start=True, cap_end=True)

    p.begin_material(leaf_slot)
    _leaf_canopy(p, leaf_uv)

    p.add_note("trunk is 0.52 m across at the floor and 6 segments; base at y=0, horizontally centred")
    p.add_note("place it solid with a level size of roughly [0.8, 6.4, 0.8] so only the trunk blocks the player")
    p.add_note("canopy is alpha-cutout leaf cards (MASK, cutoff 0.5) fitted to the 4.6 x 4.6 m footprint")


# --------------------------------------------------------------------- lamps
#
# One coherent family: a warm pane recessed in a dark metal housing that opens
# towards +Z, framed in metal, capped by a truncated four-sided pyramid. The
# pane is its own emissive material group; the housing wraps only the back and
# sides, so the pane's outward hemisphere is open and the level's point light
# sits in front of the glass, never inside the metal shell.


def _lantern(p: PropBuilder, *, x: float, y: float, z_front: float, width: float, height: float,
             depth: float, uv_metal, uv_pane, metal_slot: int, pane_slot: int,
             metal_color, cap_color, cap_side: float, cap_height: float, cap_top_side: float,
             cap_z: float, frame: float = 0.03, recess: float = 0.02) -> None:
    """The family's lantern head: open-front shell, recessed pane, cap.

    ``(x, y)`` is the shell's bottom centre and ``z_front`` the front plane;
    the shell extends backwards to ``z_front - depth``. Metal primitives are
    emitted with ``metal_slot`` and the pane with ``pane_slot`` last, so the
    model carries exactly two material runs per lantern.
    """
    p.begin_material(metal_slot)
    shell_center = (x, y + height * 0.5, z_front - depth * 0.5)
    start = len(p.mesh.indices)
    p.box(shell_center, (width, height, depth),
          uv={"+z": None, "-z": uv_metal, "+x": uv_metal, "-x": uv_metal, "+y": uv_metal, "-y": uv_metal},
          color=metal_color)
    orient_outward(p.mesh, start, shell_center)

    # Frame: four strips around the pane, a whisker proud of the shell face.
    strip_depth = recess + 0.008
    strip_z = z_front - recess * 0.5 + 0.002
    for side in (-1.0, 1.0):
        solid_box(p, (x + side * (width - frame) * 0.5, y + height * 0.5, strip_z),
                  (frame, height, strip_depth), uv=uv_metal, color=metal_color)
    for side in (-1.0, 1.0):
        solid_box(p, (x, y + height * 0.5 + side * (height - frame) * 0.5, strip_z),
                  (width - frame * 2.0, frame, strip_depth), uv=uv_metal, color=metal_color)

    # Truncated four-sided pyramid cap (flats facing the axes at rotation 45).
    # Its base is sunk 1 cm into the shell so the closing disc is never
    # coplanar with the shell top, and the disc faces down: the cap stays
    # opaque when a player looks up under the lantern.
    outward_lathe(p, (x, y + height - 0.01, cap_z),
                  ((0.0, cap_side * math.sqrt(0.5)), (cap_height + 0.01, cap_top_side * math.sqrt(0.5))),
                  segments=4, axis="y", rotation=math.pi * 0.25,
                  uv=uv_metal, color=cap_color, cap_start=True, cap_end=True)

    # The pane last: a flat quad covering the opening, bright vertex colour.
    p.begin_material(pane_slot)
    p.mesh.plane((x, y + height * 0.5, z_front - recess),
                 (width - frame * 2.0, height - frame * 2.0, 0.0),
                 normal="z", uv=uv_pane, color=(255, 255, 255), ao=1.0)


def _lamp_texture(p: PropBuilder, seed: int) -> Texture:
    tex = p.set_texture(128, seed=seed)
    tex.auto("metal", "pane")
    _paint_metal_housing(tex, "metal", LAMP_METAL, seed=seed + 2)
    _paint_warm_pane(tex, "pane", seed=seed + 11)
    return tex


def build_lamp_stand(p: PropBuilder) -> None:
    """Ground-standing garden lamp: base plate, post and a +Z lantern head.

    Mount: the base plate rests on the floor at y = 0, horizontally centred
    (0.34 x 0.34 m footprint), and the pane faces +Z. The level's point light
    sits just in front of the glass: offset [0, 0.86, 0] (2.5 cm clear of the
    recessed pane), which is outside the metal shell.
    """
    size = p.size  # [0.34, 1.05, 0.34]
    tex = _lamp_texture(p, seed=1201)
    metal_uv = tex.uv("metal", inset=2)
    pane_uv = tex.uv("pane", inset=2)
    metal_slot = p.material("lamp_metal")
    pane_slot = p.material("lamp_pane", emissive=(1.0, 0.83, 0.62), strength=1.25)

    p.begin_material(metal_slot)
    solid_box(p, (0.0, 0.025, 0.0), (size[0], 0.05, size[2]),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.30))
    solid_cylinder(p, (0.0, 0.04, 0.0), 0.035, 0.72, segments=8, taper=0.82,
                   side_uv=metal_uv, cap_uv=metal_uv, color=_tint(LAMP_METAL, 0.36))
    _lantern(p, x=0.0, y=0.73, z_front=0.0, width=0.26, height=0.26, depth=0.17,
             uv_metal=metal_uv, uv_pane=pane_uv, metal_slot=metal_slot, pane_slot=pane_slot,
             metal_color=_tint(LAMP_METAL, 0.34), cap_color=_tint(LAMP_METAL, 0.40),
             cap_side=0.26, cap_height=0.06, cap_top_side=0.08, cap_z=-0.04)

    p.add_note("mount: base plate on the floor at y=0, horizontally centred; pane faces +Z")
    p.add_note("suggested level light: shape point, offset [0, 0.86, 0], colour [1.0, 0.86, 0.68], "
               "intensity 0.7, range 7.0, falloff smooth (2.5 cm in front of the pane)")
    p.add_note("pane is its own emissive material (strength 1.25); the housing opens to +Z")


def build_lamp_fence(p: PropBuilder) -> None:
    """Post-cap lantern: a saddle bracket, a squat lantern and a cap.

    Mount: the saddle plate's underside is the local origin (0, 0, 0): set the
    prop at the top of a 0.12 m square post (post axis through the origin) and
    the bracket seats on the post's top face. The pane faces +Z; the level's
    point light sits just in front of the glass at offset [0, 0.30, 0.05].
    """
    tex = _lamp_texture(p, seed=1301)
    metal_uv = tex.uv("metal", inset=2)
    pane_uv = tex.uv("pane", inset=2)
    metal_slot = p.material("lamp_metal")
    pane_slot = p.material("lamp_pane", emissive=(1.0, 0.83, 0.62), strength=1.25)

    p.begin_material(metal_slot)
    # Saddle plate straddling the post top (0.36 m deep: it owns the depth)
    # and the collar that carries the lantern body.
    solid_box(p, (0.0, 0.015, 0.0), (0.28, 0.03, 0.36),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.30))
    solid_box(p, (0.0, 0.045, 0.0), (0.16, 0.03, 0.16),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.36))
    _lantern(p, x=0.0, y=0.06, z_front=0.04, width=0.26, height=0.28, depth=0.20,
             uv_metal=metal_uv, uv_pane=pane_uv, metal_slot=metal_slot, pane_slot=pane_slot,
             metal_color=_tint(LAMP_METAL, 0.34), cap_color=_tint(LAMP_METAL, 0.40),
             cap_side=0.32, cap_height=0.08, cap_top_side=0.08, cap_z=-0.02)

    p.add_note("mount: saddle plate underside is the origin; seat it on a 0.12 m square post top, "
               "post axis through (0, y, 0), pane faces +Z")
    p.add_note("suggested level light: shape point, offset [0, 0.30, 0.05], colour [1.0, 0.86, 0.68], "
               "intensity 0.5, range 4.0, falloff smooth (3 cm in front of the pane)")
    p.add_note("pane is its own emissive material (strength 1.25); the housing opens to +Z")


def build_lamp_wall(p: PropBuilder) -> None:
    """Eave-hanging lantern: wall plate, arch, hanging body and cap.

    Mount: the wall plate lies in the local z = 0 plane at y = 0.52 (it
    straddles the plane by 17 mm either side). The eave hook behind it reaches
    z = -0.17, normally hidden inside the eave or fascia, and the lantern hangs
    in front. Put the origin at the wall/eave face with +Z away from the wall;
    on the doorway panel the documented eave mounts are at local
    (+/-1.15, 2.55, 0.20), i.e. the plate top meets the mount. The level's
    point light sits just in front of the glass at offset [0, 0.18, 0.12].
    """
    size = p.size  # [0.30, 0.52, 0.34]
    tex = _lamp_texture(p, seed=1401)
    metal_uv = tex.uv("metal", inset=2)
    pane_uv = tex.uv("pane", inset=2)
    metal_slot = p.material("lamp_metal")
    pane_slot = p.material("lamp_pane", emissive=(1.0, 0.83, 0.62), strength=1.25)

    p.begin_material(metal_slot)
    # Wall plate in the z = 0 plane, top edge at the catalogue height.
    solid_box(p, (0.0, size[1] - 0.055, 0.0), (0.28, 0.11, 0.035),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.30))
    # Eave hook: a strap over the top and a short back leg (the rear extreme).
    solid_box(p, (0.0, size[1] - 0.0125, -0.09), (0.06, 0.025, 0.16),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.34))
    solid_box(p, (0.0, 0.435, -0.1575), (0.06, 0.12, 0.025),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.34))
    # Arched bracket: from the plate out and down to the hanging body.
    p.tube_path([(0.0, 0.45, 0.0), (0.0, 0.415, 0.055), (0.0, 0.365, 0.09), (0.0, 0.315, 0.105)],
                radii=[0.018, 0.018, 0.018, 0.016], segments=5, uv=metal_uv,
                color=_tint(LAMP_METAL, 0.36), cap_start=False, cap_end=True)
    # Small finial under the body: the model's floor/lowest point at y = 0.
    solid_box(p, (0.0, 0.02, 0.01), (0.06, 0.04, 0.06),
              uv=metal_uv, color=_tint(LAMP_METAL, 0.32))
    _lantern(p, x=0.0, y=0.04, z_front=0.10, width=0.24, height=0.26, depth=0.18,
             uv_metal=metal_uv, uv_pane=pane_uv, metal_slot=metal_slot, pane_slot=pane_slot,
             metal_color=_tint(LAMP_METAL, 0.34), cap_color=_tint(LAMP_METAL, 0.40),
             cap_side=0.30, cap_height=0.06, cap_top_side=0.08, cap_z=0.02)

    p.add_note("mount: wall plate in the local z=0 plane, top at y=0.52; eave hook occupies z<0, "
               "lantern hangs in front (+Z); seat the plate top on the eave mount")
    p.add_note("doorway panel eave mounts: local [+/-1.15, 2.55, 0.20], +Z away from the wall")
    p.add_note("suggested level light: shape point, offset [0, 0.18, 0.12], colour [1.0, 0.86, 0.68], "
               "intensity 0.6, range 5.0, falloff smooth (4 cm in front of the pane)")
    p.add_note("pane is its own emissive material (strength 1.25); the housing opens to +Z")


# --------------------------------------------------------------- fence post


def build_fence_post(p: PropBuilder) -> None:
    """Capped wooden fence post, slightly tapered, 0.12 m square at the base.

    The top seat is a flat cap at y = 1.05: ``outdoor:lamp_fence`` seats its
    saddle plate directly on it.
    """
    size = p.size  # [0.12, 1.05, 0.12]
    tex = p.set_texture(128, seed=1501)
    tex.auto("wood", "end")
    _paint_wood(tex, "wood", POST_WOOD, seed=1503)
    tex.fill("end", palette.shade(POST_WOOD, 0.72), jitter=6, seed=1509)
    tex.grain("end", palette.shade(POST_WOOD, 0.5), seed=1511, density=0.4, alpha=60)

    wood_uv = tex.uv("wood", inset=2)
    end_uv = tex.uv("end", inset=2)
    slot = p.material("post_wood")
    p.begin_material(slot)
    # Square section: a four-segment cylinder at 45 degrees gives axis-facing
    # flats; the circumradius is the half-side times sqrt(2).
    half_side = size[0] * 0.5
    solid_cylinder(p, (0.0, 0.0, 0.0), half_side * math.sqrt(2.0), 0.99,
                   segments=4, rotation=math.pi * 0.25, taper=0.83,
                   side_uv=wood_uv, cap_uv=end_uv, color=_tint(POST_WOOD, 0.45))
    # Flat cap plate: the lamp's seat, 6 cm proud of the tapered shaft top.
    solid_box(p, (0.0, 1.02, 0.0), (size[0], 0.06, size[2]),
              uv={"+y": end_uv, "-y": wood_uv, "+x": wood_uv, "-x": wood_uv,
                  "+z": wood_uv, "-z": wood_uv},
              color=_tint(POST_WOOD, 0.48))
    p.add_note("top seat is a flat cap at y = 1.05; outdoor:lamp_fence seats on it")
    p.add_note("shaft tapers 0.12 m to 0.10 m square; one opaque material")


# ---------------------------------------------------------- collision peg


def build_collision_peg(p: PropBuilder) -> None:
    """Deliberately tiny 6 cm cube: the carrier for an authored invisible collider.

    This is the one prop whose drawn geometry is expected to be buried: a level
    places it ``solid: true`` with its own collider ``size``, sets
    ``occludes: false`` and drops ``y`` below the local floor, so the cube is
    never visible and the bake never sees it. Only the level-authored collision
    box exists; the mesh and its one 32 px sheet stay as cheap and undetailed
    as possible.
    """
    tex = p.set_texture(32, seed=2301)
    tex.auto("body")
    _paint_dark_grey(tex, "body", PEG_GREY, seed=2303)
    body_uv = tex.uv("body", inset=2)
    slot = p.material("collision_peg")
    p.begin_material(slot)
    solid_box(p, (0.0, 0.03, 0.0), (0.06, 0.06, 0.06),
              uv=body_uv, color=_tint(PEG_GREY, 0.30))
    p.add_note("deliberately tiny 6 cm cube: a level-authored invisible collider carrier, not scenery")
    p.add_note("place it solid: true with the real collider size, occludes: false, and y below the "
               "local floor so the drawn cube stays buried")


# -------------------------------------------------------------- facade kit
#
# Modular house parts in metres. The wall panels share one construction: a
# siding core set back a few millimetres, two corner boards and a thin cap
# owning the catalogue width and depth, and a painted plinth band. The panels
# are symmetric front/back, so a level can rotate them freely.


def _panel_boards(p: PropBuilder, *, top: float, board_depth: float, uv, color) -> None:
    """Two corner boards owning the panel's width extents (x = +/-1.5)."""
    for side in (-1.0, 1.0):
        solid_box(p, (side * 1.3875, top * 0.5, 0.0), (0.225, top, board_depth),
                  uv={"+z": uv, "-z": uv, "+x": uv, "-x": uv, "+y": uv, "-y": uv}, color=color)


def _panel_cap(p: PropBuilder, *, depth: float, uv, color) -> None:
    """Thin cap board over the panel top (y 2.66..2.70), full width/depth."""
    solid_box(p, (0.0, 2.68, 0.0), (3.0, 0.04, depth),
              uv={"+z": uv, "-z": uv, "+x": uv, "-x": uv, "+y": uv, "-y": uv}, color=color)


def _panel_texture(p: PropBuilder, seed: int) -> Texture:
    from parts.refreshed import load_atlas_from
    from pathlib import Path
    source=Path(__file__).resolve().parents[3]/'assets/environment/outdoor/props/models/house_base.png'
    tex=load_atlas_from(p, source, ('siding','trim','glass','jamb'))
    return tex


def build_house_wall_solid(p: PropBuilder) -> None:
    """Plain exterior wall panel: siding both faces, boards, cap, plinth band."""
    tex = _panel_texture(p, seed=1601)
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    slot = p.material("house_siding")
    p.begin_material(slot)

    siding = (255,255,255)
    trim = (255,255,255)
    solid_box(p, (0.0, 1.332, 0.0), (2.57, 2.664, 0.22),
              uv={"+z": siding_uv, "-z": siding_uv, "+x": siding_uv, "-x": siding_uv,
                  "+y": trim_uv, "-y": trim_uv},
              color=siding)
    _panel_boards(p, top=2.66, board_depth=0.24, uv=trim_uv, color=trim)
    _panel_cap(p, depth=0.24, uv=trim_uv, color=trim)
    p.add_note("plain panel: clapboard siding both faces, painted 0.35 m plinth, corner boards and cap")
    p.add_note("use a level size of [3.0, 2.7, 0.24]; opaque, one material")


def build_house_wall_window(p: PropBuilder) -> None:
    """Window panel: a real 0.80 x 0.94 m recess with frame, glass, sill/head.

    The glazing is an opaque dark blue-grey slab set 7 cm behind the front
    face; the reveal surfaces are real jambs. No alpha anywhere.
    """
    tex = _panel_texture(p, seed=1701)
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    glass_uv = tex.uv("glass", inset=2)
    jamb_uv = tex.uv("jamb", inset=2)
    slot = p.material("house_siding")
    p.begin_material(slot)

    siding = (255,255,255)
    trim = (255,255,255)
    # Core in four boxes around the 0.80 x 0.94 m centred opening (x +/-0.40,
    # y 0.88..1.82); the inner boxes sink 5 mm into the columns so no visible
    # faces are coplanar.
    for side in (-1.0, 1.0):
        solid_box(p, (side * 0.8425, 1.332, 0.0), (0.885, 2.664, 0.22),
                  uv={"+z": siding_uv, "-z": siding_uv, "+x": jamb_uv, "-x": siding_uv,
                      "+y": trim_uv, "-y": trim_uv},
                  color=siding)
    solid_box(p, (0.0, 0.44, 0.0), (0.81, 0.88, 0.22),
              uv={"+z": siding_uv, "-z": siding_uv, "+y": jamb_uv, "-y": trim_uv,
                  "+x": siding_uv, "-x": siding_uv},
              color=siding)
    solid_box(p, (0.0, 2.242, 0.0), (0.81, 0.844, 0.22),
              uv={"+z": siding_uv, "-z": siding_uv, "-y": jamb_uv, "+y": trim_uv,
                  "+x": siding_uv, "-x": siding_uv},
              color=siding)
    # Glazing: opaque, set 7 cm behind the front face. A thin closed slab, so
    # the dark glass reads from inside the panel as well as from outside.
    p.begin_material(p.material("warm_window", emissive=(1.0,.78,.46), strength=.7))
    solid_box(p, (0.0, 1.35, 0.03), (0.84, 0.98, 0.03),
              uv={"+z": glass_uv, "-z": glass_uv, "+x": trim_uv, "-x": trim_uv,
                  "+y": trim_uv, "-y": trim_uv},
              color=(238, 242, 248))
    p.begin_material(slot)
    # Frame, sill and head trim, all within the panel's 0.24 m depth.
    for side in (-1.0, 1.0):
        solid_box(p, (side * 0.4375, 1.35, 0.11), (0.075, 1.09, 0.02),
                  uv=trim_uv, color=trim)
    for side in (-1.0, 1.0):
        solid_box(p, (0.0, 1.35 + side * 0.5075, 0.11), (0.80, 0.075, 0.02),
                  uv=trim_uv, color=trim)
    solid_box(p, (0.0, 0.78, 0.11), (1.02, 0.05, 0.02), uv=trim_uv, color=trim)
    solid_box(p, (0.0, 1.925, 0.11), (0.98, 0.06, 0.02), uv=trim_uv, color=trim)
    _panel_boards(p, top=2.66, board_depth=0.24, uv=trim_uv, color=trim)
    _panel_cap(p, depth=0.24, uv=trim_uv, color=trim)
    p.add_note("centred 0.80 x 0.94 m window: real 7 cm reveal, opaque dark glazing, sill and head trim")
    p.add_note("use a level size of [3.0, 2.7, 0.24]; the window is decorative and the panel stays solid")


def build_house_wall_doorway(p: PropBuilder) -> None:
    """Doorway panel: a real 1.10 x 2.15 m opening through 0.40 m.

    Jamb, head and threshold surfaces are real geometry; the panel is
    non-solid in the catalogue and the level authors the actual wall and door
    behind it. Eave lamp mounts are documented at local (+/-1.15, 2.55, 0.20)
    with +Z away from the wall.
    """
    tex = _panel_texture(p, seed=1801)
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    jamb_uv = tex.uv("jamb", inset=2)
    slot = p.material("house_siding")
    p.begin_material(slot)

    siding = (255,255,255)
    trim = (255,255,255)
    # Core in three boxes around the 1.10 x 2.15 m opening (x +/-0.55, floor
    # to 2.15); the head box sinks into the columns to avoid coplanar faces.
    for side in (-1.0, 1.0):
        solid_box(p, (side * 0.9175, 1.332, 0.0), (0.735, 2.664, 0.40),
                  uv={"+z": siding_uv, "-z": siding_uv, "+x": jamb_uv, "-x": siding_uv,
                      "+y": trim_uv, "-y": trim_uv},
                  color=siding)
    solid_box(p, (0.0, 2.407, 0.0), (1.11, 0.514, 0.40),
              uv={"+z": siding_uv, "-z": siding_uv, "-y": jamb_uv, "+y": trim_uv,
                  "+x": siding_uv, "-x": siding_uv},
              color=siding)
    # Threshold board set 1 cm inside the opening, 3 cm proud of the ground.
    solid_box(p, (0.0, 0.015, 0.0), (1.12, 0.03, 0.38),
              uv={"+y": trim_uv, "-y": trim_uv, "+z": trim_uv, "-z": trim_uv,
                  "+x": jamb_uv, "-x": jamb_uv},
              color=_tint(POST_WOOD, 0.50))
    _panel_boards(p, top=2.66, board_depth=0.408, uv=trim_uv, color=trim)
    _panel_cap(p, depth=0.408, uv=trim_uv, color=trim)
    p.add_note("real 1.10 x 2.15 m doorway: jambs, head and a 0.03 m threshold board through 0.40 m depth")
    p.add_note("non-solid; the level authors the real wall and door behind it")
    p.add_note("eave lamp mounts: local [+/-1.15, 2.55, 0.20], +Z away from the wall "
               "(outdoor:lamp_wall's plate top meets the mount, so place it 0.52 m lower)")


def build_house_roof_slope(p: PropBuilder) -> None:
    """Pitched roof slope: ~33 degrees, rising towards -Z.

    The eave is at +Z with a vertical fascia board and a horizontal soffit
    underside; the origin is the soffit/fascia underside centre at the eave.
    Two materials: a shingle field on the top face and painted trim elsewhere.
    """
    width, height, depth = p.size  # [3.4, 1.75, 2.6]
    tex = p.set_texture(256, seed=1901)
    tex.auto("shingle", "trim", cols=2)
    _paint_shingles(tex, "shingle", SHINGLE, seed=1903)
    _paint_trim(tex, "trim", palette.shade(TRIM, 0.92), seed=1913)
    shingle_uv = tex.uv("shingle", inset=2)
    trim_uv = tex.uv("trim", inset=2)

    shingle_slot = p.material("roof_shingle")
    trim_slot = p.material("roof_trim")

    half_x = width * 0.5
    eave_z, ridge_z = depth * 0.5, -depth * 0.5
    eave_top, ridge_top = 0.10, height
    slab_eave_z = eave_z - 0.01
    shingle = _tint(SHINGLE, 0.35)
    trim = _tint(palette.shade(TRIM, 0.92), 0.50)

    def quad(p0, p1, p2, p3, uv, color, shade):
        p.mesh.quad(p0, p1, p2, p3, uv=uv, color=color, shade_mult=shade)

    p.begin_material(shingle_slot)
    # Shingle field, eave (+Z, low) to ridge (-Z, high).
    quad((-half_x, eave_top, slab_eave_z), (half_x, eave_top, slab_eave_z),
         (half_x, ridge_top, ridge_z), (-half_x, ridge_top, ridge_z),
         shingle_uv, shingle, 1.0)

    p.begin_material(trim_slot)
    # Underside and the two edge strips.
    quad((-half_x, eave_top - 0.08, slab_eave_z), (-half_x, ridge_top - 0.08, ridge_z),
         (half_x, ridge_top - 0.08, ridge_z), (half_x, eave_top - 0.08, slab_eave_z),
         trim_uv, trim, 0.62)
    quad((-half_x, eave_top - 0.08, slab_eave_z), (half_x, eave_top - 0.08, slab_eave_z),
         (half_x, eave_top, slab_eave_z), (-half_x, eave_top, slab_eave_z),
         trim_uv, trim, 0.80)
    quad((half_x, ridge_top - 0.08, ridge_z), (-half_x, ridge_top - 0.08, ridge_z),
         (-half_x, ridge_top, ridge_z), (half_x, ridge_top, ridge_z),
         trim_uv, trim, 0.80)
    quad((half_x, eave_top - 0.08, slab_eave_z), (half_x, ridge_top - 0.08, ridge_z),
         (half_x, ridge_top, ridge_z), (half_x, eave_top, slab_eave_z),
         trim_uv, trim, 0.82)
    quad((-half_x, eave_top - 0.08, slab_eave_z), (-half_x, eave_top, slab_eave_z),
         (-half_x, ridge_top, ridge_z), (-half_x, ridge_top - 0.08, ridge_z),
         trim_uv, trim, 0.82)
    # Fascia at the eave and the soffit board under it.
    solid_box(p, (0.0, 0.08, 1.27), (width, 0.16, 0.06),
              uv={"+z": trim_uv, "-z": trim_uv, "+x": trim_uv, "-x": trim_uv,
                  "+y": trim_uv, "-y": trim_uv},
              color=trim)
    solid_box(p, (0.0, 0.0175, 1.10), (width, 0.035, 0.28),
              uv={"+z": trim_uv, "-z": trim_uv, "+x": trim_uv, "-x": trim_uv,
                  "+y": trim_uv, "-y": trim_uv},
              color=trim)
    p.add_note("pitch ~32.5 degrees rising towards -Z; eave at +Z with fascia and soffit, "
               "origin at the eave's underside centre")
    p.add_note("shingle top face and painted trim only; 2 materials")


def build_house_roof_ridge(p: PropBuilder) -> None:
    """Ridge cap: a triangular prism covering the joint of two slopes.

    The origin is the ridge centre; the cap's underside sits at y = 0 with its
    apex edge at y = 0.22, 3.4 m along the ridge line (local X).
    """
    width, height, depth = p.size  # [3.4, 0.22, 0.6]
    tex = p.set_texture(128, seed=2001)
    tex.auto("shingle", "trim")
    _paint_shingles(tex, "shingle", SHINGLE, seed=2003)
    _paint_trim(tex, "trim", palette.shade(TRIM, 0.80), seed=2011)
    shingle_uv = tex.uv("shingle", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    shingle = _tint(SHINGLE, 0.35)
    trim = _tint(palette.shade(TRIM, 0.80), 0.50)
    slot = p.material("ridge_shingle")
    p.begin_material(slot)
    half_x, half_z = width * 0.5, depth * 0.5
    # Two sloped top faces meeting at the apex edge.
    p.mesh.quad((-half_x, 0.0, half_z), (half_x, 0.0, half_z),
                (half_x, height, 0.0), (-half_x, height, 0.0),
                uv=shingle_uv, color=shingle, shade_mult=1.0)
    p.mesh.quad((-half_x, height, 0.0), (half_x, height, 0.0),
                (half_x, 0.0, -half_z), (-half_x, 0.0, -half_z),
                uv=shingle_uv, color=shingle, shade_mult=0.94)
    # Gable ends and the (hidden) underside close the prism.
    p.mesh.triangle((-half_x, height, 0.0), (-half_x, 0.0, -half_z), (-half_x, 0.0, half_z),
                    uvs=[((shingle_uv[0] + shingle_uv[2]) * 0.5, (shingle_uv[1] + shingle_uv[3]) * 0.5),
                         (shingle_uv[0], shingle_uv[3]), (shingle_uv[2], shingle_uv[3])],
                    color=shingle, shade_mult=0.78)
    p.mesh.triangle((half_x, height, 0.0), (half_x, 0.0, half_z), (half_x, 0.0, -half_z),
                    uvs=[((shingle_uv[0] + shingle_uv[2]) * 0.5, (shingle_uv[1] + shingle_uv[3]) * 0.5),
                         (shingle_uv[2], shingle_uv[3]), (shingle_uv[0], shingle_uv[3])],
                    color=shingle, shade_mult=0.78)
    p.mesh.quad((-half_x, 0.0, half_z), (-half_x, 0.0, -half_z),
                (half_x, 0.0, -half_z), (half_x, 0.0, half_z),
                uv=trim_uv, color=trim, shade_mult=0.62)
    p.add_note("ridge cap over the joint of two slopes: apex edge at y=0.22 along local X, "
               "underside at y=0, origin at the ridge centre")
    p.add_note("matches outdoor:house_roof_slope's ~32.5 degree pitch; one opaque material")


def build_house_corner_trim(p: PropBuilder) -> None:
    """Vertical L-section corner board (0.18 x 2.7 x 0.18 m).

    The board's two outer faces look towards -X and -Z: rotate the placement
    by 0/90/180/270 degrees to cover any house corner.
    """
    size = p.size  # [0.18, 2.7, 0.18]
    tex = p.set_texture(128, seed=2101)
    tex.auto("trim", "edge")
    _paint_trim(tex, "trim", TRIM, seed=2103)
    tex.fill("edge", palette.shade(TRIM, 0.80), jitter=5, seed=2109)
    tex.grain("edge", palette.shade(TRIM, 0.6), seed=2111, density=0.3, alpha=40)
    trim_uv = tex.uv("trim", inset=2)
    edge_uv = tex.uv("edge", inset=2)
    trim = _tint(TRIM, 0.52)
    slot = p.material("corner_trim")
    p.begin_material(slot)
    # Leg A covers the -Z side, leg B returns along -X; they meet at z = -0.03
    # so no visible faces are coplanar.
    solid_box(p, (0.0, size[1] * 0.5, -0.06), (0.18, size[1], 0.06),
              uv={"+z": trim_uv, "-z": trim_uv, "+x": edge_uv, "-x": trim_uv,
                  "+y": edge_uv, "-y": edge_uv},
              color=trim)
    solid_box(p, (-0.06, size[1] * 0.5, 0.03), (0.06, size[1], 0.12),
              uv={"+z": trim_uv, "-z": trim_uv, "+x": trim_uv, "-x": trim_uv,
                  "+y": edge_uv, "-y": edge_uv},
              color=trim)
    p.add_note("L-section corner board; outer faces look towards -X and -Z "
               "(rotate 0/90/180/270 to cover a corner)")
    p.add_note("base on the floor at y=0; one opaque trim material")


def build_concrete_step(p: PropBuilder) -> None:
    """Low cast concrete step: a chamfered slab, base on the floor."""
    width, height, depth = p.size  # [1.4, 0.18, 0.7]
    tex = p.set_texture(128, seed=2201)
    tex.auto("body", "top")
    _paint_concrete(tex, "body", seed=2203)
    tex.fill("top", palette.shade(CONCRETE, 1.06), jitter=5, seed=2211)
    tex.noise("top", amount=4, freq=4, seed=2213)
    tex.spots("top", palette.shade(CONCRETE, 0.88), count=4, seed=2217, radius=3, alpha=24)
    tex.border("top", palette.shade(CONCRETE, 0.78), width=1, alpha=60)
    body_uv = tex.uv("body", inset=2)
    top_uv = tex.uv("top", inset=2)
    tint = _tint(CONCRETE, 0.55)
    top_tint = _tint(CONCRETE, 0.68)
    slot = p.material("concrete")
    p.begin_material(slot)
    half_x, half_z = width * 0.5, depth * 0.5
    chamfer, top_h = 0.02, height
    levels = (
        (0.0, half_x, half_z),
        (top_h - chamfer, half_x, half_z),
        (top_h, half_x - chamfer, half_z - chamfer),
    )
    rings = [
        [
            (radial_x, level_y, radial_z)
            for radial_x, radial_z in ((hx, hz), (-hx, hz), (-hx, -hz), (hx, -hz))
        ]
        for level_y, hx, hz in levels
    ]

    def side(ring_low, ring_high, uv, color, shade):
        count = len(ring_low)
        for corner in range(count):
            nxt = (corner + 1) % count
            p.mesh.quad(ring_low[corner], ring_high[corner], ring_high[nxt], ring_low[nxt],
                        uv=uv, color=color, shade_mult=shade)

    side(rings[0], rings[1], body_uv, tint, 0.86)
    side(rings[1], rings[2], body_uv, tint, 0.94)
    p.mesh.quad(rings[0][0], rings[0][1], rings[0][2], rings[0][3],
                uv=body_uv, color=tint, shade_mult=0.62)
    p.mesh.quad(rings[2][0], rings[2][3], rings[2][2], rings[2][1],
                uv=top_uv, color=top_tint, shade_mult=1.0)
    p.add_note("low cast step with a 2 cm top-edge chamfer; base on the floor at y=0")
    p.add_note("pale opaque concrete, one material; author the level size with solid true to stand on it")


PROPS = {
    "outdoor:tree_01": build_tree,
    "outdoor:lamp_stand": build_lamp_stand,
    "outdoor:lamp_fence": build_lamp_fence,
    "outdoor:lamp_wall": build_lamp_wall,
    "outdoor:fence_post": build_fence_post,
    "outdoor:collision_peg": build_collision_peg,
    "outdoor:house_wall_solid": build_house_wall_solid,
    "outdoor:house_wall_window": build_house_wall_window,
    "outdoor:house_wall_doorway": build_house_wall_doorway,
    "outdoor:house_roof_slope": build_house_roof_slope,
    "outdoor:house_roof_ridge": build_house_roof_ridge,
    "outdoor:house_corner_trim": build_house_corner_trim,
    "outdoor:concrete_step": build_concrete_step,
}


# The concept reconstruction owns only these Outdoors ids.
from parts.outdoor_remade import REBUILDS
PROPS.update({key: build for key, build in REBUILDS.items() if key in PROPS})
